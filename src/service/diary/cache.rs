use super::{CacheProgress, DiaryEntry, SyncError, mapping::full_diaries};
use crate::service::auth::Session;
use crate::{
    api::diary::DiaryClient,
    repo::diary::{DiaryCache, FullDiary},
};
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
};

pub(super) const BATCH_SIZE: usize = 25;

pub(crate) struct OpenedDiary {
    pub full: FullDiary,
    pub current: bool,
}

pub(super) fn cache_sync(
    session: &Session,
    account_id: i64,
    entries: &[DiaryEntry],
    report: impl FnMut(CacheProgress) -> bool,
) -> Result<CacheProgress, Box<dyn Error>> {
    let client = DiaryClient::new(session.token());
    fill_cache(
        &DiaryCache::open()?,
        account_id,
        entries,
        |author, ids| {
            if !session.is_active() {
                return Err(SyncError::Unauthorized);
            }
            let response = client.details(author, ids)?;
            if !session.is_active() {
                return Err(SyncError::Unauthorized);
            }
            full_diaries(response, author, ids)
        },
        report,
    )
}

pub(crate) fn open_diary(
    session: &Session,
    account_id: Option<i64>,
    entry: &DiaryEntry,
    refresh: bool,
) -> Option<OpenedDiary> {
    let cache = account_id
        .zip(entry.author_id)
        .and_then(|_| DiaryCache::open().ok());
    load_diary(cache.as_ref(), account_id, entry, refresh, |author, id| {
        full_diaries(
            DiaryClient::new(session.token())
                .details(author, &[id])
                .ok()?,
            author,
            &[id],
        )
        .ok()?
        .remove(&id)
    })
}

fn load_diary(
    cache: Option<&DiaryCache>,
    account_id: Option<i64>,
    entry: &DiaryEntry,
    refresh: bool,
    fetch: impl FnOnce(i64, i64) -> Option<FullDiary>,
) -> Option<OpenedDiary> {
    let revision = entry.revision.get();
    let key = account_id.zip(entry.author_id);
    let cached = cache.zip(key).and_then(|(cache, (account, author))| {
        cache.load(account, author, entry.id).ok().flatten()
    });
    if let Some(full) = cached
        .as_ref()
        .filter(|full| !refresh && full.covers(revision))
    {
        return Some(OpenedDiary {
            full: FullDiary {
                content: full.content.clone(),
                revision: full.revision,
            },
            current: true,
        });
    }
    if entry.has_full_content && !refresh {
        let full = FullDiary {
            content: entry.content.to_string(),
            revision,
        };
        if let Some((cache, (account, author))) = cache.zip(key) {
            if let Err(error) = cache.save(account, author, entry.id, &full) {
                eprintln!("无法保存日记缓存：{error}");
            }
        }
        return Some(OpenedDiary {
            full: cached
                .filter(|old| {
                    old.revision
                        .zip(revision)
                        .is_some_and(|(old, revision)| old > revision)
                })
                .unwrap_or(full),
            current: true,
        });
    }
    let fetched = entry.author_id.and_then(|author| fetch(author, entry.id));
    if let Some(full) = fetched {
        let current = revision.is_none() || full.covers(revision);
        if let Some((cache, (account, author))) = cache.zip(key) {
            if let Err(error) = cache.save(account, author, entry.id, &full) {
                eprintln!("无法保存日记缓存：{error}");
            }
        }
        return Some(OpenedDiary {
            full: cached
                .filter(|old| old.revision > full.revision)
                .unwrap_or(full),
            current,
        });
    }
    cached
        .or_else(|| {
            entry.has_full_content.then(|| FullDiary {
                content: entry.content.to_string(),
                revision,
            })
        })
        .map(|full| OpenedDiary {
            full,
            current: false,
        })
}

impl FullDiary {
    pub(super) fn covers(&self, revision: Option<i64>) -> bool {
        covers(self.revision, revision)
    }
}

fn covers(cached: Option<i64>, revision: Option<i64>) -> bool {
    cached
        .zip(revision)
        .is_some_and(|(cached, revision)| cached >= revision)
}

fn fill_cache(
    cache: &DiaryCache,
    account: i64,
    entries: &[DiaryEntry],
    mut fetch: impl FnMut(i64, &[i64]) -> Result<HashMap<i64, FullDiary>, SyncError>,
    mut report: impl FnMut(CacheProgress) -> bool,
) -> Result<CacheProgress, Box<dyn Error>> {
    let initial = !cache.initialized(account)?;
    let revisions = cache.revisions(account)?;
    let mut progress = CacheProgress {
        completed: 0,
        total: 0,
    };
    let mut pending = BTreeMap::<i64, Vec<&DiaryEntry>>::new();
    let mut full = Vec::new();
    for entry in entries {
        let Some(author) = entry.author_id else {
            progress.total += 1;
            continue;
        };
        if entry.has_full_content {
            progress.total += 1;
            full.push((author, entry));
            continue;
        }
        let current = covers(
            revisions.get(&(author, entry.id)).copied(),
            entry.revision.get(),
        );
        if current {
            if initial {
                progress.total += 1;
                progress.completed += 1;
            }
        } else {
            progress.total += 1;
            pending.entry(author).or_default().push(entry);
        }
    }
    if !report(progress) {
        return Ok(progress);
    }

    if !full.is_empty() {
        cache.save_batch(
            account,
            full.iter().map(|(author, entry)| {
                (
                    *author,
                    entry.id,
                    entry.content.as_ref(),
                    entry.revision.get(),
                )
            }),
        )?;
        progress.completed += full.len();
        if !report(progress) {
            return Ok(progress);
        }
    }
    'authors: for (author, entries) in pending {
        for batch in entries.chunks(BATCH_SIZE) {
            let ids: Vec<_> = batch.iter().map(|entry| entry.id).collect();
            let bodies = match fetch(author, &ids) {
                Ok(bodies) => bodies,
                Err(SyncError::Unauthorized | SyncError::RequestFailed) => break 'authors,
                Err(_) if !report(progress) => break 'authors,
                Err(_) => continue,
            };
            let mut saved = 0;
            cache.save_batch(
                account,
                batch.iter().filter_map(|entry| {
                    let full = bodies.get(&entry.id)?;
                    if entry.revision.get().is_none() || full.covers(entry.revision.get()) {
                        saved += 1;
                    }
                    Some((author, entry.id, full.content.as_str(), full.revision))
                }),
            )?;
            progress.completed += saved;
            if !report(progress) {
                return Ok(progress);
            }
        }
    }
    if initial && progress.completed == progress.total {
        cache.complete(account)?;
    }
    Ok(progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, env::temp_dir, fs, os::unix::fs::PermissionsExt, time::SystemTime};

    fn entry(id: i64, author: i64, revision: i64, read: bool, full: bool) -> DiaryEntry {
        DiaryEntry {
            id,
            author_id: Some(author),
            revision: Cell::new(Some(revision)),
            read: Cell::new(Some(read)),
            title: "".into(),
            extras: Default::default(),
            date: String::new(),
            weather_code: None,
            mood_id: None,
            weather: String::new(),
            mood: String::new(),
            content: if full {
                "原始正文\n  ".into()
            } else {
                "摘要".into()
            },
            has_full_content: full,
            is_partner: author != 1,
            gender: None,
        }
    }

    #[test]
    fn stops_between_batches_and_resumes_from_committed_entries() {
        let directory = temp_dir().join(format!("yourdiary-stop-cache-{}", std::process::id()));
        let cache = DiaryCache::at(&directory.join("test.sqlite3")).unwrap();
        let entries: Vec<_> = (1..=64).map(|id| entry(id, 1, 10, false, false)).collect();
        let bodies = |ids: &[i64]| {
            ids.iter()
                .map(|id| {
                    (
                        *id,
                        FullDiary {
                            revision: Some(10),
                            content: "全文".into(),
                        },
                    )
                })
                .collect()
        };
        let first = fill_cache(
            &cache,
            1,
            &entries,
            |_, ids| {
                assert_eq!(ids.first(), Some(&1));
                Ok(bodies(ids))
            },
            |progress| progress.completed < BATCH_SIZE,
        )
        .unwrap();
        assert_eq!((first.completed, first.total), (25, 64));
        let offline = fill_cache(
            &cache,
            1,
            &entries,
            |_, ids| {
                assert_eq!(ids.first(), Some(&26));
                Err(SyncError::RequestFailed)
            },
            |_| true,
        )
        .unwrap();
        assert_eq!((offline.completed, offline.total), (25, 64));
        assert!(!cache.initialized(1).unwrap());
        let mut resumed = Vec::new();
        let complete = fill_cache(
            &cache,
            1,
            &entries,
            |_, ids| {
                resumed.extend_from_slice(ids);
                Ok(bodies(ids))
            },
            |_| true,
        )
        .unwrap();
        assert_eq!(resumed, (26..=64).collect::<Vec<_>>());
        assert_eq!(complete.completed, 64);
        assert!(cache.initialized(1).unwrap());
        drop(cache);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn full_cache_resumes_then_refreshes_stale_revisions_and_preserves_account_boundaries() {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = temp_dir().join(format!("yourdiary-cache-{}-{nonce}", std::process::id()));
        let path = directory.join("yourdiary.sqlite3");
        let cache = DiaryCache::at(&path).unwrap();
        let entries = [
            entry(1, 1, 10, true, true),
            entry(2, 1, 10, false, false),
            entry(3, 2, 10, true, false),
        ];
        let mut reports = Vec::new();
        let progress = fill_cache(
            &cache,
            1,
            &entries,
            |author, ids| {
                if author == 1 {
                    assert_eq!(ids, [2]);
                    Ok(HashMap::from([(
                        2,
                        FullDiary {
                            content: "完整的本人正文".into(),
                            revision: Some(10),
                        },
                    )]))
                } else {
                    assert_eq!((author, ids), (2, &[3][..]));
                    Err(SyncError::RequestFailed)
                }
            },
            |progress| {
                reports.push((progress.completed, progress.total));
                true
            },
        )
        .unwrap();
        assert_eq!(reports, [(0, 3), (1, 3), (2, 3)]);
        assert_eq!((progress.completed, progress.total), (2, 3));
        drop(cache);

        let cache = DiaryCache::at(&path).unwrap();
        let progress = fill_cache(
            &cache,
            1,
            &entries,
            |author, ids| {
                assert_eq!((author, ids), (2, &[3][..]));
                Ok(HashMap::from([(
                    3,
                    FullDiary {
                        content: "完整的对方正文".into(),
                        revision: Some(10),
                    },
                )]))
            },
            |_| true,
        )
        .unwrap();
        assert_eq!((progress.completed, progress.total), (3, 3));
        assert_eq!(
            cache.load(1, 1, 1).unwrap().unwrap().content,
            "原始正文\n  "
        );
        assert!(cache.load(2, 2, 3).unwrap().is_none());
        assert!(cache.load(1, 1, 3).unwrap().is_none());
        cache.confirm_read(1, 2, 3, 10).unwrap();
        cache.confirm_read(1, 2, 3, 9).unwrap();
        assert_eq!(cache.confirmed_since(1, 0).unwrap().get(&(2, 3)), Some(&10));
        assert!(cache.confirmed_since(2, 0).unwrap().is_empty());
        assert!(cache.confirmed_since(1, i64::MAX).unwrap().is_empty());
        let hits = cache.search(1, "对方").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[&(2, 3)].revision, Some(10));
        assert_eq!(hits[&(2, 3)].text, "完整的对方正文");
        cache
            .save(
                1,
                2,
                6,
                &FullDiary {
                    content: format!("{}你好{}", "前".repeat(120), "后".repeat(60)),
                    revision: Some(10),
                },
            )
            .unwrap();
        let hits = cache.search(1, "你好").unwrap();
        let hit = &hits[&(2, 6)];
        assert_eq!(
            hit.text,
            format!("{}你好{}", "前".repeat(18), "后".repeat(28))
        );
        assert!(hit.cut_before && hit.cut_after);
        assert!(cache.search(2, "对方").unwrap().is_empty());
        assert!(cache.search(1, "%").unwrap().is_empty());

        let current = [
            entry(2, 1, 10, false, false),
            entry(3, 2, 12, true, false),
            entry(4, 2, 12, false, false),
        ];
        current[2].read.set(None);
        let progress = fill_cache(
            &cache,
            1,
            &current,
            |author, ids| {
                assert_eq!((author, ids), (2, &[3, 4][..]));
                Ok(ids
                    .iter()
                    .map(|&id| {
                        (
                            id,
                            FullDiary {
                                content: format!("新版关键词{id}"),
                                revision: Some(12),
                            },
                        )
                    })
                    .collect())
            },
            |_| true,
        )
        .unwrap();
        assert_eq!((progress.completed, progress.total), (2, 2));
        assert_eq!(
            cache.search(1, "新版关键词3").unwrap()[&(2, 3)].revision,
            Some(12)
        );
        assert_eq!(current[1].read.get(), Some(true));
        assert_eq!(current[2].read.get(), None);

        let opened = load_diary(Some(&cache), Some(1), &current[0], false, |_, _| {
            panic!("当前缓存无需请求")
        })
        .unwrap();
        assert!(opened.current);
        current[1].revision.set(Some(13));
        let fallback = load_diary(Some(&cache), Some(1), &current[1], false, |_, _| None).unwrap();
        assert!(!fallback.current);
        assert_eq!(fallback.full.content, "新版关键词3");
        let refreshed_body = load_diary(Some(&cache), Some(1), &current[0], true, |_, _| {
            Some(FullDiary {
                content: "手动刷新".into(),
                revision: Some(13),
            })
        })
        .unwrap();
        assert_eq!(refreshed_body.full.content, "手动刷新");
        let opened = load_diary(Some(&cache), Some(1), &current[1], false, |author, id| {
            assert_eq!((author, id), (2, 3));
            Some(FullDiary {
                content: "打开时取得的新版全文".into(),
                revision: Some(13),
            })
        })
        .unwrap();
        assert!(opened.current);
        assert_eq!(
            cache.load(1, 2, 3).unwrap().unwrap().content,
            opened.full.content
        );

        let refreshed = [entry(1, 1, 20, true, true)];
        fill_cache(
            &cache,
            1,
            &refreshed,
            |_, _| panic!("同步已经携带全文"),
            |_| true,
        )
        .unwrap();
        cache
            .save(
                1,
                1,
                1,
                &FullDiary {
                    content: "过期旧版".into(),
                    revision: Some(15),
                },
            )
            .unwrap();
        let saved = cache.load(1, 1, 1).unwrap().unwrap();
        assert_eq!(
            (saved.revision, saved.content.as_str()),
            (Some(20), "原始正文\n  ")
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(cache);
        fs::remove_dir_all(directory).unwrap();
    }
}
