//! Discord OAuth2 (authorization code grant, `identify` scope).
//!
//! The site never stores Discord credentials: it exchanges the one-time `code`
//! for a short-lived access token, reads `users/@me`, then drops the token and
//! keeps only a signed cookie carrying the Discord user id.

use serde_json::Value;

use crate::tlshttp;

const API_HOST: &str = "discord.com";
const SCOPE: &str = "identify";

#[derive(Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
}

impl OAuthConfig {
    /// The URL a user is sent to in order to authorise the site.
    pub fn authorize_url(&self, state: &str) -> String {
        format!(
            "https://discord.com/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&prompt=none",
            urlencode(&self.client_id),
            urlencode(&self.redirect_uri),
            urlencode(SCOPE),
            urlencode(state),
        )
    }

    /// Exchange an authorization `code` for a bearer access token.
    pub fn exchange(&self, code: &str) -> Result<String, String> {
        let form = form_encode(&[
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.redirect_uri),
        ]);

        let response = tlshttp::request(
            API_HOST,
            "POST",
            "/api/oauth2/token",
            &[("Content-Type", "application/x-www-form-urlencoded")],
            Some(form.as_bytes()),
        )?;

        if response.status >= 400 {
            return Err(format!(
                "token endpoint returned {} ({})",
                response.status,
                String::from_utf8_lossy(&response.body)
            ));
        }

        response
            .json()
            .and_then(|v| v.get("access_token").and_then(Value::as_str).map(String::from))
            .ok_or_else(|| "no access_token in response".to_string())
    }

    /// Fetch the authenticated user's public identity.
    pub fn fetch_user(&self, access_token: &str) -> Result<DiscordUser, String> {
        let auth = format!("Bearer {access_token}");
        let response = tlshttp::request(
            API_HOST,
            "GET",
            "/api/users/@me",
            &[("Authorization", &auth)],
            None,
        )?;

        if response.status >= 400 {
            return Err(format!("users/@me returned {}", response.status));
        }

        let body = response.json().ok_or("invalid user payload")?;
        let id = body
            .get("id")
            .and_then(Value::as_str)
            .ok_or("missing user id")?
            .to_string();
        let username = body
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or("user")
            .to_string();
        let display_name = body
            .get("global_name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(&username)
            .to_string();
        let avatar_url = match body.get("avatar").and_then(Value::as_str) {
            Some(hash) if !hash.is_empty() => {
                format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128")
            }
            _ => {
                let index = id.parse::<u64>().unwrap_or(0) % 5;
                format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
            }
        };

        Ok(DiscordUser {
            id,
            username,
            display_name,
            avatar_url,
        })
    }
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encode a value for use in query strings and form bodies.
pub fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
