use reqwest::{
    Url,
    blocking::{Client, RequestBuilder},
};

const API_HOST: &str = "https://nideriji.cn";
pub(crate) const FILE_HOST: &str = "https://f.nideriji.cn";

/// 已从客户端调用点确认的接口。枚举参数对应 URL 路径和查询参数。
#[allow(dead_code)] // 目录同时收录桌面端尚未接入的接口。
pub(crate) enum Endpoint<'a> {
    // 账号、资料与设置
    Login,
    Signup,
    KnowYourName,
    LostPassword,
    SendSms,
    VerifySms,
    SetName,
    SetAge,
    SetDescription,
    SetRole,
    SetTheme,
    SetUserSettings,
    SetTreeholeEnabled,
    UpdateMemberThemeImage,
    SetPushTokenV2,
    SetPushTokenV3,

    // 同步、日记与图片库
    SyncV2,
    SyncV3,
    DiariesByIds {
        author_id: i64,
    },
    Images {
        user_id: i64,
    },
    DeleteImage {
        user_id: i64,
        image_id: i64,
    },
    UpdateImageUsedTime {
        image_id: i64,
    },
    UpdateReadMark {
        diary_id: i64,
    },
    DeleteDiary {
        diary_id: i64,
    },
    SearchDiaries,
    DiaryMessages {
        diary_id: i64,
    },
    SendDiaryMessage,
    StealSticker,
    WriteDiary,
    WriteDiaryV3,
    UploadAvatar,
    UploadImage,
    UploadSticker,
    StickerByUuid {
        uuid: &'a str,
    },
    StickerByImage {
        user_id: i64,
        image_id: i64,
    },

    // 配对、聊天与举报
    Pair,
    JoinRandomPair,
    CancelRandomPair,
    StopRandomPair,
    Unpair,
    RandomPairHistory,
    SendReconnectRequest {
        target_id: i64,
    },
    CancelReconnectRequest {
        target_id: i64,
    },
    BlockUser,
    UnblockUser,
    Messages {
        target_id: i64,
    },
    SendMessage,
    CheckBotChat,
    SendBotMessage,
    ReportAbuse,

    // 树洞
    TreeholeTags,
    TreeholeReactions,
    NextTreehole,
    AllTreeholes {
        offset: u32,
        sort_type: &'a str,
    },
    TreeholeList {
        offset: u32,
        tab: &'a str,
    },
    TreeholeFollowings {
        offset: u32,
    },
    TreeholeFollowers {
        offset: u32,
    },
    TreeholeTabs,
    TreeholeBanners,
    TreeholeNotifications,
    ReadAllTreeholeNotifications,
    ReadTreeholeNotification {
        notification_id: i64,
    },
    ReadTreehole,
    SendToTreehole,
    LikeTreehole {
        treehole_id: i64,
    },
    UnlikeTreehole {
        treehole_id: i64,
    },
    ReactToTreehole,
    TreeholeComments {
        latest: bool,
        treehole_id: i64,
        offset: u32,
    },
    WriteTreeholeComment,
    LikeTreeholeComment {
        treehole_id: i64,
        comment_id: i64,
    },
    UnlikeTreeholeComment {
        treehole_id: i64,
        comment_id: i64,
    },
    DislikeTreeholeComment {
        treehole_id: i64,
        comment_id: i64,
    },
    UndoDislikeTreeholeComment {
        treehole_id: i64,
        comment_id: i64,
    },
    ReadTreeholeUser,
    TreeholeUserEntries,
    FollowTreeholeUser,
    UnfollowTreeholeUser,
    ReportTreeholeAbuse,

    // 心情、支付、邀请及其他
    MoodList {
        offset: u32,
        artist: bool,
    },
    MoodStatus {
        mood_id: i64,
    },
    LikeMood,
    UnlikeMood,
    SaveMood,
    UnsaveMood,
    MoodImage {
        mood_id: i64,
    },
    CreateMood,
    /// 归档仅确认 WebView 调用的路径和方法，构造请求时需提供已确认的来源地址。
    CreateMoodFromWebView {
        origin_url: &'a Url,
    },
    CreateOrder,
    CheckOrder,
    IapCallback,
    InviteInfo,
    Annual2024,
    TrackEvent,
    Redirect {
        page: &'a str,
    },

    // 资源 URL
    DiaryImage {
        user_id: i64,
        image_id: i64,
        cache_breaker: &'a str,
    },
    TreeholeImage {
        path: &'a str,
        cache_breaker: &'a str,
    },
    TreeholeAvatar {
        id: &'a str,
    },
    TreeholeBackground {
        id: &'a str,
    },
}

impl Endpoint<'_> {
    pub(crate) fn request(self, client: &Client) -> RequestBuilder {
        let get = |path: &str| client.get(format!("{API_HOST}{path}"));
        let post = |path: &str| client.post(format!("{API_HOST}{path}"));
        let file_get = |path: &str| client.get(format!("{FILE_HOST}{path}"));
        let file_post = |path: &str| client.post(format!("{FILE_HOST}{path}"));
        match self {
            Self::Login => post("/api/login/"),
            Self::Signup => post("/api/app_signup/"),
            Self::KnowYourName => get("/api/know_your_name/"),
            Self::LostPassword => post("/api/lost_password/"),
            Self::SendSms => post("/api/send_sms/"),
            Self::VerifySms => post("/api/verify_sms/"),
            Self::SetName => post("/api/set_name/"),
            Self::SetAge => post("/api/set_age/"),
            Self::SetDescription => post("/api/set_description/"),
            Self::SetRole => post("/api/set_role/"),
            Self::SetTheme => post("/api/set_theme/"),
            Self::SetUserSettings => post("/api/set_user_setting/"),
            Self::SetTreeholeEnabled => post("/api/set_enable_treehole/"),
            Self::UpdateMemberThemeImage => post("/api/member/update_theme_image/"),
            Self::SetPushTokenV2 => post("/api/v2/set_pn/"),
            Self::SetPushTokenV3 => post("/api/v3/set_pn/"),
            Self::SyncV2 => post("/api/v2/sync/"),
            Self::SyncV3 => post("/api/v3/sync/"),
            Self::DiariesByIds { author_id } => {
                post(&format!("/api/diary/all_by_ids/{author_id}/"))
            }
            Self::Images { user_id } => get(&format!("/api/images/{user_id}/")),
            Self::DeleteImage { user_id, image_id } => {
                get(&format!("/api/delete_image/{user_id}/{image_id}/"))
            }
            Self::UpdateImageUsedTime { image_id } => {
                get(&format!("/api/update_image_used_time/{image_id}"))
            }
            Self::UpdateReadMark { diary_id } => get(&format!("/api/update_read_mark/{diary_id}/")),
            Self::DeleteDiary { diary_id } => get(&format!("/api/diary/delete/{diary_id}/")),
            Self::SearchDiaries => post("/api/diary/search/"),
            Self::DiaryMessages { diary_id } => client.get(query_url(
                API_HOST,
                "/api/v2/diary/msgs/",
                &[("diary_id", &diary_id.to_string())],
            )),
            Self::SendDiaryMessage => post("/api/v2/diary/msg/"),
            Self::StealSticker => post("/api/v2/steal_sticker/"),
            Self::WriteDiary => post("/api/write/"),
            Self::WriteDiaryV3 => post("/api/v3/diary/write/"),
            Self::UploadAvatar => file_post("/api/upload_avatar/"),
            Self::UploadImage => file_post("/api/upload_image/"),
            Self::UploadSticker => file_post("/api/upload_sticker/"),
            Self::StickerByUuid { uuid } => client.get(file_resource("/api/sticker/", [uuid, ""])),
            Self::StickerByImage { user_id, image_id } => {
                file_get(&format!("/api/sticker/{user_id}/{image_id}/"))
            }
            Self::Pair => post("/api/pair/"),
            Self::JoinRandomPair => get("/api/join_random_pair/"),
            Self::CancelRandomPair => get("/api/cancel_random_pair/"),
            Self::StopRandomPair => get("/api/stop_random_pair/"),
            Self::Unpair => get("/api/unpair/"),
            Self::RandomPairHistory => get("/api/get_random_pair_history/"),
            Self::SendReconnectRequest { target_id } => {
                get(&format!("/api/send_reconnect_request/{target_id}/"))
            }
            Self::CancelReconnectRequest { target_id } => {
                get(&format!("/api/cancel_reconnect_request/{target_id}/"))
            }
            Self::BlockUser => post("/api/block_user/"),
            Self::UnblockUser => post("/api/unblock_user/"),
            Self::Messages { target_id } => client.get(query_url(
                API_HOST,
                "/api/get_message_list/",
                &[("target_id", &target_id.to_string())],
            )),
            Self::SendMessage => post("/api/send_message/"),
            Self::CheckBotChat => get("/api/v2/bot_chat/check/"),
            Self::SendBotMessage => post("/api/v2/bot_chat/send/"),
            Self::ReportAbuse => post("/api/report_abuse/"),
            Self::TreeholeTags => get("/api/treehole/tags/"),
            Self::TreeholeReactions => get("/api/treehole/reactions/"),
            Self::NextTreehole => get("/api/treehole/next/"),
            Self::AllTreeholes { offset, sort_type } => client.get(query_url(
                API_HOST,
                "/api/treehole/all/",
                &[("offset", &offset.to_string()), ("sort_type", sort_type)],
            )),
            Self::TreeholeList { offset, tab } => client.get(query_url(
                API_HOST,
                "/api/v2/treehole/list/",
                &[("offset", &offset.to_string()), ("tab", tab)],
            )),
            Self::TreeholeFollowings { offset } => client.get(query_url(
                API_HOST,
                "/api/v2/treehole/my_followings/",
                &[("offset", &offset.to_string())],
            )),
            Self::TreeholeFollowers { offset } => client.get(query_url(
                API_HOST,
                "/api/v2/treehole/my_followers/",
                &[("offset", &offset.to_string())],
            )),
            Self::TreeholeTabs => get("/api/v2/treehole/tabs/"),
            Self::TreeholeBanners => get("/api/v2/treehole/banners/"),
            Self::TreeholeNotifications => get("/api/treehole/notifications/"),
            Self::ReadAllTreeholeNotifications => get("/api/treehole/read_all_notifications/"),
            Self::ReadTreeholeNotification { notification_id } => get(&format!(
                "/api/treehole/read_notification/{notification_id}/"
            )),
            Self::ReadTreehole => post("/api/treehole/read/"),
            Self::SendToTreehole => post("/api/send_to_treehole/"),
            Self::LikeTreehole { treehole_id } => {
                post(&format!("/api/treehole/like/{treehole_id}/"))
            }
            Self::UnlikeTreehole { treehole_id } => {
                post(&format!("/api/treehole/unlike/{treehole_id}/"))
            }
            Self::ReactToTreehole => post("/api/treehole/react_to/"),
            Self::TreeholeComments {
                latest,
                treehole_id,
                offset,
            } => client
                .get(query_url(
                    API_HOST,
                    "/api/treehole/comments/",
                    &[
                        ("treehole_id", &treehole_id.to_string()),
                        ("offset", &offset.to_string()),
                        ("sort_type", if latest { "latest" } else { "old" }),
                    ],
                ))
                .header(reqwest::header::USER_AGENT, "OhApp/3.7 Platform/Android")
                .header(reqwest::header::ACCEPT, "application/json"),
            Self::WriteTreeholeComment => post("/api/treehole/write_comment/"),
            Self::LikeTreeholeComment {
                treehole_id,
                comment_id,
            } => post(&format!(
                "/api/treehole/comment/like/{treehole_id}/{comment_id}/"
            )),
            Self::UnlikeTreeholeComment {
                treehole_id,
                comment_id,
            } => post(&format!(
                "/api/treehole/comment/unlike/{treehole_id}/{comment_id}/"
            )),
            Self::DislikeTreeholeComment {
                treehole_id,
                comment_id,
            } => post(&format!(
                "/api/treehole/comment/dis/{treehole_id}/{comment_id}/"
            )),
            Self::UndoDislikeTreeholeComment {
                treehole_id,
                comment_id,
            } => post(&format!(
                "/api/treehole/comment/undis/{treehole_id}/{comment_id}/"
            )),
            Self::ReadTreeholeUser => post("/api/treehole/user/read/"),
            Self::TreeholeUserEntries => post("/api/treehole/user/entries/"),
            Self::FollowTreeholeUser => post("/api/v2/treehole/follow/"),
            Self::UnfollowTreeholeUser => post("/api/v2/treehole/unfollow/"),
            Self::ReportTreeholeAbuse => post("/api/report_abuse_treehole/"),
            Self::MoodList { offset, artist } => {
                let mut url = query_url(
                    API_HOST,
                    "/api/v3/mood/list/",
                    &[("offset", &offset.to_string())],
                );
                if artist {
                    url.query_pairs_mut().append_pair("type", "artist");
                }
                client.get(url)
            }
            Self::MoodStatus { mood_id } => get(&format!("/api/v3/mood/status/{mood_id}")),
            Self::LikeMood => post("/api/v3/mood/like/"),
            Self::UnlikeMood => post("/api/v3/mood/unlike/"),
            Self::SaveMood => post("/api/v3/mood/save/"),
            Self::UnsaveMood => post("/api/v3/mood/unsave/"),
            Self::MoodImage { mood_id } => client.get(query_url(
                FILE_HOST,
                "/api/v3/mood/",
                &[("mood_id", &mood_id.to_string())],
            )),
            Self::CreateMood => file_post("/api/v3/mood/create/"),
            Self::CreateMoodFromWebView { origin_url } => client.post(format!(
                "{}/api/v3/mood/create",
                origin_url.origin().ascii_serialization()
            )),
            Self::CreateOrder => post("/api/order/create/"),
            Self::CheckOrder => post("/api/order/check/"),
            Self::IapCallback => post("/api/order/iap_callback/"),
            Self::InviteInfo => get("/api/v3/invite/info/"),
            Self::Annual2024 => get("/api/v2/annual/?year=2024"),
            Self::TrackEvent => file_post("/api/v2/track/event/"),
            Self::Redirect { page } => {
                client.get(query_url(API_HOST, "/api/get_redirect/", &[("page", page)]))
            }
            Self::DiaryImage {
                user_id,
                image_id,
                cache_breaker,
            } => client.get(file_resource(
                "/api/image/",
                [
                    user_id.to_string().as_str(),
                    image_id.to_string().as_str(),
                    cache_breaker,
                ],
            )),
            Self::TreeholeImage {
                path,
                cache_breaker,
            } => client.get(file_resource(
                "/api/treehole/image/",
                path.trim_matches('/').split('/').chain([cache_breaker]),
            )),
            Self::TreeholeAvatar { id } => {
                client.get(file_resource("/api/treehole_avatar/", [id, ""]))
            }
            Self::TreeholeBackground { id } => {
                client.get(file_resource("/api/treehole_background/", [id, ""]))
            }
        }
    }
}

fn query_url(host: &str, path: &str, query: &[(&str, &str)]) -> Url {
    let mut url = Url::parse(&format!("{host}{path}")).expect("固定接口地址");
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    url
}

fn file_resource<'a>(prefix: &str, segments: impl IntoIterator<Item = &'a str>) -> Url {
    let mut url = Url::parse(&format!("{FILE_HOST}{prefix}")).expect("固定资源地址");
    url.path_segments_mut()
        .expect("HTTPS 资源路径")
        .pop_if_empty()
        .extend(segments);
    url
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Method;

    #[test]
    fn route_contracts_build_without_sending_requests() {
        let client = Client::new();
        let origin = Url::parse("https://drawing.example/editor/?mode=new").unwrap();
        for (endpoint, method, url) in [
            (
                Endpoint::Login,
                Method::POST,
                "https://nideriji.cn/api/login/",
            ),
            (
                Endpoint::SyncV3,
                Method::POST,
                "https://nideriji.cn/api/v3/sync/",
            ),
            (
                Endpoint::DiariesByIds { author_id: 42 },
                Method::POST,
                "https://nideriji.cn/api/diary/all_by_ids/42/",
            ),
            (
                Endpoint::UpdateReadMark { diary_id: 8 },
                Method::GET,
                "https://nideriji.cn/api/update_read_mark/8/",
            ),
            (
                Endpoint::UpdateImageUsedTime { image_id: 8 },
                Method::GET,
                "https://nideriji.cn/api/update_image_used_time/8",
            ),
            (
                Endpoint::TreeholeList {
                    offset: 3,
                    tab: "a & b",
                },
                Method::GET,
                "https://nideriji.cn/api/v2/treehole/list/?offset=3&tab=a+%26+b",
            ),
            (
                Endpoint::TreeholeComments {
                    latest: false,
                    treehole_id: 8,
                    offset: 20,
                },
                Method::GET,
                "https://nideriji.cn/api/treehole/comments/?treehole_id=8&offset=20&sort_type=old",
            ),
            (
                Endpoint::StickerByUuid { uuid: "a/b?#" },
                Method::GET,
                "https://f.nideriji.cn/api/sticker/a%2Fb%3F%23/",
            ),
            (
                Endpoint::TreeholeImage {
                    path: "folder/photo",
                    cache_breaker: "v#2",
                },
                Method::GET,
                "https://f.nideriji.cn/api/treehole/image/folder/photo/v%232",
            ),
            (
                Endpoint::MoodList {
                    offset: 0,
                    artist: false,
                },
                Method::GET,
                "https://nideriji.cn/api/v3/mood/list/?offset=0",
            ),
            (
                Endpoint::MoodList {
                    offset: 0,
                    artist: true,
                },
                Method::GET,
                "https://nideriji.cn/api/v3/mood/list/?offset=0&type=artist",
            ),
            (
                Endpoint::CreateMood,
                Method::POST,
                "https://f.nideriji.cn/api/v3/mood/create/",
            ),
            (
                Endpoint::CreateMoodFromWebView {
                    origin_url: &origin,
                },
                Method::POST,
                "https://drawing.example/api/v3/mood/create",
            ),
        ] {
            let request = endpoint.request(&client).build().unwrap();
            assert_eq!(request.method(), method, "{url}");
            assert_eq!(request.url().as_str(), url);
        }
    }
}
