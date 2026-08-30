mod manifest;
mod page;
mod statics;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use httpd::{serve, Request, Response, ServerConfig};
use serde_json::json;

pub use manifest::{human_size, DownloadItem, Manifest};
pub use statics::content_type_of;

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
}

impl SiteConfig {
    pub fn new(addr: impl Into<String>, site_name: impl Into<String>) -> Self {
        let site_name = site_name.into();
        Self {
            addr: addr.into(),
            tagline: format!("Official downloads for {site_name}"),
            footer: format!("{site_name} · downloads served directly by the bot"),
            site_name,
            accent: "#8B5CF6".to_string(),
            manifest_path: PathBuf::from("data/downloads.json"),
            public_dir: PathBuf::from("public"),
            files_dir: PathBuf::from("files"),
            discord_url: None,
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
    if request.method != "GET" && request.method != "HEAD" {
        return not_found(config);
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
        Some(response) => response,
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
