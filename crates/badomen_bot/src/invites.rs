//! Invite-to-coins tracking.
//!
//! Discord never says which invite a new member used, so the classic trick
//! applies: snapshot every invite's `uses` counter, and after a join, diff
//! against the snapshot to find the one that went up. The reward is queued,
//! not credited immediately, to resist alt accounts farming invites: the
//! invited Discord account must already be reasonably old, and it must still
//! be in the server after the delay before the inviter is paid.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use discord::models::User;
use discord::rest::RestClient;
use web::EconomyStore;

use crate::logs;
use crate::scheduler::Scheduler;
use crate::util::now_secs;

const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;
const MIN_ACCOUNT_AGE_SECS: u64 = 7 * 86_400;
const CREDIT_DELAY_SECS: u64 = 7 * 86_400;
const INVITE_REWARD_COINS: u64 = 10;

pub struct InviteService {
    rest: Arc<RestClient>,
    economy: Arc<EconomyStore>,
    scheduler: Arc<Scheduler>,
    home_guild_id: String,
    snapshots: Mutex<HashMap<String, HashMap<String, u32>>>,
}

impl InviteService {
    pub fn new(
        rest: Arc<RestClient>,
        economy: Arc<EconomyStore>,
        scheduler: Arc<Scheduler>,
        home_guild_id: impl Into<String>,
    ) -> Self {
        Self {
            rest,
            economy,
            scheduler,
            home_guild_id: home_guild_id.into(),
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    /// Take a fresh snapshot of `guild_id`'s invites. Call this once on
    /// startup (and after every join, see `handle_join`) so the next join can
    /// be diffed against an up to date baseline.
    pub fn refresh_snapshot(&self, guild_id: &str) {
        match self.rest.get_guild_invites(guild_id) {
            Ok(invites) => {
                let uses = invites.into_iter().map(|i| (i.code, i.uses)).collect();
                self.snapshots
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(guild_id.to_string(), uses);
            }
            Err(e) => logs::warn(
                "invites",
                format!("cannot list invites for {guild_id} ({e}), the bot needs Manage Server"),
            ),
        }
    }

    /// Re-arm the coin-crediting timers for whatever invite rewards are still
    /// queued from before the last restart.
    pub fn resume_pending(&self) {
        let pending = self.economy.pending_invites();
        for reward in &pending {
            self.arm(reward.invited_id.clone(), reward.credit_at);
        }
        if !pending.is_empty() {
            logs::info("invites", format!("{} invite reward(s) resumed", pending.len()));
        }
    }

    pub fn handle_join(&self, guild_id: &str, user: &User) {
        let inviter_id = self.resolve_used_invite(guild_id);

        let Some(inviter_id) = inviter_id else {
            return;
        };
        if inviter_id == user.id {
            return;
        }

        if account_age_secs(&user.id, now_secs()) < MIN_ACCOUNT_AGE_SECS {
            logs::info(
                "invites",
                format!(
                    "{} joined via {inviter_id}'s invite but the account is too recent, no reward queued",
                    user.id
                ),
            );
            return;
        }

        let credit_at = now_secs() + CREDIT_DELAY_SECS;
        self.economy
            .queue_invite_reward(&inviter_id, &user.id, credit_at, INVITE_REWARD_COINS);
        self.arm(user.id.clone(), credit_at);
    }

    pub fn handle_leave(&self, user_id: &str) {
        self.economy.cancel_invite_reward(user_id);
    }

    fn arm(&self, invited_id: String, credit_at: u64) {
        let rest = self.rest.clone();
        let economy = self.economy.clone();
        let guild_id = self.home_guild_id.clone();
        self.scheduler.schedule_at(credit_at, move || {
            settle(&rest, &economy, &guild_id, &invited_id);
        });
    }

    /// Diff the current invite uses against the last snapshot to find which
    /// invite was just used, then refresh the snapshot for next time.
    fn resolve_used_invite(&self, guild_id: &str) -> Option<String> {
        let before = self
            .snapshots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(guild_id)
            .cloned()
            .unwrap_or_default();

        let after = self.rest.get_guild_invites(guild_id).ok()?;

        let inviter_id = after
            .iter()
            .find(|invite| invite.uses > before.get(&invite.code).copied().unwrap_or(0))
            .and_then(|invite| invite.inviter.as_ref())
            .map(|user| user.id.clone());

        let snapshot = after.into_iter().map(|i| (i.code, i.uses)).collect();
        self.snapshots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(guild_id.to_string(), snapshot);

        inviter_id
    }
}

fn settle(rest: &RestClient, economy: &EconomyStore, guild_id: &str, invited_id: &str) {
    let Some(reward) = economy.take_invite_reward(invited_id) else {
        return;
    };

    if rest.get_guild_member(guild_id, invited_id).is_err() {
        logs::info(
            "invites",
            format!("{invited_id} left before the invite reward matured, no coins credited"),
        );
        return;
    }

    economy.credit_coins(&reward.inviter_id, reward.coins);
    logs::info(
        "invites",
        format!(
            "{} coins credited to {} for inviting {invited_id}",
            reward.coins, reward.inviter_id
        ),
    );
}

/// Decode a Discord snowflake's embedded creation time.
fn account_age_secs(user_id: &str, now: u64) -> u64 {
    let Ok(id) = user_id.parse::<u64>() else {
        return 0;
    };
    let created_secs = ((id >> 22) + DISCORD_EPOCH_MS) / 1000;
    now.saturating_sub(created_secs)
}
