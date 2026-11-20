use super::{
    DiaryEntry, DiaryGender, SyncError, SyncedDiaries,
    entry::{default_mood_name, legacy_mood_name, weather_name},
};
use crate::{
    api::response::diary::{ApiInteger, DetailResponse, SyncProfile, SyncResponse},
    repo::diary::FullDiary,
    service::profile::Profile,
};
use serde::Deserialize;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
};

fn read_status(
    marks: Option<&HashMap<String, ApiInteger>>,
    diary_id: i64,
    revision: Option<i64>,
) -> Option<bool> {
    marks.map(|marks| {
        marks
            .get(&diary_id.to_string())
            .and_then(ApiInteger::positive_number)
            .is_some_and(|mark| revision.is_none_or(|revision| mark >= revision))
    })
}

pub(super) fn map_entries(
    response: SyncResponse,
    session_profile: Option<&Profile>,
    session_id: Option<i64>,
) -> Result<SyncedDiaries, SyncError> {
    if response.error != 0 {
        return Err(SyncError::Rejected);
    }
    if response.diaries.is_none() && response.diaries_paired.is_none() {
        return Err(SyncError::MissingDiaries);
    }
    let profile = response
        .user_config
        .as_ref()
        .map(SyncProfile::deserialize)
        .transpose()
        .map_err(|_| SyncError::InvalidResponse)?;
    let profile = profile.as_ref();
    let own_id = profile
        .and_then(|user| user.userid.as_ref())
        .and_then(ApiInteger::positive_number)
        .or_else(|| session_profile.map(|profile| profile.user.userid))
        .or(session_id);
    let own_role = profile
        .and_then(|user| user.role.as_deref())
        .or_else(|| session_profile.and_then(|profile| profile.user.role.as_deref()));
    let paired = profile.and_then(|user| user.paired) != Some(false);
    let partner = profile
        .and_then(|user| user.paired_user_config.as_ref())
        .filter(|_| paired);
    let session_partner = session_profile
        .and_then(|profile| profile.partner.as_ref())
        .filter(|_| paired);
    let partner_id = partner
        .and_then(|user| user.userid.as_ref())
        .and_then(ApiInteger::positive_number)
        .or_else(|| session_partner.map(|user| user.userid));
    let partner_role = partner
        .and_then(|user| user.role.as_deref())
        .or_else(|| session_partner.map(|user| user.role.as_str()));
    let own_gender = DiaryGender::from_role(own_role);
    let partner_gender = DiaryGender::from_role(partner_role);
    let has_partner = paired
        && (partner_id.is_some()
            || profile.and_then(|user| user.paired) == Some(true)
            || session_profile.is_some_and(|profile| profile.paired)
            || response
                .diaries_paired
                .as_ref()
                .is_some_and(|diaries| !diaries.entries.is_empty()));

    let own_rows = response.diaries.unwrap_or_default();
    let partner_rows = response.diaries_paired.unwrap_or_default();
    let skipped = own_rows.skipped + partner_rows.skipped;
    let mut seen = HashSet::new();
    let mut diaries: Vec<_> = partner_rows
        .entries
        .into_iter()
        .map(|diary| (true, diary))
        .chain(own_rows.entries.into_iter().filter_map(|diary| {
            match diary.user.as_ref().and_then(ApiInteger::positive_number) {
                Some(author) if Some(author) == partner_id => Some((true, diary)),
                Some(author) if own_id.is_some() && Some(author) != own_id => None,
                _ => Some((false, diary)),
            }
        }))
        .filter(|(is_partner, diary)| seen.insert((*is_partner, diary.id)))
        .collect();
    diaries.sort_by(|(left_partner, left), (right_partner, right)| {
        right
            .createddate
            .cmp(&left.createddate)
            .then_with(|| left_partner.cmp(right_partner))
            .then_with(|| right.sort_time().cmp(&left.sort_time()))
            .then_with(|| right.id.cmp(&left.id))
    });
    let saved_moods = response.saved_moods.unwrap_or_default();
    let mood_names: HashMap<_, _> = saved_moods
        .iter()
        .filter_map(|mood| {
            let id = mood.id.as_ref()?.positive_number()?;
            let name = [mood.name.as_deref(), mood.label.as_deref()]
                .into_iter()
                .flatten()
                .map(str::trim)
                .find(|name| !name.is_empty())?;
            Some((id, name))
        })
        .collect();
    let entries = diaries
        .into_iter()
        .map(|(is_partner, diary)| {
            let revision = diary.ts.as_ref().and_then(ApiInteger::positive_number);
            let marks = if is_partner {
                response.own_read_marks.as_ref()
            } else {
                response.read_marks.as_ref().filter(|_| has_partner)
            };
            DiaryEntry {
                id: diary.id,
                author_id: diary
                    .user
                    .as_ref()
                    .and_then(ApiInteger::positive_number)
                    .or(if is_partner { partner_id } else { own_id }),
                revision: Cell::new(revision),
                read: Cell::new(read_status(marks, diary.id, revision)),
                extras: diary.extras,
                title: diary.title.unwrap_or_default().into(),
                date: diary.createddate,
                weather: weather_name(diary.weather.as_deref().unwrap_or_default()).into(),
                weather_code: diary.weather,
                mood_id: diary.mood_id.as_ref().and_then(ApiInteger::positive_number),
                mood: match diary.mood_id.as_ref().and_then(ApiInteger::positive_number) {
                    Some(id) => mood_names
                        .get(&id)
                        .copied()
                        .unwrap_or_else(|| default_mood_name(id)),
                    None => legacy_mood_name(diary.mood.as_deref().unwrap_or_default()),
                }
                .into(),
                has_full_content: !diary.partial && diary.content.is_some(),
                content: diary.content.unwrap_or_default().into(),
                is_partner,
                gender: if is_partner {
                    partner_gender
                } else {
                    own_gender
                },
            }
        })
        .collect();
    let mut seen_moods = HashSet::new();
    let moods = saved_moods
        .iter()
        .filter_map(|m| {
            let id = m.id.as_ref()?.positive_number()?;
            seen_moods.insert(id).then(|| {
                (
                    id,
                    mood_names
                        .get(&id)
                        .copied()
                        .unwrap_or_else(|| default_mood_name(id))
                        .to_owned(),
                )
            })
        })
        .collect();
    Ok(SyncedDiaries {
        skipped,
        moods,
        account_id: own_id,
        entries,
        profile: match &response.user_config {
            Some(config) => Profile::deserialize(config).ok(),
            None => session_profile.cloned(),
        },
    })
}

pub(super) fn full_diaries(
    response: DetailResponse,
    author_id: i64,
    diary_ids: &[i64],
) -> Result<HashMap<i64, FullDiary>, SyncError> {
    Ok(full_rows(response, author_id, diary_ids)?
        .map(|(id, full, _)| (id, full))
        .collect())
}

pub(super) fn full_rows<'a>(
    response: DetailResponse,
    author_id: i64,
    diary_ids: &'a [i64],
) -> Result<
    impl Iterator<Item = (i64, FullDiary, crate::api::response::diary::DiaryExtras)> + 'a,
    SyncError,
> {
    if response.error != 0 {
        return Err(SyncError::Rejected);
    }
    Ok(response
        .diaries
        .ok_or(SyncError::MissingDiaries)?
        .into_iter()
        .filter_map(move |diary| {
            let id = diary.id.positive_number()?;
            let author = diary.user.as_ref().and_then(ApiInteger::positive_number);
            if diary.partial
                || !diary_ids.contains(&id)
                || author.is_some_and(|author| author != author_id)
            {
                return None;
            }
            Some((
                id,
                FullDiary {
                    content: diary.content?,
                    revision: diary.ts.as_ref().and_then(ApiInteger::positive_number),
                },
                diary.extras,
            ))
        }))
}

#[cfg(test)]
mod tests {
    use super::{DiaryEntry, DiaryGender, SyncError};
    use serde_json::json;

    fn parse_entries(body: &str) -> Result<Vec<DiaryEntry>, SyncError> {
        super::map_entries(
            serde_json::from_str(body).map_err(|_| SyncError::InvalidResponse)?,
            None,
            None,
        )
        .map(|sync| sync.entries)
    }

    #[test]
    fn sync_exposes_profile_without_requiring_login_data() {
        let mut response = json!({
            "error": 0,
            "diaries": [],
            "user_config": {
                "userid": 42,
                "useremail": "example@example.com",
                "name": "本人",
                "description": null,
                "paired": true,
                "diary_count": 7,
                "word_count": 100,
                "image_count": 0,
                "is_member": false,
                "enable_treehole": true,
                "paired_user_config": {
                    "userid": 99,
                    "name": "对方",
                    "avatar": "sample-avatar",
                    "role": "boy",
                    "diary_count": 2,
                    "word_count": 50,
                    "image_count": 1,
                    "paired_time": 1,
                    "last_login_time": 2,
                    "ts": 3
                }
            }
        });
        let sync = super::map_entries(
            serde_json::from_value(response.clone()).unwrap(),
            None,
            None,
        )
        .unwrap();
        let sync: super::SyncedDiaries =
            serde_json::from_slice(&serde_json::to_vec(&sync).unwrap()).unwrap();
        assert_eq!(sync.account_id, Some(42));
        let profile = sync.profile.unwrap();
        assert!(profile.paired);
        assert_eq!(
            profile.user.useremail.as_deref(),
            Some("example@example.com")
        );
        assert_eq!(profile.user.diary_count, 7);
        assert_eq!(profile.partner.as_ref().unwrap().userid, 99);

        response["user_config"]["paired"] = false.into();
        let sync = super::map_entries(
            serde_json::from_value(response.clone()).unwrap(),
            Some(&profile),
            None,
        )
        .unwrap();
        assert!(
            sync.profile
                .is_some_and(|p| !p.paired && p.partner.is_none())
        );

        response["user_config"] = json!({"userid": 42});
        let sync = super::map_entries(
            serde_json::from_value(response.clone()).unwrap(),
            Some(&profile),
            None,
        )
        .unwrap();
        assert_eq!(sync.account_id, Some(42));
        assert!(sync.profile.is_none());

        response.as_object_mut().unwrap().remove("user_config");
        let sync = super::map_entries(
            serde_json::from_value(response.clone()).unwrap(),
            Some(&profile),
            None,
        )
        .unwrap();
        assert!(sync.profile.is_some_and(|p| p.paired));
        response["user_config"] = json!({"role": "girl"});
        response["diaries"] =
            json!([{"id": 1, "title": "示例", "createddate": "2026-11-19", "content": "摘要"}]);
        let restored =
            super::map_entries(serde_json::from_value(response).unwrap(), None, Some(42)).unwrap();
        assert_eq!(restored.account_id, Some(42));
        assert_eq!(restored.entries[0].author_id, Some(42));
    }

    #[test]
    fn edits_after_the_read_mark_are_unread_for_both_authors() {
        let diary = |id, user, ts| {
            json!({
                "id": id, "user": user, "ts": ts, "createddate": "2026-11-18",
                "title": "示例", "content": "摘要"
            })
        };
        let response = json!({
            "error": 0,
            "user_config": {"userid": 1, "paired": true, "paired_user_config": {"userid": 2}},
            "diaries": [diary(1, 1, 100), diary(2, 1, 100), diary(3, 2, 100), diary(4, 2, 100), diary(5, 2, 100)],
            "read_marks": {"1": 100, "2": 99, "5": 999},
            "own_read_marks": {"3": 99, "4": 101, "2": 999}
        });
        let actual: std::collections::HashMap<_, _> = parse_entries(&response.to_string())
            .unwrap()
            .into_iter()
            .map(|entry| (entry.id, entry.read.get()))
            .collect();
        assert_eq!(
            actual,
            [(1, true), (2, false), (3, false), (4, true), (5, false)]
                .into_iter()
                .map(|(id, read)| (id, Some(read)))
                .collect()
        );
    }

    #[test]
    fn opening_a_newer_partner_revision_requires_a_new_read_mark() {
        let response = json!({
            "error": 0, "user_config": {"userid": 1, "paired": true},
            "diaries": [{"id": 2, "title": "", "createddate": "", "ts": 10}],
            "diaries_paired": [{"id": 3, "title": "", "createddate": "", "ts": 10}],
            "own_read_marks": {"3": 10}
        });
        let entries = parse_entries(&response.to_string()).unwrap();
        let own = &entries[0];
        let partner = &entries[1];
        assert!(!own.needs_read_mark(Some(11)));
        assert_eq!(own.revision.get(), Some(10));
        assert!(!partner.needs_read_mark(Some(10)));
        assert!(partner.needs_read_mark(Some(11)));
        assert_eq!(
            (partner.revision.get(), partner.read.get()),
            (Some(11), Some(false))
        );
        assert!(!partner.needs_read_mark(Some(10)));
    }

    #[test]
    fn sync_resolves_legacy_and_v3_weather_and_moods_in_one_response() {
        let base = json!({
            "id": 42,
            "title": "示例日记",
            "createddate": "2026-11-16",
            "content": "示例内容",
        });
        let variants = [
            (
                json!({"weather": "sunny", "mood": "happy"}),
                ("晴朗", "开心"),
            ),
            (
                json!({"weather": "partly-sunny", "mood": "happy", "mood_id": "123"}),
                ("多云转晴", "专注"),
            ),
            (
                json!({"weather": "lightning-rainy", "mood_id": 124}),
                ("雷雨", "放松"),
            ),
            (json!({"weather": null, "mood_id": 10008}), ("", "喜欢")),
            (
                json!({"weather": "霰", "mood_id": 99999}),
                ("霰", "未命名心情"),
            ),
            (
                json!({"weather": "", "mood": null, "mood_id": null}),
                ("", ""),
            ),
        ];
        let diaries: Vec<_> = variants
            .iter()
            .enumerate()
            .map(|(index, (fields, _))| {
                let mut diary = base.clone();
                diary["id"] = (variants.len() - index).into();
                diary
                    .as_object_mut()
                    .unwrap()
                    .extend(fields.as_object().unwrap().clone());
                diary
            })
            .collect();
        let entries = parse_entries(
            &json!({
                "error": 0,
                "diaries": diaries,
                "saved_moods": [
                    {"id": 123, "name": " 专注 ", "label": "旧名称"},
                    {"id": "124", "name": " ", "label": "放松"}
                ]
            })
            .to_string(),
        )
        .unwrap();

        let actual: Vec<_> = entries
            .iter()
            .map(|entry| (entry.weather.as_str(), entry.mood.as_str()))
            .collect();
        assert_eq!(actual, variants.map(|(_, expected)| expected));
    }

    #[test]
    fn sync_merges_both_shapes_and_orders_each_day_with_own_entries_first() {
        let diary = |id, user, date, time, title| {
            json!({
                "id": id, "user": user, "createddate": date, "createdtime": time,
                "title": title, "content": ""
            })
        };
        let partner = diary(5, 2, "2026-11-18", 999, "对方较晚");
        let mixed = json!({
            "error": 0,
            "user_config": {
                "userid": 1, "role": "girl", "paired": true,
                "paired_user_config": {"userid": "2", "role": "boy"}
            },
            "diaries": [
                diary(1, 1, "2026-11-17", 500, "本人前一天"),
                partner.clone(),
                diary(2, 1, "2026-11-18", 100, "本人较早"),
                diary(7, 2, "2026-11-19", 100, "对方后一天"),
                diary(4, 1, "2026-11-18", 200, "本人较晚"),
                diary(8, 3, "2026-11-20", 200, "其他账号")
            ]
        });
        let mut split = mixed.clone();
        split["diaries_paired"] = json!([partner]);
        for response in [mixed, split] {
            let actual: Vec<_> = parse_entries(&response.to_string())
                .unwrap()
                .into_iter()
                .map(|entry| (entry.title, entry.is_partner, entry.gender.unwrap()))
                .collect();
            let expected = [
                ("对方后一天", true, DiaryGender::Boy),
                ("本人较晚", false, DiaryGender::Girl),
                ("本人较早", false, DiaryGender::Girl),
                ("对方较晚", true, DiaryGender::Boy),
                ("本人前一天", false, DiaryGender::Girl),
            ];
            assert_eq!(
                actual,
                expected.map(|(title, partner, gender)| (
                    std::sync::Arc::<str>::from(title),
                    partner,
                    gender
                ))
            );
        }
    }

    #[test]
    fn sync_distinguishes_failures_from_an_empty_list_and_maps_entries() {
        assert_eq!(
            parse_entries("invalid").err(),
            Some(SyncError::InvalidResponse)
        );
        assert_eq!(
            parse_entries(r#"{"error":1}"#).err(),
            Some(SyncError::Rejected)
        );
        assert_eq!(
            parse_entries(r#"{"error":0}"#).err(),
            Some(SyncError::MissingDiaries)
        );
        assert!(
            parse_entries(r#"{"error":0,"diaries":[]}"#)
                .unwrap()
                .is_empty()
        );

        let entries = parse_entries(
            r#"{
            "error": 0,
            "diaries": [{
                "id": 42,
                "title": "示例日记",
                "createddate": "2026-11-16",
                "weather": "晴",
                "mood": "开心",
                "content": "示例内容"
            }]
        }"#,
        )
        .unwrap();
        let [entry] = entries.as_slice() else {
            panic!("应返回一篇日记");
        };
        assert_eq!(
            (
                &*entry.title,
                &*entry.date,
                &*entry.weather,
                &*entry.mood,
                &*entry.content
            ),
            ("示例日记", "2026-11-16", "晴", "开心", "示例内容")
        );
    }
    #[test]
    fn batch_details_require_requested_ids_and_complete_bodies() {
        let response = r#"{"error":0,"diaries":[{"id":20,"content":"其他日记"},{"id":"21","content":"全文","ts":123},{"id":22,"content":"摘要","is_simple":1},{"id":23},{"id":24,"user":3,"content":"不同作者"}]}"#;
        let mut diaries = super::full_diaries(
            serde_json::from_str(response).unwrap(),
            1,
            &[21, 22, 23, 24],
        )
        .unwrap();
        let diary = diaries.remove(&21).unwrap();
        assert_eq!(diary.content, "全文");
        assert_eq!(diary.revision, Some(123));
        assert!(diaries.is_empty());
    }
}
