use std::collections::HashMap;

use serde::{Deserialize, Deserializer, de::IgnoredAny};
use serde_json::Value;

fn is_partial<'de, D: Deserializer<'de>>(field: D) -> Result<bool, D::Error> {
    // 已确认的全文样本省略 is_simple，任何显式值都按需要补全处理。
    IgnoredAny::deserialize(field).map(|_| true)
}

fn read_marks<'de, D: Deserializer<'de>>(
    field: D,
) -> Result<Option<HashMap<String, ApiInteger>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Marks {
        ByDiary(HashMap<String, ApiInteger>),
        Empty([(); 0]),
    }

    Ok(
        Option::<Marks>::deserialize(field)?.map(|marks| match marks {
            Marks::ByDiary(values) => values,
            Marks::Empty([]) => HashMap::new(),
        }),
    )
}

#[derive(Deserialize)]
pub(crate) struct SyncResponse {
    pub error: i64,
    pub diaries: Option<Vec<SyncDiary>>,
    pub diaries_paired: Option<Vec<SyncDiary>>,
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
    pub paired_user_config: Option<SyncPartner>,
}

#[derive(Deserialize)]
pub(crate) struct SyncPartner {
    pub userid: Option<ApiInteger>,
    pub role: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SyncDiary {
    pub id: i64,
    pub user: Option<ApiInteger>,
    pub createddate: String,
    pub createdtime: Option<ApiInteger>,
    pub ts: Option<ApiInteger>,
    pub title: String,
    pub content: Option<String>,
    #[serde(default, rename = "is_simple", deserialize_with = "is_partial")]
    pub partial: bool,
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
    #[serde(default, rename = "is_simple", deserialize_with = "is_partial")]
    pub partial: bool,
}

#[derive(Deserialize)]
pub(in crate::api) struct WriteResponse {
    #[serde(default)]
    pub error: i64,
    pub diary_id: Option<ApiInteger>,
    pub id: Option<ApiInteger>,
    pub diary: Option<WrittenDiary>,
}

#[derive(Deserialize)]
pub(in crate::api) struct WrittenDiary {
    pub id: Option<ApiInteger>,
}

impl WriteResponse {
    pub(in crate::api) fn created_id(&self) -> Option<i64> {
        [
            self.diary_id.as_ref(),
            self.id.as_ref(),
            self.diary.as_ref().and_then(|d| d.id.as_ref()),
        ]
        .into_iter()
        .flatten()
        .find_map(ApiInteger::positive_number)
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
                .created_id()
        };
        assert_eq!(id(r#"{"diary_id":"23"}"#), Some(23));
        assert_eq!(id(r#"{"id":23}"#), Some(23));
        assert_eq!(id(r#"{"diary":{"id":"23"}}"#), Some(23));
        assert_eq!(id(r#"{"error":0,"id":0}"#), None);
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
            assert!(
                serde_json::from_value::<SyncResponse>(serde_json::json!({
                    "error": 0, (key): [123]
                }))
                .is_err()
            );
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
        let diaries = response.diaries.expect("缺少日记数组");
        assert_eq!(diaries.len(), 1);
        let diary = &diaries[0];
        assert_eq!(diary.id, 42);
        assert_eq!(diary.createddate, "2026-11-16");
        assert_eq!(diary.title, "示例日记");
        assert_eq!(diary.content.as_deref(), Some("这是虚构内容"));
        assert_eq!(diary.weather.as_deref(), Some("晴"));
        assert_eq!(diary.mood.as_deref(), Some("开心"));
    }

    #[test]
    fn preserves_missing_and_empty_diary_lists() {
        let missing: SyncResponse = serde_json::from_str(r#"{"error": 1}"#).unwrap();
        let empty: SyncResponse = serde_json::from_str(r#"{"error": 0, "diaries": []}"#).unwrap();
        assert!(missing.diaries.is_none());
        assert!(empty.diaries.is_some_and(|diaries| diaries.is_empty()));
    }
}
