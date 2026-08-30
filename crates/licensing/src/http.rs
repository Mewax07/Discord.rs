use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::crypto::constant_time_eq;
use crate::model::{License, LicenseError};
use crate::service::{now_secs, IssueRequest, LicenseService};

const MAX_BODY: usize = 64 * 1024;
const MAX_HEAD: usize = 16 * 1024;
const RATE_WINDOW: u64 = 60;
const RATE_LIMIT: u32 = 120;
const IO_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_CONNECTIONS: usize = 64;

pub struct ApiConfig {
    pub addr: String,
    pub admin_token: Option<String>,
    pub product: String,
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Limiter {
    hits: Mutex<HashMap<String, (u64, u32)>>,
}

impl Limiter {
    fn new() -> Self {
        Self {
            hits: Mutex::new(HashMap::new()),
        }
    }

    fn allow(&self, client: &str) -> bool {
        let now = now_secs();
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());

        hits.retain(|_, (start, _)| now.saturating_sub(*start) < RATE_WINDOW * 2);

        let entry = hits.entry(client.to_string()).or_insert((now, 0));
        if now.saturating_sub(entry.0) >= RATE_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= RATE_LIMIT
    }
}

pub fn spawn(service: Arc<LicenseService>, config: ApiConfig) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(&config.addr)?;
    let local = listener.local_addr()?;
    let limiter = Arc::new(Limiter::new());
    let config = Arc::new(config);
    let started = now_secs();
    let live = Arc::new(AtomicUsize::new(0));

    thread::Builder::new()
        .name("licence-api".to_string())
        .spawn(move || {
            for incoming in listener.incoming() {
                let Ok(mut stream) = incoming else {
                    continue;
                };

                if live.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                    write_response(&mut stream, 503, &json!({"error": "busy"}));
                    continue;
                }

                live.fetch_add(1, Ordering::SeqCst);
                let guard = ConnectionGuard(live.clone());
                let service = service.clone();
                let limiter = limiter.clone();
                let config = config.clone();

                let worker = thread::Builder::new().stack_size(196_608).spawn(move || {
                    let _guard = guard;
                    handle(stream, &service, &limiter, &config, started);
                });

                if worker.is_err() {
                    eprintln!("licence api: unable to spawn a worker thread");
                    live.fetch_sub(1, Ordering::SeqCst);
                }
            }
        })?;

    Ok(local)
}

fn handle(
    mut stream: TcpStream,
    service: &LicenseService,
    limiter: &Limiter,
    config: &ApiConfig,
    started: u64,
) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

    let client = stream
        .peer_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    if !limiter.allow(&client) {
        write_response(&mut stream, 429, &json!({"error": "rate_limited"}));
        return;
    }

    let Some(request) = read_request(&mut stream) else {
        write_response(&mut stream, 400, &json!({"error": "bad_request"}));
        return;
    };

    let (status, body) = route(&request, service, config, started);
    write_response(&mut stream, status, &body);
}

fn route(
    request: &Request,
    service: &LicenseService,
    config: &ApiConfig,
    started: u64,
) -> (u16, Value) {
    let method = request.method.as_str();
    let path = request.path.as_str();

    match (method, path) {
        ("GET", "/") | ("GET", "/v1") => (
            200,
            json!({
                "service": "badomen-licensing",
                "product": config.product,
                "version": 1,
                "endpoints": [
                    "GET /v1/health",
                    "GET /v1/public-key",
                    "POST /v1/activate",
                    "POST /v1/validate",
                    "POST /v1/refresh"
                ]
            }),
        ),
        ("GET", "/v1/health") => (
            200,
            json!({"status": "ok", "uptime": now_secs().saturating_sub(started)}),
        ),
        ("GET", "/v1/public-key") => (
            200,
            json!({
                "algorithm": "ed25519",
                "public_key_hex": service.public_key_hex(),
                "public_key_base64url": service.public_key_base64(),
                "offline_grace_seconds": service.offline_grace()
            }),
        ),
        ("POST", "/v1/activate") => activate(request, service),
        ("POST", "/v1/validate") => validate(request, service),
        ("POST", "/v1/refresh") => refresh(request, service),
        _ if path.starts_with("/v1/admin") => admin(request, service, config),
        _ => (404, json!({"error": "not_found"})),
    }
}

fn activate(request: &Request, service: &LicenseService) -> (u16, Value) {
    let Some(payload) = request.json() else {
        return failure(LicenseError::InvalidRequest);
    };
    let (Some(key), Some(hwid)) = (string_of(&payload, "key"), string_of(&payload, "hwid")) else {
        return failure(LicenseError::InvalidRequest);
    };

    match service.activate(&key, &hwid) {
        Ok((license, token)) => (
            200,
            json!({
                "status": license.status(now_secs()).as_str(),
                "licence": public_view(&license),
                "token": token.token,
                "expires_at": token.expires_at,
                "offline_until": token.offline_until
            }),
        ),
        Err(e) => failure(e),
    }
}

fn validate(request: &Request, service: &LicenseService) -> (u16, Value) {
    let Some(payload) = request.json() else {
        return failure(LicenseError::InvalidRequest);
    };

    if let Some(token) = string_of(&payload, "token") {
        return match service.verify_token(&token) {
            Ok(claims) => {
                let matching = string_of(&payload, "hwid")
                    .map(|hwid| hwid == claims.hwid)
                    .unwrap_or(true);
                if matching {
                    (
                        200,
                        json!({
                            "status": "valid",
                            "key": claims.key,
                            "product": claims.product,
                            "plan": claims.plan,
                            "expires_at": claims.expires_at,
                            "offline_until": claims.offline_until
                        }),
                    )
                } else {
                    failure(LicenseError::InvalidHardware)
                }
            }
            Err(e) => failure(e),
        };
    }

    let (Some(key), Some(hwid)) = (string_of(&payload, "key"), string_of(&payload, "hwid")) else {
        return failure(LicenseError::InvalidRequest);
    };

    match service.validate(&key, &hwid) {
        Ok(license) => (
            200,
            json!({"status": "valid", "licence": public_view(&license)}),
        ),
        Err(e) => failure(e),
    }
}

fn refresh(request: &Request, service: &LicenseService) -> (u16, Value) {
    let Some(payload) = request.json() else {
        return failure(LicenseError::InvalidRequest);
    };
    let (Some(key), Some(hwid)) = (string_of(&payload, "key"), string_of(&payload, "hwid")) else {
        return failure(LicenseError::InvalidRequest);
    };

    match service.refresh(&key, &hwid) {
        Ok((license, token)) => (
            200,
            json!({
                "status": license.status(now_secs()).as_str(),
                "token": token.token,
                "expires_at": token.expires_at,
                "offline_until": token.offline_until
            }),
        ),
        Err(e) => failure(e),
    }
}

fn admin(request: &Request, service: &LicenseService, config: &ApiConfig) -> (u16, Value) {
    let Some(expected) = config.admin_token.as_deref() else {
        return (503, json!({"error": "admin_api_disabled"}));
    };

    let presented = request
        .header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");

    if !constant_time_eq(presented, expected) {
        return (401, json!({"error": "unauthorized"}));
    }

    let path = request.path.as_str();
    let method = request.method.as_str();

    if method == "GET" && path == "/v1/admin/stats" {
        return (
            200,
            serde_json::to_value(service.stats()).unwrap_or_default(),
        );
    }

    if method == "POST" && path == "/v1/admin/licenses" {
        let Some(payload) = request.json() else {
            return failure(LicenseError::InvalidRequest);
        };

        let days = payload.get("days").and_then(Value::as_u64).unwrap_or(0);
        let issue = IssueRequest::new(
            string_of(&payload, "product").unwrap_or_else(|| config.product.clone()),
            string_of(&payload, "plan").unwrap_or_else(|| "standard".to_string()),
        )
        .duration((days > 0).then(|| days * 86_400))
        .machines(
            payload
                .get("machines")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .min(50) as u32,
        )
        .owner(string_of(&payload, "owner_id"))
        .note(string_of(&payload, "note"))
        .issued_by(Some("api".to_string()));

        let issued = service.issue(issue);
        let mut view = admin_view(&issued.license);
        view["key"] = Value::String(issued.key);
        return (201, view);
    }

    let Some(rest) = path.strip_prefix("/v1/admin/licenses/") else {
        return (404, json!({"error": "not_found"}));
    };
    let (key, action) = match rest.split_once('/') {
        Some((key, action)) => (key, action),
        None => (rest, ""),
    };

    let outcome = match (method, action) {
        ("GET", "") => service.resolve(key),
        ("POST", "revoke") => service.revoke(
            key,
            request.json().as_ref().and_then(|p| string_of(p, "reason")),
        ),
        ("POST", "restore") => service.restore(key),
        ("POST", "reset-hwid") => service.reset_hardware(key),
        ("POST", "assign") => service.assign(
            key,
            request
                .json()
                .as_ref()
                .and_then(|p| string_of(p, "owner_id")),
        ),
        _ => return (404, json!({"error": "not_found"})),
    };

    match outcome {
        Ok(license) => (200, admin_view(&license)),
        Err(e) => failure(e),
    }
}

fn failure(error: LicenseError) -> (u16, Value) {
    (
        error.http_status(),
        json!({"error": error.code(), "message": error.message()}),
    )
}

fn public_view(license: &License) -> Value {
    json!({
        "key_prefix": license.key_prefix,
        "product": license.product,
        "plan": license.plan,
        "lifetime": license.is_lifetime(),
        "expires_at": license.expires_at,
        "machines_used": license.activations.len(),
        "machines_allowed": license.max_activations
    })
}

fn admin_view(license: &License) -> Value {
    let now = now_secs();
    json!({
        "key_prefix": license.key_prefix,
        "product": license.product,
        "plan": license.plan,
        "status": license.status(now).as_str(),
        "owner_id": license.owner_id,
        "note": license.note,
        "created_at": license.created_at,
        "duration_secs": license.duration_secs,
        "expires_at": license.expires_at,
        "machines_allowed": license.max_activations,
        "activations": license
            .activations
            .iter()
            .map(|activation| {
                json!({
                    "hwid": activation.hwid,
                    "first_seen": activation.first_seen,
                    "last_seen": activation.last_seen,
                    "checks": activation.checks
                })
            })
            .collect::<Vec<_>>(),
        "revoked": license.revoked,
        "revoked_reason": license.revoked_reason,
        "issued_by": license.issued_by,
        "last_check": license.last_check
    })
}

fn string_of(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
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
    let path = target.split('?').next().unwrap_or(target).to_string();

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
        .min(MAX_BODY);

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
        path,
        headers,
        body,
    })
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_response(stream: &mut TcpStream, status: u16, body: &Value) {
    let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "Error",
    };

    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        payload.len()
    );

    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&payload);
    let _ = stream.flush();
}
