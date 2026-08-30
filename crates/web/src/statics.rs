use std::fs;
use std::path::{Path, PathBuf};

pub fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    if url_path.contains('\0') {
        return None;
    }

    let mut candidate = root.to_path_buf();

    for segment in url_path.trim_start_matches('/').split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".."
            || segment.contains('\\')
            || segment.contains(':')
            || segment.starts_with('.')
        {
            return None;
        }
        candidate.push(segment);
    }

    if candidate.is_dir() {
        candidate.push("index.html");
    }

    let real_root = fs::canonicalize(root).ok()?;
    let real_file = fs::canonicalize(&candidate).ok()?;

    real_file
        .starts_with(&real_root)
        .then_some(real_file)
        .filter(|path| path.is_file())
}

pub fn content_type_of(file_name: &str) -> &'static str {
    let lower = file_name.to_ascii_lowercase();
    let extension = lower.rsplit('.').next().unwrap_or("");

    match extension {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "exe" | "msi" => "application/vnd.microsoft.portable-executable",
        "zip" => "application/zip",
        "jar" => "application/java-archive",
        "dmg" => "application/x-apple-diskimage",
        "deb" => "application/vnd.debian.binary-package",
        "appimage" => "application/x-executable",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
        "pdf" => "application/pdf",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub fn cache_policy(content_type: &str) -> &'static str {
    let revalidate = content_type.starts_with("text/")
        || content_type.starts_with("application/json")
        || content_type.starts_with("image/svg");

    if revalidate {
        "no-cache"
    } else {
        "public, max-age=86400"
    }
}
