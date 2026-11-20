use reqwest::blocking::{Client, RequestBuilder};

use crate::api::Endpoint;

use super::{
    request::{LoginRequest, encoded_request},
    response::login::LoginResponse,
};

fn login_request(client: &Client, credentials: LoginRequest) -> RequestBuilder {
    encoded_request(Endpoint::Login.request(client), credentials.encoded_form())
}

pub(crate) fn login(credentials: LoginRequest) -> Option<LoginResponse> {
    let response = login_request(&super::account_client(), credentials)
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    serde_json::from_str(&response.text().ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_posts_urlencoded_to_expected_path() {
        let credentials = LoginRequest::new("example@example.com".into(), "a+b &c".into()).unwrap();
        let request = login_request(&Client::new(), credentials).build().unwrap();
        assert_eq!(request.method(), reqwest::Method::POST);
        assert_eq!(request.url().as_str(), "https://nideriji.cn/api/login/");
        assert_eq!(
            request.headers()[reqwest::header::CONTENT_TYPE],
            "application/x-www-form-urlencoded"
        );
        assert_eq!(
            request.body().unwrap().as_bytes().unwrap(),
            b"email=example%40example.com&password=a%2Bb+%26c"
        );
    }
}
