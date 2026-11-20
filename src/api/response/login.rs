use serde::Deserialize;

use super::profile::ProfileResponse;

#[derive(Deserialize)]
pub(crate) struct LoginResponse {
    pub error: i64,
    pub token: Option<String>,
    pub userid: Option<i64>,
    pub version: Option<String>,
    #[serde(default, deserialize_with = "super::optional_object")]
    pub user_config: Option<ProfileResponse>,
}

#[cfg(test)]
mod tests {
    use super::LoginResponse;

    #[test]
    fn parses_login_response_with_extra_fields() {
        let json = r#"{
            "error": 0,
            "token": "test-token",
            "userid": 42,
            "version": "3.7",
            "extra_field": "ignored",
            "user_config": {
                "userid": 42,
                "name": "示例用户",
                "description": null,
                "paired": false,
                "diary_count": 2,
                "word_count": 100,
                "image_count": 0,
                "is_member": false,
                "enable_treehole": true,
                "avatar": "sample-avatar"
            }
        }"#;

        let response: LoginResponse = serde_json::from_str(json).expect("解析登录响应失败");
        assert_eq!(response.error, 0);
        assert_eq!(response.token.as_deref(), Some("test-token"));
        assert_eq!(response.userid, Some(42));
        assert_eq!(response.version.as_deref(), Some("3.7"));
        let config = response.user_config.expect("缺少用户配置");
        assert_eq!(config.user.name, "示例用户");
        assert!(config.user.description.is_none());
        assert_eq!(config.user.avatar.as_deref(), Some("sample-avatar"));
        assert_eq!(config.user.diary_count, 2);
        assert!(config.user.enable_treehole);
    }

    #[test]
    fn parses_response_with_missing_optional_fields() {
        let response: LoginResponse =
            serde_json::from_str(r#"{"error": 1}"#).expect("解析缺少可选字段的响应失败");
        assert_eq!(response.error, 1);
        assert!(response.token.is_none());
        assert!(response.userid.is_none());
        assert!(response.version.is_none());
        assert!(response.user_config.is_none());
    }
}
