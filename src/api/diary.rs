use super::{
    Endpoint,
    request::{DiariesRequest, DiaryMessageRequest, DiaryWrite, SyncRequest},
    response::diary::{
        DetailResponse, DiaryMessage, MessagesResponse, StatusResponse, SyncResponse, WriteResponse,
    },
};
use reqwest::{
    StatusCode,
    blocking::{Client, RequestBuilder, Response},
};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SyncError {
    Unauthorized,
    RequestFailed,
    InvalidResponse,
    Rejected,
    MissingDiaries,
}

pub(crate) struct DiaryClient<'a> {
    client: Client,
    token: &'a str,
}

impl<'a> DiaryClient<'a> {
    pub(crate) fn new(token: &'a str) -> Self {
        Self {
            client: super::account_client(),
            token,
        }
    }

    pub(crate) fn details(&self, author_id: i64, ids: &[i64]) -> Result<DetailResponse, SyncError> {
        self.send(
            Endpoint::DiariesByIds { author_id }
                .request(&self.client)
                .multipart(DiariesRequest { diary_ids: ids }.into_form())
                .timeout(Duration::from_secs(20)),
        )
    }

    pub(crate) fn messages(&self, diary_id: i64) -> Result<Vec<DiaryMessage>, SyncError> {
        let response: MessagesResponse = self.send(
            Endpoint::DiaryMessages { diary_id }
                .request(&self.client)
                .header(reqwest::header::USER_AGENT, "OhApp/3.7 Platform/Android")
                .header(reqwest::header::ACCEPT, "application/json")
                .timeout(Duration::from_secs(20)),
        )?;
        if response.error != 0 {
            return Err(SyncError::Rejected);
        }
        response.msgs.ok_or(SyncError::InvalidResponse)
    }

    pub(crate) fn write(&self, diary: DiaryWrite) -> Result<i64, SyncError> {
        let expected_id = diary.id;
        let response: WriteResponse = self.send(
            Endpoint::WriteDiaryV3
                .request(&self.client)
                .header(reqwest::header::USER_AGENT, "OhApp/3.7 Platform/Android")
                .header(reqwest::header::ACCEPT, "application/json")
                .multipart(diary.into_form())
                .timeout(Duration::from_secs(20)),
        )?;
        if response.error != 0 {
            return Err(SyncError::Rejected);
        }
        response
            .created_id()
            .filter(|id| expected_id.is_none_or(|expected| *id == expected))
            .ok_or(SyncError::InvalidResponse)
    }

    pub(crate) fn send_message(&self, diary_id: i64, content: String) -> Result<(), SyncError> {
        let response: StatusResponse = self.send(
            Endpoint::SendDiaryMessage
                .request(&self.client)
                .header(reqwest::header::USER_AGENT, "OhApp/3.7 Platform/Android")
                .header(reqwest::header::ACCEPT, "application/json")
                .multipart(DiaryMessageRequest { diary_id, content }.into_form())
                .timeout(Duration::from_secs(20)),
        )?;
        if response.error == 0 {
            Ok(())
        } else {
            Err(SyncError::Rejected)
        }
    }

    pub(crate) fn mark_read(&self, diary_id: i64) -> Result<(), SyncError> {
        let response: StatusResponse = self.send(
            Endpoint::UpdateReadMark { diary_id }
                .request(&self.client)
                .timeout(Duration::from_secs(15)),
        )?;
        if response.error == 0 {
            Ok(())
        } else {
            Err(SyncError::Rejected)
        }
    }

    pub(crate) fn sync(&self) -> Result<SyncResponse, SyncError> {
        self.send(
            Endpoint::SyncV3
                .request(&self.client)
                .multipart(SyncRequest::default().into_form()),
        )
    }

    fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<T, SyncError> {
        let body = request
            .header("auth", format!("token {}", self.token))
            .send()
            .and_then(Response::error_for_status)
            .and_then(Response::text)
            .map_err(|error| match error.status() {
                Some(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => SyncError::Unauthorized,
                _ if super::temporarily_unavailable(&error) => SyncError::RequestFailed,
                _ => SyncError::Rejected,
            })?;
        serde_json::from_str(&body).map_err(|_| SyncError::InvalidResponse)
    }
}
