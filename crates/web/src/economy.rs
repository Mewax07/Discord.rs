//! Roulette economy: coins, weekly spins and cosmetic grants per Discord user
//! id, plus a queue of invite rewards waiting out their anti-abuse delay.
//! Persisted the same way as `visits.rs` (a short mutex, then an atomic
//! rename) so every user fits in one small indexed file instead of scattered
//! ad-hoc state.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct CosmeticGrant {
    pub item_id: String,
    pub kind: String,
    pub hwid: String,
    pub granted_at: u64,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct UserEconomy {
    #[serde(default)]
    pub coins: u64,
    #[serde(default)]
    pub next_free_spin_at: u64,
    #[serde(default)]
    pub spins_done: u64,
    #[serde(default)]
    pub cosmetics: Vec<CosmeticGrant>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PendingInvite {
    pub inviter_id: String,
    pub invited_id: String,
    pub credit_at: u64,
    pub coins: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct EconomyData {
    #[serde(default)]
    users: HashMap<String, UserEconomy>,
    #[serde(default)]
    pending_invites: Vec<PendingInvite>,
}

pub struct SpinDenied {
    pub coins: u64,
    pub next_free_spin_at: u64,
}

pub struct EconomyStore {
    path: PathBuf,
    data: Mutex<EconomyData>,
}

impl EconomyStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    pub fn snapshot(&self, user_id: &str) -> UserEconomy {
        self.data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .users
            .get(user_id)
            .cloned()
            .unwrap_or_default()
    }

    fn write<R>(&self, f: impl FnOnce(&mut EconomyData) -> R) -> R {
        let mut guard = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let result = f(&mut guard);
        if let Err(e) = self.persist(&guard) {
            eprintln!("economy store write failed ({}): {e}", self.path.display());
        }
        result
    }

    fn persist(&self, data: &EconomyData) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)
    }

    /// Spend the weekly free spin if it is available, otherwise `cost` coins.
    pub fn try_spend_spin(&self, user_id: &str, cost: u64, free_interval: u64, now: u64) -> Result<(), SpinDenied> {
        self.write(|data| {
            let entry = data.users.entry(user_id.to_string()).or_default();
            if entry.next_free_spin_at <= now {
                entry.next_free_spin_at = now + free_interval;
                entry.spins_done += 1;
                Ok(())
            } else if entry.coins >= cost {
                entry.coins -= cost;
                entry.spins_done += 1;
                Ok(())
            } else {
                Err(SpinDenied {
                    coins: entry.coins,
                    next_free_spin_at: entry.next_free_spin_at,
                })
            }
        })
    }

    pub fn credit_coins(&self, user_id: &str, amount: u64) {
        self.write(|data| {
            data.users.entry(user_id.to_string()).or_default().coins += amount;
        });
    }

    pub fn grant_cosmetic(&self, user_id: &str, item_id: &str, kind: &str, hwid: &str, now: u64) {
        self.write(|data| {
            data.users
                .entry(user_id.to_string())
                .or_default()
                .cosmetics
                .push(CosmeticGrant {
                    item_id: item_id.to_string(),
                    kind: kind.to_string(),
                    hwid: hwid.to_string(),
                    granted_at: now,
                });
        });
    }

    /// Queue a coin reward for `inviter_id` once `invited_id` has stayed long
    /// enough, unless one is already queued for that invited member.
    pub fn queue_invite_reward(&self, inviter_id: &str, invited_id: &str, credit_at: u64, coins: u64) {
        self.write(|data| {
            if data.pending_invites.iter().any(|p| p.invited_id == invited_id) {
                return;
            }
            data.pending_invites.push(PendingInvite {
                inviter_id: inviter_id.to_string(),
                invited_id: invited_id.to_string(),
                credit_at,
                coins,
            });
        });
    }

    /// Drop a queued invite reward, e.g. because the invited member left
    /// before the anti-abuse delay elapsed.
    pub fn cancel_invite_reward(&self, invited_id: &str) {
        self.write(|data| {
            data.pending_invites.retain(|p| p.invited_id != invited_id);
        });
    }

    /// Every reward still waiting out its delay, so a freshly started process
    /// can re-arm its timers (the in-memory scheduler does not survive a
    /// restart, this JSON file does).
    pub fn pending_invites(&self) -> Vec<PendingInvite> {
        self.data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_invites
            .clone()
    }

    /// Remove and return the pending reward for `invited_id`, if any is still
    /// queued (it may already have been cancelled because the member left).
    pub fn take_invite_reward(&self, invited_id: &str) -> Option<PendingInvite> {
        self.write(|data| {
            let index = data
                .pending_invites
                .iter()
                .position(|p| p.invited_id == invited_id)?;
            Some(data.pending_invites.remove(index))
        })
    }
}
