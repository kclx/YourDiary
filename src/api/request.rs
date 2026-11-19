use reqwest::blocking::multipart::Form;

pub(crate) struct LoginRequest {
    email: String,
    password: String,
}

impl LoginRequest {
    pub(crate) fn new(email: String, password: String) -> Option<Self> {
        (!email.is_empty() && !password.is_empty()).then_some(Self { email, password })
    }

    pub(super) fn into_form(self) -> Form {
        Form::new()
            .text("email", self.email)
            .text("password", self.password)
    }
}

#[derive(Default)]
pub(super) struct SyncRequest {
    user_config_ts: u64,
    diaries_ts: u64,
    readmark_ts: u64,
    images_ts: u64,
}

impl SyncRequest {
    pub(super) fn into_form(self) -> Form {
        Form::new()
            .text("user_config_ts", self.user_config_ts.to_string())
            .text("diaries_ts", self.diaries_ts.to_string())
            .text("readmark_ts", self.readmark_ts.to_string())
            .text("images_ts", self.images_ts.to_string())
    }
}

pub(super) struct DiariesRequest<'a> {
    pub diary_ids: &'a [i64],
}

impl DiariesRequest<'_> {
    pub(super) fn into_form(self) -> Form {
        let ids = self
            .diary_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        Form::new().text("diary_ids", ids)
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct DiaryWrite {
    pub id: Option<i64>,
    pub title: String,
    pub content: String,
    pub date: String,
    pub weather: Option<String>,
    pub mood_id: Option<i64>,
}

impl DiaryWrite {
    pub(super) fn into_form(self) -> Form {
        let mut form = Form::new()
            .text("content", self.content)
            .text("date", self.date);
        if let Some(id) = self.id {
            form = form.text("id", id.to_string());
        }
        if !self.title.trim().is_empty() {
            form = form.text("title", self.title.trim().to_owned());
        }
        if let Some(weather) = self.weather {
            form = form.text("weather", weather);
        }
        if let Some(id) = self.mood_id {
            form = form.text("mood_id", id.to_string());
        }
        form
    }
}

pub(super) struct DiaryMessageRequest {
    pub diary_id: i64,
    pub content: String,
}

impl DiaryMessageRequest {
    pub(super) fn into_form(self) -> Form {
        Form::new()
            .text("diary_id", self.diary_id.to_string())
            .text("content", self.content)
            .text("comment_type", "0")
    }
}

pub(super) struct TreeholeCommentRequest {
    pub treehole_id: i64,
    pub content: String,
    pub reply_to: Option<i64>,
}

impl TreeholeCommentRequest {
    pub(super) fn into_form(self) -> Form {
        let form = Form::new()
            .text("treehole_id", self.treehole_id.to_string())
            .text("content", self.content)
            .text("comment_type", "0");
        match self.reply_to {
            Some(id) => form.text("reply_to", id.to_string()),
            None => form,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn request_forms_keep_their_wire_fields() {
        let login = LoginRequest::new("a@b.cn".into(), "test".into()).unwrap();
        for (form, fields) in [
            (
                login.into_form(),
                &[("email", "a@b.cn"), ("password", "test")][..],
            ),
            (
                SyncRequest::default().into_form(),
                &[
                    ("user_config_ts", "0"),
                    ("diaries_ts", "0"),
                    ("readmark_ts", "0"),
                    ("images_ts", "0"),
                ][..],
            ),
            (
                DiariesRequest {
                    diary_ids: &[8, 42],
                }
                .into_form(),
                &[("diary_ids", "8,42")][..],
            ),
            (
                DiaryWrite {
                    id: None,
                    title: "  ".into(),
                    content: "第一行\n第二行".into(),
                    date: "2026-11-19".into(),
                    weather: None,
                    mood_id: None,
                }
                .into_form(),
                &[("content", "第一行\n第二行"), ("date", "2026-11-19")][..],
            ),
            (
                DiaryWrite {
                    id: Some(19),
                    title: " 周四 ".into(),
                    content: "下雨了".into(),
                    date: "2026-11-19".into(),
                    weather: Some("rainy".into()),
                    mood_id: Some(42),
                }
                .into_form(),
                &[
                    ("id", "19"),
                    ("title", "周四"),
                    ("content", "下雨了"),
                    ("date", "2026-11-19"),
                    ("weather", "rainy"),
                    ("mood_id", "42"),
                ][..],
            ),
            (
                TreeholeCommentRequest {
                    treehole_id: 17,
                    content: "评论".into(),
                    reply_to: None,
                }
                .into_form(),
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
                .into_form(),
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
                .into_form(),
                &[
                    ("diary_id", "7"),
                    ("content", "留言"),
                    ("comment_type", "0"),
                ][..],
            ),
        ] {
            let mut body = String::new();
            form.into_reader().read_to_string(&mut body).unwrap();
            assert_eq!(body.matches("Content-Disposition:").count(), fields.len());
            for (name, value) in fields {
                assert!(
                    body.contains(&format!("name=\"{name}\"\r\n\r\n{value}\r\n")),
                    "{name}"
                );
            }
        }
    }
}
