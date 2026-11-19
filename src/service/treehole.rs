mod comments;
mod notifications;
pub(crate) use crate::api::response::treehole::{Comment, Notification, Post};
use crate::repo::snapshot::Snapshot;
use crate::{
    api::{
        response::treehole::Tab,
        treehole::{self, TreeholeError},
    },
    service::{auth::Session, cache},
};
pub(crate) use comments::comment_order;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashSet;

#[derive(Clone)]
pub(crate) struct TreeholeClient(treehole::TreeholeClient);

impl TreeholeClient {
    pub(crate) fn new() -> Self {
        Self(treehole::TreeholeClient::new())
    }

    pub(crate) fn tabs(&self, session: &Session) -> Result<Vec<Tab>, &'static str> {
        Self::page(&session.snapshot("treehole/tabs"), || {
            self.0.tabs(session.token())
        })
        .map(|(rows, _)| rows)
    }

    pub(crate) fn posts(
        &self,
        session: &Session,
        tab: &str,
        offset: u32,
    ) -> Result<Vec<Post>, &'static str> {
        let key = format!("treehole/posts/{offset}/{tab}");
        let snapshot = session.snapshot(&key);
        let (mut posts, source) =
            Self::page(&snapshot, || self.0.posts(session.token(), tab, offset))?;
        if let cache::Source::Cache(since) = source {
            let keys: Vec<_> = posts.iter().map(|p| like_key(p.id, None)).collect();
            for (post, value) in posts.iter_mut().zip(cached_likes(&snapshot, &keys, since)) {
                if let Some((liked, count)) = value {
                    post.liked = Some(liked);
                    post.like_count = Some(count);
                }
            }
        }
        Ok(posts)
    }

    pub(crate) fn comments(
        &self,
        session: &Session,
        treehole_id: i64,
        offset: u32,
        latest: bool,
    ) -> Result<Vec<Comment>, &'static str> {
        let key = format!(
            "treehole/comments/{treehole_id}/{offset}{}",
            if latest { "/latest" } else { "" }
        );
        let snapshot = session.snapshot(&key);
        let (mut rows, source) = Self::page(&snapshot, || {
            self.0
                .comments(session.token(), treehole_id, offset, latest)
        })?;
        if let cache::Source::Cache(since) = source {
            let keys: Vec<_> = rows
                .iter()
                .map(|row| like_key(treehole_id, Some(row.id)))
                .collect();
            for (row, value) in rows.iter_mut().zip(cached_likes(&snapshot, &keys, since)) {
                if let Some((liked, count)) = value {
                    row.liked = Some(liked);
                    row.like_count = Some(count);
                }
            }
        }
        Ok(rows)
    }

    pub(crate) fn set_like(
        &self,
        session: &Session,
        post: i64,
        comment: Option<i64>,
        liked: bool,
        count: u64,
    ) -> Result<(), String> {
        if post <= 0 || comment.is_some_and(|id| id <= 0) {
            return Err("点赞目标无效".into());
        }
        self.0.set_like(session.token(), post, comment, liked)?;
        let key = like_key(post, comment);
        if let Err(error) = session
            .snapshot(&key)
            .write(Some(&serde_json::to_vec(&(liked, count)).unwrap()))
        {
            eprintln!("无法缓存点赞状态：{error}");
        }
        Ok(())
    }

    pub(crate) fn write_comment(
        &self,
        session: &Session,
        treehole_id: i64,
        text: String,
        reply_to: Option<i64>,
    ) -> Result<(), String> {
        if treehole_id <= 0 || reply_to.is_some_and(|id| id <= 0) || text.trim().is_empty() {
            return Err("评论内容或回复目标无效".into());
        }
        self.0
            .write_comment(session.token(), treehole_id, text, reply_to)?;
        if let Err(error) = session
            .snapshot(&format!("treehole/comments/{treehole_id}/0/latest"))
            .write(None)
        {
            eprintln!("无法更新最新评论缓存：{error}");
        }
        Ok(())
    }

    fn page<T: Serialize + DeserializeOwned>(
        snapshot: &Snapshot,
        fetch: impl FnOnce() -> Result<Vec<T>, TreeholeError>,
    ) -> Result<(Vec<T>, cache::Source), &'static str> {
        cache::refresh(snapshot, fetch, |e| matches!(e, TreeholeError::Unavailable)).map_err(|e| {
            match e {
                TreeholeError::Unavailable => "树洞暂时无法连接",
                TreeholeError::Other(message) => message,
            }
        })
    }
}

fn like_key(post: i64, comment: Option<i64>) -> String {
    format!("treehole/like/{post}/{}", comment.unwrap_or(0))
}

fn cached_likes(
    snapshot: &Snapshot,
    keys: &[String],
    since: i64,
) -> impl Iterator<Item = Option<(bool, u64)>> {
    let changes = snapshot.read_related(keys, since).unwrap_or_default();
    keys.iter()
        .map(move |key| serde_json::from_slice(changes.get(key)?).ok())
}

/// 每个分类独立记录服务端偏移量。展示时去重，翻页仍按原始条数推进。
#[derive(Default)]
pub(crate) struct Feed {
    pub posts: Vec<Post>,
    page_starts: Vec<(usize, u32)>,
    pub pages: Pagination,
}

#[derive(Default)]
pub(crate) struct Pagination {
    seen: HashSet<i64>,
    pub offset: u32,
    pub finished: bool,
}

/// 手动刷新重新拉取已展示的范围。保留各页边界，让去重和服务端偏移沿用普通翻页逻辑。
pub(crate) fn fetch_pages<T>(
    offset: u32,
    through: u32,
    fetch: impl Fn(u32) -> Result<Vec<T>, &'static str>,
) -> Result<Vec<Vec<T>>, &'static str> {
    let mut next = offset;
    let mut pages = Vec::new();
    loop {
        let page = fetch(next)?;
        let count = page.len() as u32;
        next = next.saturating_add(count);
        pages.push(page);
        if count == 0 || next >= through {
            return Ok(pages);
        }
    }
}

impl Feed {
    pub fn append(&mut self, page: Vec<Post>) {
        let offset = self.pages.offset;
        let posts = self.pages.new_items(page, |post| post.id);
        if !posts.is_empty() {
            self.page_starts.push((self.posts.len(), offset));
            self.posts.extend(posts);
        }
    }

    pub fn page_offset(&self, index: usize) -> u32 {
        let end = self
            .page_starts
            .partition_point(|(first, _)| *first <= index);
        self.page_starts[end - 1].1
    }
}

impl Pagination {
    pub fn contains(&self, id: i64) -> bool {
        self.seen.contains(&id)
    }

    pub fn new_items<T>(&mut self, mut page: Vec<T>, id: impl Fn(&T) -> i64) -> Vec<T> {
        self.offset = self.offset.saturating_add(page.len() as u32);
        page.retain(|item| self.seen.insert(id(item)));
        self.finished = page.is_empty();
        page
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offset_counts_raw_items_and_repeated_pages_stop() {
        let mut feed = Feed::default();
        let page = |ids: &[i64]| {
            ids.iter()
                .map(|id| serde_json::from_value(serde_json::json!({"id":id})).unwrap())
                .collect()
        };
        feed.append(page(&[1, 1, 2]));
        feed.append(page(&[2, 3]));
        assert_eq!(
            (feed.posts.len(), feed.pages.offset, feed.pages.finished),
            (3, 5, false)
        );
        feed.append(page(&[3]));
        assert!(feed.pages.finished);
        let mut empty = Feed::default();
        empty.append(vec![]);
        assert!(empty.pages.finished);
    }
}
