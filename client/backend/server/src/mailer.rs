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
use serde::Serialize;

/// The env vars that must both be non-empty for `Mailer` to build a real SMTP
/// transport. Mirrors `checkout::LS_KEYS`, which the Helm chart gates
/// credential injection on (`mail.existingSecret`,
/// `charts/definitely-not-crosswords/templates/deployment.yaml`).
pub const MAIL_CREDENTIAL_KEYS: [&str; 2] = ["SMTP_USER", "SMTP_PASSWORD"];

/// What `Mailer::send` will actually do with a message. `Smtp` = a real
/// transport exists and the message leaves the process. `Log` = no transport,
/// so the body is written to the pod log and dropped — a successful HTTP
/// response to the caller either way, which is why this is worth surfacing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryMode {
    Smtp,
    Log,
}

/// The single source of truth for "does this process have mail credentials",
/// shared by `from_env` (which needs the values) and `delivery_mode_with`
/// (which only needs the boolean). Both must be set AND non-empty; an empty
/// string in a Secret is the classic half-wired deploy.
fn credentials_with(get: impl Fn(&str) -> Option<String>) -> Option<(String, String)> {
    let user = get(MAIL_CREDENTIAL_KEYS[0]).filter(|v| !v.is_empty())?;
    let pass = get(MAIL_CREDENTIAL_KEYS[1]).filter(|v| !v.is_empty())?;
    Some((user, pass))
}

/// The mode the *environment* implies, without touching the transport. Takes
/// the lookup so the derivation is unit-testable without the process env — the
/// same seam `checkout::ls_configured_with` provides. `/api/config` prefers
/// `Mailer::delivery_mode`, which reads the live transport, so a relay that
/// failed to build also reports `log` instead of lying about a working SMTP
/// transport it does not hold.
pub fn delivery_mode_with(get: impl Fn(&str) -> Option<String>) -> DeliveryMode {
    if credentials_with(get).is_some() {
        DeliveryMode::Smtp
    } else {
        DeliveryMode::Log
    }
}

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

        let smtp = match credentials_with(|k| std::env::var(k).ok()) {
            Some((user, pass)) => {
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

    /// True when a real SMTP transport exists and `send` will hand messages to
    /// the network. False means bodies land in the pod log instead (DEF-201).
    pub fn is_configured(&self) -> bool {
        self.smtp.is_some()
    }

    /// The live delivery mode, read off the transport rather than the env — so
    /// a relay that failed to build reports `log` too, which is the truth.
    /// This is what `/api/config`'s `mailDelivery` reports.
    pub fn delivery_mode(&self) -> DeliveryMode {
        if self.smtp.is_some() {
            DeliveryMode::Smtp
        } else {
            DeliveryMode::Log
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(map: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            map.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    const BOTH: [(&str, &str); 2] = [("SMTP_USER", "api_token"), ("SMTP_PASSWORD", "cf-token")];

    #[test]
    fn smtp_only_when_both_credentials_present() {
        assert_eq!(delivery_mode_with(lookup(&BOTH)), DeliveryMode::Smtp);
        // Unrelated vars don't matter — the chart injects a lot of them.
        assert_eq!(
            delivery_mode_with(lookup(&[
                ("SMTP_USER", "api_token"),
                ("SMTP_PASSWORD", "cf-token"),
                ("MAIL_FROM", "noreply@noreply.casazza.io"),
                ("APP_ORIGIN", "https://crosswords-staging.casazza.io"),
            ])),
            DeliveryMode::Smtp
        );
    }

    /// The half-wired deploy: the secret exists but is missing a key, so the
    /// chart injects `SMTP_USER` and nothing else. Must report `log`, not
    /// `smtp` — this is the case that silently ate the alpha invites (DEF-201).
    #[test]
    fn log_when_either_credential_missing() {
        assert_eq!(
            delivery_mode_with(lookup(&[("SMTP_USER", "api_token")])),
            DeliveryMode::Log
        );
        assert_eq!(
            delivery_mode_with(lookup(&[("SMTP_PASSWORD", "cf-token")])),
            DeliveryMode::Log
        );
        assert_eq!(delivery_mode_with(lookup(&[])), DeliveryMode::Log);
    }

    /// An empty string is not a credential. A Secret key present but blank is
    /// indistinguishable from unset to `from_env`, so the flag must agree.
    #[test]
    fn log_when_either_credential_empty() {
        assert_eq!(
            delivery_mode_with(lookup(&[("SMTP_USER", ""), ("SMTP_PASSWORD", "cf-token")])),
            DeliveryMode::Log
        );
        assert_eq!(
            delivery_mode_with(lookup(&[("SMTP_USER", "api_token"), ("SMTP_PASSWORD", "")])),
            DeliveryMode::Log
        );
    }

    /// The flag must be derived from the same predicate `from_env` builds the
    /// transport with, or `/api/config` can disagree with what `send` does.
    #[test]
    fn dropping_any_credential_key_flips_the_mode_to_log() {
        assert_eq!(MAIL_CREDENTIAL_KEYS, ["SMTP_USER", "SMTP_PASSWORD"]);
        assert!(credentials_with(lookup(&BOTH)).is_some());
        assert_eq!(delivery_mode_with(lookup(&BOTH)), DeliveryMode::Smtp);
        for drop in MAIL_CREDENTIAL_KEYS {
            let partial: Vec<(&str, &str)> =
                BOTH.iter().copied().filter(|(k, _)| *k != drop).collect();
            assert!(
                credentials_with(lookup(&partial)).is_none(),
                "{drop} should be required"
            );
            assert_eq!(delivery_mode_with(lookup(&partial)), DeliveryMode::Log);
        }
    }

    /// `/api/config` is unauthenticated: it may only ever serialise the mode,
    /// never a host, port, username, From, or origin.
    #[test]
    fn serialized_mode_leaks_nothing() {
        let json = serde_json::to_string(&DeliveryMode::Smtp).unwrap();
        assert_eq!(json, "\"smtp\"");
        assert_eq!(
            serde_json::to_string(&DeliveryMode::Log).unwrap(),
            "\"log\""
        );
    }
}
