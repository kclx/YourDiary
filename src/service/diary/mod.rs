mod cache;
mod comments;
mod entry;
mod export;
mod mapping;
pub(crate) use comments::{Message, messages, send_message};
mod sync;

pub(crate) use crate::api::{diary::SyncError, request::DiaryWrite};
pub(crate) use entry::{WEATHER, default_mood_name, weather_name};
pub(crate) use export::export_own;

mod writing;
pub(crate) use crate::repo::diary::{cached_body, search_cached};
pub(crate) use cache::open_diary;
pub(crate) use entry::{DiaryEntry, DiaryGender};
pub(crate) use sync::{CacheProgress, SyncEvent, mark_read, start_sync};
pub(crate) use writing::{TodayDiary, WriteError, load_today, save_today, today};

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct SyncedDiaries {
    #[serde(default)]
    pub moods: Vec<(i64, String)>,
    pub account_id: Option<i64>,
    pub entries: Vec<DiaryEntry>,
    /// 同步返回的资料。未返回 user_config 时沿用本次会话的登录资料。
    pub profile: Option<crate::service::profile::Profile>,
}

pub(crate) fn adjacent_index(index: usize, offset: isize, len: usize) -> Option<usize> {
    index
        .checked_add_signed(offset)
        .filter(|&index| index < len)
}

#[cfg(test)]
mod tests {
    use super::adjacent_index;

    #[test]
    fn adjacent_index_stays_within_the_list() {
        assert_eq!(adjacent_index(0, -1, 2), None);
        assert_eq!(adjacent_index(0, 1, 2), Some(1));
        assert_eq!(adjacent_index(1, -1, 2), Some(0));
        assert_eq!(adjacent_index(1, 1, 2), None);
        assert_eq!(adjacent_index(0, 1, 1), None);
    }
}
