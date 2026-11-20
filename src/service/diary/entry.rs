use serde::{Deserialize, Serialize};
use std::{cell::Cell, sync::Arc};

/// 从同步响应映射得到的一篇日记的展示数据。
#[derive(Deserialize, Serialize, Clone)]
pub(crate) struct DiaryEntry {
    pub id: i64,
    pub author_id: Option<i64>,
    pub revision: Cell<Option<i64>>,
    pub read: Cell<Option<bool>>,
    pub title: Arc<str>,
    pub date: String,
    #[serde(default)]
    pub weather_code: Option<String>,
    #[serde(default)]
    pub mood_id: Option<i64>,
    pub weather: String,
    pub mood: String,
    #[serde(default)]
    pub extras: crate::api::response::diary::DiaryExtras,
    pub content: Arc<str>,
    pub has_full_content: bool,
    pub is_partner: bool,
    pub gender: Option<DiaryGender>,
}

impl DiaryEntry {
    /// 新版本需重新确认已读。旧正文和本人日记不向服务端发送已读请求。
    pub(crate) fn needs_read_mark(&self, full_revision: Option<i64>) -> bool {
        if !self.is_partner
            || full_revision
                .zip(self.revision.get())
                .is_some_and(|(full, expected)| full < expected)
        {
            return false;
        }
        if full_revision > self.revision.get() {
            self.revision.set(full_revision);
            self.read.set(Some(false));
        }
        self.read.get() != Some(true)
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiaryGender {
    Boy,
    Girl,
}

impl DiaryGender {
    pub(super) fn from_role(role: Option<&str>) -> Option<Self> {
        match role {
            Some("boy") => Some(Self::Boy),
            Some("girl") => Some(Self::Girl),
            _ => None,
        }
    }
}

pub(crate) const WEATHER: [&str; 10] = [
    "sunny",
    "partly-sunny",
    "cloudy",
    "rainy",
    "pouring",
    "lightning-rainy",
    "windy",
    "fog",
    "snowy",
    "hail",
];

pub(crate) fn weather_name(value: &str) -> &str {
    match value.trim() {
        "hail" => "冰雹",
        "windy" => "有风",
        "fog" => "有雾",
        "sunny" => "晴朗",
        "partly-sunny" => "多云转晴",
        "cloudy" => "多云",
        "rainy" => "下雨",
        "snowy" => "下雪",
        "pouring" => "大雨",
        "lightning-rainy" => "雷雨",
        other => other,
    }
}

pub(super) fn legacy_mood_name(value: &str) -> &str {
    match value.trim() {
        "tongue" => "调皮",
        "devil" => "坏笑",
        "happy" => "开心",
        "neutral" => "平静",
        "sad" => "难过",
        "excited" => "兴奋",
        "cool" => "酷",
        "poop" => "糟糕",
        "dead" => "疲惫",
        "normal" => "普通",
        "peaceful" => "安宁",
        "tired" => "累",
        "joyful" => "愉快",
        "thoughtful" => "思考",
        "grateful" => "感激",
        "angry" => "生气",
        other => other,
    }
}

pub(crate) fn default_mood_name(id: i64) -> &'static str {
    match id {
        10006 => "开心",
        10007 => "大笑",
        10008 => "喜欢",
        10009 => "忧郁",
        10010 => "愤怒",
        _ => "未命名心情",
    }
}
