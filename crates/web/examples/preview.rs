use std::sync::Arc;

use licensing::{IssueRequest, LicenseService};
use web::{AdminConfig, OAuthConfig, SiteConfig};

fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    // A throwaway licence service seeded with one activated licence so the
    // admin panel and account space have something to show.
    let dir = std::env::temp_dir().join("web-preview");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let service = LicenseService::open(dir.join("licenses.json"), dir.join("key.pk8"), 86_400)
        .expect("licence service");
    let issued = service.issue(
        IssueRequest::new("BadOmen Visuals", "premium")
            .machines(3)
            .duration(Some(30 * 86_400))
            .owner(Some("42".to_string())),
    );
    service.activate(&issued.key, "DESKTOP-PREVIEW-7788").ok();
    println!("demo licence: {}", issued.key);

    let config = SiteConfig::new(addr, "BadOmen")
        .public("public")
        .files("files")
        .manifest("data/downloads.json")
        .track_visits(dir.join("visits.json"))
        .licenses(Some(Arc::new(service)))
        .session_secret(b"preview-secret-please-change".to_vec())
        .admin(Some(AdminConfig {
            password: "preview".to_string(),
            allowed_ips: Vec::new(),
            trust_forwarded_for: false,
        }))
        .oauth(Some(OAuthConfig {
            client_id: std::env::var("DISCORD_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("DISCORD_CLIENT_SECRET").unwrap_or_default(),
            redirect_uri: "http://127.0.0.1:8080/account/callback".to_string(),
        }));

    let local = web::spawn(config).expect("unable to start the preview server");

    println!("preview on http://{local}");
    println!("admin:   http://{local}/admin  (password: preview)");
    println!("account: http://{local}/account");

    loop {
        std::thread::park();
    }
}
