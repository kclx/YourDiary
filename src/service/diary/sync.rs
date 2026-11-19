use super::{SyncError, SyncedDiaries, cache::cache_sync, mapping::map_entries};
use crate::service::auth::{Session, SessionStore};
use crate::{
    api::diary::DiaryClient,
    repo::diary::DiaryCache,
    service::cache::{Source, refresh},
};
use std::{
    sync::{
        Arc,
        mpsc::{Receiver, channel},
    },
    thread::spawn,
};

#[derive(Clone, Copy)]
pub(crate) struct CacheProgress {
    pub completed: usize,
    pub total: usize,
}

pub(crate) enum SyncEvent {
    List(Result<SyncedDiaries, SyncError>),
    Cached,
    Progress(CacheProgress),
    Complete(Option<CacheProgress>),
}

pub(crate) fn start_sync(session: Arc<Session>, store: SessionStore) -> Receiver<SyncEvent> {
    let (sender, receiver) = channel();
    spawn(move || {
        let snapshot = session.snapshot("diary/list");
        match refresh(
            &snapshot,
            || {
                DiaryClient::new(session.token())
                    .sync()
                    .and_then(|r| map_entries(r, session.profile(), session.account_id()))
            },
            |e| *e == SyncError::RequestFailed,
        ) {
            Ok((mut result, source)) => {
                if let Source::Cache(since) = source {
                    // 兼容旧版本在缺少 user_config 时写出的本账号快照。
                    if result.account_id.is_none()
                        && let Some(account) = session.account_id()
                    {
                        result.account_id = Some(account);
                        result.entries.retain_mut(|entry| {
                            entry.is_partner || *entry.author_id.get_or_insert(account) == account
                        });
                    }
                    if let Some(account) = result.account_id
                        && let Ok(cache) = DiaryCache::open()
                        && let Ok(marks) = cache.confirmed_since(account, since)
                    {
                        for entry in result.entries.iter().filter(|e| e.is_partner) {
                            if let Some(author) = entry.author_id
                                && let Some(revision) = entry.revision.get()
                                && marks
                                    .get(&(author, entry.id))
                                    .is_some_and(|mark| *mark >= revision)
                            {
                                entry.read.set(Some(true));
                            }
                        }
                    }
                    let _ = sender.send(SyncEvent::List(Ok(result)));
                    let _ = sender.send(SyncEvent::Cached);
                    return;
                }
                if sender
                    .send(SyncEvent::List(Ok(SyncedDiaries {
                        moods: result.moods,
                        account_id: result.account_id,
                        entries: result.entries.clone(),
                        profile: result.profile,
                    })))
                    .is_err()
                {
                    return;
                }
                let finished = result.account_id.and_then(|account| {
                    match cache_sync(&session, account, &result.entries, |progress| {
                        session.is_active() && sender.send(SyncEvent::Progress(progress)).is_ok()
                    }) {
                        Ok(progress) => Some(progress),
                        Err(error) => {
                            eprintln!("无法同步日记缓存：{error}");
                            None
                        }
                    }
                });
                let _ = sender.send(SyncEvent::Complete(finished));
            }
            Err(error) => {
                if error == SyncError::Unauthorized {
                    session.end();
                    if let Err(error) = store.forget(session.token()) {
                        eprintln!("无法清除失效的登录会话：{error}");
                    }
                }
                let _ = sender.send(SyncEvent::List(Err(error)));
            }
        }
    });
    receiver
}

pub(crate) fn mark_read(
    session: &Session,
    diary_id: i64,
    account: Option<i64>,
    author: Option<i64>,
    revision: Option<i64>,
) -> Result<(), SyncError> {
    DiaryClient::new(session.token()).mark_read(diary_id)?;
    if let (Some(account), Some(author), Some(revision)) = (account, author, revision)
        && let Ok(cache) = DiaryCache::open()
        && let Err(error) = cache.confirm_read(account, author, diary_id, revision)
    {
        eprintln!("无法缓存已读状态：{error}");
    }
    Ok(())
}
