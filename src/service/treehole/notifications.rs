use super::{Notification, Post, TreeholeClient, cache, treehole::TreeholeError};
use crate::{repo::snapshot::Snapshot, service::auth::Session};

impl Notification {
    pub(crate) fn action(&self) -> &'static str {
        match self.kind.as_str() {
            "NEW_COMMENT" => "评论了你的帖子",
            "COMMENT_REPLY" => "回复了你的评论",
            "COMMENT_LIKE" => "赞了你的评论",
            "ENTRY_LIKE" => "赞了你的帖子",
            "NEW_ENTRY" => "发布了树洞帖子",
            _ => "树洞互动",
        }
    }
}

impl TreeholeClient {
    pub(crate) fn notifications(
        &self,
        session: &Session,
    ) -> Result<(Vec<Notification>, bool), &'static str> {
        notification_list(&session.snapshot("treehole/notifications"), true, || {
            self.0.notifications(session.token())
        })
    }

    pub(crate) fn mark_notifications(
        &self,
        session: &Session,
        id: Option<i64>,
    ) -> Result<Vec<Notification>, &'static str> {
        notification_list(&session.snapshot("treehole/notifications"), false, || {
            self.0.mark_notifications(session.token(), id)
        })
        .map(|(rows, _)| rows)
    }

    pub(crate) fn notification_post(
        &self,
        session: &Session,
        open_id: &str,
        id: Option<i64>,
    ) -> Result<Post, &'static str> {
        let key = format!("treehole/detail/{open_id}");
        post_for_notification(&session.snapshot(&key), id, || {
            self.0.detail(session.token(), open_id)
        })
    }
}

fn post_for_notification(
    snapshot: &Snapshot,
    id: Option<i64>,
    fetch: impl FnOnce() -> Result<Post, TreeholeError>,
) -> Result<Post, &'static str> {
    let result = cache::refresh(
        snapshot,
        || {
            let post = fetch()?;
            if id.is_some_and(|id| id != post.id) {
                return Err(TreeholeError::Other("通知与帖子编号不一致"));
            }
            Ok(post)
        },
        |e| matches!(e, TreeholeError::Unavailable),
    );
    match result {
        Ok((mut post, source)) => {
            if id.is_some_and(|id| id != post.id) {
                return Err("通知与帖子编号不一致");
            }
            if let cache::Source::Cache(since) = source
                && let Some(Some((liked, count))) =
                    super::cached_likes(snapshot, &[super::like_key(post.id, None)], since).next()
            {
                post.liked = Some(liked);
                post.like_count = Some(count);
            }
            Ok(post)
        }
        Err(TreeholeError::Unavailable) => id
            .and_then(|id| snapshot.find_treehole_post(id).ok().flatten())
            .and_then(|(_, json)| serde_json::from_str(&json).ok())
            .ok_or("离线，暂无帖子缓存"),
        Err(TreeholeError::Other(message)) => Err(message),
    }
}

fn notification_list(
    snapshot: &Snapshot,
    allow_offline: bool,
    fetch: impl FnOnce() -> Result<Vec<Notification>, TreeholeError>,
) -> Result<(Vec<Notification>, bool), &'static str> {
    let (mut rows, source) = cache::refresh(snapshot, fetch, |e| {
        allow_offline && matches!(e, TreeholeError::Unavailable)
    })
    .map_err(|e| match e {
        TreeholeError::Unavailable => "无法连接树洞，请稍后重试",
        TreeholeError::Other(message) => message,
    })?;
    rows.sort_by_key(|n| std::cmp::Reverse((n.createdtime, n.id)));
    Ok((rows, matches!(source, cache::Source::Cache(_))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_and_read_receipts_are_account_scoped() {
        let dir =
            std::env::temp_dir().join(format!("yourdiary-notifications-{}", std::process::id()));
        let db = dir.join("test.sqlite3");
        let snapshot = |owner, key| Snapshot::new(owner, key).at(&db);
        let inbox = || snapshot("alice", "treehole/notifications");
        let sample = r#"[
            {"id":1,"type":"NEW_COMMENT","createdtime":1,"read":true,"comment_added":"新评论"},
            {"id":2,"type":"COMMENT_REPLY","createdtime":2,"read":false,"comment_added":"新回复","comment_replied":"原评论"},
            {"id":3,"type":"COMMENT_LIKE","createdtime":3,"read":false,"comment_replied":"原评论"},
            {"id":4,"type":"FUTURE_EVENT","createdtime":0,"read":true}
        ]"#;
        let rows = || serde_json::from_str::<Vec<Notification>>(sample).unwrap();
        let (items, offline) = notification_list(&inbox(), true, || Ok(rows())).unwrap();
        assert!(!offline);
        assert_eq!(
            items.iter().map(Notification::action).collect::<Vec<_>>(),
            [
                "赞了你的评论",
                "回复了你的评论",
                "评论了你的帖子",
                "树洞互动"
            ]
        );
        let unavailable = || Err(TreeholeError::Unavailable);
        assert!(notification_list(&inbox(), false, unavailable).is_err());
        let (saved, offline) = notification_list(&inbox(), true, unavailable).unwrap();
        assert!(offline && !saved[0].read && !saved[1].read);
        assert!(
            notification_list(
                &snapshot("bob", "treehole/notifications"),
                true,
                unavailable
            )
            .is_err()
        );
        for target in [Some(2), None] {
            let mut confirmed = rows();
            for n in &mut confirmed {
                if target.is_none_or(|id| id == n.id) {
                    n.read = true;
                }
            }
            notification_list(&inbox(), false, || Ok(confirmed)).unwrap();
            let saved = notification_list(&inbox(), true, unavailable).unwrap().0;
            assert!(saved[1].read);
            assert_eq!(saved[0].read, target.is_none());
        }
        let page = "treehole/posts/20/best";
        snapshot("alice", page)
            .write(Some(br#"[{"id":7,"title":"first"}]"#))
            .unwrap();
        snapshot("bob", page)
            .write(Some(br#"[{"id":7,"title":"foreign"}]"#))
            .unwrap();
        let details = snapshot("alice", "treehole/detail/key");
        assert!(
            post_for_notification(&details, Some(8), || Ok(serde_json::from_str(
                r#"{"id":7}"#
            )
            .unwrap()))
            .is_err()
        );
        assert!(details.read(None).unwrap().is_none());
        let post =
            post_for_notification(&details, Some(7), || Err(TreeholeError::Unavailable)).unwrap();
        assert_eq!(post.title.as_deref(), Some("first"));
        let remote = || Ok(serde_json::from_str(r#"{"id":9,"content":"完整帖子"}"#).unwrap());
        assert_eq!(post_for_notification(&details, None, remote).unwrap().id, 9);
        assert_eq!(
            post_for_notification(&details, None, || Err(TreeholeError::Unavailable))
                .unwrap()
                .content
                .as_deref(),
            Some("完整帖子")
        );
        assert!(
            post_for_notification(&snapshot("bob", "treehole/detail/key"), None, || Err(
                TreeholeError::Unavailable
            ))
            .is_err()
        );
        let (key, json) = inbox().find_treehole_post(7).unwrap().unwrap();
        assert_eq!(key, page);
        assert_eq!(
            serde_json::from_str::<Post>(&json)
                .unwrap()
                .title
                .as_deref(),
            Some("first")
        );
        assert!(inbox().find_treehole_post(8).unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
