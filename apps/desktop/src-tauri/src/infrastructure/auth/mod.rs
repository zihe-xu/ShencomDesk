use std::time::Duration;

use async_trait::async_trait;
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE},
    Client, Request,
};

use crate::{
    application::auth_service::{AuthBackend, AuthBackendResponse, AuthServiceError},
    domain::auth::{LoginRequest, LoginResponse},
};

const LOGIN_URL: &str = "https://tst-crm.shencom.cn/service-uaa/user/login";
const SCID: &str = "sca15516911b95f35b";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);

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
    async fn login(
        &self,
        request: &LoginRequest,
    ) -> Result<AuthBackendResponse, AuthServiceError> {
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

#[cfg(test)]
mod tests {
    use reqwest::Method;
    use serde_json::Value;

    use super::*;

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
}
