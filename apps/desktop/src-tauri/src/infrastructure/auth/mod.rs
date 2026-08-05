use std::time::Duration;

use async_trait::async_trait;
use keyring::Entry;
use reqwest::{header::ACCEPT, Client, Request};
use serde::{Deserialize, Serialize};

use crate::{
    application::auth_service::{
        AuthBackend, AuthBackendResponse, AuthServiceError, AuthSessionStore,
        RefreshBackendResponse,
    },
    domain::auth::{
        AccessToken, LoginRequest, LoginResponse, RefreshTokenRequest, RefreshTokenResponse,
    },
};

const AUTH_ENVIRONMENT_VARIABLE: &str = "SHENDESK_AUTH_ENVIRONMENT";
const AUTH_HOST_VARIABLE: &str = "SHENDESK_AUTH_HOST";
const AUTH_SCID_VARIABLE: &str = "SHENDESK_AUTH_SCID";
const LOGIN_PATH: &str = "/service-uaa/user/login";
const REFRESH_PATH: &str = "/service-uaa/auth/token-user/refresh";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
const KEYRING_SERVICE: &str = "com.shencom.shendesk.auth";
const AUTH_SESSION_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthEnvironment {
    name: String,
    base_url: String,
    scid: String,
}

impl AuthEnvironment {
    pub fn from_process_environment() -> Result<Self, AuthServiceError> {
        Self::new(
            environment_value(
                AUTH_ENVIRONMENT_VARIABLE,
                option_env!("SHENDESK_EMBEDDED_AUTH_ENVIRONMENT"),
            )?,
            environment_value(
                AUTH_HOST_VARIABLE,
                option_env!("SHENDESK_EMBEDDED_AUTH_HOST"),
            )?,
            environment_value(
                AUTH_SCID_VARIABLE,
                option_env!("SHENDESK_EMBEDDED_AUTH_SCID"),
            )?,
        )
    }

    fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        scid: impl Into<String>,
    ) -> Result<Self, AuthServiceError> {
        let name = name.into().trim().to_ascii_lowercase();
        if !matches!(name.as_str(), "development" | "production" | "test") {
            return Err(AuthServiceError::unavailable(format!(
                "unsupported {AUTH_ENVIRONMENT_VARIABLE} value: {name}"
            )));
        }

        let base_url = base_url.into().trim().trim_end_matches('/').to_owned();
        let parsed_url = reqwest::Url::parse(&base_url).map_err(|error| {
            AuthServiceError::unavailable(format!("{AUTH_HOST_VARIABLE} is invalid: {error}"))
        })?;
        if parsed_url.scheme() != "https" || parsed_url.host_str().is_none() {
            return Err(AuthServiceError::unavailable(format!(
                "{AUTH_HOST_VARIABLE} must be an HTTPS origin"
            )));
        }

        let scid = scid.into().trim().to_owned();
        if scid.is_empty() {
            return Err(AuthServiceError::unavailable(format!(
                "{AUTH_SCID_VARIABLE} must not be empty"
            )));
        }

        Ok(Self {
            name,
            base_url,
            scid,
        })
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn scid(&self) -> &str {
        &self.scid
    }

    fn keyring_account(&self) -> String {
        format!("{}-session-v1", self.name)
    }
}

fn environment_value(
    name: &str,
    embedded_value: Option<&'static str>,
) -> Result<String, AuthServiceError> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(std::env::VarError::NotPresent) => embedded_value
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| AuthServiceError::unavailable(format!("{name} is not configured"))),
        Err(error) => Err(AuthServiceError::unavailable(format!(
            "{name} could not be read: {error}"
        ))),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredAuthSession {
    version: u8,
    token: AccessToken,
}

#[derive(Debug, Clone)]
pub struct ShencomAuthBackend {
    client: Client,
    environment: AuthEnvironment,
}

impl ShencomAuthBackend {
    pub fn new(environment: AuthEnvironment) -> Self {
        Self {
            client: Client::new(),
            environment,
        }
    }

    fn build_login_request(&self, request: &LoginRequest) -> Result<Request, AuthServiceError> {
        self.client
            .post(format!("{}{LOGIN_PATH}", self.environment.base_url()))
            .header("scid", self.environment.scid())
            .header(ACCEPT, "*/*")
            .timeout(LOGIN_TIMEOUT)
            .json(request)
            .build()
            .map_err(|error| {
                AuthServiceError::unavailable(format!("failed to build login request: {error}"))
            })
    }

    fn build_refresh_request(&self, refresh_token: &str) -> Result<Request, AuthServiceError> {
        self.client
            .post(format!("{}{REFRESH_PATH}", self.environment.base_url()))
            .header("scid", self.environment.scid())
            .header(ACCEPT, "*/*")
            .timeout(LOGIN_TIMEOUT)
            .json(&RefreshTokenRequest {
                refresh_token: refresh_token.to_owned(),
            })
            .build()
            .map_err(|error| {
                AuthServiceError::unavailable(format!("failed to build refresh request: {error}"))
            })
    }
}

#[async_trait]
impl AuthBackend for ShencomAuthBackend {
    async fn login(&self, request: &LoginRequest) -> Result<AuthBackendResponse, AuthServiceError> {
        let response = self
            .client
            .execute(self.build_login_request(request)?)
            .await
            .map_err(|error| {
                AuthServiceError::unavailable(format!("login request failed: {error}"))
            })?;
        let http_status = response.status().as_u16();
        let payload = response.json::<LoginResponse>().await.map_err(|error| {
            AuthServiceError::unavailable(format!("login response could not be decoded: {error}"))
        })?;

        Ok(AuthBackendResponse {
            http_status,
            payload,
        })
    }

    async fn refresh(
        &self,
        refresh_token: &str,
    ) -> Result<RefreshBackendResponse, AuthServiceError> {
        let response = self
            .client
            .execute(self.build_refresh_request(refresh_token)?)
            .await
            .map_err(|error| {
                AuthServiceError::unavailable(format!("refresh request failed: {error}"))
            })?;
        let http_status = response.status().as_u16();
        let payload = response
            .json::<RefreshTokenResponse>()
            .await
            .map_err(|error| {
                AuthServiceError::unavailable(format!(
                    "refresh response could not be decoded: {error}"
                ))
            })?;

        Ok(RefreshBackendResponse {
            http_status,
            payload,
        })
    }
}

pub struct KeyringAuthSessionStore {
    entry: Option<Entry>,
}

impl std::fmt::Debug for KeyringAuthSessionStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeyringAuthSessionStore")
            .finish_non_exhaustive()
    }
}

impl KeyringAuthSessionStore {
    pub fn new(environment: &AuthEnvironment) -> Result<Self, AuthServiceError> {
        let entry =
            Entry::new(KEYRING_SERVICE, &environment.keyring_account()).map_err(|error| {
                AuthServiceError::storage(format!(
                    "system credential store could not be initialized: {error}"
                ))
            })?;

        Ok(Self { entry: Some(entry) })
    }

    pub fn disabled() -> Self {
        Self { entry: None }
    }
}

impl AuthSessionStore for KeyringAuthSessionStore {
    fn load(&self) -> Result<Option<AccessToken>, AuthServiceError> {
        let Some(entry) = &self.entry else {
            return Ok(None);
        };
        let serialized = match entry.get_password() {
            Ok(serialized) => serialized,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(error) => {
                return Err(AuthServiceError::storage(format!(
                    "stored authentication session could not be read: {error}"
                )))
            }
        };

        match decode_stored_session(&serialized) {
            Ok(token) => Ok(Some(token)),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "stored authentication session is invalid and will be removed"
                );
                if let Err(clear_error) = self.clear() {
                    tracing::warn!(
                        error = %clear_error,
                        "invalid authentication session could not be removed"
                    );
                }
                Ok(None)
            }
        }
    }

    fn save(&self, token: &AccessToken) -> Result<(), AuthServiceError> {
        let Some(entry) = &self.entry else {
            return Ok(());
        };
        let serialized = encode_stored_session(token)?;
        entry.set_password(&serialized).map_err(|error| {
            AuthServiceError::storage(format!(
                "authentication session could not be stored: {error}"
            ))
        })
    }

    fn clear(&self) -> Result<(), AuthServiceError> {
        let Some(entry) = &self.entry else {
            return Ok(());
        };
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AuthServiceError::storage(format!(
                "stored authentication session could not be removed: {error}"
            ))),
        }
    }
}

fn encode_stored_session(token: &AccessToken) -> Result<String, AuthServiceError> {
    serde_json::to_string(&StoredAuthSession {
        version: AUTH_SESSION_VERSION,
        token: token.clone(),
    })
    .map_err(|error| {
        AuthServiceError::storage(format!(
            "authentication session could not be encoded: {error}"
        ))
    })
}

fn decode_stored_session(serialized: &str) -> Result<AccessToken, AuthServiceError> {
    let stored: StoredAuthSession = serde_json::from_str(serialized).map_err(|error| {
        AuthServiceError::storage(format!(
            "stored authentication session could not be decoded: {error}"
        ))
    })?;
    if stored.version != AUTH_SESSION_VERSION {
        return Err(AuthServiceError::storage(format!(
            "stored authentication session version {} is not supported",
            stored.version
        )));
    }

    Ok(stored.token)
}

#[cfg(test)]
mod tests {
    use reqwest::{header::CONTENT_TYPE, Method};
    use serde_json::Value;

    use super::*;

    fn access_token() -> AccessToken {
        serde_json::from_value(serde_json::json!({
            "additionalInformation": {
                "realname": "测试用户",
                "phone": "13800000000",
                "username": "13800000000",
                "uid": "user-id"
            },
            "expiration": 4_102_444_800_i64,
            "expiresIn": 3_600,
            "refreshToken": { "value": "refresh-token" },
            "scope": ["all"],
            "tokenType": "bearer",
            "value": "access-token"
        }))
        .expect("access token should deserialize")
    }

    #[test]
    fn builds_login_requests_from_environment_configuration() {
        for (name, base_url, scid) in [
            (
                "development",
                "https://development.example.com",
                "development-scid",
            ),
            (
                "production",
                "https://production.example.com",
                "production-scid",
            ),
            ("test", "https://test.example.com", "test-scid"),
        ] {
            let environment =
                AuthEnvironment::new(name, base_url, scid).expect("environment should be valid");
            let request = ShencomAuthBackend::new(environment)
                .build_login_request(&LoginRequest {
                    username: "13800000000".to_owned(),
                    password: "password".to_owned(),
                })
                .expect("login request should build");

            assert_eq!(request.method(), Method::POST);
            assert_eq!(request.url().as_str(), format!("{base_url}{LOGIN_PATH}"));
            assert_eq!(
                request.headers()["scid"]
                    .to_str()
                    .expect("scid header should be text"),
                scid
            );
            assert_eq!(
                request.headers()[ACCEPT]
                    .to_str()
                    .expect("accept header should be text"),
                "*/*"
            );
            assert_eq!(
                request.headers()[CONTENT_TYPE]
                    .to_str()
                    .expect("content type header should be text"),
                "application/json"
            );
            assert_eq!(request.timeout(), Some(&LOGIN_TIMEOUT));

            let body: Value = serde_json::from_slice(
                request
                    .body()
                    .and_then(reqwest::Body::as_bytes)
                    .expect("JSON body should be buffered"),
            )
            .expect("request body should contain JSON");
            assert_eq!(body["username"], "13800000000");
            assert_eq!(body["password"], "password");
        }
    }

    #[test]
    fn builds_refresh_requests_from_environment_configuration() {
        let environment =
            AuthEnvironment::new("test", "https://test.example.com", "environment-scid")
                .expect("environment should be valid");
        let request = ShencomAuthBackend::new(environment)
            .build_refresh_request("refresh-token")
            .expect("refresh request should build");
        let body: Value = serde_json::from_slice(
            request
                .body()
                .and_then(reqwest::Body::as_bytes)
                .expect("JSON body should be buffered"),
        )
        .expect("request body should contain JSON");

        assert_eq!(request.method(), Method::POST);
        assert_eq!(
            request.url().as_str(),
            "https://test.example.com/service-uaa/auth/token-user/refresh"
        );
        assert_eq!(request.headers()["scid"], "environment-scid");
        assert_eq!(body["refreshToken"], "refresh-token");
        assert_eq!(request.timeout(), Some(&LOGIN_TIMEOUT));
    }

    #[test]
    fn validates_authentication_environment_values() {
        let environment = AuthEnvironment::new(
            " PRODUCTION ",
            "https://crm.example.com/",
            " production-scid ",
        )
        .expect("production should parse");

        assert_eq!(environment.name, "production");
        assert_eq!(environment.base_url(), "https://crm.example.com");
        assert_eq!(environment.scid(), "production-scid");
        assert!(AuthEnvironment::new("staging", "https://example.com", "scid").is_err());
        assert!(AuthEnvironment::new("test", "http://example.com", "scid").is_err());
        assert!(AuthEnvironment::new("test", "https://example.com", " ").is_err());
    }

    #[test]
    fn encodes_and_decodes_the_versioned_authentication_session() {
        let serialized = encode_stored_session(&access_token()).expect("session should serialize");
        let stored: Value =
            serde_json::from_str(&serialized).expect("stored session should be JSON");
        let decoded = decode_stored_session(&serialized).expect("version one should decode");

        assert_eq!(stored["version"], AUTH_SESSION_VERSION);
        assert_eq!(decoded.value, "access-token");
    }

    #[test]
    fn rejects_corrupt_and_unknown_authentication_session_versions() {
        assert!(decode_stored_session("{not-json").is_err());

        let serialized = serde_json::json!({
            "version": AUTH_SESSION_VERSION + 1,
            "token": access_token()
        })
        .to_string();
        assert!(decode_stored_session(&serialized).is_err());
    }
}
