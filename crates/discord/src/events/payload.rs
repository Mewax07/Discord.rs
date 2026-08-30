use serde_json::{from_value, Value};

use crate::{events::Event, Result};

pub fn parse_dispatch(name: &str, data: Value) -> Result<Event> {
    Ok(match name {
        "READY" => Event::Ready,
        "GUILD_CREATE" => Event::GuildCreate {
            id: data
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: data.get("name").and_then(Value::as_str).map(str::to_string),
        },
        "MESSAGE_CREATE" => Event::MessageCreate(from_value(data)?),
        "INTERACTION_CREATE" => Event::InteractionCreate(serde_json::from_value(data)?),
        other => Event::Unknown {
            name: other.to_string(),
        },
    })
}
