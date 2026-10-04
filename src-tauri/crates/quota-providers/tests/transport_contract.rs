//! Production adapters exercised through the public fixture transport seam.
#![cfg(feature = "test-fixtures")]
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests"
)]
#![expect(
    clippy::expect_used,
    reason = "transport contract assertions must fail loudly"
)]

use std::time::Duration;

use quota_core::ports::Secret;
use quota_domain::provider::ProviderId;
use quota_providers::{ProviderRegistry, retarget};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::test]
async fn external_callers_retarget_get_json_post_and_form_post() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        retarget(&format!(
            "http://{}",
            listener.local_addr().expect("the server address")
        ));
        let server = tokio::spawn(serve(listener));
        let registry = ProviderRegistry::production(quota_providers::secrets::unavailable())
            .expect("the production registry");
        let minimax = registry.provider(ProviderId::Minimax).expect("MiniMax");
        let accounts = minimax
            .discover_with(&Secret::new("fixture-key".to_owned()))
            .await
            .expect("a local MiniMax verification");
        assert_eq!(accounts.len(), 1);
        assert_eq!(
            accounts
                .first()
                .expect("an account")
                .identity
                .plan_label
                .as_deref(),
            Some("fixture-plan")
        );

        let muse = registry.provider(ProviderId::MuseCode).expect("Muse Code");
        let accounts = muse
            .discover_with(&Secret::new(
                r#"{"access_token":"fixture-token"}"#.to_owned(),
            ))
            .await
            .expect("a local Muse verification");
        assert_eq!(accounts.len(), 1);
        assert_eq!(
            accounts
                .first()
                .expect("an account")
                .principal_id
                .as_ref()
                .expect("a principal")
                .as_str(),
            "fixture-user"
        );
        let authorization = muse
            .begin_device_sign_in()
            .await
            .expect("a local device request");
        assert_eq!(authorization.user_code, "FIXTURE");
        assert_eq!(
            authorization.verification_uri,
            "https://auth.meta.com/device"
        );
        server.await.expect("the server completed");
    })
    .await
    .expect("local transport requests complete within five seconds");
}

async fn serve(listener: TcpListener) {
    for (request_line, content_type, body, response) in [
        (
            "GET /v1/token_plan/remains HTTP/1.1\r\n",
            None,
            "",
            r#"{"base_resp":{"status_code":0},"plan_name":"fixture-plan"}"#,
        ),
        (
            "POST /muse-code/key HTTP/1.1\r\n",
            Some("application/json"),
            "{}",
            r#"{"user_id":"fixture-user","is_subs_active":true}"#,
        ),
        (
            "POST /oidc/device/authorization/ HTTP/1.1\r\n",
            Some("application/x-www-form-urlencoded"),
            "client_id=1031625952748946",
            r#"{"device_code":"fixture-device","user_code":"FIXTURE","verification_uri":"https://auth.meta.com/device"}"#,
        ),
    ] {
        let (stream, _) = listener.accept().await.expect("a provider request");
        let mut stream = BufReader::new(stream);
        let mut line = String::new();
        stream.read_line(&mut line).await.expect("the request line");
        assert_eq!(line, request_line);
        let mut length = 0;
        let mut actual_type = None;
        loop {
            line.clear();
            assert!(stream.read_line(&mut line).await.expect("a header") > 0);
            if line == "\r\n" {
                break;
            }
            let (name, value) = line.split_once(':').expect("a header pair");
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().expect("a body length");
            } else if name.eq_ignore_ascii_case("content-type") {
                actual_type = Some(value.trim().to_owned());
            }
        }
        assert_eq!(actual_type.as_deref(), content_type);
        let mut bytes = vec![0; length];
        stream
            .read_exact(&mut bytes)
            .await
            .expect("the request body");
        assert_eq!(bytes, body.as_bytes());
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
            response.len()
        );
        stream
            .get_mut()
            .write_all(reply.as_bytes())
            .await
            .expect("the fixture response");
    }
}
