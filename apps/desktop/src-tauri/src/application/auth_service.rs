use std::{
    error::Error,
    fmt,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;

use crate::domain::{
    auth::{AccessToken, AuthState, LoginRequest, LoginResponse},
    event::AppEvent,
};

use super::event_bus::EventBus;

const SUCCESS_CODE: &str = "0000";
const SUCCESS_HTTP_STATUS: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthServiceErrorKind {
    Validation,
    Rejected,
    Unavailable,
    Storage,
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

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(AuthServiceErrorKind::Storage, message)
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
    async fn login(&self, request: &LoginRequest) -> Result<AuthBackendResponse, AuthServiceError>;
}

pub trait AuthSessionStore: Send + Sync {
    fn load(&self) -> Result<Option<AccessToken>, AuthServiceError>;
    fn save(&self, token: &AccessToken) -> Result<(), AuthServiceError>;
    fn clear(&self) -> Result<(), AuthServiceError>;
}

#[derive(Clone)]
pub struct AuthService {
    backend: Arc<dyn AuthBackend>,
    session_store: Arc<dyn AuthSessionStore>,
    session: Arc<RwLock<Option<AccessToken>>>,
    events: EventBus,
}

impl fmt::Debug for AuthService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthService")
            .finish_non_exhaustive()
    }
}

impl AuthService {
    pub fn new(
        backend: Arc<dyn AuthBackend>,
        session_store: Arc<dyn AuthSessionStore>,
        events: EventBus,
    ) -> Result<Self, AuthServiceError> {
        let session = match session_store.load() {
            Ok(session) => session,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "stored authentication session could not be restored; starting signed out"
                );
                None
            }
        };
        let service = Self {
            backend,
            session_store,
            session: Arc::new(RwLock::new(session)),
            events,
        };

        if service.session_is_expired() {
            if let Err(error) = service.session_store.clear() {
                tracing::warn!(
                    error = %error,
                    "expired authentication session could not be removed"
                );
            }
            *service.write_session() = None;
        }

        Ok(service)
    }

    pub async fn login(&self, mut request: LoginRequest) -> Result<AuthState, AuthServiceError> {
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

        let token = response
            .payload
            .data
            .map(|data| data.additional_information)
            .ok_or_else(|| {
                AuthServiceError::unavailable(
                    "successful login response did not contain authentication data",
                )
            })?;
        self.session_store.save(&token)?;
        *self.write_session() = Some(token.clone());
        self.events.publish(AppEvent::UserLoggedIn {
            user_id: token.additional_information.uid.clone(),
        });

        Ok(AuthState::from_token(&token))
    }

    pub fn state(&self) -> Result<AuthState, AuthServiceError> {
        if self.session_is_expired() {
            if let Err(error) = self.session_store.clear() {
                tracing::warn!(
                    error = %error,
                    "expired authentication session could not be removed"
                );
            }
            *self.write_session() = None;
        }

        Ok(self
            .read_session()
            .as_ref()
            .map(AuthState::from_token)
            .unwrap_or_else(AuthState::signed_out))
    }

    pub fn logout(&self) -> Result<AuthState, AuthServiceError> {
        self.session_store.clear()?;
        let session = self.write_session().take();
        if let Some(token) = session {
            self.events.publish(AppEvent::UserLoggedOut {
                user_id: token.additional_information.uid,
            });
        }

        Ok(AuthState::signed_out())
    }

    fn session_is_expired(&self) -> bool {
        self.read_session()
            .as_ref()
            .is_some_and(|token| token.expiration <= unix_time_seconds())
    }

    fn read_session(&self) -> std::sync::RwLockReadGuard<'_, Option<AccessToken>> {
        self.session
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_session(&self) -> std::sync::RwLockWriteGuard<'_, Option<AccessToken>> {
        self.session
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn unix_time_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::domain::event::EventKind;

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

    #[derive(Debug, Default)]
    struct RecordingStore {
        session: Mutex<Option<AccessToken>>,
        clear_count: Mutex<u32>,
        load_error: bool,
        save_error: bool,
        clear_error: bool,
    }

    impl AuthSessionStore for RecordingStore {
        fn load(&self) -> Result<Option<AccessToken>, AuthServiceError> {
            if self.load_error {
                return Err(AuthServiceError::storage("session store unavailable"));
            }

            Ok(self.session.lock().expect("session lock").clone())
        }

        fn save(&self, token: &AccessToken) -> Result<(), AuthServiceError> {
            if self.save_error {
                return Err(AuthServiceError::storage("session store unavailable"));
            }

            *self.session.lock().expect("session lock") = Some(token.clone());
            Ok(())
        }

        fn clear(&self) -> Result<(), AuthServiceError> {
            *self.clear_count.lock().expect("clear count lock") += 1;
            if self.clear_error {
                return Err(AuthServiceError::storage("session store unavailable"));
            }

            *self.session.lock().expect("session lock") = None;
            Ok(())
        }
    }

    fn access_token(expiration: i64) -> AccessToken {
        serde_json::from_value(serde_json::json!({
            "additionalInformation": {
                "realname": "测试用户",
                "phone": "13800000000",
                "username": "13800000000",
                "uid": "user-id"
            },
            "expiration": expiration,
            "expiresIn": 3_600,
            "refreshToken": { "value": "refresh-token" },
            "scope": ["all"],
            "tokenType": "bearer",
            "value": "access-token"
        }))
        .expect("access token should deserialize")
    }

    fn response(
        http_status: u16,
        errcode: &str,
        errmsg: &str,
        has_data: bool,
    ) -> AuthBackendResponse {
        let data = has_data.then(|| {
            serde_json::from_value(serde_json::json!({
                "additionalInformation": access_token(4_102_444_800)
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

    fn service(
        response: AuthBackendResponse,
        initial_session: Option<AccessToken>,
    ) -> (
        AuthService,
        Arc<RecordingBackend>,
        Arc<RecordingStore>,
        EventBus,
    ) {
        let backend = Arc::new(RecordingBackend {
            response,
            requests: Mutex::new(Vec::new()),
        });
        let store = Arc::new(RecordingStore {
            session: Mutex::new(initial_session),
            clear_count: Mutex::new(0),
            load_error: false,
            save_error: false,
            clear_error: false,
        });
        let events = EventBus::new(8);
        let auth_service = AuthService::new(backend.clone(), store.clone(), events.clone())
            .expect("auth service should initialize");

        (auth_service, backend, store, events)
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
            let (service, backend, store, events) =
                service(response(200, SUCCESS_CODE, "", true), None);
            let mut subscriber = events.subscribe_to([EventKind::UserLoggedIn]);
            let state = service
                .login(request())
                .await
                .expect("confirmed response should succeed");

            assert!(state.authenticated);
            assert_eq!(state.user.expect("user").uid, "user-id");
            assert_eq!(
                store
                    .session
                    .lock()
                    .expect("session lock")
                    .as_ref()
                    .expect("stored token")
                    .value,
                "access-token"
            );
            assert_eq!(
                backend.requests.lock().expect("requests lock")[0].username,
                "13800000000"
            );
            let event = subscriber.recv().await.expect("login event");
            assert_eq!(event.event.kind(), EventKind::UserLoggedIn);
        });
    }

    #[test]
    fn rejects_invalid_input_before_calling_the_backend() {
        tauri::async_runtime::block_on(async {
            let (service, backend, _, _) = service(response(200, SUCCESS_CODE, "", true), None);
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
            let (service, _, _, _) =
                service(response(200, "601002", "账号或密码有误", false), None);
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
                let (service, _, _, _) = service(backend_response, None);
                let error = service
                    .login(request())
                    .await
                    .expect_err("unavailable response should fail");

                assert_eq!(error.kind(), AuthServiceErrorKind::Unavailable);
            }
        });
    }

    #[test]
    fn reports_session_storage_failures_separately_from_login_service_failures() {
        tauri::async_runtime::block_on(async {
            let backend = Arc::new(RecordingBackend {
                response: response(200, SUCCESS_CODE, "", true),
                requests: Mutex::new(Vec::new()),
            });
            let store = Arc::new(RecordingStore {
                session: Mutex::new(None),
                clear_count: Mutex::new(0),
                load_error: false,
                save_error: true,
                clear_error: false,
            });
            let service = AuthService::new(backend, store, EventBus::new(8))
                .expect("auth service should initialize");

            let error = service
                .login(request())
                .await
                .expect_err("session storage failure should fail login");

            assert_eq!(error.kind(), AuthServiceErrorKind::Storage);
            assert_eq!(
                service.state().expect("state should load"),
                AuthState::signed_out()
            );
        });
    }

    #[test]
    fn restores_a_non_expired_session_without_exposing_tokens() {
        let (service, _, _, _) = service(
            response(200, SUCCESS_CODE, "", true),
            Some(access_token(4_102_444_800)),
        );

        let state = service.state().expect("stored session should load");
        let value = serde_json::to_value(&state).expect("state should serialize");

        assert!(state.authenticated);
        assert_eq!(state.user.expect("user").uid, "user-id");
        assert!(value.get("value").is_none());
        assert!(value.get("refreshToken").is_none());
    }

    #[test]
    fn clears_an_expired_session_during_initialization() {
        let (service, _, store, _) =
            service(response(200, SUCCESS_CODE, "", true), Some(access_token(1)));

        assert_eq!(
            service.state().expect("state should load"),
            AuthState::signed_out()
        );
        assert_eq!(*store.clear_count.lock().expect("clear count lock"), 1);
    }

    #[test]
    fn starts_signed_out_when_the_session_store_cannot_be_loaded() {
        let backend = Arc::new(RecordingBackend {
            response: response(200, SUCCESS_CODE, "", true),
            requests: Mutex::new(Vec::new()),
        });
        let store = Arc::new(RecordingStore {
            session: Mutex::new(None),
            clear_count: Mutex::new(0),
            load_error: true,
            save_error: false,
            clear_error: false,
        });

        let service = AuthService::new(backend, store, EventBus::new(8))
            .expect("session recovery failure should not prevent startup");

        assert_eq!(
            service.state().expect("signed-out state should load"),
            AuthState::signed_out()
        );
    }

    #[test]
    fn signs_out_an_expired_session_even_when_persistent_cleanup_fails() {
        let backend = Arc::new(RecordingBackend {
            response: response(200, SUCCESS_CODE, "", true),
            requests: Mutex::new(Vec::new()),
        });
        let store = Arc::new(RecordingStore {
            session: Mutex::new(Some(access_token(4_102_444_800))),
            clear_count: Mutex::new(0),
            load_error: false,
            save_error: false,
            clear_error: true,
        });
        let service = AuthService::new(backend, store.clone(), EventBus::new(8))
            .expect("auth service should initialize");
        *service.write_session() = Some(access_token(1));

        let state = service
            .state()
            .expect("persistent cleanup failure should not keep an expired session");

        assert_eq!(state, AuthState::signed_out());
        assert_eq!(*store.clear_count.lock().expect("clear count lock"), 1);
    }

    #[test]
    fn logout_clears_the_session_and_publishes_an_event() {
        let (service, _, store, events) = service(
            response(200, SUCCESS_CODE, "", true),
            Some(access_token(4_102_444_800)),
        );
        let mut subscriber = events.subscribe_to([EventKind::UserLoggedOut]);

        let state = service.logout().expect("logout should succeed");

        assert_eq!(state, AuthState::signed_out());
        assert!(store.session.lock().expect("session lock").is_none());
        let event = tauri::async_runtime::block_on(async {
            subscriber
                .recv()
                .await
                .expect("logout event should publish")
        });
        assert_eq!(event.event.kind(), EventKind::UserLoggedOut);
    }
}
