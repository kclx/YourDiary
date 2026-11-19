use serde::Deserialize;

#[derive(Deserialize)]
pub(in crate::api) struct Entries<T> {
    pub error: i64,
    #[serde(alias = "comments", alias = "notifications")]
    pub entries: Option<Vec<T>>,
}

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct Notification {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub user: Option<String>,
    pub treehole: Option<String>,
    pub treehole_id: Option<i64>,
    pub comment_added: Option<String>,
    pub comment_replied: Option<String>,
    pub createdtime: Option<i64>,
    pub read: bool,
}

#[derive(Deserialize, serde::Serialize)]
pub(crate) struct Tab {
    pub key: String,
    pub label: String,
}

#[derive(Deserialize, serde::Serialize)]
pub(crate) struct Post {
    pub id: i64,
    pub title: Option<String>,
    pub content: Option<String>,
    pub username: Option<String>,
    pub createdtime: Option<i64>,
    pub like_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub liked: Option<bool>,
}

#[derive(Deserialize, serde::Serialize)]
pub(crate) struct Comment {
    pub id: i64,
    pub username: Option<String>,
    pub content: Option<String>,
    pub createdtime: Option<i64>,
    pub is_author: Option<bool>,
    pub liked: Option<bool>,
    pub like_count: Option<u64>,
    pub reply_to: Option<ReplyTo>,
}

#[derive(Deserialize, serde::Serialize)]
pub(crate) struct ReplyTo {
    pub id: Option<i64>,
    pub username: Option<String>,
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub(in crate::api) struct TreeholeActionResponse {
    pub error: i64,
    pub msg: Option<String>,
    pub title: Option<String>,
}
