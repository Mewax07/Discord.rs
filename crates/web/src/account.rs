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
    if request.path == "/account/roulette" || request.path == "/account/roulette/spin" {
        return crate::roulette::handle(request, config);
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/account") | ("GET", "/account/") => page(request, config),
        ("GET", "/account/login") => login(config),
        ("GET", "/account/callback") => callback(request, config),
        ("POST", "/account/logout") | ("GET", "/account/logout") => logout(),
        _ => crate::not_found(config),
    }
}

pub(crate) fn session_user(request: &Request, config: &SiteConfig) -> Option<Value> {
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

    Response::redirect(&oauth.authorize_url(&state)).header(
        "Set-Cookie",
        session::set_cookie(STATE_COOKIE, &state, STATE_TTL),
    )
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
        .header(
            "Set-Cookie",
            session::set_cookie(USER_COOKIE, &cookie_value, SESSION_TTL),
        )
        .header("Set-Cookie", session::clear_cookie(STATE_COOKIE))
}

fn logout() -> Response {
    Response::redirect("/account").header("Set-Cookie", session::clear_cookie(USER_COOKIE))
}

fn page(request: &Request, config: &SiteConfig) -> Response {
    if config.oauth.is_none() {
        return Response::html(200, shell(config, &unconfigured_body(), None, None));
    }

    match session_user(request, config) {
        Some(user) => Response::html(
            200,
            shell(config, &account_body(config, &user), Some(&user), Some("licenses")),
        ),
        None => Response::html(200, shell(config, &signed_out_body(config), None, None)),
    }
}

fn error_page(config: &SiteConfig, message: &str) -> Response {
    let body = format!(
        r#"<div class="auth-panel">
  <div class="auth-icon error">
    <svg viewBox="0 0 24 24" fill="none"><path d="M12 9v4M12 17h.01M10.3 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.7 3.86a2 2 0 0 0-3.4 0Z" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>
  </div>
  <span class="eyebrow" data-i18n="account.error.eyebrow">ERREUR / CONNEXION</span>
  <h1 data-i18n="account.error.title">Un souci est survenu</h1>
  <p class="muted">{message}</p>
  <a class="btn primary" href="/account/login" data-i18n="account.error.retry">Reessayer</a>
</div>"#,
        message = escape_html(message),
    );
    Response::html(400, shell(config, &body, None, None))
}

fn unconfigured_body() -> String {
    r#"<div class="auth-panel">
  <div class="auth-icon">
    <svg viewBox="0 0 24 24" fill="none"><path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" stroke="currentColor" stroke-width="1.6"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 8.98 19.3a1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.62 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.62 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09A1.7 1.7 0 0 0 15.06 4.65a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9c.15.62.7 1.06 1.34 1.06H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.03Z" stroke="currentColor" stroke-width="1.4"/></svg>
  </div>
  <span class="eyebrow" data-i18n="account.unconfigured.eyebrow">ESPACE UTILISATEUR</span>
  <h1 data-i18n="account.unconfigured.title">Bientot disponible</h1>
  <p class="muted" data-i18n="account.unconfigured.text">La connexion Discord n'est pas encore configuree sur ce serveur. Reviens un peu plus tard.</p>
  <a class="btn ghost" href="/" data-i18n="account.unconfigured.home">Retour a l'accueil</a>
</div>"#
        .to_string()
}

fn signed_out_body(config: &SiteConfig) -> String {
    format!(
        r#"<div class="auth-panel">
  <div class="auth-icon discord">
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M20.3 4.5A19 19 0 0 0 15.5 3l-.24.5a14 14 0 0 1 4.2 2.1A15.8 15.8 0 0 0 12 4.6 15.8 15.8 0 0 0 4.5 5.6a14 14 0 0 1 4.2-2.1L8.5 3A19 19 0 0 0 3.7 4.5C1.4 8 .8 11.4 1.1 14.8a19 19 0 0 0 5.8 2.9l.7-1a12 12 0 0 1-1.9-.9l.5-.3a13.6 13.6 0 0 0 11.6 0l.5.3c-.6.4-1.2.7-1.9.9l.7 1a19 19 0 0 0 5.8-2.9c.4-4-.6-7.4-2.3-10.3ZM8.4 12.9c-.9 0-1.7-.9-1.7-1.9s.8-1.9 1.7-1.9 1.7.9 1.7 1.9-.8 1.9-1.7 1.9Zm7.2 0c-.9 0-1.7-.9-1.7-1.9s.8-1.9 1.7-1.9 1.7.9 1.7 1.9-.8 1.9-1.7 1.9Z"/></svg>
  </div>
  <span class="eyebrow" data-i18n="account.signedOut.eyebrow">ESPACE UTILISATEUR / CONNEXION</span>
  <h1 data-i18n-html="account.signedOut.title">Vos licences <em>{site}</em></h1>
  <p class="muted" data-i18n="account.signedOut.text">Connectez-vous avec Discord pour retrouver vos licences, vos cles et les machines qui leur sont associees. Aucun mot de passe, aucune donnee partagee au-dela de votre identifiant.</p>
  <a class="btn primary discord" href="/account/login">
    <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M20.3 4.5A19 19 0 0 0 15.5 3l-.24.5a14 14 0 0 1 4.2 2.1A15.8 15.8 0 0 0 12 4.6 15.8 15.8 0 0 0 4.5 5.6a14 14 0 0 1 4.2-2.1L8.5 3A19 19 0 0 0 3.7 4.5C1.4 8 .8 11.4 1.1 14.8a19 19 0 0 0 5.8 2.9l.7-1a12 12 0 0 1-1.9-.9l.5-.3a13.6 13.6 0 0 0 11.6 0l.5.3c-.6.4-1.2.7-1.9.9l.7 1a19 19 0 0 0 5.8-2.9c.4-4-.6-7.4-2.3-10.3ZM8.4 12.9c-.9 0-1.7-.9-1.7-1.9s.8-1.9 1.7-1.9 1.7.9 1.7 1.9-.8 1.9-1.7 1.9Zm7.2 0c-.9 0-1.7-.9-1.7-1.9s.8-1.9 1.7-1.9 1.7.9 1.7 1.9-.8 1.9-1.7 1.9Z"/></svg>
    <span data-i18n="account.signedOut.button">Se connecter avec Discord</span>
  </a>
  <p class="fineprint" data-i18n-html="account.signedOut.fine">Nous lisons uniquement votre identifiant Discord (<code>identify</code>).</p>
</div>"#,
        site = escape_html(&config.site_name),
    )
}

fn account_body(config: &SiteConfig, user: &Value) -> String {
    let id = user.get("id").and_then(Value::as_str).unwrap_or("");
    let name = user
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Utilisateur");
    let avatar = user.get("avatar").and_then(Value::as_str).unwrap_or("");

    let licenses = config
        .licenses
        .as_ref()
        .map(|service| service.for_owner(id))
        .unwrap_or_default();

    let now = crate::now_secs();

    let active = licenses
        .iter()
        .filter(|l| !l.revoked && (l.is_lifetime() || l.remaining(now).map_or(false, |s| s > 0)))
        .count();
    let expired = licenses.len().saturating_sub(active);
    let machines: usize = licenses.iter().map(|l| l.activations.len()).sum();

    let cards = if licenses.is_empty() {
        r#"<div class="empty-state">
  <div class="empty-icon">
    <svg viewBox="0 0 24 24" fill="none"><path d="M4 7.5 12 3l8 4.5v9L12 21l-8-4.5v-9Z" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M4 7.5 12 12l8-4.5M12 12v9" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/></svg>
  </div>
  <h3 data-i18n="account.empty.title">Aucune licence pour le moment</h3>
  <p class="muted" data-i18n="account.empty.text">Aucune licence n'est associee a ce compte Discord. Si tu viens d'en recevoir une, patiente quelques minutes ou contacte le support.</p>
  <a class="btn ghost" href="/#products" data-i18n="account.empty.offers">Voir les offres</a>
</div>"#
            .to_string()
    } else {
        licenses
            .iter()
            .map(license_card)
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"<section class="page-head">
  <span class="eyebrow" data-i18n="account.hero.eyebrow">ESPACE UTILISATEUR / TABLEAU DE BORD</span>
  <h1 data-i18n-html="account.hero.hello" data-v-name="{name}">Bonjour, <em>{name}</em></h1>
  <p class="lead" data-i18n="account.hero.sub">Toutes vos licences {site} et les machines activees, reunies au meme endroit.</p>
</section>

<section class="stats">
  <article class="stat"><span class="stat-label" data-i18n="account.stat.total">Licence(s) au total</span><strong>{total}</strong></article>
  <article class="stat"><span class="stat-label" data-i18n="account.stat.active">Active(s)</span><strong>{active}</strong></article>
  <article class="stat"><span class="stat-label" data-i18n="account.stat.expired">Expiree(s) / revoquee(s)</span><strong>{expired}</strong></article>
  <article class="stat"><span class="stat-label" data-i18n="account.stat.machines">Machine(s) activee(s)</span><strong>{machines}</strong></article>
</section>

<section class="block">
  <div class="block-head">
    <div>
      <span class="eyebrow" data-i18n="account.section.eyebrow">VOS CLES / DETAIL</span>
      <h2 data-i18n="account.section.title">Licences et machines</h2>
    </div>
    <p class="muted" data-i18n="account.section.text">Les machines listees sont celles activees avec chaque cle. Une cle ne peut etre active que sur un nombre limite d'appareils.</p>
  </div>
  <div class="grid">
    {cards}
  </div>
</section>"#,
        name = escape_html(name),
        site = escape_html(&config.site_name),
        total = licenses.len(),
        active = active,
        expired = expired,
        machines = machines,
        cards = cards,
    )
}

fn license_card(license: &License) -> String {
    let now = crate::now_secs();
    let status = license.status(now).as_str();

    let (status_label, status_class) = match status {
        "active" => ("Active", "active"),
        "unused" => ("Non utilisee", "unused"),
        "expired" => ("Expiree", "expired"),
        "revoked" => ("Revoquee", "revoked"),
        _ => (status, "unused"),
    };
    let status_key = format!("status.{status_class}");

    // Chaque libelle porte sa cle et ses variables brutes : le runtime i18n
    // les remet en forme dans la langue choisie sans nouvel aller-retour.
    let expiry_main;
    let expiry_main_attrs;
    let expiry_sub;
    let expiry_sub_attrs;

    if license.revoked {
        expiry_main = "Acces revoque".to_string();
        expiry_main_attrs = " data-i18n=\"account.expiry.revoked\"".to_string();
        expiry_sub = "Contacte le support si c'est une erreur.".to_string();
        expiry_sub_attrs = " data-i18n=\"account.expiry.revokedSub\"".to_string();
    } else if license.is_lifetime() {
        expiry_main = "Licence a vie".to_string();
        expiry_main_attrs = " data-i18n=\"account.expiry.lifetime\"".to_string();
        expiry_sub = "Aucune expiration prevue.".to_string();
        expiry_sub_attrs = " data-i18n=\"account.expiry.lifetimeSub\"".to_string();
    } else {
        match license.remaining(now) {
            Some(0) | None => {
                expiry_main = "Expiree".to_string();
                expiry_main_attrs = " data-i18n=\"account.expiry.expired\"".to_string();
            }
            Some(secs) => {
                expiry_main = format!("Expire dans {}", human_duration(secs));
                expiry_main_attrs = format!(" data-i18n=\"account.expiry.expiresIn\" data-dur=\"{secs}\"");
            }
        }

        match license.expires_at {
            Some(date) => {
                let label = human_date(date);
                expiry_sub_attrs = format!(
                    " data-i18n=\"account.expiry.on\" data-v-date=\"{}\"",
                    escape_html(&label)
                );
                expiry_sub = format!("Le {label}");
            }
            None => {
                expiry_sub = String::new();
                expiry_sub_attrs = String::new();
            }
        }
    }

    let used = license.activations.len();
    let allowed = license.max_activations;
    let pct = if allowed == 0 {
        0
    } else {
        ((used as f32 / allowed as f32) * 100.0).min(100.0) as u32
    };
    let full_class = if used >= allowed as usize { "full" } else { "" };

    let machines = if license.activations.is_empty() {
        r#"<div class="empty-machines">
          <svg viewBox="0 0 24 24" fill="none"><rect x="3" y="4" width="18" height="12" rx="2" stroke="currentColor" stroke-width="1.4"/><path d="M8 20h8M12 16v4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>
          <span data-i18n="account.machine.none">Aucune machine activee pour le moment.</span>
        </div>"#
            .to_string()
    } else {
        let rows = license
            .activations
            .iter()
            .map(|activation| {
                format!(
                    r#"<li class="machine">
  <div class="machine-icon">
    <svg viewBox="0 0 24 24" fill="none"><rect x="3" y="4" width="18" height="12" rx="2" stroke="currentColor" stroke-width="1.4"/><path d="M8 20h8M12 16v4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>
  </div>
  <div class="machine-body">
    <code class="machine-hwid">{hwid}</code>
    <span class="machine-meta" data-i18n="account.machine.meta" data-v-first="{first}" data-v-last="{last}">Activee le {first} - vue le {last}</span>
  </div>
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
        r#"<article class="license-card">
  <header class="license-head">
    <div class="license-title">
      <h3>{product}</h3>
      <span class="license-plan">{plan} <span data-i18n="account.license.key">cle</span> <code>{prefix}...</code></span>
    </div>
    <span class="pill {status_class}"><i></i><span data-i18n="{status_key}">{status_label}</span></span>
  </header>

  <div class="license-meta">
    <div class="meta-block">
      <span class="meta-label" data-i18n="account.label.status">STATUT</span>
      <strong class="meta-value"{expiry_main_attrs}>{expiry_main}</strong>
      <small class="meta-sub"{expiry_sub_attrs}>{expiry_sub}</small>
    </div>
    <div class="meta-block">
      <span class="meta-label" data-i18n="account.label.machines">MACHINES</span>
      <strong class="meta-value">{used} / {allowed}</strong>
      <div class="progress {full_class}"><i style="width:{pct}%"></i></div>
    </div>
  </div>

  <div class="machine-block">
    <div class="machine-head">
      <span class="meta-label" data-i18n="account.label.devices">APPAREILS ASSOCIES</span>
    </div>
    {machines}
  </div>
</article>"#,
        product = escape_html(&license.product),
        plan = escape_html(&license.plan),
        prefix = escape_html(&license.key_prefix),
        status_class = status_class,
        status_key = status_key,
        status_label = status_label,
        expiry_main = escape_html(&expiry_main),
        expiry_main_attrs = expiry_main_attrs,
        expiry_sub = escape_html(&expiry_sub),
        expiry_sub_attrs = expiry_sub_attrs,
        used = used,
        allowed = allowed,
        pct = pct,
        full_class = full_class,
        machines = machines,
    )
}

fn short_hwid(hwid: &str) -> String {
    if hwid.len() <= 14 {
        hwid.to_string()
    } else {
        format!("...{}", &hwid[hwid.len() - 12..])
    }
}

pub(crate) fn shell(
    config: &SiteConfig,
    body: &str,
    user: Option<&Value>,
    active: Option<&str>,
) -> String {
    let topbar_right = match user {
        Some(u) => {
            let name = u
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Utilisateur");
            let avatar = u.get("avatar").and_then(Value::as_str).unwrap_or("");
            let avatar_html = if avatar.is_empty() {
                format!(
                    "<span class=\"chip-avatar fallback\">{}</span>",
                    escape_html(
                        &name
                            .chars()
                            .next()
                            .unwrap_or('U')
                            .to_string()
                            .to_uppercase()
                    )
                )
            } else {
                format!(
                    "<img class=\"chip-avatar\" src=\"{}\" alt=\"\" width=\"32\" height=\"32\" loading=\"lazy\">",
                    escape_html(avatar)
                )
            };
            format!(
                r#"<div class="profile-chip">
      {avatar}
      <div class="chip-text">
        <strong>{name}</strong>
        <small data-i18n="account.discord">Discord</small>
      </div>
    </div>
    <form method="post" action="/account/logout" class="logout-form">
      <button class="btn ghost sm" type="submit">
        <svg viewBox="0 0 24 24" fill="none" width="14" height="14"><path d="M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3M10 17l-5-5 5-5M5 12h11" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>
        <span data-i18n="common.logout">Deconnexion</span>
      </button>
    </form>"#,
                avatar = avatar_html,
                name = escape_html(name),
            )
        }
        None => String::new(),
    };

    let nav = if user.is_some() {
        let tab = |key: &str, href: &str, i18n: &str, label: &str| {
            format!(
                r#"<a class="tab{on}" href="{href}" data-i18n="{i18n}">{label}</a>"#,
                on = if active == Some(key) { " is-active" } else { "" },
            )
        };
        format!(
            r#"<nav class="tabs" aria-label="Espace utilisateur">{}{}</nav>"#,
            tab("licenses", "/account", "account.nav.licenses", "Licences"),
            tab("roulette", "/account/roulette", "account.nav.roulette", "Roulette"),
        )
    } else {
        String::new()
    };

    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta name="robots" content="noindex, nofollow">
    <meta name="theme-color" content='#090909'>
    <title>Mon compte - {site}</title>
    <link rel="stylesheet" href="/css/theme.css">
    <link rel="stylesheet" href="/css/account.css">
    <script src="/js/lang.js"></script>
    <script src="/js/i18n.js"></script>
</head>
<body data-site="{site}" data-year="{year}" data-i18n-title="account.doctitle">
    <div class="noise" aria-hidden="true"></div>
    <div class="page">
        <header class="topbar">
            <a class="brand" href="/">
            <span class="brand-mark">
                <img src="/assets/Logo.png" alt=""/>
            </span>
            <span class="brand-text">{site}</span>
            </a>
            <div class="topbar-right">
                <span data-lang-switch></span>
                {topbar_right}
            </div>
        </header>
        
        {nav}
        <main class="wrap">
            {body}
        </main>
        
        <footer class="footer">
            <span data-i18n="account.footer">(c) {year} {site} - Espace utilisateur</span>
            <a href="/" data-i18n="common.backToSite">Retour au site</a>
        </footer>
    </div>
</body>
</html>
"#,
        site = escape_html(&config.site_name),
        topbar_right = topbar_right,
        nav = nav,
        body = body,
        year = 2026,
    )
}
