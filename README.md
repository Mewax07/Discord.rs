# BadOmen Bot

A Discord bot and a licence server for the BadOmen products, written in Rust with no third party Discord library. The gateway client, the REST client, the HTTP stack and the licence API are all implemented in this repository on top of `rustls` and `serde`.

## What is inside

| Crate | Role |
| --- | --- |
| `crates/discord` | Discord gateway, REST client, models, components v2, slash command registry |
| `crates/licensing` | Licence issuing, Ed25519 signing, HWID binding, HTTP API |
| `crates/badomen_bot` | Tickets, rules, self roles, polls, giveaways, moderation, configuration |

Everything runs in a single process: the bot connects to the gateway while the licence API listens on its own port.

## Requirements

- Rust 1.80 or newer
- A Discord application with a bot user

The bot needs these permissions in the server: `Manage Roles`, `Manage Channels`, `Manage Messages`, `View Channels`, `Send Messages`, `Embed Links`, `Attach Files`, `Read Message History`.

**Role hierarchy matters.** Discord refuses to grant a role that sits above the bot in the role list, with `50013 Missing Permissions`. Drag the bot role above the member role and above every notification role. The bot audits this at startup and names the offending role in the console.

## Setup

```bash
cp .env.example .env
```

Fill in `DISCORD_TOKEN` and `GUILD_ID`, then generate an API token:

```bash
openssl rand -hex 24
```

Run it:

```bash
cargo run --release
```

Slash commands are registered on the guild at startup, so they appear immediately.

## Configuration

Everything is configured from Discord, nothing is hardcoded. `/config view` shows the full state of the server and lists what is still missing.

| Command | Purpose |
| --- | --- |
| `/config tickets` | Ticket category, staff role, category pings, transcripts, author recap |
| `/config rules` | Rules channel and the role granted on acceptance |
| `/config logs` | Route each log category to a channel |
| `/config selfroles` | Map a panel entry to a real role |
| `/config brand` | Name, accent colour, logo, banner and footer of every widget |

## Commands

| Command | Access | Purpose |
| --- | --- | --- |
| `/ticket panel` | Manage Server | Publish the ticket opening panel |
| `/ticket close`, `add`, `remove` | Staff or ticket author | Manage an open ticket |
| `/rules` | Manage Server | Write, reorder and publish the rules panel |
| `/selfroles` | Manage Server | Publish the notification roles panel |
| `/poll create`, `/poll end` | Administrator | Native Discord polls |
| `/giveaway` | Administrator | Giveaways, optionally paying out licence keys |
| `/clear` | Administrator | Delete recent messages, with member and bot filters |
| `/license` | Administrator | Issue, inspect, revoke and reassign licences |
| `/config` | Manage Server | Everything above |

## Licence API

The server listens on `LICENSE_API_ADDR` (`127.0.0.1:8787` by default).

| Endpoint | Auth | Purpose |
| --- | --- | --- |
| `GET /v1/health` | none | Liveness |
| `GET /v1/public-key` | none | Ed25519 public key to embed in the client |
| `POST /v1/activate` | none | Bind a key to a machine, returns a signed token |
| `POST /v1/validate` | none | Check a key and HWID, or a token |
| `POST /v1/refresh` | none | Renew the offline window |
| `POST /v1/admin/licenses` | bearer | Issue a licence |
| `GET /v1/admin/licenses/{key}` | bearer | Inspect a licence by key or prefix |
| `POST /v1/admin/licenses/{key}/revoke` | bearer | Revoke |
| `POST /v1/admin/licenses/{key}/restore` | bearer | Undo a revocation |
| `POST /v1/admin/licenses/{key}/reset-hwid` | bearer | Unbind every machine |
| `GET /v1/admin/stats` | bearer | Global counters |

Admin routes expect `Authorization: Bearer $LICENSE_API_TOKEN` and stay disabled when the token is unset.

### Client side

Activation returns a token signed with Ed25519. The client stores it and verifies it locally, which keeps the product usable without network access until `offline_until` passes:

```rust
let claims = licensing::verify_offline_hex(PUBLIC_KEY_HEX, &token, &hwid, now)?;
```

Verification checks the signature, the hardware identifier, the expiry and the offline window. A revoked licence dies at the next successful refresh, at the latest when the offline window closes.

## Security

Licence keys are never stored. The database keeps a SHA-256 hash plus a nine character prefix used to identify a licence in logs and tickets. A leaked database therefore hands out nothing usable, and a key that a customer loses has to be reissued rather than read back.

Three secrets must stay out of the repository and off any public host:

- `DISCORD_TOKEN`
- `LICENSE_API_TOKEN`
- `data/license_signing_key.pk8`, the Ed25519 private key generated on first run

Lose the signing key and every token already issued becomes unverifiable, so back it up somewhere safe. The API speaks plain HTTP and binds to localhost by default; put a reverse proxy in front of it for TLS before exposing it, and rate limit there too, since the built in limiter keys on the peer address and would see every proxied request as one client.

No obfuscation is claimed here. Software running on a customer machine can always be patched. What this design guarantees is that nobody can forge a licence without the private key, and that a key cannot be shared across machines.

## License

Copyright (C) 2026 Mewax07

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
