use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
pub(in crate::api) struct PostDetail {
    pub error: i64,
    pub treehole: Option<Post>,
}

fn post_id<'de, D: Deserializer<'de>>(field: D) -> Result<Option<i64>, D::Error> {
    Ok(Option::<super::diary::ApiInteger>::deserialize(field)?.and_then(|id| id.positive_number()))
}

fn open_id<'de, D: Deserializer<'de>>(field: D) -> Result<Option<String>, D::Error> {
    Ok(
        match Option::<super::diary::ApiInteger>::deserialize(field)? {
            Some(super::diary::ApiInteger::Text(id)) if !id.trim().is_empty() => Some(id),
            Some(super::diary::ApiInteger::Number(id)) if id > 0 => Some(id.to_string()),
            _ => None,
        },
    )
}

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
    #[serde(default, deserialize_with = "post_id")]
    pub treehole_id: Option<i64>,
    #[serde(default, deserialize_with = "open_id")]
    pub treehole_open_id: Option<String>,
    pub comment_added: Option<String>,
    pub comment_replied: Option<String>,
    pub createdtime: Option<i64>,
    #[serde(default, deserialize_with = "super::bool_flag")]
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
