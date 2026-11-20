use form_urlencoded::Serializer;

pub(super) fn encoded_request(
    request: reqwest::blocking::RequestBuilder,
    body: String,
) -> reqwest::blocking::RequestBuilder {
    request
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
}

pub(crate) struct LoginRequest {
    email: String,
    password: String,
}

impl LoginRequest {
    pub(crate) fn new(email: String, password: String) -> Option<Self> {
        (!email.is_empty() && !password.is_empty()).then_some(Self { email, password })
    }

    pub(super) fn encoded_form(self) -> String {
        Serializer::new(String::new())
            .append_pair("email", &self.email)
            .append_pair("password", &self.password)
            .finish()
    }
}

pub(super) struct SyncRequest;

impl SyncRequest {
    pub(super) fn encoded_form(self) -> String {
        "diaries_ts=0".into()
    }
}

pub(super) struct DiariesRequest<'a> {
    pub diary_ids: &'a [i64],
}

impl DiariesRequest<'_> {
    pub(super) fn encoded_form(self) -> String {
        let ids = self
            .diary_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        Serializer::new(String::new())
            .append_pair("diary_ids", &ids)
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct DiaryWrite {
    pub extras: super::response::diary::DiaryExtras,
    pub id: Option<i64>,
    pub title: String,
    pub content: String,
    pub date: String,
    pub weather: Option<String>,
    pub mood_id: Option<i64>,
}

impl DiaryWrite {
    pub(super) fn encoded_form(&self) -> String {
        let mut form = form_urlencoded::Serializer::new(String::new());
        form.append_pair("content", &self.content)
            .append_pair("date", &self.date)
            .append_pair("title", self.title.trim());
        if let Some(id) = self.id {
            form.append_pair("id", &id.to_string());
        }
        if let Some(weather) = &self.weather {
            form.append_pair("weather", weather);
        }
        if let Some(id) = self.mood_id {
            form.append_pair("mood_id", &id.to_string());
        }
        for (key, version_key, value) in [
            ("styles", "styles_v", &self.extras.styles),
            ("temperature", "temperature_v", &self.extras.temperature),
        ] {
            if let Some(value) = value {
                if let Some(text) = value.as_str() {
                    form.append_pair(key, text);
                } else {
                    form.append_pair(key, &value.to_string());
                }
                form.append_pair(version_key, "1");
            }
        }
        form.finish()
    }
}

pub(super) struct DiaryMessageRequest {
    pub diary_id: i64,
    pub content: String,
}

impl DiaryMessageRequest {
    pub(super) fn encoded_form(self) -> String {
        Serializer::new(String::new())
            .append_pair("diary_id", &self.diary_id.to_string())
            .append_pair("content", &self.content)
            .append_pair("comment_type", "0")
            .finish()
    }
}

pub(super) struct TreeholeCommentRequest {
    pub treehole_id: i64,
    pub content: String,
    pub reply_to: Option<i64>,
}

impl TreeholeCommentRequest {
    pub(super) fn encoded_form(self) -> String {
        let mut form = Serializer::new(String::new());
        form.append_pair("treehole_id", &self.treehole_id.to_string())
            .append_pair("content", self.content.trim())
            .append_pair("comment_type", "0");
        if let Some(id) = self.reply_to {
            form.append_pair("reply_to", &id.to_string());
        }
        form.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_encodes_empty_title_and_preserves_known_metadata() {
        use std::collections::HashMap;
        let edit = DiaryWrite {
            id: Some(19),
            content: "雨 & 雪+".into(),
            weather: Some("rainy".into()),
            mood_id: Some(42),
            ..Default::default()
        };
        let read = |d: &DiaryWrite| {
            form_urlencoded::parse(d.encoded_form().as_bytes())
                .into_owned()
                .collect::<HashMap<_, _>>()
        };
        let fields = read(&edit);
        assert_eq!(
            (&fields["content"], fields["title"].as_str()),
            (&edit.content, "")
        );
        assert_eq!(
            (
                &fields["id"][..],
                &fields["weather"][..],
                &fields["mood_id"][..]
            ),
            ("19", "rainy", "42")
        );
        assert!(!fields.contains_key("styles_v") && !fields.contains_key("temperature"));
        let mut edit = edit;
        edit.extras.styles = Some(serde_json::json!("[1,2]"));
        edit.extras.temperature = Some(serde_json::json!(12));
        let fields = read(&edit);
        assert_eq!(
            (
                &fields["styles"][..],
                &fields["temperature"][..],
                &fields["styles_v"][..],
                &fields["temperature_v"][..]
            ),
            ("[1,2]", "12", "1", "1")
        );
    }

    #[test]
    fn request_forms_keep_their_wire_fields() {
        let login = LoginRequest::new("a@b.cn".into(), "test".into()).unwrap();
        for (form, fields) in [
            (
                login.encoded_form(),
                &[("email", "a@b.cn"), ("password", "test")][..],
            ),
            (SyncRequest.encoded_form(), &[("diaries_ts", "0")][..]),
            (
                DiariesRequest {
                    diary_ids: &[8, 42],
                }
                .encoded_form(),
                &[("diary_ids", "8,42")][..],
            ),
            (
                TreeholeCommentRequest {
                    treehole_id: 17,
                    content: "评论".into(),
                    reply_to: None,
                }
                .encoded_form(),
                &[
                    ("treehole_id", "17"),
                    ("content", "评论"),
                    ("comment_type", "0"),
                ][..],
            ),
            (
                TreeholeCommentRequest {
                    treehole_id: 17,
                    content: "回复".into(),
                    reply_to: Some(21),
                }
                .encoded_form(),
                &[
                    ("treehole_id", "17"),
                    ("content", "回复"),
                    ("comment_type", "0"),
                    ("reply_to", "21"),
                ][..],
            ),
            (
                DiaryMessageRequest {
                    diary_id: 7,
                    content: "留言".into(),
                }
                .encoded_form(),
                &[
                    ("diary_id", "7"),
                    ("content", "留言"),
                    ("comment_type", "0"),
                ][..],
            ),
        ] {
            let decoded: Vec<_> = form_urlencoded::parse(form.as_bytes()).collect();
            let actual: Vec<_> = decoded
                .iter()
                .map(|(key, value)| (key.as_ref(), value.as_ref()))
                .collect();
            assert_eq!(actual, fields);
        }
    }
}
