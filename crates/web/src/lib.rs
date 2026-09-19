mod account;
mod admin;
mod manifest;
mod oauth;
mod page;
mod render;
mod session;
mod statics;
mod tlshttp;
mod visits;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use httpd::{serve, Request, Response, ServerConfig};
use licensing::LicenseService;
use serde_json::json;

pub use manifest::{human_size, DownloadItem, Manifest};
pub use oauth::OAuthConfig;
pub use statics::content_type_of;
pub use visits::{VisitData, VisitStore};

pub(crate) fn now_secs() -> u64 {
    httpd::now_secs()
}

/// Access control for the admin panel: an IP allowlist plus a password.
#[derive(Clone)]
pub struct AdminConfig {
    pub password: String,
    pub allowed_ips: Vec<String>,
    pub trust_forwarded_for: bool,
}

pub struct SiteConfig {
    pub addr: String,
    pub site_name: String,
    pub tagline: String,
    pub footer: String,
    pub accent: String,
    pub manifest_path: PathBuf,
    pub public_dir: PathBuf,
    pub files_dir: PathBuf,
    pub discord_url: Option<String>,
    pub licenses: Option<Arc<LicenseService>>,
    pub admin: Option<AdminConfig>,
    pub oauth: Option<OAuthConfig>,
    pub visits: Option<Arc<VisitStore>>,
    pub session_secret: Arc<Vec<u8>>,
}

impl SiteConfig {
    pub fn new(addr: impl Into<String>, site_name: impl Into<String>) -> Self {
        let site_name = site_name.into();
        Self {
            addr: addr.into(),
            tagline: format!("Official downloads for {site_name}"),
            footer: format!("{site_name} - telechargements servis directement par le bot"),
            site_name,
            accent: "#cc56dd".to_string(),
            manifest_path: PathBuf::from("data/downloads.json"),
            public_dir: PathBuf::from("public"),
            files_dir: PathBuf::from("files"),
            discord_url: None,
            licenses: None,
            admin: None,
            oauth: None,
            visits: None,
            session_secret: Arc::new(licensing::crypto::random_bytes(32)),
        }
    }

    pub fn tagline(mut self, value: impl Into<String>) -> Self {
        self.tagline = value.into();
        self
    }

    pub fn footer(mut self, value: impl Into<String>) -> Self {
        self.footer = value.into();
        self
    }

    pub fn accent(mut self, value: impl Into<String>) -> Self {
        self.accent = value.into();
        self
    }

    pub fn manifest(mut self, path: impl Into<PathBuf>) -> Self {
        self.manifest_path = path.into();
        self
    }

    pub fn public(mut self, path: impl Into<PathBuf>) -> Self {
        self.public_dir = path.into();
        self
    }

    pub fn files(mut self, path: impl Into<PathBuf>) -> Self {
        self.files_dir = path.into();
        self
    }

    pub fn discord(mut self, url: Option<String>) -> Self {
        self.discord_url = url;
        self
    }

    pub fn licenses(mut self, service: Option<Arc<LicenseService>>) -> Self {
        self.licenses = service;
        self
    }

    pub fn admin(mut self, admin: Option<AdminConfig>) -> Self {
        self.admin = admin;
        self
    }

    pub fn oauth(mut self, oauth: Option<OAuthConfig>) -> Self {
        self.oauth = oauth;
        self
    }

    /// Enable visit counting, persisted to `path`.
    pub fn track_visits(mut self, path: impl Into<PathBuf>) -> Self {
        self.visits = Some(Arc::new(VisitStore::open(path)));
        self
    }

    /// Override the secret used to sign session cookies (32 bytes recommended).
    pub fn session_secret(mut self, secret: Vec<u8>) -> Self {
        if !secret.is_empty() {
            self.session_secret = Arc::new(secret);
        }
        self
    }
}

pub fn router(config: Arc<SiteConfig>) -> impl Fn(&Request) -> Response + Send + Sync + 'static {
    move |request| route(request, &config)
}

pub fn server_config(addr: impl Into<String>) -> ServerConfig {
    ServerConfig::new(addr, "web")
        .max_connections(128)
        .rate_limit(240, 60)
}

pub fn prepare(config: &SiteConfig) -> std::io::Result<()> {
    for directory in [&config.public_dir, &config.files_dir] {
        if !directory.exists() {
            std::fs::create_dir_all(directory)?;
        }
    }
    Ok(())
}

pub fn spawn(config: SiteConfig) -> std::io::Result<SocketAddr> {
    prepare(&config)?;

    let addr = config.addr.clone();
    let handler = router(Arc::new(config));

    serve(server_config(addr), move |request| handler(request))
}

fn route(request: &Request, config: &SiteConfig) -> Response {
    if request.path == "/admin" || request.path.starts_with("/admin/") {
        return admin::handle(request, config);
    }

    if request.path == "/account" || request.path.starts_with("/account/") {
        return account::handle(request, config);
    }

    if request.method != "GET" && request.method != "HEAD" {
        return not_found(config);
    }

    // Count a home-page view once, before static assets can short-circuit it
    // (an index.html in the public folder is served by `asset` below).
    if request.method == "GET" && request.path == "/" {
        if let Some(visits) = &config.visits {
            visits.record_page("/");
        }
    }

    let manifest = Manifest::load(&config.manifest_path);

    if request.path == "/downloads.json" {
        return Response::json(200, &listing(config, &manifest));
    }

    if request.path == "/health" {
        return Response::json(
            200,
            &json!({"status": "ok", "downloads": manifest.visible().len()}),
        );
    }

    if let Some(id) = request.path.strip_prefix("/d/") {
        return download(id, config, &manifest);
    }

    if let Some(response) = asset(&request.path, config) {
        return response;
    }

    if request.path == "/" {
        return Response::html(200, page::index(config, &manifest));
    }

    not_found(config)
}

fn listing(config: &SiteConfig, manifest: &Manifest) -> serde_json::Value {
    let items: Vec<_> = manifest
        .visible()
        .iter()
        .map(|item| {
            json!({
                "id": item.id,
                "name": item.name,
                "version": item.version,
                "platform": item.platform,
                "description": item.description,
                "size": item.size(&config.files_dir),
                "sha256": item.sha256,
                "url": format!("/d/{}", item.id)
            })
        })
        .collect();

    json!({ "site": config.site_name, "items": items })
}

fn asset(path: &str, config: &SiteConfig) -> Option<Response> {
    let file = statics::resolve(&config.public_dir, path)?;
    let name = file.file_name()?.to_string_lossy().to_string();
    let content_type = statics::content_type_of(&name);

    Some(
        Response::stream_file(&file, content_type)?
            .header("Cache-Control", statics::cache_policy(content_type)),
    )
}

fn download(id: &str, config: &SiteConfig, manifest: &Manifest) -> Response {
    let Some(item) = manifest.find(id) else {
        return not_found(config);
    };
    if item.hidden {
        return not_found(config);
    }
    let Some(path) = item.resolve(&config.files_dir) else {
        return not_found(config);
    };

    match Response::file(&path, statics::content_type_of(&item.file), &item.file) {
        Some(response) => {
            if let Some(visits) = &config.visits {
                visits.record_download(&item.id);
            }
            response
        }
        None => not_found(config),
    }
}

fn not_found(config: &SiteConfig) -> Response {
    match statics::resolve(&config.public_dir, "/404.html") {
        Some(file) => Response::stream_file(&file, "text/html; charset=utf-8")
            .map(|response| Response {
                status: 404,
                ..response
            })
            .unwrap_or_else(|| Response::html(404, page::not_found(config))),
        None => Response::html(404, page::not_found(config)),
    }
}

/// Resolve the client IP, honouring `X-Forwarded-For` only when trusted.
pub(crate) fn client_ip(request: &Request, trust_forwarded: bool) -> String {
    if trust_forwarded {
        if let Some(forwarded) = request.header("x-forwarded-for") {
            if let Some(first) = forwarded.split(',').next() {
                let candidate = first.trim();
                if !candidate.is_empty() {
                    return candidate.to_string();
                }
            }
        }
    }
    request.peer.clone()
}

pub(crate) fn is_loopback(ip: &str) -> bool {
    ip == "127.0.0.1" || ip == "::1" || ip.starts_with("127.")
}

/// Parse `application/x-www-form-urlencoded` request bodies.
pub(crate) fn parse_form(body: &[u8]) -> std::collections::HashMap<String, String> {
    let text = String::from_utf8_lossy(body);
    let mut map = std::collections::HashMap::new();
    for pair in text.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(form_decode(key), form_decode(value));
    }
    map
}

/// Read a single parameter out of a raw query string.
pub(crate) fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if form_decode(key) == name {
            return Some(form_decode(value));
        }
    }
    None
}

fn form_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(decoded) => {
                        out.push(decoded);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}
