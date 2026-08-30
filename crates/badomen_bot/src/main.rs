use std::sync::Arc;

use discord::{
    commands::CommandRegistry,
    events::{parse_dispatch, Event},
    gateway::{intents, ActivityType, Gateway, GatewayConfig, GatewayEvent, PresenceStatus},
    rest::RestClient,
    shutdown::install_handler,
};
use licensing::{ApiConfig, LicenseService, DEFAULT_OFFLINE_GRACE};

use crate::{
    commands::{
        ClearCommand, ConfigCommand, GiveawayCommand, GiveawayEndHandler, GiveawayEnterHandler,
        GiveawayRerollHandler, GiveawayService, LicenseCommand, PollCommand, RulesAcceptHandler,
        RulesCommand, SelfRolesCommand, SelfRolesSelectHandler, TicketBugBackToProductHandler,
        TicketBugBackToVersionHandler, TicketBugOsHandler, TicketBugProductHandler,
        TicketBugVersionHandler, TicketClaimHandler, TicketCloseHandler, TicketCloseModalHandler,
        TicketCommand, TicketHoldHandler, TicketOpenHandler, TicketPanelHandler, TicketService,
    },
    logs::{AuditEntry, Logger},
    scheduler::Scheduler,
    storage::{ConfigStore, GiveawayStore, PollStore, TicketStore, LOG_DEFAULT, LOG_SYSTEM},
};

mod commands;
mod diagnostics;
mod logs;
mod scheduler;
mod storage;
mod ui;
mod util;

pub fn main() {
    install_handler();
    load_dotenv(".env");

    let token = std::env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN in env file");
    let guild_id = std::env::var("GUILD_ID").expect("Missing GUILD_ID in env file");
    let product = std::env::var("PRODUCT_NAME").unwrap_or_else(|_| "BadOmen Visuals".to_string());

    let rest = Arc::new(RestClient::new(token.clone()));
    let config = Arc::new(ConfigStore::open("data/config.json"));
    let tickets = Arc::new(TicketStore::open("data/tickets.json"));
    let giveaways = Arc::new(GiveawayStore::open("data/giveaways.json"));
    let polls = Arc::new(PollStore::open("data/polls.json"));
    let scheduler = Arc::new(Scheduler::start());
    let logger = Arc::new(Logger::new(rest.clone(), config.clone()));

    let licenses = Arc::new(
        LicenseService::open(
            "data/licenses.json",
            "data/license_signing_key.pk8",
            std::env::var("LICENSE_OFFLINE_DAYS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .map(|days| days * 86_400)
                .unwrap_or(DEFAULT_OFFLINE_GRACE),
        )
        .expect("Unable to open the licence service"),
    );

    start_license_api(licenses.clone(), &product);

    if let Ok(channel_id) = std::env::var("LOGS_CHANNEL_ID") {
        config.update(&guild_id, |c| {
            c.log_channels
                .entry(LOG_DEFAULT.to_string())
                .or_insert(channel_id);
        });
    }

    let app = rest
        .get_application_info()
        .expect("Unable to retrieve the application");
    logs::info("startup", format!("application {}", app.id));

    let ticket_service = TicketService {
        config: config.clone(),
        tickets: tickets.clone(),
        logger: logger.clone(),
        scheduler: scheduler.clone(),
        rest: rest.clone(),
    };

    let giveaway_service = GiveawayService {
        giveaways: giveaways.clone(),
        config: config.clone(),
        logger: logger.clone(),
        scheduler: scheduler.clone(),
        rest: rest.clone(),
        licenses: licenses.clone(),
        product: product.clone(),
    };

    let pending_polls = polls.all_pending();
    for (message_id, record) in &pending_polls {
        commands::schedule_poll_end(
            polls.clone(),
            rest.clone(),
            config.clone(),
            logger.clone(),
            message_id.clone(),
            record.ends_at,
            &scheduler,
        );
    }

    let pending_giveaways = giveaways.all_pending();
    for (message_id, record) in &pending_giveaways {
        commands::schedule_giveaway_end(
            giveaway_service.clone(),
            message_id.clone(),
            record.ends_at,
        );
    }

    logs::info(
        "startup",
        format!(
            "{} polls and {} giveaways rescheduled",
            pending_polls.len(),
            pending_giveaways.len()
        ),
    );

    if config.get(&guild_id).log_channels.is_empty() {
        logs::warn(
            "startup",
            "no log channel routed yet, run /config logs set to enable auditing",
        );
    }

    diagnostics::report(&diagnostics::audit_permissions(
        &rest, &config, &guild_id, &app.id,
    ));

    let registry = CommandRegistry::new()
        .register(ConfigCommand {
            config: config.clone(),
            logger: logger.clone(),
        })
        .register(TicketCommand {
            service: ticket_service.clone(),
        })
        .register(RulesCommand {
            config: config.clone(),
            logger: logger.clone(),
        })
        .register(SelfRolesCommand {
            config: config.clone(),
        })
        .register(GiveawayCommand {
            service: giveaway_service.clone(),
        })
        .register(PollCommand {
            polls: polls.clone(),
            config: config.clone(),
            scheduler: scheduler.clone(),
            rest: rest.clone(),
            logger: logger.clone(),
        })
        .register(ClearCommand {
            config: config.clone(),
            logger: logger.clone(),
        })
        .register(LicenseCommand {
            licenses: licenses.clone(),
            config: config.clone(),
            logger: logger.clone(),
            product: product.clone(),
        })
        .register_component(TicketPanelHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketBugProductHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketBugBackToProductHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketBugVersionHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketBugBackToVersionHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketBugOsHandler)
        .register_component(TicketOpenHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketClaimHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketHoldHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketCloseHandler {
            service: ticket_service.clone(),
        })
        .register_component(TicketCloseModalHandler {
            service: ticket_service,
        })
        .register_component(RulesAcceptHandler {
            config: config.clone(),
            logger: logger.clone(),
        })
        .register_component(SelfRolesSelectHandler {
            config: config.clone(),
            logger: logger.clone(),
        })
        .register_component(GiveawayEnterHandler {
            service: giveaway_service.clone(),
        })
        .register_component(GiveawayEndHandler {
            service: giveaway_service.clone(),
        })
        .register_component(GiveawayRerollHandler {
            service: giveaway_service,
        });

    match registry.sync_with_discord(&rest, &app.id, &guild_id) {
        Ok(()) => logs::info("startup", "slash commands synced"),
        Err(e) => logs::error("startup", format!("command sync failed: {e}")),
    }

    let gw_config = GatewayConfig::new(
        token,
        intents::GUILDS | intents::GUILD_MESSAGES | intents::MESSAGE_CONTENT,
    )
    .with_presence(
        "BadOmen On Top!",
        ActivityType::Watching,
        PresenceStatus::Dnd,
    );

    let mut gateway = Gateway::new(gw_config);

    logger.audit(
        &guild_id,
        AuditEntry::new(LOG_SYSTEM, "Bot online")
            .accent(ui::SUCCESS)
            .field("Application", app.id.clone())
            .field("Product", product.clone())
            .field("Licences", licenses.stats().total.to_string()),
    );

    gateway.run(
        |GatewayEvent::Dispatch { name, data }| match parse_dispatch(&name, data) {
            Ok(Event::Ready) => logs::ready("gateway", "connected and listening"),
            Ok(Event::InteractionCreate(interaction)) => registry.dispatch(&rest, &interaction),
            Ok(Event::Unknown { name }) => {
                logs::debug("gateway", format!("unhandled event {name}"))
            }
            Ok(_) => {}
            Err(e) => logs::error("gateway", format!("cannot parse {name}: {e}")),
        },
    );

    logger.audit(
        &guild_id,
        AuditEntry::new(LOG_SYSTEM, "Bot offline")
            .accent(ui::DANGER)
            .field("Guild", guild_id.clone()),
    );
    logs::info("shutdown", "stopped cleanly");
}

fn start_license_api(licenses: Arc<LicenseService>, product: &str) {
    let addr = std::env::var("LICENSE_API_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let admin_token = std::env::var("LICENSE_API_TOKEN")
        .ok()
        .filter(|value| value.len() >= 24);

    if admin_token.is_none() {
        logs::warn(
            "licence-api",
            "LICENSE_API_TOKEN is missing or shorter than 24 characters, the admin endpoints stay disabled",
        );
    }

    let config = ApiConfig {
        addr: addr.clone(),
        admin_token,
        product: product.to_string(),
    };

    match licensing::spawn_api(licenses.clone(), config) {
        Ok(local) => {
            logs::ready("licence-api", format!("listening on http://{local}"));
            logs::info(
                "licence-api",
                format!("ed25519 public key {}", licenses.public_key_hex()),
            );
        }
        Err(e) => logs::error("licence-api", format!("cannot bind {addr}: {e}")),
    }
}

fn load_dotenv(path: &str) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim();
        if let Some(stripped) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            value = stripped;
        } else if let Some(stripped) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
            value = stripped;
        }
        if std::env::var(key).is_err() {
            std::env::set_var(key, value);
        }
    }
}
