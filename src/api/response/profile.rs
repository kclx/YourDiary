use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct UserProfile {
    pub userid: i64,
    pub useremail: Option<String>,
    pub name: String,
    pub avatar: Option<String>,
    pub description: Option<String>,
    pub age_range: Option<String>,
    pub role: Option<String>,
    pub diary_count: i64,
    pub word_count: i64,
    pub image_count: i64,
    pub is_member: bool,
    pub member_expiry_time: Option<i64>,
    pub enable_treehole: bool,
}

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct PairedProfile {
    pub userid: i64,
    pub name: String,
    pub avatar: String,
    pub description: Option<String>,
    pub role: String,
    pub diary_count: i64,
    pub word_count: i64,
    pub image_count: i64,
    pub paired_time: i64,
    pub last_login_time: i64,
    pub ts: i64,
}

#[derive(Deserialize)]
pub(crate) struct ProfileResponse {
    #[serde(flatten)]
    pub user: UserProfile,
    pub paired: bool,
    pub paired_user_config: Option<Value>,
}
