use super::SyncError;
use crate::{
    api::{
        diary::DiaryClient,
        response::diary::{ApiInteger, DiaryMessage},
    },
    service::auth::Session,
};

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Message {
    pub sender: Option<i64>,
    pub time: Option<i64>,
    pub text: String,
}

impl Message {
    pub fn is_mine(
        &self,
        account: Option<i64>,
        (author, partner_diary): (Option<i64>, bool),
    ) -> Option<bool> {
        let sender = self.sender?;
        account
            .map(|me| me == sender)
            .or_else(|| author.map(|id| (id == sender) != partner_diary))
    }
}

fn ordered(mut messages: Vec<DiaryMessage>) -> Vec<Message> {
    let number = |value: &Option<ApiInteger>| value.as_ref().and_then(ApiInteger::positive_number);
    let seconds = |value: &Option<ApiInteger>| {
        number(value).map(|t| if t >= 100_000_000_000 { t / 1000 } else { t })
    };
    messages.sort_by_key(|m| (seconds(&m.createdtime), number(&m.id)));
    messages
        .into_iter()
        .map(|m| Message {
            sender: number(&m.user),
            time: seconds(&m.createdtime),
            text: m.content.unwrap_or_default(),
        })
        .collect()
}

pub(crate) fn messages(session: &Session, diary_id: i64) -> Result<Vec<Message>, SyncError> {
    let key = format!("diary/messages/{diary_id}");
    crate::service::cache::refresh(
        &session.snapshot(&key),
        || {
            DiaryClient::new(session.token())
                .messages(diary_id)
                .map(ordered)
        },
        |e| *e == SyncError::RequestFailed,
    )
    .map(|(messages, _)| messages)
}

pub(crate) fn send_message(
    session: &Session,
    diary_id: i64,
    text: String,
) -> Result<(), SyncError> {
    if text.trim().is_empty() {
        return Err(SyncError::Rejected);
    }
    DiaryClient::new(session.token()).send_message(diary_id, text)?;
    let key = format!("diary/messages/{diary_id}");
    if let Err(error) = session.snapshot(&key).write(None) {
        eprintln!("无法清除旧的留言缓存：{error}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_sender_ids_and_sorts_messages_by_time() {
        let msgs=ordered(serde_json::from_str(r#"[{"id":2,"user":"8","createdtime":1700000010000,"content":"后"},{"id":"1","user":3,"createdtime":"1700000000","content":"先"}]"#).unwrap());
        assert_eq!(msgs[0].is_mine(Some(3), (Some(8), true)), Some(true));
        assert_eq!(msgs[1].is_mine(Some(3), (Some(3), false)), Some(false));
        assert_eq!(msgs[1].is_mine(None, (Some(8), true)), Some(false));
        assert_eq!(msgs[1].is_mine(None, (None, false)), None);
        assert_eq!(
            msgs.iter()
                .map(|m| (m.sender, m.time, m.text.as_str()))
                .collect::<Vec<_>>(),
            [
                (Some(3), Some(1700000000), "先"),
                (Some(8), Some(1700000010), "后")
            ]
        );
    }
}
