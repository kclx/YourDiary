use super::{DiaryWrite, SyncError, SyncedDiaries, mapping::map_entries, open_diary};
use crate::{api::diary::DiaryClient, service::auth::Session};

pub(crate) fn today() -> String {
    gtk::glib::DateTime::now_local()
        .unwrap()
        .format("%Y-%m-%d")
        .unwrap()
        .into()
}

pub(crate) struct TodayDiary {
    pub entry: DiaryWrite,
    pub moods: Vec<(i64, String)>,
    pub mood_name: String,
}

pub(crate) enum WriteError {
    Retry(&'static str),
    Reload(&'static str),
}

pub(crate) fn load_today(session: &Session, date: &str) -> Result<TodayDiary, &'static str> {
    let response = DiaryClient::new(session.token())
        .sync()
        .map_err(|_| "无法同步今天的日记，请返回后重试")?;
    if response.diaries.is_none() {
        return Err("未取得本人的日记列表，请返回后重试");
    }
    let synced = map_entries(response, session.profile(), session.account_id())
        .map_err(|_| "无法读取日记列表，请返回后重试")?;
    prepare(synced, date, |account, entry| {
        open_diary(session, account, entry, false)
            .filter(|opened| opened.current)
            .map(|opened| opened.full.content)
    })
}

fn prepare(
    synced: SyncedDiaries,
    date: &str,
    full_content: impl FnOnce(Option<i64>, &super::DiaryEntry) -> Option<String>,
) -> Result<TodayDiary, &'static str> {
    let mut entry = DiaryWrite {
        date: date.into(),
        ..Default::default()
    };
    let mut mood_name = String::new();
    if let Some(own) = synced
        .entries
        .into_iter()
        .find(|e| !e.is_partner && e.date == date)
    {
        if own.id <= 0 || own.author_id.is_some() && synced.account_id != own.author_id {
            return Err("无法确认今天这篇日记的作者或编号");
        }
        entry.content = if own.has_full_content {
            own.content.to_string()
        } else {
            full_content(synced.account_id, &own).ok_or("无法取得今天日记的全文，请返回后重试")?
        };
        entry.id = Some(own.id);
        entry.title = own.title.to_string();
        entry.weather = own.weather_code.filter(|s| !s.is_empty());
        entry.mood_id = own.mood_id;
        mood_name = own.mood;
    }
    Ok(TodayDiary {
        entry,
        moods: synced.moods,
        mood_name,
    })
}

fn require_today(date: &str) -> Result<(), WriteError> {
    if date == today() {
        Ok(())
    } else {
        Err(WriteError::Reload("日期已变更，请返回后打开今天的日记"))
    }
}

pub(crate) fn save_today(
    session: &Session,
    original: &DiaryWrite,
    entry: DiaryWrite,
) -> Result<i64, WriteError> {
    if entry.content.trim().is_empty() {
        return Err(WriteError::Retry("请先填写正文"));
    }
    require_today(&entry.date)?;
    if entry.date != original.date || entry.id != original.id {
        return Err(WriteError::Reload("写入目标已变更，请返回后重新打开"));
    }
    let current = load_today(session, &entry.date).map_err(WriteError::Retry)?;
    require_today(&entry.date)?;
    if &current.entry != original {
        return Err(WriteError::Reload(
            "今天的日记已更新，可先复制草稿，再返回重新打开",
        ));
    }
    if let Some(id) = entry.id
        && entry == current.entry
    {
        return Ok(id);
    }
    let id = DiaryClient::new(session.token())
        .write(entry)
        .map_err(|error| {
            WriteError::Retry(match error {
                SyncError::Unauthorized => "登录已失效，请重新登录",
                SyncError::Rejected => "服务端未接受这篇日记",
                _ => "未能确认保存结果，请先刷新日记列表，再决定是否重试",
            })
        })?;
    if let Err(error) = session.snapshot("diary/list").write(None) {
        eprintln!("无法使日记列表缓存失效：{error}");
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_own_today_is_editable_and_requires_full_content() {
        let date = "2026-11-19";
        let own = json!({"id":5,"user":1,"createddate":date,"title":"标题","content":"摘","is_simple":1,"weather":"rainy","mood_id":42});
        let partner = json!({"id":9,"user":2,"createddate":date,"title":"","content":"对方"});
        let yesterday =
            json!({"id":3,"user":1,"createddate":"2026-11-18","title":"","content":"昨天"});
        let sync = |items| {
            map_entries(
                serde_json::from_value(json!({
                    "error":0, "user_config":{"userid":1,"paired_user_config":{"userid":2}},
                    "diaries":items, "diaries_paired":[partner]
                }))
                .unwrap(),
                None,
                None,
            )
            .unwrap()
        };
        let blank = prepare(
            sync(vec![partner.clone(), yesterday.clone()]),
            date,
            |_, _| panic!(),
        )
        .unwrap();
        assert!(blank.entry.id.is_none() && blank.entry.content.is_empty());
        assert!(prepare(sync(vec![own.clone()]), date, |_, _| None).is_err());
        let existing = prepare(sync(vec![yesterday, own]), date, |account, row| {
            assert_eq!((account, row.id), (Some(1), 5));
            Some("完整正文".into())
        })
        .unwrap()
        .entry;
        assert_eq!(
            (
                existing.id,
                existing.content.as_str(),
                existing.weather.as_deref(),
                existing.mood_id
            ),
            (Some(5), "完整正文", Some("rainy"), Some(42))
        );
    }

    #[test]
    fn old_draft_cannot_be_saved_after_midnight() {
        assert!(require_today(&today()).is_ok());
        assert!(matches!(
            require_today("2000-01-01"),
            Err(WriteError::Reload(_))
        ));
    }
}
