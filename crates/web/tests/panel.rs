use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

use licensing::{IssueRequest, LicenseService};
use web::{AdminConfig, OAuthConfig, SiteConfig};

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("web-panel-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("site")).unwrap();
    std::fs::create_dir_all(dir.join("files")).unwrap();
    std::fs::write(dir.join("downloads.json"), r#"{"items":[]}"#).unwrap();
    dir
}

struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Reply {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap()
    }
    fn header(&self, name: &str) -> Option<String> {
        self.head
            .lines()
            .find(|line| {
                line.to_ascii_lowercase()
                    .starts_with(&format!("{}:", name.to_ascii_lowercase()))
            })
            .map(|line| line.split_once(':').unwrap().1.trim().to_string())
    }
}

fn send(addr: SocketAddr, raw: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    stream.flush().unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let status = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    Reply {
        status,
        head,
        body: buf[split + 4..].to_vec(),
    }
}

fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let cookie_line = cookie
        .map(|c| format!("Cookie: {c}\r\n"))
        .unwrap_or_default();
    let raw =
        format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{cookie_line}Connection: close\r\n\r\n");
    send(addr, &raw)
}

fn post_form(addr: SocketAddr, path: &str, body: &str, cookie: Option<&str>) -> Reply {
    let cookie_line = cookie
        .map(|c| format!("Cookie: {c}\r\n"))
        .unwrap_or_default();
    let raw = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n{cookie_line}Connection: close\r\n\r\n{body}",
        body.len()
    );
    send(addr, &raw)
}

fn post_json(addr: SocketAddr, path: &str, body: &str, cookie: Option<&str>) -> Reply {
    let cookie_line = cookie
        .map(|c| format!("Cookie: {c}\r\n"))
        .unwrap_or_default();
    let raw = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{cookie_line}Connection: close\r\n\r\n{body}",
        body.len()
    );
    send(addr, &raw)
}

fn cookie_value(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_string()
}

fn service(dir: &PathBuf) -> Arc<LicenseService> {
    let service =
        LicenseService::open(dir.join("licenses.json"), dir.join("key.pk8"), 86_400).unwrap();
    // One licence owned by user 42, activated on one machine.
    let issued = service.issue(
        IssueRequest::new("BadOmen Visuals", "premium")
            .machines(2)
            .owner(Some("42".to_string())),
    );
    service
        .activate(&issued.key, "MACHINE-ABCDEF-0001")
        .unwrap();
    Arc::new(service)
}

fn site(dir: &PathBuf, svc: Arc<LicenseService>) -> SiteConfig {
    SiteConfig::new("127.0.0.1:0", "BadOmen")
        .public(dir.join("site"))
        .files(dir.join("files"))
        .manifest(dir.join("downloads.json"))
        .licenses(Some(svc))
        .track_visits(dir.join("visits.json"))
        .session_secret(b"test-secret-that-is-long-enough".to_vec())
        .admin(Some(AdminConfig {
            password: "hunter2secret".to_string(),
            allowed_ips: Vec::new(),
            trust_forwarded_for: false,
        }))
        .oauth(Some(OAuthConfig {
            client_id: "client".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://127.0.0.1:8080/account/callback".to_string(),
        }))
}

#[test]
fn admin_requires_password_then_serves_data_and_actions() {
    let dir = workspace("admin");
    let svc = service(&dir);
    let addr = web::spawn(site(&dir, svc)).unwrap();

    // No cookie: the login page is shown, not the dashboard.
    let anon = get(addr, "/admin", None);
    assert_eq!(anon.status, 200);
    assert!(anon.text().contains("Panneau d'administration"));
    assert!(!anon.text().contains("id=\"stats\""));

    // Wrong password is rejected.
    let bad = post_form(addr, "/admin/login", "password=nope", None);
    assert_eq!(bad.status, 401);

    // Correct password sets a session cookie and redirects.
    let ok = post_form(addr, "/admin/login", "password=hunter2secret", None);
    assert_eq!(ok.status, 302);
    let cookie = cookie_value(&ok.header("set-cookie").expect("session cookie"));
    assert!(cookie.starts_with("bo_admin="));

    // The dashboard now renders.
    let dash = get(addr, "/admin", Some(&cookie));
    assert!(dash.text().contains("id=\"stats\""));

    // Data endpoint returns stats, visits and licences.
    let data = get(addr, "/admin/data", Some(&cookie));
    assert_eq!(data.status, 200);
    let json = data.json();
    assert_eq!(json["stats"]["total"], 1);
    assert_eq!(json["stats"]["active"], 1);
    assert_eq!(json["stats"]["machines"], 1);
    let prefix = json["licenses"][0]["key_prefix"].as_str().unwrap().to_string();
    assert_eq!(json["licenses"][0]["machines_used"], 1);

    // Data endpoint refuses an unauthenticated caller.
    assert_eq!(get(addr, "/admin/data", None).status, 401);

    // Revoke through the panel, then confirm the status flipped.
    let revoked = post_json(
        addr,
        &format!("/admin/license/{prefix}/revoke"),
        r#"{"reason":"test"}"#,
        Some(&cookie),
    );
    assert_eq!(revoked.status, 200);
    assert_eq!(revoked.json()["status"], "revoked");

    let after = get(addr, "/admin/data", Some(&cookie));
    assert_eq!(after.json()["stats"]["revoked"], 1);
}

#[test]
fn admin_data_is_hidden_without_a_password() {
    // A wrong-cookie caller is treated as anonymous.
    let dir = workspace("admin-guard");
    let svc = service(&dir);
    let addr = web::spawn(site(&dir, svc)).unwrap();
    assert_eq!(get(addr, "/admin/data", Some("bo_admin=forged.token")).status, 401);
}

#[test]
fn account_shows_discord_login_when_signed_out() {
    let dir = workspace("account-out");
    let svc = service(&dir);
    let addr = web::spawn(site(&dir, svc)).unwrap();

    let page = get(addr, "/account", None);
    assert_eq!(page.status, 200);
    assert!(page.text().contains("Se connecter avec Discord"));

    // The login route redirects to Discord and plants a signed state cookie.
    let login = get(addr, "/account/login", None);
    assert_eq!(login.status, 302);
    assert!(login.header("location").unwrap().starts_with("https://discord.com/oauth2/authorize"));
    assert!(cookie_value(&login.header("set-cookie").unwrap()).starts_with("bo_state="));
}

#[test]
fn account_callback_rejects_a_forged_state() {
    let dir = workspace("account-state");
    let svc = service(&dir);
    let addr = web::spawn(site(&dir, svc)).unwrap();

    // No matching state cookie -> the callback refuses before any network call.
    let reply = get(addr, "/account/callback?code=abc&state=made-up", None);
    assert_eq!(reply.status, 400);
    assert!(reply.text().contains("expiré"));
}

#[test]
fn visits_are_counted_on_the_home_page() {
    let dir = workspace("visits");
    std::fs::write(dir.join("site").join("index.html"), "<h1>hi</h1>").unwrap();
    let svc = service(&dir);
    let addr = web::spawn(site(&dir, svc)).unwrap();

    get(addr, "/", None);
    get(addr, "/", None);

    let cookie = cookie_value(
        &post_form(addr, "/admin/login", "password=hunter2secret", None)
            .header("set-cookie")
            .unwrap(),
    );
    let data = get(addr, "/admin/data", Some(&cookie));
    assert!(data.json()["visits"]["total"].as_u64().unwrap() >= 2);
}
