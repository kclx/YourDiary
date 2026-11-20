use std::collections::HashMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

fn read_marks<'de, D: Deserializer<'de>>(
    field: D,
) -> Result<Option<HashMap<String, ApiInteger>>, D::Error> {
    Ok(match Value::deserialize(field)? {
        Value::Object(values) => Some(
            values
                .into_iter()
                .filter_map(|(key, value)| {
                    serde_json::from_value(value).ok().map(|mark| (key, mark))
                })
                .collect(),
        ),
        Value::Array(values) if values.is_empty() => Some(HashMap::new()),
        _ => None,
    })
}

#[derive(Default)]
pub(crate) struct SyncRows {
    pub entries: Vec<SyncDiary>,
    pub skipped: usize,
}

impl<'de> Deserialize<'de> for SyncRows {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        struct RowsVisitor;
        impl<'de> serde::de::Visitor<'de> for RowsVisitor {
            type Value = SyncRows;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an array of diary entries")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<SyncRows, A::Error> {
                let mut rows = SyncRows::default();
                while let Some(raw) = seq.next_element::<Value>()? {
                    match serde_json::from_value::<SyncDiary>(raw) {
                        Ok(diary)
                            if diary.id > 0
                                && diary
                                    .user
                                    .as_ref()
                                    .is_none_or(|id| id.positive_number().is_some()) =>
                        {
                            rows.entries.push(diary)
                        }
                        _ => rows.skipped += 1,
                    }
                }
                Ok(rows)
            }
        }
        input.deserialize_seq(RowsVisitor)
    }
}

#[derive(Deserialize)]
pub(crate) struct SyncResponse {
    #[serde(default)]
    pub error: i64,
    pub diaries: Option<SyncRows>,
    pub diaries_paired: Option<SyncRows>,
    pub user_config: Option<Value>,
    pub saved_moods: Option<Vec<SavedMood>>,
    #[serde(default, deserialize_with = "read_marks")]
    pub read_marks: Option<HashMap<String, ApiInteger>>,
    #[serde(default, deserialize_with = "read_marks")]
    pub own_read_marks: Option<HashMap<String, ApiInteger>>,
}

#[derive(Deserialize)]
pub(crate) struct SyncProfile {
    pub userid: Option<ApiInteger>,
    pub role: Option<String>,
    pub paired: Option<bool>,
    #[serde(default, deserialize_with = "super::optional_object")]
    pub paired_user_config: Option<SyncPartner>,
}

#[derive(Deserialize)]
pub(crate) struct SyncPartner {
    pub userid: Option<ApiInteger>,
    pub role: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq, serde::Serialize, Deserialize)]
pub(crate) struct DiaryExtras {
    pub styles: Option<Value>,
    pub temperature: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct SyncDiary {
    pub id: i64,
    pub user: Option<ApiInteger>,
    pub createddate: String,
    pub createdtime: Option<ApiInteger>,
    pub ts: Option<ApiInteger>,
    pub title: Option<String>,
    pub content: Option<String>,
    #[serde(default, rename = "is_simple", deserialize_with = "super::bool_flag")]
    pub partial: bool,
    #[serde(flatten)]
    pub extras: DiaryExtras,
    pub weather: Option<String>,
    pub mood: Option<String>,
    pub mood_id: Option<ApiInteger>,
}

impl SyncDiary {
    pub fn sort_time(&self) -> Option<i64> {
        self.createdtime
            .as_ref()
            .and_then(ApiInteger::positive_number)
            .or_else(|| self.ts.as_ref().and_then(ApiInteger::positive_number))
    }
}

#[derive(Deserialize)]
pub(crate) struct SavedMood {
    pub id: Option<ApiInteger>,
    pub name: Option<String>,
    pub label: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum ApiInteger {
    Number(i64),
    Text(String),
}

impl ApiInteger {
    pub fn positive_number(&self) -> Option<i64> {
        match self {
            Self::Number(id) => Some(*id),
            Self::Text(id) => id.trim().parse().ok(),
        }
        .filter(|&id| id > 0)
    }
}

#[derive(Deserialize)]
pub(crate) struct DetailResponse {
    #[serde(default)]
    pub error: i64,
    pub diaries: Option<Vec<DetailDiary>>,
}

#[derive(Deserialize)]
pub(crate) struct DetailDiary {
    pub id: ApiInteger,
    pub user: Option<ApiInteger>,
    pub content: Option<String>,
    pub ts: Option<ApiInteger>,
    #[serde(default, rename = "is_simple", deserialize_with = "super::bool_flag")]
    pub partial: bool,
    #[serde(flatten)]
    pub extras: DiaryExtras,
}

#[derive(Deserialize)]
pub(in crate::api) struct WriteResponse {
    pub error: Option<i64>,
    pub diary_id: Option<ApiInteger>,
    pub id: Option<ApiInteger>,
    pub diary: Option<WrittenDiary>,
}

#[derive(Deserialize)]
pub(in crate::api) struct WrittenDiary {
    pub id: Option<ApiInteger>,
}

impl WriteResponse {
    pub(in crate::api) fn saved_id(&self, expected: Option<i64>) -> Option<i64> {
        if self.error.is_some_and(|code| code != 0) {
            return None;
        }
        let ids = [
            self.diary_id.as_ref(),
            self.id.as_ref(),
            self.diary.as_ref().and_then(|d| d.id.as_ref()),
        ];
        if ids.iter().all(Option::is_none) {
            return expected.filter(|_| self.error == Some(0));
        }
        ids.into_iter()
            .flatten()
            .find_map(ApiInteger::positive_number)
            .filter(|id| expected.is_none_or(|expected| expected == *id))
    }
}

#[derive(Deserialize)]
pub(crate) struct StatusResponse {
    pub error: i64,
}

#[derive(Deserialize)]
pub(crate) struct MessagesResponse {
    pub error: i64,
    pub msgs: Option<Vec<DiaryMessage>>,
}

#[derive(Deserialize)]
pub(crate) struct DiaryMessage {
    pub id: Option<ApiInteger>,
    pub user: Option<ApiInteger>,
    pub createdtime: Option<ApiInteger>,
    pub content: Option<String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn write_response_accepts_confirmed_diary_ids() {
        let id = |json| {
            serde_json::from_str::<super::WriteResponse>(json)
                .unwrap()
                .saved_id(None)
        };
        assert_eq!(id(r#"{"diary_id":"23"}"#), Some(23));
        assert_eq!(id(r#"{"id":23}"#), Some(23));
        assert_eq!(id(r#"{"diary":{"id":"23"}}"#), Some(23));
        assert_eq!(id(r#"{"error":0,"id":0}"#), None);
        let edit = |s| {
            serde_json::from_str::<super::WriteResponse>(s)
                .unwrap()
                .saved_id(Some(23))
        };
        assert_eq!(edit(r#"{"error":0}"#), Some(23));
        assert_eq!(edit(r#"{"error":1}"#), None);
        assert_eq!(edit("{}"), None);
        assert_eq!(edit(r#"{"error":0,"id":24}"#), None);
        assert_eq!(edit(r#"{"error":0,"id":0}"#), None);
        assert_eq!(id(r#"{"error":0}"#), None);
    }

    use super::SyncResponse;

    #[test]
    fn sync_accepts_empty_read_mark_lists_and_preserves_missing_marks() {
        for key in ["read_marks", "own_read_marks"] {
            for (wire, expected_count) in [
                (serde_json::json!([]), Some(0)),
                (serde_json::json!({}), Some(0)),
                (serde_json::json!({"7": "123"}), Some(1)),
                (serde_json::Value::Null, None),
                (serde_json::json!([123]), None),
                (serde_json::json!({"1": 10, "2": {}}), Some(1)),
            ] {
                let response: SyncResponse = serde_json::from_value(serde_json::json!({
                    "error": 0, (key): wire
                }))
                .unwrap();
                let (present, missing) = if key == "read_marks" {
                    (response.read_marks, response.own_read_marks)
                } else {
                    (response.own_read_marks, response.read_marks)
                };
                assert_eq!(present.as_ref().map(|m| m.len()), expected_count);
                assert!(missing.is_none());
            }
        }
    }

    #[test]
    fn parses_sync_response_with_extra_fields() {
        let json = r#"{
            "error": 0,
            "diaries": [{
                "id": 42,
                "createddate": "2026-11-16",
                "title": "示例日记",
                "content": "这是虚构内容",
                "weather": "晴",
                "mood": "开心",
                "mood_id": null
            }],
            "images": []
        }"#;
        let response: SyncResponse = serde_json::from_str(json).expect("解析同步响应失败");
        assert_eq!(response.error, 0);
        let diaries = response.diaries.expect("缺少日记数组").entries;
        assert_eq!(diaries.len(), 1);
        let diary = &diaries[0];
        assert_eq!(diary.id, 42);
        assert_eq!(diary.createddate, "2026-11-16");
        assert_eq!(diary.title.as_deref(), Some("示例日记"));
        assert_eq!(diary.content.as_deref(), Some("这是虚构内容"));
        assert_eq!(diary.weather.as_deref(), Some("晴"));
        assert_eq!(diary.mood.as_deref(), Some("开心"));
    }

    #[test]
    fn sync_and_details_decode_summary_flag_by_value() {
        use serde_json::{Value, json};
        for (flag, partial) in [
            (None, false),
            (Some(Value::Null), false),
            (Some(json!(false)), false),
            (Some(json!(0)), false),
            (Some(json!("False")), false),
            (Some(json!("0")), false),
            (Some(json!("unknown")), false),
            (Some(json!([])), false),
            (Some(json!(true)), true),
            (Some(json!("TRUE")), true),
            (Some(json!(1)), true),
            (Some(json!("-1")), true),
        ] {
            let mut row = json!({"id":1, "createddate":"2026-11-20", "content":"正文"});
            if let Some(flag) = flag {
                row["is_simple"] = flag;
                row["title"] = Value::Null;
            }
            let diary: super::SyncDiary = serde_json::from_value(row.clone()).unwrap();
            let detail: super::DetailDiary = serde_json::from_value(row).unwrap();
            assert!(diary.title.is_none());
            assert_eq!((diary.partial, detail.partial), (partial, partial));
        }
    }

    #[test]
    fn preserves_missing_and_empty_diary_lists() {
        let missing: SyncResponse = serde_json::from_str(r#"{"error": 1}"#).unwrap();
        let empty: SyncResponse = serde_json::from_str(r#"{"error": 0, "diaries": []}"#).unwrap();
        assert!(missing.diaries.is_none());
        assert!(
            empty
                .diaries
                .is_some_and(|diaries| diaries.entries.is_empty())
        );
    }
}
