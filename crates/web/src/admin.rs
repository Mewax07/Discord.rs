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
        .and_then(|claims| claims.get("role").and_then(Value::as_str).map(str::to_string))
        .map(|role| role == "admin")
        .unwrap_or(false)
}

fn login(request: &Request, config: &SiteConfig, admin: &AdminConfig) -> Response {
    let form = crate::parse_form(&request.body);
    let password = form.get("password").map(String::as_str).unwrap_or("");

    if !constant_time_eq(password, &admin.password) || admin.password.is_empty() {
        return Response::html(401, login_page(config, true));
    }

    let token = session::issue(&config.session_secret, json!({ "role": "admin" }), SESSION_TTL);
    Response::redirect("/admin").header("Set-Cookie", session::set_cookie(COOKIE, &token, SESSION_TTL))
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
        "<p class=\"error\">Mot de passe incorrect.</p>"
    } else {
        ""
    };

    shell(
        config,
        "Connexion admin",
        &format!(
            r#"<div class="login">
      <h1>Panneau d'administration</h1>
      <p class="muted">{site} · accès restreint</p>
      {error}
      <form method="post" action="/admin/login">
        <label>Mot de passe
          <input type="password" name="password" autocomplete="current-password" autofocus required>
        </label>
        <button type="submit">Se connecter</button>
      </form>
    </div>"#,
            site = escape_html(&config.site_name),
        ),
    )
}

fn dashboard(config: &SiteConfig) -> String {
    shell(
        config,
        "Panneau admin",
        &format!(
            r#"<div class="bar">
      <h1>{site} · admin</h1>
      <form method="post" action="/admin/logout"><button class="ghost" type="submit">Déconnexion</button></form>
    </div>
    <section class="cards" id="stats"></section>
    <section class="panel">
      <h2>Visites</h2>
      <div id="visits" class="muted">Chargement…</div>
    </section>
    <section class="panel">
      <div class="panel-head">
        <h2>Licences</h2>
        <input id="filter" type="search" placeholder="Filtrer (clé, produit, propriétaire)…">
      </div>
      <div id="licenses" class="muted">Chargement…</div>
    </section>
    <div id="toast" class="toast" hidden></div>
    <script>{script}</script>"#,
            site = escape_html(&config.site_name),
            script = DASHBOARD_JS,
        ),
    )
}

fn shell(config: &SiteConfig, title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<title>{title} · {site}</title>
<style>{css}</style>
</head>
<body>
<main class="wrap">
{body}
</main>
</body>
</html>
"#,
        title = escape_html(title),
        site = escape_html(&config.site_name),
        css = ADMIN_CSS,
        body = body,
    )
}

const ADMIN_CSS: &str = r#"
:root { color-scheme: dark; --accent: #8B5CF6; --bg: #0d0d11; --surface: #16161c; --line: #26262f; --text: #ececf1; --muted: #9a9aa8; --ok: #34d399; --warn: #fbbf24; --bad: #f87171; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text); font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; line-height: 1.5; }
.wrap { max-width: 1080px; margin: 0 auto; padding: 32px 20px 72px; }
a { color: var(--accent); }
h1 { font-size: 1.5rem; margin: 0; letter-spacing: -0.02em; }
h2 { font-size: 1.05rem; margin: 0 0 14px; }
.muted { color: var(--muted); }
.bar { display: flex; align-items: center; justify-content: space-between; margin-bottom: 28px; gap: 16px; }
button { font: inherit; cursor: pointer; border-radius: 9px; border: 1px solid var(--line); background: var(--accent); color: #fff; padding: 9px 16px; font-weight: 600; }
button.ghost { background: transparent; color: var(--text); }
button.ghost:hover { border-color: var(--accent); }
button.small { padding: 6px 11px; font-size: 0.8rem; font-weight: 500; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 14px; margin-bottom: 28px; }
.card { background: var(--surface); border: 1px solid var(--line); border-radius: 14px; padding: 16px 18px; }
.card .n { font-size: 1.8rem; font-weight: 700; letter-spacing: -0.02em; }
.card .l { color: var(--muted); font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; }
.panel { background: var(--surface); border: 1px solid var(--line); border-radius: 16px; padding: 22px; margin-bottom: 22px; }
.panel-head { display: flex; align-items: center; justify-content: space-between; gap: 14px; margin-bottom: 14px; flex-wrap: wrap; }
.panel-head h2 { margin: 0; }
input[type=search], input[type=password], input[type=text] { font: inherit; background: #0f0f14; border: 1px solid var(--line); color: var(--text); border-radius: 9px; padding: 9px 12px; width: 100%; max-width: 320px; }
table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
th, td { text-align: left; padding: 10px 10px; border-bottom: 1px solid var(--line); vertical-align: top; }
th { color: var(--muted); font-weight: 600; font-size: 0.76rem; text-transform: uppercase; letter-spacing: 0.05em; }
code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.pill { display: inline-block; padding: 2px 9px; border-radius: 999px; font-size: 0.74rem; font-weight: 600; border: 1px solid var(--line); }
.pill.active { color: var(--ok); border-color: #1f5c47; }
.pill.unused { color: var(--muted); }
.pill.expired { color: var(--warn); border-color: #6b551a; }
.pill.revoked { color: var(--bad); border-color: #6b2626; }
.machines { margin: 6px 0 0; padding: 0; list-style: none; color: var(--muted); font-size: 0.8rem; }
.machines li { padding: 2px 0; }
.row-actions { display: flex; gap: 6px; flex-wrap: wrap; }
.bars { display: flex; align-items: flex-end; gap: 4px; height: 90px; margin-top: 10px; }
.bars .b { flex: 1; background: var(--accent); border-radius: 4px 4px 0 0; min-height: 2px; opacity: 0.85; }
.bars .b span { display: block; }
.dl-list { display: grid; gap: 8px; margin-top: 6px; }
.dl-list .dl { display: flex; justify-content: space-between; gap: 12px; border-bottom: 1px dashed var(--line); padding-bottom: 6px; }
.login { max-width: 360px; margin: 12vh auto 0; background: var(--surface); border: 1px solid var(--line); border-radius: 16px; padding: 30px; }
.login h1 { font-size: 1.3rem; }
.login form { margin-top: 18px; display: grid; gap: 14px; }
.login label { display: grid; gap: 6px; font-size: 0.85rem; color: var(--muted); }
.login button { width: 100%; padding: 11px; }
.error { color: var(--bad); font-size: 0.88rem; margin: 12px 0 0; }
.toast { position: fixed; bottom: 22px; left: 50%; transform: translateX(-50%); background: #1f1f28; border: 1px solid var(--line); padding: 12px 18px; border-radius: 12px; font-size: 0.88rem; }
@media (max-width: 640px) { table, thead, tbody, th, td, tr { display: block; } th { display: none; } td { border: none; padding: 3px 0; } tr { border-bottom: 1px solid var(--line); padding: 12px 0; } }
"#;

const DASHBOARD_JS: &str = r#"
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[c]));
let all = [];
function toast(msg) { const t = document.getElementById("toast"); t.textContent = msg; t.hidden = false; clearTimeout(t._h); t._h = setTimeout(() => t.hidden = true, 2600); }
async function load() {
  const res = await fetch("/admin/data", { headers: { "Accept": "application/json" } });
  if (res.status === 401) { location.reload(); return; }
  const data = await res.json();
  renderStats(data.stats);
  renderVisits(data.visits);
  all = data.licenses || [];
  renderLicenses();
}
function renderStats(s) {
  const cells = [["Total", s.total], ["Actives", s.active], ["Inutilisées", s.unused], ["Expirées", s.expired], ["Révoquées", s.revoked], ["Machines", s.machines]];
  document.getElementById("stats").innerHTML = cells.map(([l, n]) => `<div class="card"><div class="n">${n}</div><div class="l">${l}</div></div>`).join("");
}
function renderVisits(v) {
  const box = document.getElementById("visits");
  if (v.disabled) { box.textContent = "Suivi des visites désactivé."; return; }
  const days = v.days || {};
  const keys = Object.keys(days).sort().slice(-14);
  const max = Math.max(1, ...keys.map((k) => days[k]));
  const bars = keys.map((k) => `<div class="b" style="height:${Math.round((days[k] / max) * 100)}%" title="${k}: ${days[k]}"></div>`).join("");
  const dls = Object.entries(v.downloads || {}).sort((a, b) => b[1] - a[1]);
  const dlHtml = dls.length ? `<div class="dl-list">${dls.map(([id, n]) => `<div class="dl"><code>${esc(id)}</code><strong>${n}</strong></div>`).join("")}</div>` : '<p class="muted">Aucun téléchargement compté.</p>';
  box.innerHTML = `
    <div class="cards">
      <div class="card"><div class="n">${v.total}</div><div class="l">Visites totales</div></div>
      <div class="card"><div class="n">${v.today}</div><div class="l">Aujourd'hui</div></div>
    </div>
    <p class="muted" style="margin:14px 0 0">14 derniers jours</p>
    <div class="bars">${bars || '<span class="muted">Pas encore de données.</span>'}</div>
    <h2 style="margin-top:22px">Téléchargements</h2>
    ${dlHtml}`;
}
function statusPill(s) { return `<span class="pill ${esc(s)}">${esc(s)}</span>`; }
function renderLicenses() {
  const q = (document.getElementById("filter").value || "").toLowerCase();
  const rows = all.filter((l) => !q || [l.key_prefix, l.product, l.plan, l.owner_id].some((f) => String(f ?? "").toLowerCase().includes(q)));
  if (!rows.length) { document.getElementById("licenses").innerHTML = '<p class="muted">Aucune licence.</p>'; return; }
  const body = rows.map((l) => {
    const machines = (l.activations || []).map((a) => `<li>💻 <code>${esc(a.hwid_short)}</code> · vu ${esc(a.last_seen_label)} · ${a.checks} checks</li>`).join("");
    const owner = l.owner_id ? `<code>${esc(l.owner_id)}</code>` : '<span class="muted">—</span>';
    const expiry = l.lifetime ? "à vie" : (l.expires_label || "—");
    const actions = `<div class="row-actions">
      ${l.revoked ? `<button class="ghost small" onclick="act('${esc(l.key_prefix)}','restore')">Restaurer</button>` : `<button class="ghost small" onclick="act('${esc(l.key_prefix)}','revoke')">Révoquer</button>`}
      <button class="ghost small" onclick="act('${esc(l.key_prefix)}','reset-hwid')">Reset HWID</button>
      <button class="ghost small" onclick="assign('${esc(l.key_prefix)}')">Assigner</button>
    </div>`;
    return `<tr>
      <td><code>${esc(l.key_prefix)}</code><br>${statusPill(l.status)}</td>
      <td>${esc(l.product)}<br><span class="muted">${esc(l.plan)}</span></td>
      <td>${owner}</td>
      <td>${l.machines_used}/${l.machines_allowed}${machines ? `<ul class="machines">${machines}</ul>` : ""}</td>
      <td>${esc(expiry)}<br><span class="muted">${esc(l.created_label)}</span></td>
      <td>${actions}</td>
    </tr>`;
  }).join("");
  document.getElementById("licenses").innerHTML = `<div style="overflow-x:auto"><table>
    <thead><tr><th>Clé</th><th>Produit</th><th>Propriétaire</th><th>Machines</th><th>Expiration</th><th>Actions</th></tr></thead>
    <tbody>${body}</tbody></table></div>`;
}
async function act(ref, action, extra) {
  if (action === "revoke" && !confirm("Révoquer cette licence ?")) return;
  if (action === "reset-hwid" && !confirm("Réinitialiser les machines liées ?")) return;
  const res = await fetch(`/admin/license/${encodeURIComponent(ref)}/${action}`, {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(extra || {})
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) { toast(data.message || data.error || "Erreur"); return; }
  toast("Fait.");
  await load();
}
function assign(ref) {
  const owner = prompt("ID Discord du propriétaire (vide pour retirer) :", "");
  if (owner === null) return;
  act(ref, "assign", { owner_id: owner });
}
document.getElementById("filter").addEventListener("input", renderLicenses);
load();
setInterval(load, 30000);
"#;
