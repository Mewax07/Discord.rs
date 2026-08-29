use discord::{
    commands::{CommandContext, SlashCommand},
    models::{CommandChoice, CommandDefinition, CommandOption},
    Result,
};

pub struct LookupCommand;

impl SlashCommand for LookupCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new(
            "lookup",
            "Affiche les infos d'un membre, salon, rôle ou produit",
        )
        .option(CommandOption::user("member", "Le membre à inspecter").required(false))
        .option(CommandOption::channel("channel", "Le salon à inspecter").required(false))
        .option(CommandOption::role("role", "Le rôle à inspecter").required(false))
        .option(
            CommandOption::string("product", "Nom du produit")
                .required(false)
                .autocomplete(true),
        )
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let mut lines = Vec::new();

        if let Some(user) = ctx.option_user("member") {
            lines.push(format!("👤 Membre: {} ({})", user.display_name(), user.id));
        }
        if let Some(channel) = ctx.option_channel("channel") {
            let name = channel.name.clone().unwrap_or_default();
            lines.push(format!("💬 Salon: #{name} ({})", channel.id));
        }
        if let Some(role) = ctx.option_role("role") {
            lines.push(format!("🎭 Rôle: {} ({})", role.name, role.id));
        }
        if let Some(product) = ctx.option_string("product") {
            lines.push(format!("📦 Produit: {product}"));
        }

        if lines.is_empty() {
            lines.push("Aucune option fournie.".to_string());
        }

        ctx.reply(lines.join("\n"))
    }

    fn autocomplete(&self, ctx: &CommandContext) -> Vec<CommandChoice> {
        let partial = ctx
            .focused_option()
            .and_then(|o| o.value.as_str())
            .unwrap_or("")
            .to_lowercase();

        ["BadOmenVisual", "BadOmenCore", "BadOmenPremium"]
            .iter()
            .filter(|p| p.to_lowercase().contains(&partial))
            .map(|p| CommandChoice {
                name: p.to_string(),
                value: (*p).into(),
            })
            .collect()
    }
}
