//! Domain contracts shared by the Rust migration crates.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub const fn capabilities(self) -> &'static [Capability] {
        match self {
            Self::User => &[Capability::GamePlay, Capability::ProfileManage],
            Self::Admin => &[
                Capability::GamePlay,
                Capability::ProfileManage,
                Capability::AdminAccess,
                Capability::GeneratorManage,
            ],
        }
    }

    pub fn has(self, capability: Capability) -> bool {
        self.capabilities().contains(&capability)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    GamePlay,
    ProfileManage,
    AdminAccess,
    GeneratorManage,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::GamePlay => "game:play",
            Self::ProfileManage => "profile:manage",
            Self::AdminAccess => "admin:access",
            Self::GeneratorManage => "generator:manage",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameActionDto {
    pub id: String,
    pub active_game_id: String,
    pub user_id: String,
    pub cord_x: i32,
    pub cord_y: i32,
    pub action_type: String,
    pub previous_state: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GenerationEvent {
    Started {
        job_id: String,
        at_ms: u64,
    },
    Progress {
        job_id: String,
        stage: String,
        progress: f32,
        message: String,
        at_ms: u64,
    },
    Completed {
        job_id: String,
        game_id: String,
        title: String,
        question_count: usize,
        at_ms: u64,
    },
    Failed {
        job_id: Option<String>,
        error: String,
        at_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppEvent {
    GameActionsAdded {
        active_game_id: String,
        // Raw camelCase action objects (as the client consumes them), so the WS
        // forwards exactly what `activeGame.addActions` produced.
        actions: Vec<serde_json::Value>,
    },
    GameCompleted {
        active_game_id: String,
        completed_game_id: String,
    },
    /// Ephemeral co-op presence: which clue a member is focused on right now.
    /// `number`/`direction` are `None` when the player clears their selection.
    /// Not persisted — broadcast-only, fan-out via `activeGame.onPresence`.
    GamePresence {
        active_game_id: String,
        user_id: String,
        name: String,
        number: Option<i32>,
        direction: Option<String>,
    },
    GenerationProgress {
        event: GenerationEvent,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("missing capability {0}")]
    Forbidden(Capability),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn envelope(&self) -> ErrorEnvelope {
        match self {
            Self::Unauthorized => ErrorEnvelope {
                code: "UNAUTHORIZED".into(),
                message: self.to_string(),
            },
            Self::Forbidden(_) => ErrorEnvelope {
                code: "FORBIDDEN".into(),
                message: self.to_string(),
            },
            Self::BadRequest(_) => ErrorEnvelope {
                code: "BAD_REQUEST".into(),
                message: self.to_string(),
            },
            Self::Internal(_) => ErrorEnvelope {
                code: "INTERNAL".into(),
                message: self.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DbConfig {
    /// Base database URL (e.g. from `DATABASE_URL` environment variable)
    pub database_url: String,
    /// Connection pool limit (connection_limit parameter)
    pub connection_limit: Option<u32>,
    /// SSL mode (sslmode parameter, e.g. "require", "prefer", "disable")
    pub ssl_mode: Option<String>,
    /// Path to SSL root certificate (sslrootcert parameter)
    pub ssl_root_cert: Option<String>,
    /// SSL client certificate (sslcert parameter)
    pub ssl_cert: Option<String>,
    /// SSL client key (sslkey parameter)
    pub ssl_key: Option<String>,
}

impl DbConfig {
    /// Constructs a connection URL containing the connection pooling and SSL options.
    pub fn build_url(&self) -> String {
        let mut url = self.database_url.clone();

        let mut params = Vec::new();
        if let Some(limit) = self.connection_limit {
            params.push(format!("connection_limit={}", limit));
        }
        if let Some(ref ssl_mode) = self.ssl_mode {
            params.push(format!("sslmode={}", ssl_mode));
        }
        if let Some(ref ssl_root_cert) = self.ssl_root_cert {
            params.push(format!("sslrootcert={}", ssl_root_cert));
        }
        if let Some(ref ssl_cert) = self.ssl_cert {
            params.push(format!("sslcert={}", ssl_cert));
        }
        if let Some(ref ssl_key) = self.ssl_key {
            params.push(format!("sslkey={}", ssl_key));
        }

        if !params.is_empty() {
            let joiner = if url.contains('?') { "&" } else { "?" };
            url.push_str(joiner);
            url.push_str(&params.join("&"));
        }

        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_inherits_generator_capability() {
        assert!(Role::Admin.has(Capability::GeneratorManage));
        assert!(!Role::User.has(Capability::GeneratorManage));
    }

    #[test]
    fn db_config_url_building() {
        let config = DbConfig {
            database_url: "postgresql://postgres:pass@localhost:5432/db".to_string(),
            connection_limit: Some(5),
            ssl_mode: Some("require".to_string()),
            ssl_root_cert: Some("/path/to/cert.pem".to_string()),
            ssl_cert: None,
            ssl_key: None,
        };
        let url = config.build_url();
        assert!(url.contains("postgresql://postgres:pass@localhost:5432/db"));
        assert!(url.contains("connection_limit=5"));
        assert!(url.contains("sslmode=require"));
        assert!(url.contains("sslrootcert=/path/to/cert.pem"));

        // Test with existing query parameters
        let config_with_query = DbConfig {
            database_url: "postgresql://postgres:pass@localhost:5432/db?existing=true".to_string(),
            connection_limit: Some(10),
            ssl_mode: None,
            ssl_root_cert: None,
            ssl_cert: None,
            ssl_key: None,
        };
        let url_with_query = config_with_query.build_url();
        assert_eq!(
            url_with_query,
            "postgresql://postgres:pass@localhost:5432/db?existing=true&connection_limit=10"
        );
    }
}
