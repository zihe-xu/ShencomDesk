use std::time::Duration;

use async_trait::async_trait;
use keyring::Entry;
use reqwest::{header::ACCEPT, Client, Request};
use serde::{Deserialize, Serialize};

use crate::{
    application::auth_service::{
        AuthBackend, AuthBackendResponse, AuthServiceError, AuthSessionStore,
    },
    domain::auth::{AccessToken, LoginRequest, LoginResponse},
};

const LOGIN_URL: &str = "https://tst-crm.shencom.cn/service-uaa/user/login";
const SCID: &str = "sca15516911b95f35b";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
const KEYRING_SERVICE: &str = "com.shencom.shendesk.auth";
const KEYRING_ACCOUNT: &str = "test-session-v1";
const AUTH_SESSION_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct StoredAuthSession {
    version: u8,
    token: AccessToken,
}

#[derive(Debug, Clone)]
pub struct ShencomAuthBackend {
    client: Client,
}

impl ShencomAuthBackend {
    pub fn test_environment() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn build_login_request(&self, request: &LoginRequest) -> Result<Request, AuthServiceError> {
        self.client
            .post(LOGIN_URL)
            .header("scid", SCID)
            .header(ACCEPT, "*/*")
            .timeout(LOGIN_TIMEOUT)
            .json(request)
            .build()
            .map_err(|error| {
                AuthServiceError::unavailable(format!("failed to build login request: {error}"))
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
    pub fn new() -> Result<Self, AuthServiceError> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|error| {
            AuthServiceError::unavailable(format!(
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
                return Err(AuthServiceError::unavailable(format!(
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
            AuthServiceError::unavailable(format!(
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
            Err(error) => Err(AuthServiceError::unavailable(format!(
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
        AuthServiceError::unavailable(format!(
            "authentication session could not be encoded: {error}"
        ))
    })
}

fn decode_stored_session(serialized: &str) -> Result<AccessToken, AuthServiceError> {
    let stored: StoredAuthSession = serde_json::from_str(serialized).map_err(|error| {
        AuthServiceError::unavailable(format!(
            "stored authentication session could not be decoded: {error}"
        ))
    })?;
    if stored.version != AUTH_SESSION_VERSION {
        return Err(AuthServiceError::unavailable(format!(
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
    fn builds_the_confirmed_test_environment_request() {
        let backend = ShencomAuthBackend::test_environment();
        let request = backend
            .build_login_request(&LoginRequest {
                username: "13800000000".to_owned(),
                password: "password".to_owned(),
            })
            .expect("login request should build");

        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.url().as_str(), LOGIN_URL);
        assert_eq!(
            request.headers()["scid"]
                .to_str()
                .expect("scid header should be text"),
            SCID
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
