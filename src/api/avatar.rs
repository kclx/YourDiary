use std::{io::Read, time::Duration};

use crate::api::FILE_HOST;
use reqwest::{Url, blocking::Client};

/// 服务端头像通常是文件主机上的相对路径，也接受完整 HTTPS 地址。
pub(crate) fn url(raw: &str) -> Option<Url> {
    let base = Url::parse(FILE_HOST).ok()?;
    let url = base.join(raw.trim()).ok()?;
    (url.scheme() == "https").then_some(url)
}

pub(crate) enum AvatarError {
    Unavailable,
    Invalid,
}

pub(crate) fn load(url: Url) -> Result<Vec<u8>, AvatarError> {
    let response = Client::builder()
        .timeout(Duration::from_secs(10))
        .https_only(true)
        .build()
        .map_err(|_| AvatarError::Invalid)?
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| {
            if super::temporarily_unavailable(&e) {
                AvatarError::Unavailable
            } else {
                AvatarError::Invalid
            }
        })?;
    const LIMIT: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    response
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AvatarError::Unavailable)?;
    (!bytes.is_empty() && bytes.len() as u64 <= LIMIT)
        .then_some(bytes)
        .ok_or(AvatarError::Invalid)
}
