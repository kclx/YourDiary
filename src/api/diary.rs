use super::{
    Endpoint,
    request::{DiariesRequest, DiaryMessageRequest, DiaryWrite, SyncRequest, encoded_request},
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
            encoded_request(
                Endpoint::DiariesByIds { author_id }.request(&self.client),
                DiariesRequest { diary_ids: ids }.encoded_form(),
            )
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
            encoded_request(
                Endpoint::WriteDiaryV3.request(&self.client),
                diary.encoded_form(),
            )
            .header(reqwest::header::USER_AGENT, "OhApp/3.7 Platform/Android")
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(Duration::from_secs(20)),
        )?;
        if response.error.is_some_and(|code| code != 0) {
            return Err(SyncError::Rejected);
        }
        response
            .saved_id(expected_id)
            .ok_or(SyncError::InvalidResponse)
    }

    pub(crate) fn send_message(&self, diary_id: i64, content: String) -> Result<(), SyncError> {
        let response: StatusResponse = self.send(
            encoded_request(
                Endpoint::SendDiaryMessage.request(&self.client),
                DiaryMessageRequest { diary_id, content }.encoded_form(),
            )
            .header(reqwest::header::USER_AGENT, "OhApp/3.7 Platform/Android")
            .header(reqwest::header::ACCEPT, "application/json")
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
        self.send(encoded_request(
            Endpoint::SyncV3.request(&self.client),
            SyncRequest.encoded_form(),
        ))
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
