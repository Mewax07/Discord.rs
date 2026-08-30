use std::collections::HashMap;

use discord::models::{Role, PERM_ADMINISTRATOR, PERM_MANAGE_CHANNELS, PERM_MANAGE_ROLES};
use discord::rest::RestClient;

use crate::logs;
use crate::storage::ConfigStore;

pub struct Report {
    pub problems: Vec<String>,
}

pub fn audit_permissions(
    rest: &RestClient,
    config: &ConfigStore,
    guild_id: &str,
    application_id: &str,
) -> Report {
    let mut problems = Vec::new();

    let roles = match rest.get_guild_roles(guild_id) {
        Ok(roles) => roles,
        Err(e) => {
            problems.push(format!("unable to read the server roles: {e}"));
            return Report { problems };
        }
    };
    let me = match rest.get_guild_member(guild_id, application_id) {
        Ok(member) => member,
        Err(e) => {
            problems.push(format!("unable to read the bot member entry: {e}"));
            return Report { problems };
        }
    };

    let by_id: HashMap<&str, &Role> = roles.iter().map(|role| (role.id.as_str(), role)).collect();

    let own: Vec<&Role> = me
        .roles
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .collect();

    let top_position = own.iter().map(|role| role.position).max().unwrap_or(0);
    let granted: u64 = own
        .iter()
        .map(|role| role.permission_bits())
        .fold(0, |a, b| a | b);
    let is_admin = granted & PERM_ADMINISTRATOR != 0;

    if !is_admin && granted & PERM_MANAGE_ROLES == 0 {
        problems.push(
            "the bot is missing the Manage Roles permission, it cannot grant the member or notification roles".to_string(),
        );
    }
    if !is_admin && granted & PERM_MANAGE_CHANNELS == 0 {
        problems.push(
            "the bot is missing the Manage Channels permission, ticket channels cannot be created"
                .to_string(),
        );
    }

    let cfg = config.get(guild_id);
    let mut managed: Vec<(String, String)> = Vec::new();
    if let Some(role_id) = &cfg.member_role_id {
        managed.push(("member role".to_string(), role_id.clone()));
    }
    for (key, role_id) in &cfg.self_roles {
        managed.push((format!("notification role {key}"), role_id.clone()));
    }

    for (label, role_id) in managed {
        match by_id.get(role_id.as_str()) {
            None => problems.push(format!(
                "the configured {label} ({role_id}) no longer exists on this server"
            )),
            Some(role) if role.position >= top_position => problems.push(format!(
                "the {label} `{}` sits above the bot in the role list, drag the bot role higher to let it be granted",
                role.name
            )),
            Some(role) if role.managed => problems.push(format!(
                "the {label} `{}` is managed by an integration and cannot be granted by the bot",
                role.name
            )),
            Some(_) => {}
        }
    }

    Report { problems }
}

pub fn report(report: &Report) {
    if report.problems.is_empty() {
        logs::ready("permissions", "role hierarchy and permissions look correct");
        return;
    }

    for problem in &report.problems {
        logs::warn("permissions", problem);
    }
    logs::warn(
        "permissions",
        "fix the items above in Server Settings, Roles, then restart the bot",
    );
}
