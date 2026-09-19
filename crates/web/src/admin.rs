//! The admin panel.
//!
//! Two barriers protect it: the request must arrive from an allowed IP (the
//! panel returns a plain 404 otherwise, so it never reveals it exists), and a
//! password must be entered, which yields a signed session cookie.

use httpd::{escape_html, Request, Response};
use serde_json::{json, Value};

use crate::render::license_json;
use crate::{now_secs, session, AdminConfig, SiteConfig};

const COOKIE: &str = "bo_admin";
const SESSION_TTL: u64 = 12 * 3_600;

pub fn handle(request: &Request, config: &SiteConfig) -> Response {
    let Some(admin) = &config.admin else {
        return crate::not_found(config);
    };

    let ip = crate::client_ip(request, admin.trust_forwarded_for);
    if !ip_allowed(&ip, admin) {
        return crate::not_found(config);
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/admin/login") => login(request, config, admin),
        ("POST", "/admin/logout") => logout(),
        ("GET", "/admin") | ("GET", "/admin/") => {
            if is_authed(request, config) {
                Response::html(200, dashboard(config))
            } else {
                Response::html(200, login_page(config, false))
            }
        }
        ("GET", "/admin/data") => data(request, config),
        _ => {
            if let Some(rest) = request.path.strip_prefix("/admin/license/") {
                return license_action(request, config, rest);
            }
            crate::not_found(config)
        }
    }
}

fn ip_allowed(ip: &str, admin: &AdminConfig) -> bool {
    crate::is_loopback(ip) || admin.allowed_ips.iter().any(|allowed| allowed == ip)
}

fn is_authed(request: &Request, config: &SiteConfig) -> bool {
    session::cookie(request.header("cookie"), COOKIE)
        .and_then(|token| session::verify(&config.session_secret, token))
        .and_then(|claims| {
            claims
                .get("role")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .map(|role| role == "admin")
        .unwrap_or(false)
}

fn login(request: &Request, config: &SiteConfig, admin: &AdminConfig) -> Response {
    let form = crate::parse_form(&request.body);
    let password = form.get("password").map(String::as_str).unwrap_or("");

    if !constant_time_eq(password, &admin.password) || admin.password.is_empty() {
        return Response::html(401, login_page(config, true));
    }

    let token = session::issue(
        &config.session_secret,
        json!({ "role": "admin" }),
        SESSION_TTL,
    );
    Response::redirect("/admin").header(
        "Set-Cookie",
        session::set_cookie(COOKIE, &token, SESSION_TTL),
    )
}

fn logout() -> Response {
    Response::redirect("/admin").header("Set-Cookie", session::clear_cookie(COOKIE))
}

fn data(request: &Request, config: &SiteConfig) -> Response {
    if !is_authed(request, config) {
        return Response::json(401, &json!({"error": "unauthorized"}));
    }

    let Some(service) = &config.licenses else {
        return Response::json(503, &json!({"error": "licensing_unavailable"}));
    };

    let stats = service.stats();
    let licenses: Vec<Value> = service.all().iter().map(license_json).collect();

    let visits = match &config.visits {
        Some(store) => {
            let snapshot = store.snapshot();
            let today = crate::visits::today(now_secs());
            json!({
                "total": snapshot.total,
                "today": snapshot.days.get(&today).copied().unwrap_or(0),
                "days": snapshot.days,
                "downloads": snapshot.downloads,
                "pages": snapshot.pages,
                "first_seen": snapshot.first_seen,
                "last_seen": snapshot.last_seen,
            })
        }
        None => json!({ "disabled": true }),
    };

    Response::json(
        200,
        &json!({
            "site": config.site_name,
            "stats": {
                "total": stats.total,
                "active": stats.active,
                "unused": stats.unused,
                "expired": stats.expired,
                "revoked": stats.revoked,
                "machines": stats.machines,
            },
            "visits": visits,
            "licenses": licenses,
        }),
    )
}

fn license_action(request: &Request, config: &SiteConfig, rest: &str) -> Response {
    if !is_authed(request, config) {
        return Response::json(401, &json!({"error": "unauthorized"}));
    }
    if request.method != "POST" {
        return Response::json(405, &json!({"error": "method_not_allowed"}));
    }

    let Some(service) = &config.licenses else {
        return Response::json(503, &json!({"error": "licensing_unavailable"}));
    };

    let (reference, action) = match rest.split_once('/') {
        Some((reference, action)) => (reference, action),
        None => (rest, ""),
    };

    let payload = request.json();
    let field = |name: &str| {
        payload
            .as_ref()
            .and_then(|value| value.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from)
    };

    let outcome = match action {
        "revoke" => service.revoke(reference, field("reason")),
        "restore" => service.restore(reference),
        "reset-hwid" => service.reset_hardware(reference),
        "assign" => service.assign(reference, field("owner_id")),
        _ => return Response::json(404, &json!({"error": "unknown_action"})),
    };

    match outcome {
        Ok(license) => Response::json(200, &license_json(&license)),
        Err(error) => Response::json(
            error.http_status(),
            &json!({"error": error.code(), "message": error.message()}),
        ),
    }
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn login_page(config: &SiteConfig, failed: bool) -> String {
    let error = if failed {
        r#"<div class="auth-alert error">
  <svg viewBox="0 0 24 24" fill="none" width="16" height="16"><path d="M12 9v4M12 17h.01M10.3 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.7 3.86a2 2 0 0 0-3.4 0Z" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>
  <span data-i18n="admin.login.wrongPassword">Mot de passe incorrect</span>
</div>"#
    } else {
        ""
    };

    let body = format!(
        r#"<div class="auth-wrap">
  <div class="auth-panel">
    <span data-lang-switch class="auth-lang"></span>
    <div class="auth-icon">
      <svg viewBox="0 0 24 24" fill="none"><path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" stroke="currentColor" stroke-width="1.6"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 8.98 19.3a1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.62 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.62 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09A1.7 1.7 0 0 0 15.06 4.65a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9c.15.62.7 1.06 1.34 1.06H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.03Z" stroke="currentColor" stroke-width="1.4"/></svg>
    </div>
    <span class="eyebrow" data-i18n="admin.login.eyebrow">ESPACE ADMIN / RESTREINT</span>
    <h1 data-i18n="admin.login.title">Panneau d'administration</h1>
    <p class="muted" data-i18n="admin.login.sub">{site} - acces protege par IP et mot de passe.</p>
    {error}
    <form method="post" action="/admin/login" autocomplete="off">
      <label>
        <span data-i18n="admin.login.password">Mot de passe</span>
        <div class="input">
          <svg viewBox="0 0 24 24" fill="none" width="16" height="16"><rect x="4" y="10" width="16" height="11" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M8 10V7a4 4 0 0 1 8 0v3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          <input type="password" name="password" autocomplete="current-password" autofocus required>
        </div>
      </label>
      <button class="btn primary" type="submit">
        <span data-i18n="admin.login.submit">Se connecter</span>
        <svg viewBox="0 0 24 24" fill="none" width="16" height="16"><path d="M5 12h14M13 5l7 7-7 7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>
      </button>
    </form>
    <p class="fineprint" data-i18n="admin.login.fine">Les tentatives sont journalisees. Toute connexion hors IP autorisee retourne une 404.</p>
  </div>
</div>"#,
        site = escape_html(&config.site_name),
        error = error,
    );

    shell(config, "Connexion admin", "admin.doctitle.login", &body)
}

fn dashboard(config: &SiteConfig) -> String {
    let body = format!(
        r#"<section class="hero">
  <div class="hero-glow" aria-hidden="true"></div>
  <span class="eyebrow" data-i18n="admin.hero.eyebrow">PANNEAU ADMIN / CONSOLE</span>
  <div class="hero-row">
    <div>
      <h1 data-i18n-html="admin.hero.title">Console <em>{site}</em></h1>
      <p class="hero-sub" data-i18n="admin.hero.sub">Gestion des licences, des machines et du trafic. Les actions sensibles sont confirmees avant execution.</p>
    </div>
    <div class="hero-actions">
      <span data-lang-switch></span>
      <span class="live-pill"><i></i><span data-i18n="admin.live">Live</span></span>
      <form method="post" action="/admin/logout">
        <button class="btn ghost sm" type="submit">
          <svg viewBox="0 0 24 24" fill="none" width="14" height="14"><path d="M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3M10 17l-5-5 5-5M5 12h11" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>
          <span data-i18n="common.logout">Deconnexion</span>
        </button>
      </form>
    </div>
  </div>
</section>

<section class="cards" id="stats">
  <div class="card skeleton"><div class="n">-</div><div class="l" data-i18n="admin.stat.total">Total</div></div>
  <div class="card skeleton"><div class="n">-</div><div class="l" data-i18n="admin.stat.active">Actives</div></div>
  <div class="card skeleton"><div class="n">-</div><div class="l" data-i18n="admin.stat.unused">Inutilisees</div></div>
  <div class="card skeleton"><div class="n">-</div><div class="l" data-i18n="admin.stat.expired">Expirees</div></div>
  <div class="card skeleton"><div class="n">-</div><div class="l" data-i18n="admin.stat.revoked">Revoquees</div></div>
  <div class="card skeleton"><div class="n">-</div><div class="l" data-i18n="admin.stat.machines">Machines</div></div>
</section>

<section class="panel" id="visits-panel">
  <header class="panel-head">
    <div>
      <span class="eyebrow" data-i18n="admin.visits.eyebrow">TRAFIC / 14 JOURS</span>
      <h2 data-i18n="admin.visits.title">Visites</h2>
    </div>
  </header>
  <div id="visits" class="muted" data-i18n="common.loading">Chargement...</div>
</section>

<section class="panel">
  <header class="panel-head">
    <div>
      <span class="eyebrow" data-i18n="admin.licenses.eyebrow">INVENTAIRE / LICENCES</span>
      <h2 data-i18n="admin.licenses.title">Licences</h2>
    </div>
    <div class="search">
      <svg viewBox="0 0 24 24" fill="none" width="16" height="16"><circle cx="11" cy="11" r="7" stroke="currentColor" stroke-width="1.6"/><path d="m20 20-3.5-3.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
      <input id="filter" type="search" data-i18n-attr="placeholder:admin.search" placeholder="Cle, produit, plan, proprietaire...">
    </div>
  </header>
  <div id="licenses" class="muted" data-i18n="common.loading">Chargement...</div>
</section>

<div id="toast" class="toast" hidden></div>
<script src="/js/admin.js" defer></script>"#,
        site = escape_html(&config.site_name),
    );

    shell(config, "Panneau admin", "admin.doctitle.dashboard", &body)
}

fn shell(config: &SiteConfig, title: &str, title_key: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<meta name="theme-color" content='#090909'>
<title>{title} - {site}</title>
<link rel="stylesheet" href="/css/theme.css">
<link rel="stylesheet" href="/css/admin.css">
<script src="/js/lang.js"></script>
<script src="/js/i18n.js"></script>
</head>
<body data-site="{site}" data-year="{year}" data-i18n-title="{title_key}">
<div class="noise" aria-hidden="true"></div>
<div class="page">
{body}
  <footer class="footer">
    <span data-i18n="admin.footer">(c) {year} {site} - Console admin</span>
    <a href="/" data-i18n="common.backToSite">Retour au site</a>
  </footer>
</div>
</body>
</html>
"#,
        title = escape_html(title),
        title_key = escape_html(title_key),
        site = escape_html(&config.site_name),
        body = body,
        year = 2026,
    )
}
