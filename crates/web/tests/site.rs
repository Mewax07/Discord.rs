use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

use web::SiteConfig;

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("web-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("site")).unwrap();
    std::fs::create_dir_all(dir.join("files")).unwrap();
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
                    .starts_with(&name.to_ascii_lowercase())
            })
            .map(|line| line.split_once(':').unwrap().1.trim().to_string())
    }
}

fn call(addr: SocketAddr, path: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();

    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
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
        body: raw[split + 4..].to_vec(),
    }
}

fn config(dir: &PathBuf) -> SiteConfig {
    SiteConfig::new("127.0.0.1:0", "BadOmen")
        .manifest(dir.join("downloads.json"))
        .public(dir.join("site"))
        .files(dir.join("files"))
}

fn start(dir: &PathBuf) -> SocketAddr {
    web::spawn(config(dir)).unwrap()
}

#[test]
fn serves_the_static_site_from_the_public_folder() {
    let dir = workspace("statics");
    std::fs::write(dir.join("site").join("index.html"), "<h1>BadOmen</h1>").unwrap();
    std::fs::write(dir.join("site").join("style.css"), "body{color:red}").unwrap();
    std::fs::write(dir.join("site").join("app.js"), "console.log(1)").unwrap();
    std::fs::write(dir.join("downloads.json"), r#"{"items":[]}"#).unwrap();

    let addr = start(&dir);

    let page = call(addr, "/");
    assert_eq!(page.status, 200);
    assert_eq!(page.text(), "<h1>BadOmen</h1>");
    assert_eq!(
        page.header("content-type").unwrap(),
        "text/html; charset=utf-8"
    );

    let css = call(addr, "/style.css");
    assert_eq!(css.status, 200);
    assert_eq!(css.text(), "body{color:red}");
    assert_eq!(
        css.header("content-type").unwrap(),
        "text/css; charset=utf-8"
    );

    let js = call(addr, "/app.js");
    assert_eq!(js.status, 200);
    assert_eq!(
        js.header("content-type").unwrap(),
        "text/javascript; charset=utf-8"
    );
}

#[test]
fn falls_back_to_the_built_in_page_without_an_index() {
    let dir = workspace("fallback");
    std::fs::write(
        dir.join("downloads.json"),
        r#"{"items":[{"id":"nouga","name":"Nouga Launcher","file":"launcher.bin"}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("files").join("launcher.bin"), b"PAYLOAD").unwrap();

    let addr = start(&dir);
    let page = call(addr, "/");

    assert_eq!(page.status, 200);
    assert!(page.text().contains("Nouga Launcher"));
}

#[test]
fn serves_declared_files_and_refuses_everything_else() {
    let dir = workspace("serving");
    std::fs::write(dir.join("files").join("launcher.bin"), b"NOUGA-PAYLOAD").unwrap();
    std::fs::write(dir.join("secret.txt"), b"private").unwrap();
    std::fs::write(
        dir.join("downloads.json"),
        r#"{
          "items": [
            {"id":"nouga","name":"Nouga Launcher","version":"1.0.0","file":"launcher.bin",
             "description":"The launcher","platform":"Windows"},
            {"id":"escape","name":"Escape","file":"../secret.txt"},
            {"id":"draft","name":"Draft","file":"launcher.bin","hidden":true}
          ]
        }"#,
    )
    .unwrap();

    let addr = start(&dir);

    let download = call(addr, "/d/nouga");
    assert_eq!(download.status, 200);
    assert_eq!(download.body, b"NOUGA-PAYLOAD");
    assert!(download
        .header("content-disposition")
        .unwrap()
        .contains("launcher.bin"));

    assert_eq!(call(addr, "/d/escape").status, 404);
    assert_eq!(call(addr, "/d/draft").status, 404);
    assert_eq!(call(addr, "/d/unknown").status, 404);
}

#[test]
fn path_traversal_is_rejected_on_both_trees() {
    let dir = workspace("traversal");
    std::fs::write(dir.join("secret.txt"), b"private").unwrap();
    std::fs::write(dir.join("site").join("index.html"), "<h1>site</h1>").unwrap();
    std::fs::write(dir.join("downloads.json"), r#"{"items":[]}"#).unwrap();

    let addr = start(&dir);

    for path in [
        "/../secret.txt",
        "/..%2fsecret.txt",
        "/d/../secret.txt",
        "/site/../../secret.txt",
        "/.env",
    ] {
        let reply = call(addr, path);
        assert_eq!(reply.status, 404, "{path} should not be served");
        assert!(!reply.text().contains("private"), "{path} leaked content");
    }
}

#[test]
fn manifest_endpoint_lists_only_visible_items() {
    let dir = workspace("manifest");
    std::fs::write(dir.join("files").join("a.zip"), b"zip").unwrap();
    std::fs::write(
        dir.join("downloads.json"),
        r#"{"items":[
            {"id":"a","name":"A","file":"a.zip"},
            {"id":"b","name":"B","file":"a.zip","hidden":true}
        ]}"#,
    )
    .unwrap();

    let addr = start(&dir);
    let reply = call(addr, "/downloads.json");
    let parsed = reply.json();

    assert_eq!(reply.status, 200);
    assert_eq!(parsed["items"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["items"][0]["id"].as_str().unwrap(), "a");
    assert_eq!(parsed["items"][0]["size"].as_u64().unwrap(), 3);
    assert_eq!(parsed["items"][0]["url"].as_str().unwrap(), "/d/a");
}

#[test]
fn a_custom_404_page_is_used_when_present() {
    let dir = workspace("notfound");
    std::fs::write(dir.join("site").join("404.html"), "<p>lost</p>").unwrap();
    std::fs::write(dir.join("downloads.json"), r#"{"items":[]}"#).unwrap();

    let addr = start(&dir);
    let reply = call(addr, "/nope");

    assert_eq!(reply.status, 404);
    assert_eq!(reply.text(), "<p>lost</p>");
}

#[test]
fn one_port_serves_both_the_site_and_the_licence_api() {
    let dir = workspace("layered");
    std::fs::write(dir.join("site").join("index.html"), "<h1>BadOmen</h1>").unwrap();
    std::fs::write(dir.join("files").join("launcher.bin"), b"PAYLOAD").unwrap();
    std::fs::write(
        dir.join("downloads.json"),
        r#"{"items":[{"id":"nouga","name":"Nouga Launcher","file":"launcher.bin"}]}"#,
    )
    .unwrap();

    let service = Arc::new(
        licensing::LicenseService::open(dir.join("licenses.json"), dir.join("key.pk8"), 86_400)
            .unwrap(),
    );
    let api = licensing::http::router(
        service,
        Arc::new(licensing::ApiConfig {
            addr: String::new(),
            admin_token: None,
            product: "BadOmen".to_string(),
        }),
    );
    let pages = web::router(Arc::new(config(&dir)));

    let addr = httpd::serve(web::server_config("127.0.0.1:0"), move |request| {
        api(request).unwrap_or_else(|| pages(request))
    })
    .unwrap();

    assert_eq!(call(addr, "/").text(), "<h1>BadOmen</h1>");
    assert_eq!(call(addr, "/health").json()["downloads"], 1);
    assert_eq!(call(addr, "/v1/health").json()["status"], "ok");
    assert_eq!(call(addr, "/v1/public-key").json()["algorithm"], "ed25519");
    assert_eq!(call(addr, "/d/nouga").body, b"PAYLOAD");
}
