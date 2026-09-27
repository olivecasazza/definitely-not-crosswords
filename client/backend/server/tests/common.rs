//! Shared test helpers for integration tests in `tests/`.

use crossword_auth::AuthContext;
use crossword_db::{AuthUser, Role};
use crossword_server::ctx::Ctx;
use crossword_server::mailer::Mailer;
use std::env;

/// Build a `PgPool` from `DATABASE_URL`.
pub async fn pool() -> sqlx::PgPool {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await
        .expect("must connect to the test database")
}

/// A standard admin-caped user for tests that don't specifically test role gates.
pub fn admin_user() -> AuthUser {
    AuthUser {
        id: "integration-test-admin".to_string(),
        email: "admin@test".to_string(),
        role: Role::Admin,
    }
}

/// Build a `Ctx` for a given pool + user.
pub fn ctx(pool: &sqlx::PgPool, user: &AuthUser) -> Ctx {
    ctx_as(pool, Some(user))
}

/// Build a `Ctx` for a given pool + optional user. `None` is an anonymous
/// caller — the state `authenticate` produces when there is no session cookie.
pub fn ctx_as(pool: &sqlx::PgPool, user: Option<&AuthUser>) -> Ctx {
    let auth = AuthContext {
        user: user.cloned(),
        ..Default::default()
    };
    Ctx {
        pool: pool.clone(),
        auth,
        events: crossword_events::EventBus::default(),
        mailer: Mailer::from_env("test"),
    }
}

/// A pool that is never dialled, for auth-gate tests that must be refused
/// before any query runs. Port 1 is reserved and nothing listens on it, so a
/// handler that *does* reach the database fails immediately rather than waiting
/// out the default 30 s acquire timeout.
pub fn undialled_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(250))
        .connect_lazy("postgres://localhost:1/never-dialled")
        .expect("lazy pool builds without a server")
}
