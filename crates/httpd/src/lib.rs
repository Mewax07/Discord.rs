use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

const MAX_HEAD: usize = 16 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub peer: String,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    pub fn bearer(&self) -> Option<&str> {
        self.header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
    }

    pub fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }

    pub fn segments(&self) -> Vec<&str> {
        self.path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect()
    }

    pub fn is(&self, method: &str, path: &str) -> bool {
        self.method == method && self.path == path
    }
}

pub enum Body {
    Bytes(Vec<u8>),
    Stream { path: PathBuf, len: u64 },
}

impl Body {
    pub fn len(&self) -> u64 {
        match self {
            Body::Bytes(bytes) => bytes.len() as u64,
            Body::Stream { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    pub fn bytes(status: u16, body: Vec<u8>, content_type: &str) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body: Body::Bytes(body),
        }
    }

    pub fn stream_file(path: &Path, content_type: &str) -> Option<Self> {
        let len = std::fs::metadata(path).ok().filter(|m| m.is_file())?.len();

        Some(Self {
            status: 200,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body: Body::Stream {
                path: path.to_path_buf(),
                len,
            },
        })
    }

    pub fn file(path: &Path, content_type: &str, file_name: &str) -> Option<Self> {
        Some(
            Self::stream_file(path, content_type)?
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{}\"", sanitize_filename(file_name)),
                )
                .header("Accept-Ranges", "none"),
        )
    }

    pub fn json(status: u16, value: &Value) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
        Self::bytes(status, body, "application/json")
    }

    pub fn html(status: u16, body: impl Into<String>) -> Self {
        Self::bytes(status, body.into().into_bytes(), "text/html; charset=utf-8")
    }

    pub fn text(status: u16, body: impl Into<String>) -> Self {
        Self::bytes(
            status,
            body.into().into_bytes(),
            "text/plain; charset=utf-8",
        )
    }

    pub fn attachment(body: Vec<u8>, content_type: &str, file_name: &str) -> Self {
        Self::bytes(200, body, content_type).header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", sanitize_filename(file_name)),
        )
    }

    pub fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            headers: vec![("Location".to_string(), location.to_string())],
            body: Body::Bytes(Vec::new()),
        }
    }

    pub fn not_found() -> Self {
        Self::json(404, &serde_json::json!({"error": "not_found"}))
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }
}

pub struct ServerConfig {
    pub addr: String,
    pub name: &'static str,
    pub max_connections: usize,
    pub max_body: usize,
    pub rate_limit: Option<(u32, u64)>,
}

impl ServerConfig {
    pub fn new(addr: impl Into<String>, name: &'static str) -> Self {
        Self {
            addr: addr.into(),
            name,
            max_connections: 64,
            max_body: 64 * 1024,
            rate_limit: Some((120, 60)),
        }
    }

    pub fn max_connections(mut self, value: usize) -> Self {
        self.max_connections = value.max(1);
        self
    }

    pub fn max_body(mut self, value: usize) -> Self {
        self.max_body = value;
        self
    }

    pub fn rate_limit(mut self, requests: u32, window_secs: u64) -> Self {
        self.rate_limit = Some((requests, window_secs));
        self
    }

    pub fn no_rate_limit(mut self) -> Self {
        self.rate_limit = None;
        self
    }
}

struct Limiter {
    hits: Mutex<HashMap<String, (u64, u32)>>,
    allowance: u32,
    window: u64,
}

impl Limiter {
    fn allow(&self, client: &str) -> bool {
        let now = now_secs();
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());

        hits.retain(|_, (start, _)| now.saturating_sub(*start) < self.window * 2);

        let entry = hits.entry(client.to_string()).or_insert((now, 0));
        if now.saturating_sub(entry.0) >= self.window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.allowance
    }
}

struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn serve<F>(config: ServerConfig, handler: F) -> std::io::Result<SocketAddr>
where
    F: Fn(&Request) -> Response + Send + Sync + 'static,
{
    let listener = TcpListener::bind(&config.addr)?;
    let local = listener.local_addr()?;

    let handler = Arc::new(handler);
    let limiter = config.rate_limit.map(|(allowance, window)| {
        Arc::new(Limiter {
            hits: Mutex::new(HashMap::new()),
            allowance,
            window,
        })
    });
    let live = Arc::new(AtomicUsize::new(0));
    let max_connections = config.max_connections;
    let max_body = config.max_body;
    let name = config.name;

    thread::Builder::new()
        .name(format!("{name}-accept"))
        .spawn(move || {
            for incoming in listener.incoming() {
                let Ok(mut stream) = incoming else {
                    continue;
                };

                if live.load(Ordering::SeqCst) >= max_connections {
                    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                    write_response(
                        &mut stream,
                        &Response::json(503, &serde_json::json!({"error": "busy"})),
                    );
                    continue;
                }

                live.fetch_add(1, Ordering::SeqCst);
                let guard = ConnectionGuard(live.clone());
                let handler = handler.clone();
                let limiter = limiter.clone();

                let worker = thread::Builder::new().stack_size(262_144).spawn(move || {
                    let _guard = guard;
                    handle(stream, handler.as_ref(), limiter.as_deref(), max_body);
                });

                if worker.is_err() {
                    eprintln!("{name}: unable to spawn a worker thread");
                    live.fetch_sub(1, Ordering::SeqCst);
                }
            }
        })?;

    Ok(local)
}

fn handle<F>(mut stream: TcpStream, handler: &F, limiter: Option<&Limiter>, max_body: usize)
where
    F: Fn(&Request) -> Response,
{
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

    let peer = stream
        .peer_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    if let Some(limiter) = limiter {
        if !limiter.allow(&peer) {
            write_response(
                &mut stream,
                &Response::json(429, &serde_json::json!({"error": "rate_limited"})),
            );
            return;
        }
    }

    let Some(request) = read_request(&mut stream, peer, max_body) else {
        write_response(
            &mut stream,
            &Response::json(400, &serde_json::json!({"error": "bad_request"})),
        );
        return;
    };

    let response = handler(&request);
    write_response(&mut stream, &response);
}

fn read_request(stream: &mut TcpStream, peer: String, max_body: usize) -> Option<Request> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    let head_end = loop {
        if let Some(position) = find_head_end(&buffer) {
            break position;
        }
        if buffer.len() > MAX_HEAD {
            return None;
        }
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let head = std::str::from_utf8(&buffer[..head_end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();

    let method = request_line.next()?.to_ascii_uppercase();
    let target = request_line.next()?;
    let (raw_path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query.to_string()),
        None => (target, String::new()),
    };

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
        .min(max_body);

    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);

    Some(Request {
        method,
        path: percent_decode(raw_path),
        query,
        headers,
        body,
        peer,
    })
}

fn write_response(stream: &mut TcpStream, response: &Response) {
    let reason = reason_phrase(response.status);
    let mut head = format!("HTTP/1.1 {} {reason}\r\n", response.status);

    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }

    head.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
    head.push_str("X-Content-Type-Options: nosniff\r\n");
    head.push_str("Referrer-Policy: no-referrer\r\n");
    head.push_str("Connection: close\r\n\r\n");

    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }

    match &response.body {
        Body::Bytes(bytes) => {
            let _ = stream.write_all(bytes);
        }
        Body::Stream { path, len } => {
            let _ = stream_file(stream, path, *len);
        }
    }

    let _ = stream.flush();
}

fn stream_file(stream: &mut TcpStream, path: &Path, len: u64) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut sent = 0u64;

    while sent < len {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let take = read.min((len - sent) as usize);
        stream.write_all(&buffer[..take])?;
        sent += take as u64;
    }

    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            match u8::from_str_radix(hex, 16) {
                Ok(decoded) => {
                    out.push(decoded);
                    index += 3;
                    continue;
                }
                Err(_) => {}
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).to_string()
}

pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' '))
        .collect()
}

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
