//! Per-request context handed to every router handler.

use crossword_auth::AuthContext;
use crossword_db::AuthUser;
use sqlx::PgPool;

pub struct Ctx {
    pub pool: PgPool,
    pub auth: AuthContext,
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
