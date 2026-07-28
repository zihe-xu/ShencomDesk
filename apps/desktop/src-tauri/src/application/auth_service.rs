use std::{error::Error, fmt, sync::Arc};

use async_trait::async_trait;

use crate::domain::auth::{LoginRequest, LoginResponse};

const SUCCESS_CODE: &str = "0000";
const SUCCESS_HTTP_STATUS: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthServiceErrorKind {
    Validation,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct AuthServiceError {
    kind: AuthServiceErrorKind,
    message: String,
}

impl AuthServiceError {
    pub fn new(kind: AuthServiceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(AuthServiceErrorKind::Unavailable, message)
    }

    pub fn kind(&self) -> AuthServiceErrorKind {
        self.kind
    }
}

impl fmt::Display for AuthServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AuthServiceError {}

#[derive(Debug, Clone)]
pub struct AuthBackendResponse {
    pub http_status: u16,
    pub payload: LoginResponse,
}

#[async_trait]
pub trait AuthBackend: Send + Sync {
    async fn login(
        &self,
        request: &LoginRequest,
    ) -> Result<AuthBackendResponse, AuthServiceError>;
}

#[derive(Clone)]
pub struct AuthService {
    backend: Arc<dyn AuthBackend>,
}

impl fmt::Debug for AuthService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthService")
            .finish_non_exhaustive()
    }
}

impl AuthService {
    pub fn new(backend: Arc<dyn AuthBackend>) -> Self {
        Self { backend }
    }

    pub async fn login(
        &self,
        mut request: LoginRequest,
    ) -> Result<LoginResponse, AuthServiceError> {
        request.username = request.username.trim().to_owned();
        if request.username.is_empty() || request.password.is_empty() {
            return Err(AuthServiceError::new(
                AuthServiceErrorKind::Validation,
                "login credentials are required",
            ));
        }

        let response = self.backend.login(&request).await?;
        if response.http_status != SUCCESS_HTTP_STATUS {
            return Err(AuthServiceError::unavailable(format!(
                "login endpoint returned HTTP {}",
                response.http_status
            )));
        }

        if response.payload.errcode != SUCCESS_CODE {
            let message = if response.payload.errmsg.trim().is_empty() {
                "手机号或密码不正确。".to_owned()
            } else {
                response.payload.errmsg.clone()
            };
            return Err(AuthServiceError::new(
                AuthServiceErrorKind::Rejected,
                message,
            ));
        }

        if response.payload.data.is_none() {
            return Err(AuthServiceError::unavailable(
                "successful login response did not contain data",
            ));
        }

        Ok(response.payload)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug)]
    struct RecordingBackend {
        response: AuthBackendResponse,
        requests: Mutex<Vec<LoginRequest>>,
    }

    #[async_trait]
    impl AuthBackend for RecordingBackend {
        async fn login(
            &self,
            request: &LoginRequest,
        ) -> Result<AuthBackendResponse, AuthServiceError> {
            self.requests
                .lock()
                .expect("requests lock")
                .push(request.clone());
            Ok(self.response.clone())
        }
    }

    fn response(
        http_status: u16,
        errcode: &str,
        errmsg: &str,
        has_data: bool,
    ) -> AuthBackendResponse {
        let data = has_data
            .then(|| {
                serde_json::from_value(serde_json::json!({
                    "additionalInformation": {
                        "additionalInformation": {
                            "realname": "测试用户",
                            "phone": "13800000000",
                            "username": "13800000000",
                            "uid": "user-id"
                        },
                        "expiration": 1_800_000_000,
                        "expiresIn": 3_600,
                        "refreshToken": { "value": "refresh-token" },
                        "scope": ["all"],
                        "tokenType": "bearer",
                        "value": "access-token"
                    }
                }))
                .expect("login data should deserialize")
            });

        AuthBackendResponse {
            http_status,
            payload: LoginResponse {
                data,
                errcode: errcode.to_owned(),
                errmsg: errmsg.to_owned(),
            },
        }
    }

    fn service(response: AuthBackendResponse) -> (AuthService, Arc<RecordingBackend>) {
        let backend = Arc::new(RecordingBackend {
            response,
            requests: Mutex::new(Vec::new()),
        });
        (AuthService::new(backend.clone()), backend)
    }

    fn request() -> LoginRequest {
        LoginRequest {
            username: " 13800000000 ".to_owned(),
            password: "password".to_owned(),
        }
    }

    #[test]
    fn accepts_the_confirmed_success_code_and_normalizes_the_phone() {
        tauri::async_runtime::block_on(async {
            let (service, backend) = service(response(200, SUCCESS_CODE, "", true));
            let payload = service
                .login(request())
                .await
                .expect("confirmed response should succeed");

            assert!(payload.data.is_some());
            assert_eq!(
                backend.requests.lock().expect("requests lock")[0].username,
                "13800000000"
            );
        });
    }

    #[test]
    fn rejects_invalid_input_before_calling_the_backend() {
        tauri::async_runtime::block_on(async {
            let (service, backend) = service(response(200, SUCCESS_CODE, "", true));
            let error = service
                .login(LoginRequest {
                    username: " ".to_owned(),
                    password: String::new(),
                })
                .await
                .expect_err("empty credentials should fail");

            assert_eq!(error.kind(), AuthServiceErrorKind::Validation);
            assert!(backend.requests.lock().expect("requests lock").is_empty());
        });
    }

    #[test]
    fn preserves_user_facing_business_errors() {
        tauri::async_runtime::block_on(async {
            let (service, _) = service(response(200, "601002", "账号或密码有误", false));
            let error = service
                .login(request())
                .await
                .expect_err("business rejection should fail");

            assert_eq!(error.kind(), AuthServiceErrorKind::Rejected);
            assert_eq!(error.to_string(), "账号或密码有误");
        });
    }

    #[test]
    fn treats_http_failures_and_missing_success_data_as_unavailable() {
        tauri::async_runtime::block_on(async {
            for backend_response in [
                response(503, "500000", "internal database detail", false),
                response(200, SUCCESS_CODE, "", false),
            ] {
                let (service, _) = service(backend_response);
                let error = service
                    .login(request())
                    .await
                    .expect_err("unavailable response should fail");

                assert_eq!(error.kind(), AuthServiceErrorKind::Unavailable);
            }
        });
    }
}
