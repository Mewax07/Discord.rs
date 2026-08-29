use discord::{
    commands::CommandRegistry,
    events::{parse_dispatch, Event},
    gateway::{intents, ActivityType, Gateway, GatewayConfig, GatewayEvent, PresenceStatus},
    rest::RestClient,
    shutdown::install_handler,
};

use crate::commands::{LookupCommand, PingCommand};

mod commands;

pub fn main() {
    install_handler();
    load_dotenv(".env");

    let token = std::env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN in env file");
    let guild_id = std::env::var("GUILD_ID").expect("Missing GUILD_ID in env file");
    let log_channel_id = std::env::var("LOGS_CHANNEL_ID").ok();

    let rest = RestClient::new(token.clone());

    let app = rest
        .get_application_info()
        .expect("Unable to retrieve the application");
    println!("Application ID: {}", app.id);

    let registry = CommandRegistry::new()
        .register(PingCommand)
        .register(LookupCommand);

    registry
        .sync_with_discord(&rest, &app.id, &guild_id)
        .expect("Failed to sync commands with Discord");
    println!("Commands synced.");

    let config = GatewayConfig::new(
        token,
        intents::GUILDS | intents::GUILD_MESSAGES | intents::MESSAGE_CONTENT,
    )
    .with_presence(
        "BadOmen On Top!",
        ActivityType::Watching,
        PresenceStatus::Dnd,
    );

    let mut gateway = Gateway::new(config);

    gateway.run(
        |GatewayEvent::Dispatch { name, data }| match parse_dispatch(&name, data) {
            Ok(Event::Ready) => println!("Bot ready!"),
            Ok(Event::InteractionCreate(interaction)) => registry.dispatch(&rest, &interaction),
            Ok(Event::Unknown { name }) => println!("(Unknown event: {name})"),
            Ok(_) => {}
            Err(e) => eprintln!("Parsing error on {name}: {e}"),
        },
    );

    if let Some(channel_id) = log_channel_id {
        let _ = rest.send_message(&channel_id, Some("Bot shut down properly."), vec![]);
    }
    println!("Stop completed.");
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
