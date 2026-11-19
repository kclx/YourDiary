pub(crate) use crate::api::response::profile::PairedProfile;
use crate::api::response::profile::{ProfileResponse, UserProfile};
use crate::{api::avatar, repo::snapshot::Snapshot};

use serde::Deserialize;

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(from = "ProfileResponse")]
pub(crate) struct Profile {
    #[serde(flatten)]
    pub user: UserProfile,
    pub paired: bool,
    #[serde(rename = "paired_user_config")]
    pub partner: Option<PairedProfile>,
}

impl From<ProfileResponse> for Profile {
    fn from(response: ProfileResponse) -> Self {
        // 配对资料缺失或不完整时，会话仍可用于同步本人的日记。
        let partner = response
            .paired_user_config
            .filter(|_| response.paired)
            .and_then(|data| serde_json::from_value(data).ok());
        Self {
            user: response.user,
            paired: response.paired,
            partner,
        }
    }
}

pub(crate) fn load_avatar(path: &str) -> Option<Vec<u8>> {
    let url = avatar::url(path)?;
    let snapshot = Snapshot::new(url.as_str(), "avatar");
    if let Ok(Some((_, bytes))) = snapshot.read(Some(24 * 60 * 60)) {
        return Some(bytes);
    }
    match avatar::load(url) {
        Ok(bytes) => {
            if let Err(error) = snapshot.write(Some(&bytes)) {
                eprintln!("无法保存头像：{error}");
            }
            Some(bytes)
        }
        Err(avatar::AvatarError::Unavailable) => {
            snapshot.read(None).ok().flatten().map(|(_, bytes)| bytes)
        }
        Err(avatar::AvatarError::Invalid) => None,
    }
}
