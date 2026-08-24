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
    let auth = AuthContext {
        user: Some(user.clone()),
        ..Default::default()
    };
    Ctx {
        pool: pool.clone(),
        auth,
        events: crossword_events::EventBus::default(),
        mailer: Mailer::from_env("test"),
    }
}
