//! A very small blocking HTTPS client used for the Discord OAuth exchange.
//!
//! The site only needs two calls (token exchange and `users/@me`), so this
//! stays deliberately minimal: one request per connection, `Connection: close`,
//! and support for chunked responses which Discord sometimes returns.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

const IO_TIMEOUT: Duration = Duration::from_secs(15);

pub struct HttpsResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpsResponse {
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

pub fn request(
    host: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<HttpsResponse, String> {
    let server_name = ServerName::try_from(host.to_string()).map_err(|_| "invalid host".to_string())?;
    let conn = ClientConnection::new(client_config(), server_name).map_err(|e| e.to_string())?;
    let tcp = TcpStream::connect((host, 443)).map_err(|e| e.to_string())?;
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(IO_TIMEOUT)).ok();
    tcp.set_write_timeout(Some(IO_TIMEOUT)).ok();
    let mut stream = StreamOwned::new(conn, tcp);

    let mut head = String::new();
    head.push_str(&format!("{method} {path} HTTP/1.1\r\n"));
    head.push_str(&format!("Host: {host}\r\n"));
    head.push_str("User-Agent: BadOmen-Site/1.0\r\n");
    head.push_str("Connection: close\r\n");
    head.push_str("Accept: application/json\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(payload) = body {
        head.push_str(&format!("Content-Length: {}\r\n", payload.len()));
    }
    head.push_str("\r\n");

    stream.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
    if let Some(payload) = body {
        stream.write_all(payload).map_err(|e| e.to_string())?;
    }
    stream.flush().ok();

    let mut raw = Vec::new();
    // read_to_end returns an error on some TLS close_notify variations; tolerate
    // a clean EOF and parse whatever arrived.
    if let Err(e) = stream.read_to_end(&mut raw) {
        if raw.is_empty() {
            return Err(e.to_string());
        }
    }

    parse(&raw)
}

fn parse(raw: &[u8]) -> Result<HttpsResponse, String> {
    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("malformed response")?;

    let head = std::str::from_utf8(&raw[..sep]).map_err(|_| "non-utf8 headers")?;
    let raw_body = &raw[sep + 4..];

    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or("missing status line")?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("invalid status")?;

    let chunked = lines.clone().any(|line| {
        line.split_once(':')
            .map(|(k, v)| {
                k.trim().eq_ignore_ascii_case("transfer-encoding")
                    && v.trim().eq_ignore_ascii_case("chunked")
            })
            .unwrap_or(false)
    });

    let body = if chunked {
        dechunk(raw_body)?
    } else {
        raw_body.to_vec()
    };

    Ok(HttpsResponse { status, body })
}

fn dechunk(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(body.len());
    let mut pos = 0;

    loop {
        let line_end = body[pos..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("chunk size missing")?
            + pos;
        let size_line = std::str::from_utf8(&body[pos..line_end]).map_err(|_| "chunk size utf8")?;
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16).map_err(|_| "chunk size hex")?;
        pos = line_end + 2;
        if size == 0 {
            break;
        }
        if pos + size > body.len() {
            return Err("truncated chunk".to_string());
        }
        out.extend_from_slice(&body[pos..pos + size]);
        pos += size + 2;
    }

    Ok(out)
}

fn client_config() -> Arc<ClientConfig> {
    let mut store = RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(store)
            .with_no_client_auth(),
    )
}
