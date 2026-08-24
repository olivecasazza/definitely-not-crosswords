//! Outbound email over SMTP submission — Cloudflare Email Service, since the
//! zone is already on Cloudflare and onboarding auto-publishes SPF/DKIM/DMARC.
//! Credentials are literally `api_token` plus a Cloudflare API token carrying
//! "Email Sending: Edit". The whole provider surface is one function (`send`),
//! so swapping providers later is a one-function edit.
//!
//! Implicit TLS on 465 — Cloudflare does NOT support STARTTLS on this endpoint.
//!
//! Unconfigured (no `SMTP_USER`/`SMTP_PASSWORD`): logs the message body instead
//! of sending, so local dev and e2e work with zero creds and verification/reset
//! links can be fished out of the server log.
//!
//! ponytail: no retry/queue. A failed send is logged and dropped — the user can
//! re-request a reset from the UI. Add a queue only if delivery failures show up
//! in practice.

use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};

#[derive(Clone)]
pub struct Mailer {
    /// None when unconfigured — send() then logs instead of sending.
    smtp: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from: String,
    /// Absolute origin used to build links in email bodies.
    origin: String,
}

impl Mailer {
    /// `SMTP_USER` + `SMTP_PASSWORD` (both required to send; for Cloudflare the
    /// user is the literal `api_token` and the password is an API token with
    /// "Email Sending: Edit"), `SMTP_HOST` (default smtp.mx.cloudflare.net),
    /// `SMTP_PORT` (default 465, implicit TLS), `MAIL_FROM` (optional — must be
    /// on a domain onboarded for Email Sending), `APP_ORIGIN` (optional —
    /// defaults derived from APP_ENV, which maps 1:1 to a public host).
    pub fn from_env(app_env: &str) -> Self {
        let origin = std::env::var("APP_ORIGIN").unwrap_or_else(|_| {
            match app_env {
                "production" => "https://crosswords.casazza.io",
                "staging" => "https://crosswords-staging.casazza.io",
                _ => "http://localhost:3001",
            }
            .to_string()
        });
        // noreply.casazza.io (not the apex) is what's onboarded for Email
        // Sending, so the From must sit on that subdomain or Cloudflare rejects
        // it — hence the doubled "noreply".
        let from = std::env::var("MAIL_FROM")
            .unwrap_or_else(|_| "Definitely Not Crosswords <noreply@noreply.casazza.io>".into());

        let non_empty = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let smtp = match (non_empty("SMTP_USER"), non_empty("SMTP_PASSWORD")) {
            (Some(user), Some(pass)) => {
                let host = std::env::var("SMTP_HOST")
                    .unwrap_or_else(|_| "smtp.mx.cloudflare.net".to_string());
                let port = std::env::var("SMTP_PORT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(465u16);
                // relay() = implicit TLS. Cloudflare rejects STARTTLS on 465.
                match AsyncSmtpTransport::<Tokio1Executor>::relay(&host) {
                    Ok(b) => Some(
                        b.port(port)
                            .credentials(Credentials::new(user, pass))
                            .build(),
                    ),
                    Err(e) => {
                        tracing::error!("SMTP transport for {host}:{port} failed to build: {e}");
                        None
                    }
                }
            }
            _ => None,
        };
        if smtp.is_none() {
            tracing::warn!("SMTP_USER/SMTP_PASSWORD unset — emails will be logged, not sent");
        }
        Self { smtp, from, origin }
    }

    /// Send (or log) one email. Errors are logged, not returned: no caller
    /// should fail a signup/reset request because the mail provider hiccuped —
    /// the user can always retry from the UI.
    async fn send(&self, to: &str, subject: &str, html: String) {
        let Some(smtp) = &self.smtp else {
            // Dev fallback: the link IS the payload — make it easy to grab.
            tracing::warn!("mail (not sent) to={to} subject={subject:?} body={html}");
            return;
        };
        let msg = match (self.from.parse(), to.parse()) {
            (Ok(from), Ok(to_addr)) => Message::builder()
                .from(from)
                .to(to_addr)
                .subject(subject)
                .header(ContentType::TEXT_HTML)
                .body(html),
            _ => {
                tracing::error!("mail to={to} skipped: unparseable from/to address");
                return;
            }
        };
        match msg {
            Ok(m) => {
                if let Err(e) = smtp.send(m).await {
                    tracing::error!("mail to={to} failed: {e}");
                }
            }
            Err(e) => tracing::error!("mail to={to} failed to build: {e}"),
        }
    }

    pub async fn send_verification(&self, to: &str, token: &str) {
        let url = format!("{}/auth/verify-email?token={token}", self.origin);
        self.send(
            to,
            "Verify your email — Definitely Not Crosswords",
            format!(
                "<p>Welcome! Confirm this address to finish setting up your account:</p>\
                 <p><a href=\"{url}\">Verify my email</a></p>\
                 <p>Or paste this link into your browser:<br>{url}</p>\
                 <p>This link expires in 24 hours. If you didn't sign up, ignore this email.</p>"
            ),
        )
        .await;
    }

    pub async fn send_password_reset(&self, to: &str, token: &str) {
        let url = format!("{}/auth/reset-password?token={token}", self.origin);
        self.send(
            to,
            "Reset your password — Definitely Not Crosswords",
            format!(
                "<p>Someone (hopefully you) asked to reset this account's password:</p>\
                 <p><a href=\"{url}\">Choose a new password</a></p>\
                 <p>Or paste this link into your browser:<br>{url}</p>\
                 <p>This link expires in 1 hour. If you didn't ask, ignore this email — \
                 your password is unchanged.</p>"
            ),
        )
        .await;
    }
}
