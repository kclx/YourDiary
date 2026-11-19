pub(crate) mod auth;
pub(crate) mod avatar;
pub(crate) mod diary;
mod endpoint;
pub(crate) mod request;
pub(crate) mod response;

use endpoint::{Endpoint, FILE_HOST};
pub(crate) mod treehole;

fn account_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|hop| {
            hop.error("账号接口拒绝重定向")
        }))
        .build()
        .expect("无法初始化账号请求客户端")
}

pub(super) fn temporarily_unavailable(error: &reqwest::Error) -> bool {
    error.status().map_or_else(
        || error.is_connect() || error.is_timeout() || error.is_body() || error.is_request(),
        |status| status.is_server_error() || status == reqwest::StatusCode::REQUEST_TIMEOUT,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn account_requests_reject_redirects() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            time::Duration,
        };
        let server = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = server.local_addr().unwrap();
        let handler = std::thread::spawn(move || {
            let (mut connection, _) = server.accept().unwrap();
            for line in BufReader::new(&connection).lines() {
                if line.unwrap().is_empty() {
                    break;
                }
            }
            connection.write_all(b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let error = super::account_client()
            .get(format!("http://{address}/"))
            .timeout(Duration::from_secs(3))
            .send()
            .unwrap_err();
        assert!(error.is_redirect());
        handler.join().unwrap();
    }
}
