//! The user space.
//!
//! Users sign in with Discord (OAuth `identify`). The site then lists every
//! licence whose `owner_id` is that Discord account, and the machines bound to
//! each one. No password and no server-side session table: identity lives in a
//! signed cookie carrying only the Discord id and display name.

use httpd::{escape_html, Request, Response};
use serde_json::{json, Value};

use licensing::License;

use crate::render::{human_date, human_duration};
use crate::{session, SiteConfig};

const USER_COOKIE: &str = "bo_user";
const STATE_COOKIE: &str = "bo_state";
const SESSION_TTL: u64 = 7 * 86_400;
const STATE_TTL: u64 = 600;

pub fn handle(request: &Request, config: &SiteConfig) -> Response {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/account") | ("GET", "/account/") => page(request, config),
        ("GET", "/account/login") => login(config),
        ("GET", "/account/callback") => callback(request, config),
        ("POST", "/account/logout") | ("GET", "/account/logout") => logout(),
        _ => crate::not_found(config),
    }
}

fn session_user(request: &Request, config: &SiteConfig) -> Option<Value> {
    session::cookie(request.header("cookie"), USER_COOKIE)
        .and_then(|token| session::verify(&config.session_secret, token))
}

fn login(config: &SiteConfig) -> Response {
    let Some(oauth) = &config.oauth else {
        return Response::redirect("/account");
    };

    let nonce = licensing::crypto::hex_encode(&licensing::crypto::random_bytes(8));
    let state = session::issue(
        &config.session_secret,
        json!({ "kind": "state", "n": nonce }),
        STATE_TTL,
    );

    Response::redirect(&oauth.authorize_url(&state))
        .header("Set-Cookie", session::set_cookie(STATE_COOKIE, &state, STATE_TTL))
}

fn callback(request: &Request, config: &SiteConfig) -> Response {
    let Some(oauth) = &config.oauth else {
        return Response::redirect("/account");
    };

    let state_query = crate::query_param(&request.query, "state");
    let state_cookie = session::cookie(request.header("cookie"), STATE_COOKIE).map(str::to_string);

    let valid_state = match (&state_query, &state_cookie) {
        (Some(query), Some(cookie)) => {
            query == cookie && session::verify(&config.session_secret, query).is_some()
        }
        _ => false,
    };
    if !valid_state {
        return error_page(config, "La session de connexion a expiré. Réessayez.");
    }

    let Some(code) = crate::query_param(&request.query, "code") else {
        return error_page(config, "Autorisation refusée ou annulée.");
    };

    let token = match oauth.exchange(&code) {
        Ok(token) => token,
        Err(e) => {
            eprintln!("oauth exchange failed: {e}");
            return error_page(config, "Impossible d'échanger le code Discord.");
        }
    };

    let user = match oauth.fetch_user(&token) {
        Ok(user) => user,
        Err(e) => {
            eprintln!("oauth user fetch failed: {e}");
            return error_page(config, "Impossible de lire votre profil Discord.");
        }
    };

    let cookie_value = session::issue(
        &config.session_secret,
        json!({
            "id": user.id,
            "name": user.display_name,
            "avatar": user.avatar_url,
        }),
        SESSION_TTL,
    );

    Response::redirect("/account")
        .header("Set-Cookie", session::set_cookie(USER_COOKIE, &cookie_value, SESSION_TTL))
        .header("Set-Cookie", session::clear_cookie(STATE_COOKIE))
}

fn logout() -> Response {
    Response::redirect("/account").header("Set-Cookie", session::clear_cookie(USER_COOKIE))
}

fn page(request: &Request, config: &SiteConfig) -> Response {
    if config.oauth.is_none() {
        return Response::html(200, shell(config, &unconfigured_body()));
    }

    match session_user(request, config) {
        Some(user) => Response::html(200, shell(config, &account_body(config, &user))),
        None => Response::html(200, shell(config, &signed_out_body(config))),
    }
}

fn error_page(config: &SiteConfig, message: &str) -> Response {
    let body = format!(
        r#"<div class="panel center">
      <h1>Connexion Discord</h1>
      <p class="error">{message}</p>
      <a class="btn" href="/account/login">Réessayer</a>
    </div>"#,
        message = escape_html(message),
    );
    Response::html(400, shell(config, &body))
}

fn unconfigured_body() -> String {
    r#"<div class="panel center">
      <h1>Espace utilisateur</h1>
      <p class="muted">La connexion Discord n'est pas encore configurée sur ce serveur.</p>
    </div>"#
        .to_string()
}

fn signed_out_body(config: &SiteConfig) -> String {
    format!(
        r#"<div class="panel center">
      <h1>Vos licences BadOmen</h1>
      <p class="muted">Connectez-vous avec Discord pour voir vos licences {site} et les machines qui leur sont attribuées.</p>
      <a class="btn discord" href="/account/login">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M20.3 4.5A19 19 0 0 0 15.5 3l-.24.5a14 14 0 0 1 4.2 2.1A15.8 15.8 0 0 0 12 4.6 15.8 15.8 0 0 0 4.5 5.6a14 14 0 0 1 4.2-2.1L8.5 3A19 19 0 0 0 3.7 4.5C1.4 8 .8 11.4 1.1 14.8a19 19 0 0 0 5.8 2.9l.7-1a12 12 0 0 1-1.9-.9l.5-.3a13.6 13.6 0 0 0 11.6 0l.5.3c-.6.4-1.2.7-1.9.9l.7 1a19 19 0 0 0 5.8-2.9c.4-4-.6-7.4-2.3-10.3ZM8.4 12.9c-.9 0-1.7-.9-1.7-1.9s.8-1.9 1.7-1.9 1.7.9 1.7 1.9-.8 1.9-1.7 1.9Zm7.2 0c-.9 0-1.7-.9-1.7-1.9s.8-1.9 1.7-1.9 1.7.9 1.7 1.9-.8 1.9-1.7 1.9Z"/></svg>
        Se connecter avec Discord
      </a>
      <p class="fineprint">Nous lisons seulement votre identifiant Discord (scope <code>identify</code>).</p>
    </div>"#,
        site = escape_html(&config.site_name),
    )
}

fn account_body(config: &SiteConfig, user: &Value) -> String {
    let id = user.get("id").and_then(Value::as_str).unwrap_or("");
    let name = user.get("name").and_then(Value::as_str).unwrap_or("Utilisateur");
    let avatar = user.get("avatar").and_then(Value::as_str).unwrap_or("");

    let licenses = config
        .licenses
        .as_ref()
        .map(|service| service.for_owner(id))
        .unwrap_or_default();

    let cards = if licenses.is_empty() {
        r#"<div class="panel"><p class="muted">Aucune licence n'est associée à ce compte Discord pour le moment. Si vous venez d'en recevoir une, patientez ou contactez le support.</p></div>"#.to_string()
    } else {
        licenses.iter().map(license_card).collect::<Vec<_>>().join("\n")
    };

    format!(
        r#"<div class="bar">
      <div class="who">
        {avatar}
        <div>
          <strong>{name}</strong>
          <span class="muted">Discord ID {id}</span>
        </div>
      </div>
      <form method="post" action="/account/logout"><button class="ghost" type="submit">Déconnexion</button></form>
    </div>
    <h1>Vos licences</h1>
    <p class="muted count">{count} licence(s) · les machines listées sont celles activées avec chaque clé.</p>
    <div class="grid">
      {cards}
    </div>"#,
        avatar = if avatar.is_empty() {
            String::new()
        } else {
            format!("<img class=\"avatar\" src=\"{}\" alt=\"\" width=\"48\" height=\"48\">", escape_html(avatar))
        },
        name = escape_html(name),
        id = escape_html(id),
        count = licenses.len(),
    )
}

fn license_card(license: &License) -> String {
    let now = crate::now_secs();
    let status = license.status(now).as_str();

    let expiry = if license.revoked {
        "Révoquée".to_string()
    } else if license.is_lifetime() {
        "Licence à vie".to_string()
    } else {
        match license.remaining(now) {
            Some(0) | None => "Expirée".to_string(),
            Some(secs) => format!(
                "Expire dans {} · le {}",
                human_duration(secs),
                license.expires_at.map(human_date).unwrap_or_default()
            ),
        }
    };

    let machines = if license.activations.is_empty() {
        r#"<p class="muted">Aucune machine activée.</p>"#.to_string()
    } else {
        let rows = license
            .activations
            .iter()
            .map(|activation| {
                format!(
                    r#"<li>
              <span class="mono">💻 {hwid}</span>
              <span class="muted">activée le {first} · vue le {last}</span>
            </li>"#,
                    hwid = escape_html(&short_hwid(&activation.hwid)),
                    first = escape_html(&human_date(activation.first_seen)),
                    last = escape_html(&human_date(activation.last_seen)),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("<ul class=\"machines\">{rows}</ul>")
    };

    format!(
        r#"<article class="panel license">
      <div class="license-head">
        <div>
          <h2>{product}</h2>
          <span class="muted">{plan} · clé <code>{prefix}…</code></span>
        </div>
        <span class="pill {status}">{status}</span>
      </div>
      <p class="expiry">{expiry}</p>
      <div class="machine-block">
        <div class="machine-title">Machines <span class="muted">({used}/{allowed})</span></div>
        {machines}
      </div>
    </article>"#,
        product = escape_html(&license.product),
        plan = escape_html(&license.plan),
        prefix = escape_html(&license.key_prefix),
        status = escape_html(status),
        expiry = escape_html(&expiry),
        used = license.activations.len(),
        allowed = license.max_activations,
    )
}

fn short_hwid(hwid: &str) -> String {
    if hwid.len() <= 14 {
        hwid.to_string()
    } else {
        format!("…{}", &hwid[hwid.len() - 12..])
    }
}

fn shell(config: &SiteConfig, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<title>Mon compte · {site}</title>
<style>{css}</style>
</head>
<body>
<main class="wrap">
  <nav class="topnav"><a href="/">← {site}</a></nav>
{body}
</main>
</body>
</html>
"#,
        site = escape_html(&config.site_name),
        css = ACCOUNT_CSS,
        body = body,
    )
}

const ACCOUNT_CSS: &str = r#"
:root { color-scheme: dark; --accent: #8B5CF6; --discord: #5865F2; --bg: #0d0d11; --surface: #16161c; --line: #26262f; --text: #ececf1; --muted: #9a9aa8; --ok: #34d399; --warn: #fbbf24; --bad: #f87171; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text); font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; line-height: 1.55; }
.wrap { max-width: 860px; margin: 0 auto; padding: 28px 20px 72px; }
a { color: var(--accent); text-decoration: none; }
.topnav { margin-bottom: 24px; font-size: 0.9rem; }
.topnav a { color: var(--muted); }
h1 { font-size: 1.6rem; margin: 0 0 6px; letter-spacing: -0.02em; }
h2 { font-size: 1.1rem; margin: 0; }
.muted { color: var(--muted); }
.count { margin: 0 0 20px; font-size: 0.9rem; }
.bar { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 28px; }
.who { display: flex; align-items: center; gap: 12px; }
.who div { display: flex; flex-direction: column; }
.who span { font-size: 0.8rem; }
.avatar { border-radius: 50%; border: 1px solid var(--line); }
button { font: inherit; cursor: pointer; border-radius: 9px; border: 1px solid var(--line); background: transparent; color: var(--text); padding: 8px 15px; font-weight: 600; }
button.ghost:hover { border-color: var(--accent); }
.panel { background: var(--surface); border: 1px solid var(--line); border-radius: 16px; padding: 22px; }
.panel.center { max-width: 460px; margin: 8vh auto 0; text-align: center; }
.panel.center h1 { margin-bottom: 12px; }
.grid { display: grid; gap: 16px; }
.license-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 14px; margin-bottom: 10px; }
.expiry { margin: 0 0 16px; color: #c6c6d2; font-size: 0.92rem; }
.machine-block { border-top: 1px solid var(--line); padding-top: 14px; }
.machine-title { font-size: 0.82rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); margin-bottom: 10px; }
.machines { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
.machines li { display: flex; flex-direction: column; gap: 2px; font-size: 0.9rem; }
.machines .muted { font-size: 0.8rem; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.pill { display: inline-block; padding: 3px 11px; border-radius: 999px; font-size: 0.76rem; font-weight: 600; border: 1px solid var(--line); white-space: nowrap; }
.pill.active { color: var(--ok); border-color: #1f5c47; }
.pill.unused { color: var(--muted); }
.pill.expired { color: var(--warn); border-color: #6b551a; }
.pill.revoked { color: var(--bad); border-color: #6b2626; }
.btn { display: inline-flex; align-items: center; gap: 9px; margin-top: 20px; background: var(--accent); color: #fff; padding: 12px 22px; border-radius: 10px; font-weight: 600; }
.btn.discord { background: var(--discord); }
.btn:hover { filter: brightness(1.1); }
.fineprint { margin-top: 18px; font-size: 0.78rem; color: var(--muted); }
.error { color: var(--bad); }
@media (max-width: 560px) { .bar { flex-direction: column; align-items: flex-start; } }
"#;
