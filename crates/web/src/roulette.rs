//! The weekly roulette: one free spin per week (or pay with coins earned
//! from spins and invites) for a chance at a cosmetic, an emote, coins or a
//! temporary licence key. Requires an activated licence, since a cosmetic or
//! emote reward is attached to the account's most recently seen hardware id.

use std::fs;
use std::path::Path;

use httpd::{escape_html, Request, Response};
use serde::Deserialize;
use serde_json::Value;

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

fn spin(request: &Request, config: &SiteConfig) -> Response {
    let Some(user) = session_user(request, config) else {
        return Response::redirect("/account");
    };
    let Some(economy) = &config.economy else {
        return Response::redirect("/account/roulette");
    };
    let id = user.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let catalog = Catalog::load(&config.roulette_path);
    let now = now_secs();

    let Some(hwid) = latest_hwid(config, &id) else {
        return page(request, config, None);
    };

    if economy
        .try_spend_spin(&id, catalog.spin_cost_coins, catalog.free_spin_interval_days * 86_400, now)
        .is_err()
    {
        return page(request, config, None);
    }

    let outcome = roll(config, economy, &catalog, &id, &hwid, now);
    page(request, config, Some(outcome))
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
    let has_hwid = latest_hwid(config, &id).is_some();
    let can_spin = has_hwid
        && (economy.next_free_spin_at <= now || economy.coins >= catalog.spin_cost_coins);

    Response::html(200, shell(config, &body(name, &economy, &catalog, has_hwid, can_spin, now, outcome), Some(&user)))
}

fn body(
    name: &str,
    economy: &UserEconomy,
    catalog: &Catalog,
    has_hwid: bool,
    can_spin: bool,
    now: u64,
    outcome: Option<Outcome>,
) -> String {
    let outcome_banner = outcome.map(outcome_html).unwrap_or_default();

    let free_spin_label = if economy.next_free_spin_at <= now {
        r#"<span data-i18n="account.roulette.freeNow">Disponible maintenant</span>"#.to_string()
    } else {
        format!(
            r#"<span data-i18n="account.roulette.freeIn" data-dur="{secs}">Dans {label}</span>"#,
            secs = economy.next_free_spin_at - now,
            label = escape_html(&human_duration(economy.next_free_spin_at - now)),
        )
    };

    let spin_hint = if !has_hwid {
        r#"<p class="muted" data-i18n="account.roulette.needHwid">Active une licence sur une machine pour pouvoir tourner la roue.</p>"#.to_string()
    } else if !can_spin {
        r#"<p class="muted" data-i18n="account.roulette.cantSpin">Pas de tour gratuit disponible et pas assez de coins.</p>"#.to_string()
    } else {
        String::new()
    };

    let cosmetics = if economy.cosmetics.is_empty() {
        r#"<p class="muted" data-i18n="account.roulette.noneYet">Aucun gain pour le moment.</p>"#.to_string()
    } else {
        let rows = economy
            .cosmetics
            .iter()
            .rev()
            .map(|grant| {
                let catalog_items = if grant.kind == "emote" {
                    &catalog.emotes
                } else {
                    &catalog.cosmetics
                };
                let name = catalog_items
                    .iter()
                    .find(|item| item.id == grant.item_id)
                    .map(|item| item.name.as_str())
                    .unwrap_or(&grant.item_id);
                format!(
                    r#"<li class="machine"><div class="machine-body"><strong>{name}</strong><span class="machine-meta">{kind}</span></div></li>"#,
                    name = escape_html(name),
                    kind = escape_html(&grant.kind),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(r#"<ul class="machines">{rows}</ul>"#)
    };

    let catalog_preview = preview(catalog);

    format!(
        r#"<section class="hero">
  <div class="hero-glow" aria-hidden="true"></div>
  <span class="eyebrow" data-i18n="account.roulette.eyebrow">ROULETTE HEBDOMADAIRE</span>
  <h1 data-i18n-html="account.roulette.hello" data-v-name="{name}">Salut, <em>{name}</em></h1>
  <p class="hero-sub" data-i18n="account.roulette.sub">Un tour gratuit chaque semaine, ou depense des coins pour retenter ta chance.</p>
</section>

{outcome_banner}

<section class="stats">
  <article class="stat">
    <div class="stat-body">
      <strong>{coins}</strong>
      <span data-i18n="account.roulette.coins">Coins</span>
    </div>
  </article>
  <article class="stat">
    <div class="stat-body">
      <strong>{spins}</strong>
      <span data-i18n="account.roulette.spinsDone">Tours joues</span>
    </div>
  </article>
  <article class="stat">
    <div class="stat-body">
      <strong>{free_spin_label}</strong>
      <span data-i18n="account.roulette.nextFree">Prochain tour gratuit</span>
    </div>
  </article>
</section>

<section class="licenses">
  <div class="section-head">
    <div>
      <span class="eyebrow" data-i18n="account.roulette.spin.eyebrow">TOURNER</span>
      <h2 data-i18n="account.roulette.spin.title">Lancer la roue</h2>
    </div>
    <p class="muted" data-i18n="account.roulette.spin.text" data-v-cost="{cost}">Un tour gratuit par semaine, sinon {cost} coins.</p>
  </div>
  <form method="post" action="/account/roulette/spin">
    <button class="btn primary" type="submit" {disabled}>
      <span data-i18n="account.roulette.spin.button">Tourner la roue</span>
    </button>
  </form>
  {spin_hint}
</section>

<section class="licenses">
  <div class="section-head">
    <div>
      <span class="eyebrow" data-i18n="account.roulette.prizes.eyebrow">LOTS POSSIBLES</span>
      <h2 data-i18n="account.roulette.prizes.title">Ce que tu peux gagner</h2>
    </div>
  </div>
  {catalog_preview}
</section>

<section class="licenses">
  <div class="section-head">
    <div>
      <span class="eyebrow" data-i18n="account.roulette.history.eyebrow">HISTORIQUE</span>
      <h2 data-i18n="account.roulette.history.title">Tes gains cosmetiques</h2>
    </div>
  </div>
  {cosmetics}
</section>

<p class="fineprint"><a href="/account" data-i18n="account.roulette.back">Retour au tableau de bord</a></p>"#,
        name = escape_html(name),
        coins = economy.coins,
        spins = economy.spins_done,
        free_spin_label = free_spin_label,
        cost = catalog.spin_cost_coins,
        disabled = if can_spin { "" } else { "disabled" },
        spin_hint = spin_hint,
        catalog_preview = catalog_preview,
        cosmetics = cosmetics,
        outcome_banner = outcome_banner,
    )
}

fn preview(catalog: &Catalog) -> String {
    let mut entries = vec![
        r#"<li class="machine"><div class="machine-body"><strong data-i18n="account.roulette.prizes.coins">Coins</strong></div></li>"#.to_string(),
        r#"<li class="machine"><div class="machine-body"><strong data-i18n="account.roulette.prizes.license">Licence temporaire (7, 14 ou 30 jours)</strong></div></li>"#.to_string(),
    ];
    for item in &catalog.emotes {
        entries.push(format!(
            r#"<li class="machine"><div class="machine-body"><strong>{}</strong></div></li>"#,
            escape_html(&item.name)
        ));
    }
    for item in &catalog.cosmetics {
        entries.push(format!(
            r#"<li class="machine"><div class="machine-body"><strong>{}</strong></div></li>"#,
            escape_html(&item.name)
        ));
    }
    format!(r#"<ul class="machines">{}</ul>"#, entries.join("\n"))
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

    format!(
        r#"<section class="roulette-outcome">
  <span class="eyebrow" data-i18n="account.roulette.outcome.eyebrow">RESULTAT</span>
  <p data-i18n="{key}"{vars}></p>
</section>"#,
        key = key,
        vars = vars,
    )
}
