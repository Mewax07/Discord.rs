//! Signed, stateless session cookies.
//!
//! A token is `base64url(json_payload).base64url(hmac_sha256(payload))`. The
//! server keeps no session table: it trusts a cookie only if the signature made
//! with `secret` verifies and the embedded `exp` is still in the future.

use ring::hmac;
use serde_json::{json, Value};

use licensing::crypto::{base64url_decode, base64url_encode};

use crate::now_secs;

/// Build a signed token embedding `claims` plus an `exp` timestamp.
pub fn issue(secret: &[u8], mut claims: Value, ttl_secs: u64) -> String {
    if let Value::Object(map) = &mut claims {
        map.insert("exp".to_string(), json!(now_secs() + ttl_secs));
    }
    let body = serde_json::to_vec(&claims).unwrap_or_else(|_| b"{}".to_vec());
    let signature = sign(secret, &body);
    format!("{}.{}", base64url_encode(&body), base64url_encode(&signature))
}

/// Verify a token and return its claims when the signature and `exp` are valid.
pub fn verify(secret: &[u8], token: &str) -> Option<Value> {
    let (body_b64, sig_b64) = token.split_once('.')?;
    let body = base64url_decode(body_b64)?;
    let signature = base64url_decode(sig_b64)?;

    if !constant_time_eq(&signature, &sign(secret, &body)) {
        return None;
    }

    let claims: Value = serde_json::from_slice(&body).ok()?;
    match claims.get("exp").and_then(Value::as_u64) {
        Some(exp) if exp <= now_secs() => None,
        _ => Some(claims),
    }
}

fn sign(secret: &[u8], message: &[u8]) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hmac::sign(&key, message).as_ref().to_vec()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Extract a cookie value from a request's `Cookie` header.
pub fn cookie<'a>(header: Option<&'a str>, name: &str) -> Option<&'a str> {
    let header = header?;
    for part in header.split(';') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            if key.trim() == name {
                return Some(value.trim());
            }
        }
    }
    None
}

/// A `Set-Cookie` value that stores `value` for the whole site.
pub fn set_cookie(name: &str, value: &str, max_age: u64) -> String {
    format!(
        "{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax"
    )
}

/// A `Set-Cookie` value that immediately clears the cookie.
pub fn clear_cookie(name: &str) -> String {
    format!("{name}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
}
