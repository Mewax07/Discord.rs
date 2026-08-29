use std::{
    io::{Read, Write},
    sync::Arc,
};

use serde::Serialize;
use serde_json::Value;

use crate::{
    net::{
        ratelimit::{RateLimiter, RetryDecision},
        TlsStream,
    },
    Error, Result,
};

pub struct HttpClient {
    host: String,
    port: u16,
    user_agent: String,
    limiter: Arc<RateLimiter>,
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

const MAX_RETRIES: u32 = 3;

impl HttpClient {
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: 443,
            user_agent: "DiscordBot (badomen, 1.0.0), Write in Rust".to_string(),
            limiter: Arc::new(RateLimiter::new()),
        }
    }

    pub fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<HttpResponse> {
        let route_key = format!("{method} {}", route_template(path));

        for attempt in 0..=MAX_RETRIES {
            self.limiter.wait_before_request(&route_key);

            let resp = self.send_once(method, path, headers, body)?;

            match self
                .limiter
                .record_response(&route_key, resp.status, &resp.headers)
            {
                RetryDecision::Done => return Ok(resp),
                RetryDecision::RetryAfter(delay) => {
                    if attempt == MAX_RETRIES {
                        return Err(Error::Http(format!(
                            "429 persists after {MAX_RETRIES} attempts (route: {route_key})"
                        )));
                    }
                    eprintln!("Rate limit on {route_key}, retry in {delay:?}");
                    std::thread::sleep(delay);
                }
            }
        }

        unreachable!()
    }

    pub fn send_once(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<HttpResponse> {
        let mut stream = TlsStream::connect(&self.host, self.port)?;

        let mut req = String::new();
        req.push_str(&format!("{method} {path} HTTP/1.1\r\n"));
        req.push_str(&format!("Host: {}\r\n", self.host));
        req.push_str(&format!("User-Agent: {}\r\n", self.user_agent));
        req.push_str("Connection: close\r\n");
        req.push_str("Accept: application/json\r\n");
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        if let Some(b) = body {
            req.push_str("Content-Type: application/json\r\n");
            req.push_str(&format!("Content-Length: {}\r\n", b.len()));
        }
        req.push_str("\r\n");

        stream.write_all(req.as_bytes())?;
        if let Some(b) = body {
            stream.write_all(b)?;
        }
        stream.flush()?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;

        parse_response(&raw)
    }

    pub fn get_json(&self, path: &str, token: &str) -> Result<Value> {
        let auth = format!("Authorization: Bot {token}");
        let resp = self.request("GET", path, &[split_header(&auth)], None)?;
        check_status(&resp)?;
        parse_json_body(&resp)
    }

    pub fn post_json<T: Serialize + ?Sized>(
        &self,
        path: &str,
        token: &str,
        body: &T,
    ) -> Result<Value> {
        let auth = format!("Authorization: Bot {token}");
        let payload = serde_json::to_vec(body)?;
        let resp = self.request("POST", path, &[split_header(&auth)], Some(&payload))?;
        check_status(&resp)?;
        if resp.body.is_empty() {
            Ok(Value::Null)
        } else {
            parse_json_body(&resp)
        }
    }

    pub fn put_json<T: Serialize + ?Sized>(
        &self,
        path: &str,
        token: &str,
        body: &T,
    ) -> Result<Value> {
        let auth = format!("Authorization: Bot {token}");
        let payload = serde_json::to_vec(body)?;
        let resp = self.request("PUT", path, &[split_header(&auth)], Some(&payload))?;
        check_status(&resp)?;
        if resp.body.is_empty() {
            Ok(Value::Null)
        } else {
            parse_json_body(&resp)
        }
    }
}

fn route_template(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if seg.chars().all(|c| c.is_ascii_digit()) && !seg.is_empty() {
                ":id"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn parse_json_body(resp: &HttpResponse) -> Result<Value> {
    serde_json::from_slice(&resp.body).map_err(|e| {
        let preview = String::from_utf8_lossy(&resp.body);
        let preview = &preview[..preview.len().min(200)];
        Error::Http(format!("Invalid JSON response ({e}) - body: {preview}"))
    })
}

fn split_header(h: &str) -> (&str, &str) {
    let (k, v) = h.split_once(": ").expect("malformed header literal");
    (k, v.trim())
}

fn check_status(resp: &HttpResponse) -> Result<()> {
    if resp.status >= 400 {
        let msg = String::from_utf8_lossy(&resp.body).to_string();
        return Err(Error::Http(format!("status {} - {msg}", resp.status)));
    }
    Ok(())
}

fn parse_response(raw: &[u8]) -> Result<HttpResponse> {
    let sep = find_double_crlf(raw).ok_or(Error::Protocol(
        "HTTP response with an endless list of headers",
    ))?;

    let head =
        std::str::from_utf8(&raw[..sep]).map_err(|_| Error::Protocol("Non-UTF-8 HTTP headers"))?;
    let raw_body = &raw[sep + 4..];

    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or(Error::Protocol("missing status line"))?;

    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or(Error::Protocol("invalid status code"))?;

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }

    let is_chunked = headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.eq_ignore_ascii_case("chunked"));

    let body = if is_chunked {
        dechunk(raw_body)?
    } else {
        raw_body.to_vec()
    };

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn dechunk(body: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(body.len());
    let mut pos = 0;

    loop {
        let line_end = find_crlf(&body[pos..]).ok_or(Error::Protocol("chunk: size missing"))? + pos;

        let size_line = std::str::from_utf8(&body[pos..line_end])
            .map_err(|_| Error::Protocol("chunk: non-UTF-8 size"))?;
        let size_str = size_line.split(';').next().unwrap().trim();
        let size = u64::from_str_radix(size_str, 16)
            .map_err(|_| Error::Protocol("chunk: invalid hex size"))? as usize;

        pos = line_end + 2;

        if size == 0 {
            break;
        }

        if pos + size > body.len() {
            return Err(Error::Protocol("chunk: truncated data"));
        }

        out.extend_from_slice(&body[pos..pos + size]);
        pos += size + 2;
    }

    Ok(out)
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}
