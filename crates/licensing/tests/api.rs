use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

use licensing::{ApiConfig, IssueRequest, LicenseService};
use serde_json::Value;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("licensing-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn service(name: &str) -> Arc<LicenseService> {
    let dir = temp_dir(name);
    Arc::new(LicenseService::open(dir.join("licenses.json"), dir.join("key.pk8"), 86_400).unwrap())
}

fn call(
    addr: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&str>,
    bearer: Option<&str>,
) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).unwrap();
    let payload = body.unwrap_or("");

    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if let Some(token) = bearer {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
        request.push_str(&format!("Content-Length: {}\r\n", payload.len()));
    }
    request.push_str("\r\n");
    request.push_str(payload);

    stream.write_all(request.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();

    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let status = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();

    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

#[test]
fn activation_flow_binds_hardware_and_survives_offline() {
    let service = service("flow");
    let addr = licensing::spawn_api(
        service.clone(),
        ApiConfig {
            addr: "127.0.0.1:0".to_string(),
            admin_token: Some("test-token-with-enough-length".to_string()),
            product: "BadOmen".to_string(),
        },
    )
    .unwrap();

    let issued = service.issue(
        IssueRequest::new("BadOmen", "monthly")
            .duration(Some(30 * 86_400))
            .machines(1),
    );
    let key = issued.key.clone();
    let prefix = issued.license.key_prefix.clone();

    assert!(service.get(&key).is_some());
    assert_eq!(
        service.resolve(&prefix).unwrap().key_hash,
        issued.license.key_hash
    );

    let (status, activated) = call(
        addr,
        "POST",
        "/v1/activate",
        Some(&format!(
            "{{\"key\":\"{key}\",\"hwid\":\"MACHINE-AAAA-1111\"}}"
        )),
        None,
    );
    assert_eq!(status, 200);

    let token = activated["token"].as_str().unwrap().to_string();
    assert!(activated["expires_at"].as_u64().unwrap() > 0);

    let claims = service.verify_token(&token).unwrap();
    assert_eq!(claims.key, key);
    assert_eq!(claims.hwid, "MACHINE-AAAA-1111");

    let mut tampered = token.clone();
    tampered.replace_range(0..1, "Z");
    assert!(service.verify_token(&tampered).is_err());

    let (status, _) = call(
        addr,
        "POST",
        "/v1/activate",
        Some(&format!(
            "{{\"key\":\"{key}\",\"hwid\":\"MACHINE-BBBB-2222\"}}"
        )),
        None,
    );
    assert_eq!(status, 403);

    let (status, _) = call(
        addr,
        "POST",
        &format!("/v1/admin/licenses/{prefix}/revoke"),
        Some("{\"reason\":\"test\"}"),
        Some("test-token-with-enough-length"),
    );
    assert_eq!(status, 200);
    assert!(service.verify_token(&token).is_err());
}

#[test]
fn admin_endpoints_require_the_token() {
    let service = service("auth");
    let addr = licensing::spawn_api(
        service,
        ApiConfig {
            addr: "127.0.0.1:0".to_string(),
            admin_token: Some("another-token-long-enough-here".to_string()),
            product: "BadOmen".to_string(),
        },
    )
    .unwrap();

    let (status, _) = call(addr, "GET", "/v1/admin/stats", None, None);
    assert_eq!(status, 401);

    let (status, _) = call(
        addr,
        "GET",
        "/v1/admin/stats",
        None,
        Some("wrong-token-value-padding"),
    );
    assert_eq!(status, 401);

    let (status, stats) = call(
        addr,
        "GET",
        "/v1/admin/stats",
        None,
        Some("another-token-long-enough-here"),
    );
    assert_eq!(status, 200);
    assert_eq!(stats["total"].as_u64().unwrap(), 0);
}

#[test]
fn unknown_key_is_rejected() {
    let service = service("unknown");
    let addr = licensing::spawn_api(
        service,
        ApiConfig {
            addr: "127.0.0.1:0".to_string(),
            admin_token: None,
            product: "BadOmen".to_string(),
        },
    )
    .unwrap();

    let (status, body) = call(
        addr,
        "POST",
        "/v1/activate",
        Some("{\"key\":\"BDM-00000-00000-00000-00000\",\"hwid\":\"MACHINE-CCCC-3333\"}"),
        None,
    );

    assert_eq!(status, 404);
    assert_eq!(body["error"].as_str().unwrap(), "unknown_key");
}
