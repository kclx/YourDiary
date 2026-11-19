use super::{LoginRequest, Session, SessionStore};
use crate::api::{auth, response::login::LoginResponse};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LoginError {
    RequestFailed,
    Rejected(i64),
    MissingToken,
}

pub(crate) fn login_and_save(
    request: LoginRequest,
    store: &SessionStore,
) -> Result<Session, LoginError> {
    let response = auth::login(request).ok_or(LoginError::RequestFailed)?;
    let session = session_from_response(response)?;
    if let Err(error) = store.save(session.token(), session.account_id) {
        eprintln!("无法保存登录会话：{error}");
    }
    Ok(session)
}

fn session_from_response(response: LoginResponse) -> Result<Session, LoginError> {
    if response.error != 0 {
        return Err(LoginError::Rejected(response.error));
    }
    response
        .token
        .filter(|token| !token.is_empty())
        .map(|token| {
            let account = response
                .user_config
                .as_ref()
                .map(|p| p.user.userid)
                .or(response.userid)
                .filter(|id| *id > 0);
            Session::new(token, account, response.user_config.map(Into::into))
        })
        .ok_or(LoginError::MissingToken)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_success_with_a_nonempty_token_creates_a_session() {
        let parse = |json: &str| session_from_response(serde_json::from_str(json).unwrap());
        assert_eq!(
            parse(r#"{"error":1,"token":"sample"}"#).err(),
            Some(LoginError::Rejected(1))
        );
        for json in [r#"{"error":0}"#, r#"{"error":0,"token":""}"#] {
            assert_eq!(parse(json).err(), Some(LoginError::MissingToken));
        }
        let session = parse(r#"{"error":0,"token":"sample"}"#).unwrap();
        assert_eq!(session.token(), "sample");
        assert!(session.profile().is_none());
    }

    #[test]
    fn session_keeps_separate_profiles_and_respects_pair_status() {
        let mut response = serde_json::json!({
            "error": 0,
            "token": "sample",
            "user_config": {
                "userid": 1,
                "name": "本人",
                "avatar": "own-avatar",
                "description": null,
                "paired": true,
                "diary_count": 5,
                "word_count": 12,
                "image_count": 0,
                "is_member": false,
                "enable_treehole": false,
                "paired_user_config": {
                    "userid": 2,
                    "name": "对方",
                    "avatar": "partner-avatar",
                    "description": "示例简介",
                    "role": "sample",
                    "diary_count": 8,
                    "word_count": 50,
                    "image_count": 3,
                    "paired_time": 1,
                    "last_login_time": 2,
                    "ts": 3
                }
            }
        });
        let login =
            |response| session_from_response(serde_json::from_value(response).unwrap()).unwrap();

        let paired = login(response.clone());
        let profile = paired.profile().unwrap();
        let partner = profile.partner.as_ref().unwrap();
        assert!(profile.paired);
        assert_eq!((profile.user.userid, profile.user.diary_count), (1, 5));
        assert_eq!((partner.userid, partner.diary_count), (2, 8));

        response["user_config"]["paired"] = false.into();
        let unpaired = login(response.clone());
        let profile = unpaired.profile().unwrap();
        assert!(!profile.paired);
        assert!(profile.partner.is_none());

        response["user_config"]["paired"] = true.into();
        response["user_config"]["paired_user_config"] = serde_json::json!({});
        let partial = login(response);
        let profile = partial.profile().unwrap();
        assert!(profile.paired);
        assert!(profile.partner.is_none());
    }
}
