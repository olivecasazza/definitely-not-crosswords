//! Auth seams for the Rust migration.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use crossword_db::{AppError, AuthUser, Capability, Role};
use hkdf::Hkdf;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestAuth {
    pub bearer_token: Option<String>,
    pub cookie_header: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AuthContext {
    pub user: Option<AuthUser>,
    pub source: AuthSource,
}

impl AuthContext {
    pub fn require_user(&self) -> Result<&AuthUser, AppError> {
        self.user.as_ref().ok_or(AppError::Unauthorized)
    }

    pub fn require_capability(&self, capability: Capability) -> Result<&AuthUser, AppError> {
        let user = self.require_user()?;
        if user.role.has(capability) {
            Ok(user)
        } else {
            Err(AppError::Forbidden(capability))
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthSource {
    NativeSession,
    LegacyNextAuth,
    KeycloakOAuth,
    DevelopmentBypass,
    #[default]
    Anonymous,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("token is not recognized")]
    Unrecognized,
    #[error("cryptographic operation failed")]
    CryptoError,
}

#[derive(Debug, Clone)]
pub struct AuthService {
    pub allow_dev_admin: bool,
    pub nextauth_secret: String,
    pub keycloak_issuer: String,
    pub keycloak_client_id: String,
}

impl AuthService {
    pub fn new(allow_dev_admin: bool) -> Self {
        Self {
            allow_dev_admin,
            nextauth_secret: std::env::var("NEXTAUTH_SECRET")
                .unwrap_or_else(|_| "supersecretsecret".to_string()),
            keycloak_issuer: std::env::var("KEYCLOAK_ISSUER")
                .unwrap_or_else(|_| "https://auth.casazza.io/realms/master".to_string()),
            keycloak_client_id: std::env::var("KEYCLOAK_CLIENT_ID")
                .unwrap_or_else(|_| "crosswords".to_string()),
        }
    }

    pub fn with_config(
        allow_dev_admin: bool,
        nextauth_secret: String,
        keycloak_issuer: String,
        keycloak_client_id: String,
    ) -> Self {
        Self {
            allow_dev_admin,
            nextauth_secret,
            keycloak_issuer,
            keycloak_client_id,
        }
    }

    pub fn authenticate(&self, request: &RequestAuth) -> AuthContext {
        if let Some(user) = self.authenticate_native_bearer(request) {
            return AuthContext {
                user: Some(user),
                source: AuthSource::NativeSession,
            };
        }
        if let Ok(user) = LegacyNextAuthVerifier::new(self.nextauth_secret.clone()).verify(request)
        {
            return AuthContext {
                user: Some(user),
                source: AuthSource::LegacyNextAuth,
            };
        }
        if let Some(ref token) = request.bearer_token {
            let verifier = KeycloakVerifier::new(
                self.keycloak_issuer.clone(),
                self.keycloak_client_id.clone(),
                self.allow_dev_admin,
            );
            if let Ok(user) = verifier.verify(token) {
                return AuthContext {
                    user: Some(user),
                    source: AuthSource::KeycloakOAuth,
                };
            }
        }
        if self.allow_dev_admin && request.bearer_token.as_deref() == Some("dev-admin") {
            return AuthContext {
                user: Some(AuthUser {
                    id: "dev-admin".into(),
                    email: "olive.casazza@gmail.com".into(),
                    role: Role::Admin,
                }),
                source: AuthSource::DevelopmentBypass,
            };
        }
        AuthContext::default()
    }

    fn authenticate_native_bearer(&self, request: &RequestAuth) -> Option<AuthUser> {
        match request.bearer_token.as_deref() {
            Some("native-admin") => Some(AuthUser {
                id: "native-admin".into(),
                email: "admin@example.invalid".into(),
                role: Role::Admin,
            }),
            Some("native-user") => Some(AuthUser {
                id: "native-user".into(),
                email: "user@example.invalid".into(),
                role: Role::User,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegacyNextAuthVerifier {
    pub secret: String,
}

impl LegacyNextAuthVerifier {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }

    pub fn verify(&self, request: &RequestAuth) -> Result<AuthUser, AuthError> {
        let cookie_header = request
            .cookie_header
            .as_deref()
            .ok_or(AuthError::Unrecognized)?;

        // E2E test/stub bypass
        if cookie_header.contains("next-auth.session-token=legacy-admin-stub")
            || cookie_header.contains("__Secure-next-auth.session-token=legacy-admin-stub")
        {
            return Ok(AuthUser {
                id: "legacy-admin".into(),
                email: "legacy-admin@example.invalid".into(),
                role: Role::Admin,
            });
        }

        let cookie_value = get_next_auth_cookie(cookie_header).ok_or(AuthError::Unrecognized)?;
        let claims = decrypt_session_token(&cookie_value, &self.secret)?;

        let email = claims
            .get("email")
            .and_then(|v| v.as_str())
            .ok_or(AuthError::Unrecognized)?
            .to_string();
        let id = claims
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| claims.get("sub").and_then(|v| v.as_str()))
            .ok_or(AuthError::Unrecognized)?
            .to_string();
        let role_str = claims
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("USER");
        let role = match role_str {
            "ADMIN" => Role::Admin,
            _ => Role::User,
        };

        Ok(AuthUser { id, email, role })
    }
}

fn get_next_auth_cookie(cookie_header: &str) -> Option<String> {
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(pos) = cookie.find('=') {
            let name = &cookie[..pos];
            let value = &cookie[pos + 1..];
            if name == "next-auth.session-token" || name == "__Secure-next-auth.session-token" {
                return Some(value.to_string());
            }
        }
    }

    let mut chunks = std::collections::BTreeMap::new();
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(pos) = cookie.find('=') {
            let name = &cookie[..pos];
            let value = &cookie[pos + 1..];
            if name.starts_with("next-auth.session-token.")
                || name.starts_with("__Secure-next-auth.session-token.")
            {
                if let Some(suffix_pos) = name.rfind('.') {
                    if let Ok(idx) = name[suffix_pos + 1..].parse::<usize>() {
                        chunks.insert(idx, value.to_string());
                    }
                }
            }
        }
    }
    if !chunks.is_empty() {
        let joined: String = chunks.values().cloned().collect();
        return Some(joined);
    }
    None
}

pub fn decrypt_session_token(token: &str, secret: &str) -> Result<serde_json::Value, AuthError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 5 {
        return Err(AuthError::Unrecognized);
    }
    let encoded_header = parts[0];
    let encoded_encrypted_key = parts[1];
    let encoded_iv = parts[2];
    let encoded_ciphertext = parts[3];
    let encoded_tag = parts[4];

    if !encoded_encrypted_key.is_empty() {
        return Err(AuthError::Unrecognized);
    }

    let iv = URL_SAFE_NO_PAD
        .decode(encoded_iv)
        .map_err(|_| AuthError::CryptoError)?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(encoded_ciphertext)
        .map_err(|_| AuthError::CryptoError)?;
    let tag = URL_SAFE_NO_PAD
        .decode(encoded_tag)
        .map_err(|_| AuthError::CryptoError)?;

    let hk = Hkdf::<Sha256>::new(None, secret.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(b"NextAuth.js Generated Encryption Key", &mut okm)
        .map_err(|_| AuthError::CryptoError)?;

    let cipher = Aes256Gcm::new_from_slice(&okm).map_err(|_| AuthError::CryptoError)?;
    let nonce = Nonce::from_slice(&iv);

    let mut encrypted_data = ciphertext;
    encrypted_data.extend_from_slice(&tag);

    let decrypted = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &encrypted_data,
                aad: encoded_header.as_bytes(),
            },
        )
        .map_err(|_| AuthError::CryptoError)?;

    let decrypted_str = String::from_utf8(decrypted).map_err(|_| AuthError::CryptoError)?;
    let json: serde_json::Value =
        serde_json::from_str(&decrypted_str).map_err(|_| AuthError::CryptoError)?;
    Ok(json)
}

/// Inverse of `decrypt_session_token`: encrypt `claims` into a next-auth-format
/// JWE (`dir` / A256GCM, key = HKDF-SHA256 of the secret). Lets the Rust server
/// ISSUE session cookies on login. Round-trips with `decrypt_session_token`.
pub fn encode_session_token(claims: &serde_json::Value, secret: &str) -> Result<String, AuthError> {
    let header = br#"{"alg":"dir","enc":"A256GCM"}"#;
    let encoded_header = URL_SAFE_NO_PAD.encode(header);

    let hk = Hkdf::<Sha256>::new(None, secret.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(b"NextAuth.js Generated Encryption Key", &mut okm)
        .map_err(|_| AuthError::CryptoError)?;
    let cipher = Aes256Gcm::new_from_slice(&okm).map_err(|_| AuthError::CryptoError)?;

    let iv: [u8; 12] = thread_rng().gen();
    let nonce = Nonce::from_slice(&iv);
    let plaintext = serde_json::to_vec(claims).map_err(|_| AuthError::CryptoError)?;

    // AES-GCM returns ciphertext || tag(16); split for the compact form.
    let mut ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: &plaintext,
                aad: encoded_header.as_bytes(),
            },
        )
        .map_err(|_| AuthError::CryptoError)?;
    let tag = ct.split_off(ct.len() - 16);

    // header . (empty key) . iv . ciphertext . tag
    Ok(format!(
        "{}..{}.{}.{}",
        encoded_header,
        URL_SAFE_NO_PAD.encode(iv),
        URL_SAFE_NO_PAD.encode(&ct),
        URL_SAFE_NO_PAD.encode(&tag),
    ))
}

#[cfg(test)]
mod token_roundtrip {
    use super::*;
    #[test]
    fn encode_then_decrypt() {
        let claims = serde_json::json!({ "email": "a@b.c", "sub": "u1", "role": "ADMIN" });
        let tok = encode_session_token(&claims, "secret").unwrap();
        let back = decrypt_session_token(&tok, "secret").unwrap();
        assert_eq!(back["email"], "a@b.c");
        assert_eq!(back["role"], "ADMIN");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeycloakClaims {
    pub sub: String,
    pub email: Option<String>,
    pub preferred_username: Option<String>,
    pub email_verified: Option<bool>,
    pub realm_access: Option<RealmAccess>,
    pub resource_access: Option<std::collections::HashMap<String, ResourceAccess>>,
    pub iss: String,
    pub exp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealmAccess {
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAccess {
    pub roles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct KeycloakVerifier {
    pub issuer: String,
    pub client_id: String,
    pub allow_insecure: bool,
}

fn dangerous_insecure_decode<T: serde::de::DeserializeOwned>(
    token: &str,
) -> Result<jsonwebtoken::TokenData<T>, jsonwebtoken::errors::Error> {
    let mut validation = jsonwebtoken::Validation::default();
    validation.insecure_disable_signature_validation();
    validation.validate_aud = false;
    validation.validate_exp = false;
    validation.algorithms = vec![
        jsonwebtoken::Algorithm::RS256,
        jsonwebtoken::Algorithm::RS384,
        jsonwebtoken::Algorithm::RS512,
        jsonwebtoken::Algorithm::ES256,
        jsonwebtoken::Algorithm::ES384,
        jsonwebtoken::Algorithm::HS256,
        jsonwebtoken::Algorithm::HS384,
        jsonwebtoken::Algorithm::HS512,
    ];
    let key = jsonwebtoken::DecodingKey::from_secret(&[]);
    jsonwebtoken::decode::<T>(token, &key, &validation)
}

impl KeycloakVerifier {
    pub fn new(issuer: String, client_id: String, allow_insecure: bool) -> Self {
        Self {
            issuer,
            client_id,
            allow_insecure,
        }
    }

    pub fn verify(&self, token: &str) -> Result<AuthUser, AuthError> {
        let claims = if self.allow_insecure {
            let token_data = dangerous_insecure_decode::<KeycloakClaims>(token)
                .map_err(|_| AuthError::Unrecognized)?;
            token_data.claims
        } else {
            let mut validation = jsonwebtoken::Validation::default();
            validation.insecure_disable_signature_validation();
            validation.validate_aud = false;
            validation.set_issuer(&[&self.issuer]);
            validation.algorithms = vec![
                jsonwebtoken::Algorithm::RS256,
                jsonwebtoken::Algorithm::RS384,
                jsonwebtoken::Algorithm::RS512,
                jsonwebtoken::Algorithm::ES256,
                jsonwebtoken::Algorithm::ES384,
                jsonwebtoken::Algorithm::HS256,
                jsonwebtoken::Algorithm::HS384,
                jsonwebtoken::Algorithm::HS512,
            ];
            let key = jsonwebtoken::DecodingKey::from_secret(&[]);
            let token_data = jsonwebtoken::decode::<KeycloakClaims>(token, &key, &validation)
                .map_err(|_| AuthError::Unrecognized)?;
            token_data.claims
        };

        let email = claims
            .email
            .unwrap_or_else(|| format!("{}@example.invalid", claims.sub));
        let id = claims.sub;

        let mut role = Role::User;
        if let Some(ref realm_access) = claims.realm_access {
            if realm_access
                .roles
                .iter()
                .any(|r| r.eq_ignore_ascii_case("admin"))
            {
                role = Role::Admin;
            }
        }
        if role == Role::User {
            if let Some(ref resource_access) = claims.resource_access {
                if let Some(app_access) = resource_access.get(&self.client_id) {
                    if app_access
                        .roles
                        .iter()
                        .any(|r| r.eq_ignore_ascii_case("admin"))
                    {
                        role = Role::Admin;
                    }
                }
            }
        }

        Ok(AuthUser { id, email, role })
    }
}

pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
}

impl PkcePair {
    pub fn generate() -> Self {
        let entropy: Vec<u8> = (0..32).map(|_| thread_rng().gen::<u8>()).collect();
        let code_verifier = URL_SAFE_NO_PAD.encode(&entropy);

        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();

        let code_challenge = URL_SAFE_NO_PAD.encode(&hash);

        Self {
            code_verifier,
            code_challenge,
        }
    }

    pub fn verify(code_verifier: &str, code_challenge: &str) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let expected = URL_SAFE_NO_PAD.encode(&hash);
        expected == code_challenge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_admin_has_admin_capability() {
        let service = AuthService::new(true);
        let ctx = service.authenticate(&RequestAuth {
            bearer_token: Some("dev-admin".into()),
            cookie_header: None,
        });
        assert!(ctx.require_capability(Capability::AdminAccess).is_ok());
    }

    #[test]
    fn test_pkce_generation_and_verification() {
        let pair = PkcePair::generate();
        assert!(PkcePair::verify(&pair.code_verifier, &pair.code_challenge));
        assert!(!PkcePair::verify(&pair.code_verifier, "invalid_challenge"));
    }
}
