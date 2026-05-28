# Data models (Madmail-compatible SQLite)

chatmail-rs uses **one consolidated SQLite database** (`state_dir/chatmail.db`) containing the Madmail **imapsql** extension tables. Madmail splits `credentials.db` (auth KV) and `imapsql.db` (everything else); chatmail-rs can still read Madmail `passwords` tables that use `key`/`value` columns.

Schema source of truth in code: `crates/chatmail-db/migrations/`.

Madmail GORM models: [`context/madmail/internal/db/models.go`](../../context/madmail/internal/db/models.go).

## `settings`

| Column | Type | Notes |
|--------|------|-------|
| `key` | TEXT PK | `__REGISTRATION_OPEN__`, etc. |
| `value` | TEXT | `true` / `false` / policy string |

Compatible with Madmail `settings_table` / credentials DB KV.

## `passwords` (dual schema)

**chatmail-rs native:**

| Column | Type |
|--------|------|
| `username` | TEXT PK |
| `hash` | TEXT (`bcrypt:…`, `argon2:…`) |
| `created_at` | INTEGER unix |

**Madmail `auth.pass_table` / `sql_table`:**

| Column | Type |
|--------|------|
| `key` | TEXT PK (username) |
| `value` | TEXT (password hash) |

Detected at runtime via `PRAGMA table_info(passwords)`.

## `quotas`

| Column | Type | Notes |
|--------|------|-------|
| `username` | TEXT PK | User or `__GLOBAL_DEFAULT__` |
| `max_storage` | BIGINT | Bytes |
| `created_at` | BIGINT | Unix |
| `first_login_at` | BIGINT | `1` = never logged in |
| `last_login_at` | BIGINT | |
| `used_token` | TEXT | Registration token consumed on first login |

Matches Madmail `db.Quota`.

## `blocked_users`

| Column | Type |
|--------|------|
| `username` | TEXT PK |
| `reason` | TEXT |
| `blocked_at` | TIMESTAMP |

## `registration_tokens`

| Column | Type |
|--------|------|
| `token` | TEXT PK |
| `max_uses` | INTEGER |
| `used_count` | INTEGER |
| `comment` | TEXT |
| `expires_at` | TIMESTAMP |
| `created_at` | TIMESTAMP |

## `dns_overrides` (endpoint overrides)

| Column | Type |
|--------|------|
| `lookup_key` | TEXT PK | Domain or IP |
| `target_host` | TEXT | Redirect target |
| `comment` | TEXT |
| `created_at`, `updated_at` | TIMESTAMP |

## `federation_rules`

Madmail shape (migrated in `20240401000000_madmail_compat.sql`):

| Column | Type |
|--------|------|
| `id` | INTEGER PK AUTOINCREMENT |
| `domain` | TEXT UNIQUE |
| `created_at` | INTEGER unix |

Policy mode is global (`__FEDERATION_POLICY__` = `ACCEPT` or `REJECT`); rows are blocklist (ACCEPT) or allowlist (REJECT).

## `federation_server_stats`

Per-domain counters flushed from `FederationTracker` every 30s. Columns match Madmail `FederationStat` snapshot fields.

## `message_stats`

| Column | Type |
|--------|------|
| `name` | TEXT PK | `sent_messages`, `outbound_messages`, `received_messages` |
| `count` | INTEGER | Flushed from `chatmail-db::message_stats` atomics every 30s |

Runtime increments: SMTP DATA (`record_smtp_accepted`), `/mxdeliv` + WebSMTP (`record_inbound_delivery`), remote queue success (`increment_outbound`). See Madmail `msgcounter.go`.

## `exchangers`

Pull-based ingress relays (optional):

| Column | Type |
|--------|------|
| `name` | TEXT PK |
| `url` | TEXT |
| `enabled` | INTEGER |
| `poll_interval` | INTEGER |
| `last_poll_at` | TIMESTAMP |
| `created_at`, `updated_at` | TIMESTAMP |

## `push_tokens` (chatmail-rs extension)

| Column | Type |
|--------|------|
| `username` | TEXT |
| `device_token` | TEXT |
| `updated_at` | TIMESTAMP |

PK (`username`, `device_token`). Not in Madmail server DB (client-side in Delta Chat); reserved for future `XDELTAPUSH`.

## Not replicated (Madmail-only)

| Madmail | Reason |
|---------|--------|
| go-imap-sql `users` / `mboxes` / `msgs` | chatmail-rs uses Maildir blobs under `state_dir/mail/` |
| `contacts` (`sharing.db`) | Contact sharing not in Phase 1–8 scope |
| `table_entries` legacy GORM KV | Superseded by `settings` |

## Mail storage (filesystem)

Under `{state_dir}/mail/{user}/Maildir/{cur,new,tmp}/` — same layout as Madmail external store / Dovecot maildir path (see `04-storage-layer.md`).

## Implementation references

| Area | Path |
|------|------|
| Migrations | `crates/chatmail-db/migrations/` |
| Passwords dual-read | `crates/chatmail-db/src/passwords.rs` |
| Settings keys | `crates/chatmail-db/src/settings_keys.rs` |
| Quota / policy RAM | `crates/chatmail-state/` |

## Related RFCs

Database tables back protocol behaviour (quotas, federation, credentials), not on-wire encoding. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local file |
|-----|-------|------------|
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Message headers referenced in indexes/stats | [rfc5322.txt](RFC/rfc5322.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP QUOTA / mailbox semantics (`quotas` table) | [rfc3501.txt](RFC/rfc3501.txt) |
| [2087](https://datatracker.ietf.org/doc/html/rfc2087) | QUOTA extension (storage limits) | [rfc2087.txt](RFC/rfc2087.txt) |
| [5464](https://datatracker.ietf.org/doc/html/rfc5464) | `push_tokens` / METADATA-related keys | [rfc5464.txt](RFC/rfc5464.txt) |
