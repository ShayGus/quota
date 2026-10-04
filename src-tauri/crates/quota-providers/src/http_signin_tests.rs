//! The HTTP refusals observed before Muse can return a device code.

use super::*;

#[tokio::test]
async fn browser_rejections_report_only_the_status_and_redirect_host() {
    use std::io::{BufRead, BufReader, Write};

    for (status, location, body, expected) in [
        (
            "302 Found",
            "Location: https://private-user:private-password@www.facebook.com/unsupportedbrowser?code=PRIVATE-CODE#PRIVATE-FRAGMENT\r\n",
            "",
            ProviderError::UnsupportedUserAgent {
                status: 302,
                redirect_host: "www.facebook.com".into(),
            },
        ),
        (
            "200 OK",
            "",
            " \r\n",
            ProviderError::EmptyResponse { status: 200 },
        ),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/device", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(&stream);
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
            }
            write!(stream, "HTTP/1.1 {status}\r\n{location}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let error = ProviderHttp::new()
            .unwrap()
            .get(GetRequest {
                url: &url,
                headers: &[],
                deadline: None,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(error, expected);
        assert!(!error.is_retryable());
        assert!(!error.to_string().contains("PRIVATE"));
        assert!(!error.to_string().contains("private-user"));
        assert!(!error.to_string().contains("private-password"));
        server.join().unwrap();
    }
}
