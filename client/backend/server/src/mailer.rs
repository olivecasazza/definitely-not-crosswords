//! Outbound email. Speaks the Resend HTTP API via `reqwest` (already a dep —
//! SMTP would drag in a new crate). The whole provider surface is one function
//! (`send`), so swapping to SES/Mailgun/SMTP later is a one-function edit.
//!
//! Unconfigured (no `RESEND_API_KEY`): logs the message body instead of
//! sending, so local dev and e2e work with zero creds and verification/reset
//! links can be fished out of the server log.

use serde_json::json;

#[derive(Clone)]
pub struct Mailer {
    api_key: Option<String>,
    from: String,
    /// Absolute origin used to build links in email bodies.
    origin: String,
    http: reqwest::Client,
}

impl Mailer {
    /// `RESEND_API_KEY` (optional), `MAIL_FROM` (optional), `APP_ORIGIN`
    /// (optional — defaults derived from APP_ENV, which maps 1:1 to a public
    /// host; override via env if that ever stops being true).
    pub fn from_env(app_env: &str) -> Self {
        let origin = std::env::var("APP_ORIGIN").unwrap_or_else(|_| {
            match app_env {
                "production" => "https://crosswords.casazza.io",
                "staging" => "https://crosswords-staging.casazza.io",
                _ => "http://localhost:3001",
            }
            .to_string()
        });
        let api_key = std::env::var("RESEND_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        if api_key.is_none() {
            tracing::warn!("RESEND_API_KEY unset — emails will be logged, not sent");
        }
        Self {
            api_key,
            from: std::env::var("MAIL_FROM")
                .unwrap_or_else(|_| "Definitely Not Crosswords <noreply@casazza.io>".into()),
            origin,
            http: reqwest::Client::new(),
        }
    }

    /// Send (or log) one email. Errors are logged, not returned: no caller
    /// should fail a signup/reset request because the mail provider hiccuped —
    /// the user can always retry from the UI.
    async fn send(&self, to: &str, subject: &str, html: String) {
        let Some(key) = &self.api_key else {
            // Dev fallback: the link IS the payload — make it easy to grab.
            tracing::warn!("mail (not sent) to={to} subject={subject:?} body={html}");
            return;
        };
        let res = self
            .http
            .post("https://api.resend.com/emails")
            .bearer_auth(key)
            .json(&json!({ "from": self.from, "to": [to], "subject": subject, "html": html }))
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => tracing::error!("mail to={to} failed: HTTP {}", r.status()),
            Err(e) => tracing::error!("mail to={to} failed: {e}"),
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
