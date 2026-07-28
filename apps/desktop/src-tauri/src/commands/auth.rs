use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use super::error::{IpcError, IpcResult};

const LOGIN_URL: &str = "https://tst-crm.shencom.cn/service-uaa/user/login";
const SCID: &str = "sca15516911b95f35b";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub data: Option<LoginData>,
    pub errcode: String,
    pub errmsg: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginData {
    pub additional_information: AccessToken,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessToken {
    pub additional_information: UserInformation,
    pub expiration: i64,
    pub expired: bool,
    pub expires_in: i64,
    pub refresh_token: RefreshToken,
    pub scope: Vec<String>,
    pub token_type: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInformation {
    pub tokenid: String,
    pub sex: i64,
    pub pid: String,
    pub is_bind_wx: bool,
    #[serde(rename = "type")]
    pub user_type: i64,
    pub realname: String,
    pub uid: String,
    pub user_auth_type: String,
    pub phone: String,
    pub id: String,
    pub scid: String,
    pub job_number: String,
    pub username: String,
    pub jti: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshToken {
    pub expiration: i64,
    pub value: String,
}

#[tauri::command]
pub async fn login(request: LoginRequest) -> IpcResult<LoginResponse> {
    if request.username.trim().is_empty() || request.password.is_empty() {
        return Err(IpcError::validation());
    }

    let response = reqwest::Client::new()
        .post(LOGIN_URL)
        .header("scid", SCID)
        .header("Accept", "*/*")
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "login request failed");
            IpcError::auth_unavailable()
        })?;

    let status = response.status();
    let payload = response.json::<LoginResponse>().await.map_err(|error| {
        tracing::error!(error = %error, "login response could not be decoded");
        IpcError::auth_unavailable()
    })?;

    if status != StatusCode::OK || payload.errcode != "0" || payload.data.is_none() {
        let message = if payload.errmsg.trim().is_empty() {
            "手机号或密码不正确。".to_owned()
        } else {
            payload.errmsg.clone()
        };
        return Err(IpcError::auth_failed(message));
    }

    Ok(payload)
}
