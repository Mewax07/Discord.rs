//! The weekly roulette: one free spin per week (or pay with coins earned
//! from spins and invites) for a chance at a cosmetic, an emote, coins or a
//! temporary licence key. Requires an activated licence, since a cosmetic or
//! emote reward is attached to the account's most recently seen hardware id.

use std::fs;
use std::path::Path;

use httpd::{escape_html, Request, Response};
use serde::Deserialize;
use serde_json::{json, Value};

use licensing::IssueRequest;

use crate::account::{session_user, shell};
use crate::economy::UserEconomy;
use crate::render::human_duration;
use crate::{now_secs, SiteConfig};

#[derive(Clone, Deserialize)]
pub struct CatalogItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Deserialize)]
pub struct Catalog {
    #[serde(default = "default_spin_cost")]
    pub spin_cost_coins: u64,
    #[serde(default = "default_free_interval_days")]
    pub free_spin_interval_days: u64,
    #[serde(default = "default_coin_min")]
    pub coin_reward_min: u64,
    #[serde(default = "default_coin_max")]
    pub coin_reward_max: u64,
    #[serde(default = "default_weight_nothing")]
    pub weight_nothing: u64,
    #[serde(default = "default_weight_coins")]
    pub weight_coins: u64,
    #[serde(default = "default_weight_emote")]
    pub weight_emote: u64,
    #[serde(default = "default_weight_cosmetic")]
    pub weight_cosmetic: u64,
    #[serde(default = "default_weight_license")]
    pub weight_license: u64,
    #[serde(default = "default_license_weight_7")]
    pub license_weight_7d: u64,
    #[serde(default = "default_license_weight_14")]
    pub license_weight_14d: u64,
    #[serde(default = "default_license_weight_30")]
    pub license_weight_30d: u64,
    #[serde(default)]
    pub emotes: Vec<CatalogItem>,
    #[serde(default)]
    pub cosmetics: Vec<CatalogItem>,
}

fn default_spin_cost() -> u64 {
    50
}
fn default_free_interval_days() -> u64 {
    7
}
fn default_coin_min() -> u64 {
    5
}
fn default_coin_max() -> u64 {
    20
}
fn default_weight_nothing() -> u64 {
    45
}
fn default_weight_coins() -> u64 {
    25
}
fn default_weight_emote() -> u64 {
    15
}
fn default_weight_cosmetic() -> u64 {
    10
}
fn default_weight_license() -> u64 {
    5
}
fn default_license_weight_7() -> u64 {
    80
}
fn default_license_weight_14() -> u64 {
    15
}
fn default_license_weight_30() -> u64 {
    5
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            spin_cost_coins: default_spin_cost(),
            free_spin_interval_days: default_free_interval_days(),
            coin_reward_min: default_coin_min(),
            coin_reward_max: default_coin_max(),
            weight_nothing: default_weight_nothing(),
            weight_coins: default_weight_coins(),
            weight_emote: default_weight_emote(),
            weight_cosmetic: default_weight_cosmetic(),
            weight_license: default_weight_license(),
            license_weight_7d: default_license_weight_7(),
            license_weight_14d: default_license_weight_14(),
            license_weight_30d: default_license_weight_30(),
            emotes: Vec::new(),
            cosmetics: Vec::new(),
        }
    }
}

impl Catalog {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy)]
enum Tier {
    Nothing,
    Coins,
    Emote,
    Cosmetic,
    License,
}

enum Outcome {
    Nothing,
    Coins(u64),
    Item { kind: &'static str, name: String },
    License { days: u64, key: String },
}

pub fn handle(request: &Request, config: &SiteConfig) -> Response {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/account/roulette") => page(request, config, None),
        ("POST", "/account/roulette/spin") => spin(request, config),
        _ => crate::not_found(config),
    }
}

fn roll(
    config: &SiteConfig,
    economy: &crate::economy::EconomyStore,
    catalog: &Catalog,
    user_id: &str,
    hwid: &str,
    now: u64,
) -> Outcome {
    let tier = weighted_pick(&[
        (catalog.weight_nothing, Tier::Nothing),
        (catalog.weight_coins, Tier::Coins),
        (catalog.weight_emote, Tier::Emote),
        (catalog.weight_cosmetic, Tier::Cosmetic),
        (catalog.weight_license, Tier::License),
    ])
    .unwrap_or(Tier::Nothing);

    match tier {
        Tier::Nothing => Outcome::Nothing,
        Tier::Coins => {
            let amount = random_range(catalog.coin_reward_min, catalog.coin_reward_max);
            economy.credit_coins(user_id, amount);
            Outcome::Coins(amount)
        }
        Tier::Emote => match pick_item(&catalog.emotes) {
            Some(item) => {
                economy.grant_cosmetic(user_id, &item.id, "emote", hwid, now);
                Outcome::Item {
                    kind: "emote",
                    name: item.name.clone(),
                }
            }
            None => Outcome::Nothing,
        },
        Tier::Cosmetic => match pick_item(&catalog.cosmetics) {
            Some(item) => {
                economy.grant_cosmetic(user_id, &item.id, "cosmetic", hwid, now);
                Outcome::Item {
                    kind: "cosmetic",
                    name: item.name.clone(),
                }
            }
            None => Outcome::Nothing,
        },
        Tier::License => {
            let days = weighted_pick(&[
                (catalog.license_weight_7d, 7u64),
                (catalog.license_weight_14d, 14),
                (catalog.license_weight_30d, 30),
            ])
            .unwrap_or(7);

            let Some(licenses) = &config.licenses else {
                return Outcome::Nothing;
            };
            let issued = licenses.issue(
                IssueRequest::new(config.product.clone(), format!("roulette-{days}d"))
                    .duration(Some(days * 86_400))
                    .machines(1)
                    .owner(Some(user_id.to_string()))
                    .note(Some("Roulette reward".to_string())),
            );
            Outcome::License {
                days,
                key: issued.key,
            }
        }
    }
}

fn latest_hwid(config: &SiteConfig, user_id: &str) -> Option<String> {
    let licenses = config.licenses.as_ref()?.for_owner(user_id);
    licenses
        .iter()
        .flat_map(|license| license.activations.iter())
        .max_by_key(|activation| activation.last_seen)
        .map(|activation| activation.hwid.clone())
}

fn weighted_pick<T: Copy>(options: &[(u64, T)]) -> Option<T> {
    let total: u64 = options.iter().map(|(weight, _)| *weight).sum();
    if total == 0 {
        return None;
    }
    let roll = random_u64() % total;
    let mut acc = 0u64;
    for (weight, value) in options {
        acc += weight;
        if roll < acc {
            return Some(*value);
        }
    }
    None
}

fn pick_item(items: &[CatalogItem]) -> Option<&CatalogItem> {
    if items.is_empty() {
        return None;
    }
    let index = (random_u64() as usize) % items.len();
    items.get(index)
}

fn random_range(min: u64, max: u64) -> u64 {
    if max <= min {
        return min;
    }
    min + random_u64() % (max - min + 1)
}

fn random_u64() -> u64 {
    let bytes = licensing::crypto::random_bytes(8);
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes);
    u64::from_le_bytes(buf)
}

struct View {
    has_hwid: bool,
    can_spin: bool,
    free_in: u64,
}

fn view(economy: &UserEconomy, catalog: &Catalog, has_hwid: bool, now: u64) -> View {
    let free_ready = economy.next_free_spin_at <= now;
    View {
        has_hwid,
        can_spin: has_hwid && (free_ready || economy.coins >= catalog.spin_cost_coins),
        free_in: economy.next_free_spin_at.saturating_sub(now),
    }
}

fn wants_json(request: &Request) -> bool {
    request
        .header("accept")
        .is_some_and(|value| value.contains("application/json"))
}

fn deny(request: &Request, config: &SiteConfig, reason: &str) -> Response {
    if wants_json(request) {
        Response::json(403, &json!({ "ok": false, "reason": reason }))
    } else {
        page(request, config, None)
    }
}

fn spin(request: &Request, config: &SiteConfig) -> Response {
    let Some(user) = session_user(request, config) else {
        if wants_json(request) {
            return Response::json(401, &json!({ "ok": false, "reason": "auth" }));
        }
        return Response::redirect("/account");
    };
    let Some(economy) = &config.economy else {
        return deny(request, config, "unavailable");
    };
    let id = user.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let catalog = Catalog::load(&config.roulette_path);
    let now = now_secs();

    let Some(hwid) = latest_hwid(config, &id) else {
        return deny(request, config, "hwid");
    };

    if economy
        .try_spend_spin(
            &id,
            catalog.spin_cost_coins,
            catalog.free_spin_interval_days * 86_400,
            now,
        )
        .is_err()
    {
        return deny(request, config, "funds");
    }

    let outcome = roll(config, economy, &catalog, &id, &hwid, now);

    if wants_json(request) {
        let snapshot = economy.snapshot(&id);
        let state = view(&snapshot, &catalog, true, now);
        return Response::json(
            200,
            &json!({
                "ok": true,
                "outcome": outcome_json(&outcome),
                "coins": snapshot.coins,
                "spins": snapshot.spins_done,
                "free_in": state.free_in,
                "can_spin": state.can_spin,
            }),
        );
    }

    page(request, config, Some(outcome))
}

fn outcome_json(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Nothing => json!({ "kind": "nothing" }),
        Outcome::Coins(amount) => json!({ "kind": "coins", "amount": amount }),
        Outcome::Item { kind, name } => json!({ "kind": kind, "name": name }),
        Outcome::License { days, key } => json!({ "kind": "license", "days": days, "key": key }),
    }
}

fn pool_json(catalog: &Catalog) -> String {
    let names = |items: &[CatalogItem]| items.iter().map(|i| i.name.clone()).collect::<Vec<_>>();
    json!({
        "coins": [catalog.coin_reward_min, catalog.coin_reward_max],
        "emotes": names(&catalog.emotes),
        "cosmetics": names(&catalog.cosmetics),
        "licenses": [7, 14, 30],
        "weights": {
            "nothing": catalog.weight_nothing,
            "coins": catalog.weight_coins,
            "emote": if catalog.emotes.is_empty() { 0 } else { catalog.weight_emote },
            "cosmetic": if catalog.cosmetics.is_empty() { 0 } else { catalog.weight_cosmetic },
            "license": catalog.weight_license,
        },
    })
    .to_string()
}

fn page(request: &Request, config: &SiteConfig, outcome: Option<Outcome>) -> Response {
    let Some(user) = session_user(request, config) else {
        return Response::redirect("/account");
    };
    let id = user.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let name = user
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Utilisateur");

    let economy = config
        .economy
        .as_ref()
        .map(|store| store.snapshot(&id))
        .unwrap_or_default();
    let catalog = Catalog::load(&config.roulette_path);
    let now = now_secs();
    let state = view(&economy, &catalog, latest_hwid(config, &id).is_some(), now);

    Response::html(
        200,
        shell(
            config,
            &body(name, &economy, &catalog, &state, outcome),
            Some(&user),
            Some("roulette"),
        ),
    )
}

fn body(
    name: &str,
    economy: &UserEconomy,
    catalog: &Catalog,
    state: &View,
    outcome: Option<Outcome>,
) -> String {
    let free_label = if state.free_in == 0 {
        r#"<span data-i18n="account.roulette.freeNow">Disponible</span>"#.to_string()
    } else {
        format!(
            r#"<span data-i18n="account.roulette.freeIn" data-dur="{secs}">{label}</span>"#,
            secs = state.free_in,
            label = escape_html(&human_duration(state.free_in)),
        )
    };

    let hint_html = if !state.has_hwid {
        r#"<span data-i18n="account.roulette.needHwid">Active une licence sur une machine pour pouvoir tourner la roue.</span>"#
    } else if !state.can_spin {
        r#"<span data-i18n="account.roulette.cantSpin">Pas de tour gratuit disponible et pas assez de coins.</span>"#
    } else {
        ""
    };

    let result_html = outcome.map(outcome_html).unwrap_or_default();
    let history = history_html(economy, catalog);
    let prizes = preview(catalog);

    format!(
        r#"<section class="page-head">
  <span class="eyebrow" data-i18n="account.roulette.eyebrow">ROULETTE HEBDOMADAIRE</span>
  <h1 data-i18n-html="account.roulette.hello" data-v-name="{name}">Salut, <em>{name}</em></h1>
  <p class="lead" data-i18n="account.roulette.sub">Un tour gratuit chaque semaine, ou depense des coins pour retenter ta chance.</p>
</section>

<section class="stats">
  <article class="stat"><span class="stat-label" data-i18n="account.roulette.coins">Coins</span><strong id="stat-coins">{coins}</strong></article>
  <article class="stat"><span class="stat-label" data-i18n="account.roulette.spinsDone">Tours joues</span><strong id="stat-spins">{spins}</strong></article>
  <article class="stat"><span class="stat-label" data-i18n="account.roulette.nextFree">Prochain tour gratuit</span><strong id="stat-free">{free_label}</strong></article>
</section>

<section class="panel roulette" id="roulette" data-pool="{pool}" data-cost="{cost}">
  <div class="reel" id="reel" aria-hidden="true">
    <div class="reel-strip" id="strip"></div>
    <div class="reel-marker"></div>
    <div class="reel-fade left"></div>
    <div class="reel-fade right"></div>
  </div>
  <div class="reel-result" id="result"{result_hidden}>{result_html}</div>
  <form class="reel-actions" id="spin-form" method="post" action="/account/roulette/spin">
    <button class="btn primary lg" id="spin-btn" type="submit"{disabled}><span data-i18n="account.roulette.spin.button">Tourner la roue</span></button>
    <span class="muted" data-i18n="account.roulette.spin.text" data-v-cost="{cost}">Un tour gratuit par semaine, sinon {cost} coins.</span>
  </form>
  <p class="reel-hint muted" id="spin-hint">{hint_html}</p>
</section>

<section class="block">
  <div class="block-head"><h2 data-i18n="account.roulette.prizes.title">Ce que tu peux gagner</h2></div>
  <div class="prizes">{prizes}</div>
</section>

<section class="block">
  <div class="block-head"><h2 data-i18n="account.roulette.history.title">Tes gains cosmetiques</h2></div>
  {history}
</section>

<script src="/js/roulette.js" defer></script>"#,
        name = escape_html(name),
        coins = economy.coins,
        spins = economy.spins_done,
        free_label = free_label,
        pool = escape_html(&pool_json(catalog)),
        cost = catalog.spin_cost_coins,
        disabled = if state.can_spin { "" } else { " disabled" },
        hint_html = hint_html,
        result_hidden = if result_html.is_empty() { " hidden" } else { "" },
        result_html = result_html,
        prizes = prizes,
        history = history,
    )
}

fn history_html(economy: &UserEconomy, catalog: &Catalog) -> String {
    let rows = economy
        .cosmetics
        .iter()
        .rev()
        .map(|grant| {
            let items = if grant.kind == "emote" {
                &catalog.emotes
            } else {
                &catalog.cosmetics
            };
            let name = items
                .iter()
                .find(|item| item.id == grant.item_id)
                .map(|item| item.name.as_str())
                .unwrap_or(&grant.item_id);
            format!(
                r#"<li class="prize kind-{kind}"><span class="prize-kind" data-i18n="account.roulette.kind.{kind}">{kind}</span><strong>{name}</strong></li>"#,
                kind = escape_html(&grant.kind),
                name = escape_html(name),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<p class="muted" id="history-empty" data-i18n="account.roulette.noneYet"{hidden}>Aucun gain pour le moment.</p>
<ul class="prizes" id="history">{rows}</ul>"#,
        hidden = if economy.cosmetics.is_empty() { "" } else { " hidden" },
    )
}

fn preview(catalog: &Catalog) -> String {
    let mut entries = vec![
        r#"<div class="prize kind-coins"><span class="prize-kind" data-i18n="account.roulette.kind.coins">Coins</span><strong data-i18n="account.roulette.prizes.coins">Coins</strong></div>"#.to_string(),
        r#"<div class="prize kind-license"><span class="prize-kind" data-i18n="account.roulette.kind.license">Licence</span><strong data-i18n="account.roulette.prizes.license">Licence temporaire</strong></div>"#.to_string(),
    ];
    for (kind, items) in [("emote", &catalog.emotes), ("cosmetic", &catalog.cosmetics)] {
        for item in items {
            entries.push(format!(
                r#"<div class="prize kind-{kind}"><span class="prize-kind" data-i18n="account.roulette.kind.{kind}">{kind}</span><strong>{}</strong></div>"#,
                escape_html(&item.name)
            ));
        }
    }
    entries.join("\n")
}

fn outcome_html(outcome: Outcome) -> String {
    let (key, vars) = match &outcome {
        Outcome::Nothing => ("account.roulette.outcome.nothing".to_string(), String::new()),
        Outcome::Coins(amount) => (
            "account.roulette.outcome.coins".to_string(),
            format!(r#" data-v-amount="{amount}""#),
        ),
        Outcome::Item { kind, name } => (
            format!("account.roulette.outcome.{kind}"),
            format!(r#" data-v-name="{}""#, escape_html(name)),
        ),
        Outcome::License { days, key } => (
            "account.roulette.outcome.license".to_string(),
            format!(r#" data-v-days="{days}" data-v-key="{}""#, escape_html(key)),
        ),
    };
    format!(r#"<p data-i18n="{key}"{vars}></p>"#)
}
