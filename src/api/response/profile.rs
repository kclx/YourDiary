use super::diary::ApiInteger;
use serde::{Deserialize, Deserializer};

fn account_id<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    ApiInteger::deserialize(d)?
        .positive_number()
        .ok_or_else(|| serde::de::Error::custom("无效的用户 ID"))
}

fn number<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .unwrap_or_default())
}

fn text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

use serde_json::Value;

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct UserProfile {
    #[serde(deserialize_with = "account_id")]
    pub userid: i64,
    pub useremail: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub name: String,
    pub avatar: Option<String>,
    pub description: Option<String>,
    pub age_range: Option<String>,
    pub role: Option<String>,
    #[serde(default, deserialize_with = "number")]
    pub diary_count: i64,
    #[serde(default, deserialize_with = "number")]
    pub word_count: i64,
    #[serde(default, deserialize_with = "number")]
    pub image_count: i64,
    #[serde(default, deserialize_with = "super::bool_flag")]
    pub is_member: bool,
    pub member_expiry_time: Option<i64>,
    #[serde(default, deserialize_with = "super::bool_flag")]
    pub enable_treehole: bool,
}

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct PairedProfile {
    #[serde(deserialize_with = "account_id")]
    pub userid: i64,
    #[serde(default, deserialize_with = "text")]
    pub name: String,
    #[serde(default, deserialize_with = "text")]
    pub avatar: String,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub role: String,
    #[serde(default, deserialize_with = "number")]
    pub diary_count: i64,
    #[serde(default, deserialize_with = "number")]
    pub word_count: i64,
    #[serde(default, deserialize_with = "number")]
    pub image_count: i64,
    #[serde(default, deserialize_with = "number")]
    pub paired_time: i64,
    #[serde(default, deserialize_with = "number")]
    pub last_login_time: i64,
    #[serde(default, deserialize_with = "number")]
    pub ts: i64,
}

#[derive(Deserialize)]
pub(crate) struct ProfileResponse {
    #[serde(flatten)]
    pub user: UserProfile,
    pub paired: bool,
    pub paired_user_config: Option<Value>,
}
