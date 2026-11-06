//! End-to-end tests of how `Client` maps responses to `transport::Error`, against a one-shot
//! local HTTP server that replies with a canned status and body.

use super::client::{AuthType, ClientBuilder};
use schema::transport::{Error, Transport};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

const DAHUA_SUBSCRIBE_FAULT: &str = r#"<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:wsnt="http://docs.oasis-open.org/wsn/b-2"><s:Body><s:Fault><s:Code><s:Value>s:Sender</s:Value></s:Code><s:Reason><s:Text xml:lang="en">Subscribe Creation Failed</s:Text></s:Reason><s:Detail><wsnt:SubscribeCreationFailedFault/></s:Detail></s:Fault></s:Body></s:Envelope>"#;

const NOT_AUTHORIZED_FAULT: &str = r#"<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:ter="http://www.onvif.org/ver10/error"><s:Body><s:Fault><s:Code><s:Value>s:Sender</s:Value><s:Subcode><s:Value>ter:NotAuthorized</s:Value></s:Subcode></s:Code><s:Reason><s:Text xml:lang="en">Sender not Authorized</s:Text></s:Reason></s:Fault></s:Body></s:Envelope>"#;

const REQUEST: &str =
    r#"<tds:GetDeviceInformation xmlns:tds="http://www.onvif.org/ver10/device/wsdl"/>"#;

async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut received = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = socket.read(&mut chunk).await.unwrap();
        received.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&received);
        if let Some(head_end) = text.find("\r\n\r\n") {
            let content_length = text[..head_end]
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if received.len() >= head_end + 4 + content_length {
                return;
            }
        }
        assert!(n > 0, "client closed before sending the whole request");
    }
}

/// Serves exactly one request with `status` and `body`; returns the URL to post to.
async fn serve_once(status: &'static str, body: &'static str) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!(
        "http://{}/onvif/device_service",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/soap+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    });
    url
}

async fn request_against(status: &'static str, body: &'static str) -> Result<String, Error> {
    let url = serve_once(status, body).await;
    ClientBuilder::new(&url)
        .auth_type(AuthType::UsernameToken)
        .build()
        .request(REQUEST)
        .await
}

fn expect_dahua_fault(result: Result<String, Error>, http_status: u16) {
    match result {
        Err(Error::Fault(fault)) => {
            assert_eq!(fault.code, "s:Sender");
            assert!(fault.subcodes.is_empty());
            assert_eq!(fault.reasons, vec!["Subscribe Creation Failed".to_string()]);
            assert!(fault.is_fault_type("SubscribeCreationFailedFault"));
            assert_eq!(fault.http_status, Some(http_status));
            let rendered = Error::Fault(fault).to_string();
            assert!(rendered.contains("s:Sender"), "{}", rendered);
            assert!(
                rendered.contains("Subscribe Creation Failed"),
                "{}",
                rendered
            );
            assert!(
                rendered.contains("SubscribeCreationFailedFault"),
                "{}",
                rendered
            );
        }
        other => panic!("expected Error::Fault, got {:?}", other),
    }
}

#[tokio::test]
async fn a_non_success_response_with_a_soap_fault_returns_the_fault() {
    expect_dahua_fault(
        request_against("500 Internal Server Error", DAHUA_SUBSCRIBE_FAULT).await,
        500,
    );
}

#[tokio::test]
async fn a_success_response_with_a_soap_fault_returns_the_fault() {
    expect_dahua_fault(request_against("200 OK", DAHUA_SUBSCRIBE_FAULT).await, 200);
}

#[tokio::test]
async fn a_not_authorized_fault_stays_an_authorization_error() {
    let result = request_against("400 Bad Request", NOT_AUTHORIZED_FAULT).await;
    assert!(
        matches!(result, Err(Error::Authorization(_))),
        "{:?}",
        result
    );
}

#[tokio::test]
async fn a_non_success_response_without_a_fault_stays_other() {
    let result = request_against("503 Service Unavailable", "<html>busy</html>").await;
    match result {
        Err(Error::Other(status)) => assert!(status.contains("503"), "{}", status),
        other => panic!("expected Error::Other, got {:?}", other),
    }
}
