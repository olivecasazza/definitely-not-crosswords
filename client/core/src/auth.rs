//! Roles and capabilities, ported from `lib/auth/roles.ts`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Role {
    #[default]
    User,
    Admin,
}

/// Capability strings, matching the TS map (used for tRPC middleware parity).
pub fn capabilities(role: Role) -> &'static [&'static str] {
    match role {
        Role::User => &["game:play", "profile:manage"],
        Role::Admin => &[
            "game:play",
            "profile:manage",
            "admin:access",
            "generator:manage",
        ],
    }
}

pub fn has_capability(role: Role, capability: &str) -> bool {
    capabilities(role).contains(&capability)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_capabilities() {
        assert!(has_capability(Role::Admin, "admin:access"));
        assert!(!has_capability(Role::User, "admin:access"));
        assert!(has_capability(Role::User, "game:play"));
    }

    #[test]
    fn role_json() {
        assert_eq!(
            serde_json::from_str::<Role>("\"ADMIN\"").unwrap(),
            Role::Admin
        );
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"USER\"");
    }
}
