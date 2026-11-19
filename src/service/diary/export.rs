use super::{
    DiaryEntry, SyncError,
    cache::BATCH_SIZE,
    mapping::{full_diaries, map_entries},
};
use crate::{
    api::{diary::DiaryClient, response::diary::SyncResponse},
    repo::diary::{DiaryCache, FullDiary},
    service::{auth::Session, profile::Profile},
};
use std::{
    collections::HashMap,
    io::{Cursor, Write},
};
use zip::{ZipWriter, write::SimpleFileOptions};

pub(crate) struct DiaryExport {
    pub zip: Vec<u8>,
    pub count: usize,
    pub unverified: usize,
}

pub(crate) fn export_own(session: &Session) -> Result<DiaryExport, &'static str> {
    let client = DiaryClient::new(session.token());
    let response = client
        .sync()
        .map_err(|_| "无法同步日记列表，请稍后重试。")?;
    let cache = DiaryCache::open().ok();
    build_archive(
        response,
        session.account_id(),
        session.profile(),
        cache.as_ref(),
        |account, ids| full_diaries(client.details(account, ids)?, account, ids),
    )
}

fn build_archive(
    mut response: SyncResponse,
    session_id: Option<i64>,
    profile: Option<&Profile>,
    cache: Option<&DiaryCache>,
    mut fetch: impl FnMut(i64, &[i64]) -> Result<HashMap<i64, FullDiary>, SyncError>,
) -> Result<DiaryExport, &'static str> {
    let diaries = response.diaries.as_mut().ok_or("未取得本人的日记列表。")?;
    let total = diaries.len();
    // 列表展示可以推断作者，导出只接受接口明确给出作者的记录。
    diaries.retain(|d| d.user.as_ref().and_then(|u| u.positive_number()).is_some());
    let unverified = total - diaries.len();
    response.diaries_paired = None;
    let synced = map_entries(response, profile, session_id).map_err(|_| "无法读取日记列表。")?;
    let account = synced.account_id.ok_or("无法确认当前账号。")?;
    if session_id.is_some_and(|id| id != account) {
        return Err("同步账号与登录账号不一致，请重新登录。");
    }
    let own: Vec<DiaryEntry> = synced
        .entries
        .into_iter()
        .rev()
        .filter(|e| !e.is_partner && e.author_id == Some(account))
        .collect();
    if own.is_empty() {
        return Err(if unverified > 0 {
            "没有可确认作者为本人的日记。"
        } else {
            "还没有可导出的本人日记。"
        });
    }

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mut names = HashMap::new();
    let count = own.len();
    let mut remaining = own.into_iter();
    while remaining.len() > 0 {
        let mut batch: Vec<_> = remaining.by_ref().take(BATCH_SIZE).collect();
        let mut missing = Vec::new();
        for e in batch.iter_mut().filter(|e| !e.has_full_content) {
            if let Some(full) = cache.and_then(|db| db.load(account, account, e.id).ok().flatten())
                && full.covers(e.revision.get())
            {
                e.content = full.content.into();
            } else {
                missing.push(e.id);
            }
        }
        if !missing.is_empty() {
            let mut bodies =
                fetch(account, &missing).map_err(|_| "无法取得完整正文，请稍后重试。")?;
            for e in batch.iter_mut().filter(|e| missing.contains(&e.id)) {
                let full = bodies
                    .remove(&e.id)
                    .filter(|full| e.revision.get().is_none() || full.covers(e.revision.get()))
                    .ok_or("部分日记缺少最新全文，请稍后重试。")?;
                if let Some(db) = cache
                    && let Err(err) = db.save(account, account, e.id, &full)
                {
                    eprintln!("无法缓存导出的日记：{err}");
                }
                e.content = full.content.into();
            }
        }
        for e in batch {
            append_entry(&mut zip, &mut names, &e)?;
        }
    }
    Ok(DiaryExport {
        zip: zip
            .finish()
            .map_err(|_| "生成 ZIP 压缩包失败。")?
            .into_inner(),
        count,
        unverified,
    })
}

fn append_entry(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    names: &mut HashMap<String, usize>,
    entry: &DiaryEntry,
) -> Result<(), &'static str> {
    let date = gtk::glib::DateTime::from_iso8601(&format!("{}T00:00:00Z", entry.date), None)
        .and_then(|d| d.format("%Y-%m-%d"))
        .map_err(|_| "日记日期无效，无法导出。")?;
    let title = entry.title.trim();
    let mut stem = date.to_string();
    if !title.is_empty() && title != date.as_str() {
        let safe = title.replace(|c: char| c.is_control() || r#"<>:"/\|?*"#.contains(c), "_");
        // 为扩展名和同名序号预留空间，按 UTF-8 字符边界缩短文件名。
        let safe = &safe[..safe.floor_char_boundary(210)];
        stem = format!("{date} {safe}");
    }
    let key = gtk::glib::normalize(
        gtk::glib::casefold(&stem),
        gtk::glib::NormalizeMode::DefaultCompose,
    );
    let mut number = names.get(key.as_str()).copied().unwrap_or(0) + 1;
    let filename = if number == 1 {
        format!("{stem}.md")
    } else {
        while names.contains_key(&format!("{key} ({number})")) {
            number += 1;
        }
        names.insert(format!("{key} ({number})"), 1);
        format!("{stem} ({number}).md")
    };
    names.insert(key.into(), number);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(filename, options)
        .map_err(|_| "无法添加日记到 ZIP 压缩包。")?;
    let mut document = String::from("---\n");
    for (key, value) in [
        ("title", title),
        ("date", date.as_str()),
        ("weather", &entry.weather),
        ("mood", &entry.mood),
    ] {
        if !value.is_empty() {
            // JSON 引号字符串也是合法的 YAML 标量，保留原标题中的引号和换行。
            document.push_str(&format!(
                "{key}: {}\n",
                serde_json::Value::String(value.to_owned())
            ));
        }
    }
    document.push_str("---\n\n");
    zip.write_all(document.as_bytes())
        .and_then(|_| zip.write_all(entry.content.as_bytes()))
        .map_err(|_| "压缩日记正文失败。")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn export_reads_only_own_full_diaries_in_date_order() {
        let response = || {
            serde_json::from_value(json!({
            "error": 0, "user_config": {"userid": 11},
            "diaries": [
                {"id": 5, "user": 11, "createddate": "2026-03-02", "title": "清晨", "content": "摘", "is_simple": 1, "ts": 80},
                {"id": 4, "user": 11, "createddate": "2026-03-01", "title": "", "content": "第一行\n第二行", "weather": "rainy"},
                {"id": 8, "user": 11, "createddate": "2026-04-24", "title": "2026-04-24", "content": "记录"},
                {"id": 6, "user": 22, "createddate": "2026-03-03", "title": "", "content": "不可导出"},
                {"id": 7, "createddate": "2026-03-04", "title": "", "content": "未知作者"}
            ],
            "diaries_paired": [{"id": 9, "user": 22, "createddate": "2026-03-05", "title": "", "content": "搭档"}]
        })).unwrap()
        };
        let dir = std::env::temp_dir().join(format!("yourdiary-export-{}", std::process::id()));
        let cache = DiaryCache::at(&dir.join("diary.sqlite3")).unwrap();
        cache
            .save(
                22,
                22,
                5,
                &FullDiary {
                    content: "其他账号".into(),
                    revision: Some(81),
                },
            )
            .unwrap();
        cache
            .save(
                11,
                11,
                5,
                &FullDiary {
                    content: "旧版".into(),
                    revision: Some(79),
                },
            )
            .unwrap();
        let export = build_archive(response(), Some(11), None, Some(&cache), |owner, ids| {
            assert_eq!((owner, ids), (11, &[5][..]));
            Ok(HashMap::from([(
                5,
                FullDiary {
                    content: "清晨的全文".into(),
                    revision: Some(80),
                },
            )]))
        })
        .unwrap();
        assert_eq!((export.count, export.unverified), (3, 1));
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(Cursor::new(&export.zip)).unwrap();
        assert_eq!(zip.len(), 3);
        for (i, name, content) in [
            (
                0,
                "2026-03-01.md",
                "---\ndate: \"2026-03-01\"\nweather: \"下雨\"\n---\n\n第一行\n第二行",
            ),
            (
                1,
                "2026-03-02 清晨.md",
                "---\ntitle: \"清晨\"\ndate: \"2026-03-02\"\n---\n\n清晨的全文",
            ),
            (
                2,
                "2026-04-24.md",
                "---\ntitle: \"2026-04-24\"\ndate: \"2026-04-24\"\n---\n\n记录",
            ),
        ] {
            let mut file = zip.by_index(i).unwrap();
            assert_eq!(file.name().unwrap(), name);
            assert_eq!(file.compression(), zip::CompressionMethod::Deflated);
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            assert_eq!(text, content);
        }
        assert_eq!(
            build_archive(response(), Some(11), None, Some(&cache), |_, _| panic!(
                "应命中本人缓存"
            ))
            .unwrap()
            .count,
            3
        );
        assert!(build_archive(response(), Some(22), None, None, |_, _| panic!()).is_err());
        assert!(
            build_archive(response(), Some(11), None, None, |_, _| Ok(HashMap::new())).is_err()
        );
        drop(cache);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn same_day_titles_cannot_create_paths_or_overwrite_an_entry() {
        let diaries: Vec<_> = ["A/B", "a:b", "", "  ", "(2)"]
            .into_iter()
            .enumerate()
            .map(|(i, title)| {
                json!({
                    "id": i + 1, "user": 1, "createddate": "2026-03-01",
                    "title": title, "content": "正文"
                })
            })
            .collect();
        let response = serde_json::from_value(json!({"error": 0, "diaries": diaries})).unwrap();
        let export = build_archive(response, Some(1), None, None, |_, _| panic!()).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(export.zip)).unwrap();
        for (i, expected) in [
            "2026-03-01 A_B.md",
            "2026-03-01 a_b (2).md",
            "2026-03-01.md",
            "2026-03-01 (2).md",
            "2026-03-01 (2) (2).md",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(archive.by_index(i).unwrap().name().unwrap(), *expected);
        }
    }
}
