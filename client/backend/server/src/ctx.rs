//! Per-request context handed to every router handler.

use crate::mailer::Mailer;
use crossword_auth::AuthContext;
use crossword_db::AuthUser;
use crossword_events::EventBus;
use sqlx::PgPool;

pub struct Ctx {
    pub pool: PgPool,
    pub auth: AuthContext,
    /// Publish AppEvents for live subscriptions (onAddActions/onGameCompleted).
    pub events: EventBus,
    /// Outbound email (verification / password reset links).
    pub mailer: Mailer,
}

impl Ctx {
    /// The authenticated user, or a tRPC-style UNAUTHORIZED error.
    /// Use in `protectedProcedure` ports: `let user = ctx.require_user()?;`
    pub fn require_user(&self) -> Result<&AuthUser, String> {
        self.auth
            .user
            .as_ref()
            .ok_or_else(|| "UNAUTHORIZED".to_string())
    }
}

/// Turn a database failure into a client-safe error, keeping the detail in the
/// server log.
///
/// Stringifying the driver error handed callers raw Postgres text — table and
/// column names, constraint names, sometimes the values that collided. That is
/// schema the client has no business reading, so the detail goes to
/// `tracing::error!` and the client gets a stable message naming only what it
/// was trying to do. `what` is a fixed, caller-chosen phrase; never interpolate
/// user input into it.
pub fn sanitised_db_error(what: &str, e: &sqlx::Error) -> String {
    tracing::error!(error = %e, operation = what, "database operation failed");
    format!("{what} failed")
}
