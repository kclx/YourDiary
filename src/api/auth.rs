use reqwest::blocking::{Client, RequestBuilder};

use crate::api::Endpoint;

use super::{request::LoginRequest, response::login::LoginResponse};

fn login_request(client: &Client, credentials: LoginRequest) -> RequestBuilder {
    Endpoint::Login
        .request(client)
        .multipart(credentials.into_form())
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
    fn login_posts_multipart_to_expected_path() {
        let credentials = LoginRequest::new("example@example.com".into(), "sample".into()).unwrap();
        let request = login_request(&Client::new(), credentials).build().unwrap();
        assert_eq!(request.method(), reqwest::Method::POST);
        assert_eq!(request.url().as_str(), "https://nideriji.cn/api/login/");
        assert!(
            request.headers()[reqwest::header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("multipart/form-data; boundary=")
        );
    }
}
