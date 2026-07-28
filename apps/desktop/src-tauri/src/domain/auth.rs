use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessToken {
    pub additional_information: UserInformation,
    pub expiration: i64,
    pub expires_in: i64,
    pub refresh_token: Value,
    pub scope: Vec<String>,
    pub token_type: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInformation {
    pub realname: String,
    pub phone: String,
    pub username: String,
    pub uid: String,
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
                    "expiration": 1_800_000_000,
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
}
