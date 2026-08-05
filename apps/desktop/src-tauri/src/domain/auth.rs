use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub data: Option<LoginData>,
    pub errcode: String,
    pub errmsg: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginData {
    pub additional_information: AccessToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenResponse {
    pub data: Option<AccessToken>,
    pub errcode: String,
    pub errmsg: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessToken {
    pub additional_information: UserInformation,
    #[serde(deserialize_with = "deserialize_unix_seconds")]
    pub expiration: i64,
    pub expires_in: i64,
    pub refresh_token: Value,
    pub scope: Vec<String>,
    pub token_type: String,
    pub value: String,
}

fn deserialize_unix_seconds<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = i64::deserialize(deserializer)?;
    Ok(if value > 100_000_000_000 {
        value / 1_000
    } else {
        value
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInformation {
    pub realname: String,
    pub phone: String,
    pub username: String,
    pub uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub authenticated: bool,
    pub user: Option<UserInformation>,
    pub expires_at: Option<i64>,
}

impl AuthState {
    pub const fn signed_out() -> Self {
        Self {
            authenticated: false,
            user: None,
            expires_at: None,
        }
    }

    pub fn from_token(token: &AccessToken) -> Self {
        Self {
            authenticated: true,
            user: Some(token.additional_information.clone()),
            expires_at: Some(token.expiration),
        }
    }
}

impl AccessToken {
    pub fn refresh_token_value(&self) -> Option<&str> {
        self.refresh_token
            .as_str()
            .or_else(|| self.refresh_token.get("value").and_then(Value::as_str))
            .filter(|value| !value.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_the_confirmed_success_contract_without_optional_profile_fields() {
        let payload: LoginResponse = serde_json::from_value(serde_json::json!({
            "data": {
                "additionalInformation": {
                    "additionalInformation": {
                        "realname": "测试用户",
                        "phone": "13800000000",
                        "username": "13800000000",
                        "uid": "user-id",
                        "jti": "unconfirmed-extra-field"
                    },
                    "expiration": 1_800_000_000_000_i64,
                    "expiresIn": 3_600,
                    "refreshToken": {
                        "expiration": 1_900_000_000,
                        "value": "refresh-token"
                    },
                    "scope": ["all"],
                    "tokenType": "bearer",
                    "value": "access-token"
                }
            },
            "errcode": "0000",
            "errmsg": ""
        }))
        .expect("confirmed login response should deserialize");

        let token = payload
            .data
            .expect("successful response should contain data")
            .additional_information;
        assert_eq!(token.additional_information.realname, "测试用户");
        assert_eq!(token.expiration, 1_800_000_000);
        assert_eq!(token.value, "access-token");
        assert_eq!(token.refresh_token["value"], "refresh-token");
    }

    #[test]
    fn deserializes_an_error_envelope_without_data() {
        let payload: LoginResponse = serde_json::from_value(serde_json::json!({
            "errcode": "601002",
            "errmsg": "账号或密码有误"
        }))
        .expect("error response may omit data");

        assert!(payload.data.is_none());
        assert_eq!(payload.errcode, "601002");
    }

    #[test]
    fn auth_state_does_not_expose_tokens() {
        let state = AuthState {
            authenticated: true,
            user: Some(UserInformation {
                realname: "测试用户".to_owned(),
                phone: "13800000000".to_owned(),
                username: "13800000000".to_owned(),
                uid: "user-id".to_owned(),
            }),
            expires_at: Some(1_800_000_000),
        };
        let value = serde_json::to_value(state).expect("auth state should serialize");

        assert_eq!(value["authenticated"], true);
        assert_eq!(value["user"]["uid"], "user-id");
        assert!(value.get("accessToken").is_none());
        assert!(value.get("refreshToken").is_none());
    }

    #[test]
    fn deserializes_the_confirmed_refresh_contract() {
        let payload: RefreshTokenResponse = serde_json::from_value(serde_json::json!({
            "data": {
                "additionalInformation": {
                    "realname": "测试用户",
                    "phone": "13800000000",
                    "username": "13800000000",
                    "uid": "user-id"
                },
                "expiration": 1_800_000_000_000_i64,
                "expiresIn": 3_600,
                "refreshToken": { "value": "rotated-refresh-token" },
                "scope": ["all"],
                "tokenType": "bearer",
                "value": "refreshed-access-token"
            },
            "errcode": "0000",
            "errmsg": "success"
        }))
        .expect("confirmed refresh response should deserialize");

        let token = payload
            .data
            .expect("successful refresh should contain data");
        assert_eq!(token.expiration, 1_800_000_000);
        assert_eq!(token.value, "refreshed-access-token");
        assert_eq!(token.refresh_token_value(), Some("rotated-refresh-token"));
    }
}
