use super::{
    Endpoint,
    request::TreeholeCommentRequest,
    response::treehole::{Comment, Entries, Notification, Post, Tab, TreeholeActionResponse},
};
use reqwest::{StatusCode, blocking::Client};
use serde::de::DeserializeOwned;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct TreeholeClient(Client);

impl TreeholeClient {
    pub(crate) fn new() -> Self {
        Self(super::account_client())
    }

    pub(crate) fn tabs(&self, token: &str) -> Result<Vec<Tab>, TreeholeError> {
        self.entries(token, Endpoint::TreeholeTabs)
    }

    pub(crate) fn notifications(&self, token: &str) -> Result<Vec<Notification>, TreeholeError> {
        self.entries(token, Endpoint::TreeholeNotifications)
    }

    pub(crate) fn mark_notifications(
        &self,
        token: &str,
        id: Option<i64>,
    ) -> Result<Vec<Notification>, TreeholeError> {
        let endpoint = match id {
            Some(notification_id) if notification_id > 0 => {
                Endpoint::ReadTreeholeNotification { notification_id }
            }
            Some(_) => return Err(TreeholeError::Other("通知编号无效")),
            None => Endpoint::ReadAllTreeholeNotifications,
        };
        self.entries(token, endpoint)
    }

    pub(crate) fn posts(
        &self,
        token: &str,
        tab: &str,
        offset: u32,
    ) -> Result<Vec<Post>, TreeholeError> {
        self.entries(token, Endpoint::TreeholeList { offset, tab })
    }

    pub(crate) fn comments(
        &self,
        token: &str,
        treehole_id: i64,
        offset: u32,
        latest: bool,
    ) -> Result<Vec<Comment>, TreeholeError> {
        self.entries(
            token,
            Endpoint::TreeholeComments {
                latest,
                treehole_id,
                offset,
            },
        )
    }

    pub(crate) fn write_comment(
        &self,
        token: &str,
        treehole_id: i64,
        content: String,
        reply_to: Option<i64>,
    ) -> Result<(), String> {
        self.write_action(
            token,
            Endpoint::WriteTreeholeComment,
            TreeholeCommentRequest {
                treehole_id,
                content,
                reply_to,
            }
            .into_form(),
            "未能确认发送结果，请先刷新评论再重试",
            "评论发布失败",
        )
    }

    pub(crate) fn set_like(
        &self,
        token: &str,
        treehole_id: i64,
        comment: Option<i64>,
        liked: bool,
    ) -> Result<(), String> {
        let endpoint = match (comment, liked) {
            (None, true) => Endpoint::LikeTreehole { treehole_id },
            (None, false) => Endpoint::UnlikeTreehole { treehole_id },
            (Some(comment_id), true) => Endpoint::LikeTreeholeComment {
                treehole_id,
                comment_id,
            },
            (Some(comment_id), false) => Endpoint::UnlikeTreeholeComment {
                treehole_id,
                comment_id,
            },
        };
        self.write_action(
            token,
            endpoint,
            reqwest::blocking::multipart::Form::new().text("null", "null"),
            "未能确认点赞状态，请先刷新再重试",
            "点赞状态更新失败",
        )
    }

    fn write_action(
        &self,
        token: &str,
        endpoint: Endpoint<'_>,
        form: reqwest::blocking::multipart::Form,
        uncertain: &'static str,
        rejected: &'static str,
    ) -> Result<(), String> {
        let response = endpoint
            .request(&self.0)
            .header("auth", format!("token {token}"))
            .header(reqwest::header::USER_AGENT, "OhApp/3.7 Platform/Android")
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(Duration::from_secs(20))
            .multipart(form)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| match e.status() {
                Some(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => "登录已失效，请重新登录",
                _ => uncertain,
            })?;
        let data: TreeholeActionResponse =
            serde_json::from_reader(response).map_err(|_| uncertain)?;
        if data.error == 0 {
            Ok(())
        } else {
            Err(data
                .msg
                .filter(|s| !s.trim().is_empty())
                .or(data.title)
                .unwrap_or_else(|| rejected.into()))
        }
    }

    fn entries<T: DeserializeOwned>(
        &self,
        token: &str,
        endpoint: Endpoint<'_>,
    ) -> Result<Vec<T>, TreeholeError> {
        let next_posts_page = matches!(endpoint, Endpoint::TreeholeList { offset: 1.., .. });
        let response = endpoint
            .request(&self.0)
            .header("auth", format!("token {token}"))
            .timeout(Duration::from_secs(20))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| match e.status() {
                Some(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
                    TreeholeError::Other("登录已失效，请重新登录")
                }
                _ if super::temporarily_unavailable(&e) => TreeholeError::Unavailable,
                _ => TreeholeError::Other("树洞请求未成功"),
            })?;
        let data: Entries<T> = serde_json::from_reader(response).map_err(|e| {
            if e.is_io() {
                TreeholeError::Unavailable
            } else {
                TreeholeError::Other("无法解析树洞数据")
            }
        })?;
        data.into_entries(next_posts_page)
    }
}

impl<T> Entries<T> {
    fn into_entries(self, next_posts_page: bool) -> Result<Vec<T>, TreeholeError> {
        // 树洞帖子翻到末尾时，服务端返回 error=4 和显式空数组。
        let end_of_posts =
            next_posts_page && self.error == 4 && self.entries.as_ref().is_some_and(Vec::is_empty);
        if self.error != 0 && !end_of_posts {
            return Err(TreeholeError::Other("树洞请求未成功"));
        }
        self.entries.ok_or(TreeholeError::Other("树洞响应缺少列表"))
    }
}

#[derive(Debug)]
pub(crate) enum TreeholeError {
    Unavailable,
    Other(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_responses_require_a_confirmed_list() {
        let decode = |body| {
            serde_json::from_str::<Entries<Notification>>(body)
                .unwrap()
                .into_entries(false)
        };
        let rows = decode(r#"{"error":0,"notifications":[{"id":9,"type":"COMMENT_REPLY","read":false,"comment_added":"新回复","comment_replied":"原评论"}]}"#).unwrap();
        assert_eq!(rows[0].comment_added.as_deref(), Some("新回复"));
        assert_eq!(rows[0].comment_replied.as_deref(), Some("原评论"));
        assert!(!rows[0].read);
        assert!(
            decode(r#"{"error":0,"notifications":[]}"#)
                .unwrap()
                .is_empty()
        );
        assert!(decode(r#"{"error":0}"#).is_err());
        assert!(decode(r#"{"error":1,"notifications":[]}"#).is_err());
    }

    #[test]
    fn empty_error_four_ends_only_a_continued_post_list() {
        let page = |json: &str, next| {
            serde_json::from_str::<Entries<Post>>(json)
                .unwrap()
                .into_entries(next)
        };
        let mut feed = crate::service::treehole::Feed::default();
        feed.append(page(r#"{"error":0,"entries":[{"id":7}]}"#, false).unwrap());
        let end = r#"{"error":4,"entries":[]}"#;
        feed.append(page(end, true).unwrap());
        assert!(feed.pages.finished);
        assert_eq!(feed.posts[0].id, 7);
        assert!(page(end, false).is_err());
        assert!(page(r#"{"error":4}"#, true).is_err());
        assert!(page(r#"{"error":4,"entries":[{"id":7}]}"#, true).is_err());
        assert!(page(r#"{"error":3,"entries":[]}"#, true).is_err());
    }
}
