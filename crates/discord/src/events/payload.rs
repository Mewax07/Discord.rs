use serde_json::{from_value, Value};

use crate::{events::Event, models::User, Result};

/// `GUILD_MEMBER_ADD`/`GUILD_MEMBER_REMOVE` both carry a `guild_id` plus a
/// `user` object; build the matching `Event` from either, or `None` when the
/// user is missing so the caller falls back to `Event::Unknown`.
fn member_event(name: &str, data: Value) -> Result<Option<Event>> {
    let guild_id = data
        .get("guild_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let Some(raw_user) = data.get("user").cloned() else {
        return Ok(None);
    };
    let user: User = from_value(raw_user)?;

    Ok(Some(if name == "GUILD_MEMBER_ADD" {
        Event::GuildMemberAdd { guild_id, user }
    } else {
        Event::GuildMemberRemove { guild_id, user }
    }))
}

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
        "GUILD_MEMBER_ADD" => match member_event(name, data)? {
            Some(event) => event,
            None => return Ok(Event::Unknown { name: name.to_string() }),
        },
        "GUILD_MEMBER_REMOVE" => match member_event(name, data)? {
            Some(event) => event,
            None => return Ok(Event::Unknown { name: name.to_string() }),
        },
        other => Event::Unknown {
            name: other.to_string(),
        },
    })
}
