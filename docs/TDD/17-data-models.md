# Data models (Madmail-compatible SQLite)

madmail-v2 uses **one consolidated SQLite database** (`state_dir/chatmail.db`) containing the Madmail **imapsql** extension tables. Madmail splits `credentials.db` (auth KV) and `imapsql.db` (everything else); madmail-v2 can still read Madmail `passwords` tables that use `key`/`value` columns.

Schema source of truth in code: `crates/chatmail-db/migrations/`.

Madmail GORM models: [`context/madmail/internal/db/models.go`](../../context/madmail/internal/db/models.go).

## `settings`

| Column | Type | Notes |
|--------|------|-------|
| `key` | TEXT PK | `__REGISTRATION_OPEN__`, etc. |
| `value` | TEXT | `true` / `false` / policy string |

Compatible with Madmail `settings_table` / credentials DB KV.

Notable madmail-v2 keys (full list: `chatmail-db::settings_keys`):

| Key | CLI / admin | Notes |
|-----|-------------|-------|
| `__MESSAGE_RETENTION_ENABLED__` | admin settings | Hourly maildir purge toggle |
| `__MESSAGE_RETENTION__` | admin settings | Duration (`30d`, `720h`, …) |
| `__APPENDLIMIT__` / `__MAX_MESSAGE_SIZE__` | [`message-size`](../guide/cli/message-size.md) | Effective cap (min of both) |
| `__PUSH_MODE__` | [`push`](../guide/cli/push.md) | `auto` / `on` / `off` |
| `__WEBIMAP_ENABLED__` / `__WEBSMTP_ENABLED__` | [`webimap`](../guide/cli/webimap.md) | HTTP mail APIs |
| `__SMTP_PORT__`, … | [`port`](../guide/cli/port.md) | Listener overrides |

## `passwords` (dual schema)

**madmail-v2 native:**

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
| `name` | TEXT PK | `sent_messages`, `outbound_messages`, `received_messages`, `push_successful_notifications` |
| `count` | INTEGER | Flushed from in-memory atomics every 30s |

Runtime increments: SMTP DATA (`record_smtp_accepted`), `/mxdeliv` + WebSMTP (`record_inbound_delivery`), remote queue success (`increment_outbound`), push notify 2xx (`chatmail-push::stats`). See Madmail `msgcounter.go`.

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

## `push_tokens` (madmail-v2 extension)

| Column | Type |
|--------|------|
| `username` | TEXT |
| `device_token` | TEXT |
| `updated_at` | TIMESTAMP |

PK (`username`, `device_token`). Populated via IMAP `SETMETADATA /private/devicetoken` (`XDELTAPUSH`). Pruned after 90 days without refresh; stale tokens removed on HTTP 410 from `notifications.delta.chat`. See [23-push-notifications.md](23-push-notifications.md).

## Not replicated (Madmail-only)

| Madmail | Reason |
|---------|--------|
| go-imap-sql `users` / `mboxes` / `msgs` | madmail-v2 uses Maildir blobs under `state_dir/mail/` |
| `contacts` (`sharing.db`) | Contact sharing not in Phase 1–8 scope |
| `table_entries` legacy GORM KV | Superseded by `settings` |

## Mail storage (filesystem)

Under `{state_dir}/`:

| Path | Purpose |
|------|---------|
| `mail/{user}/Maildir/{cur,new,tmp}/` | Per-user maildir (Madmail external store / Dovecot parity) |
| `mail/{user}/Maildir/chatmail-uidlist` | Persistent IMAP UID index (`chatmail-storage::uidlist`) |
| `mail/{user}/folders/…` | Additional IMAP mailboxes (e.g. `DeltaChat`) |
| `blobs/{hh}/{sha256}` | Content-addressed dedup store when `blob_dedup on` |
| `remote_queue/` | Outbound federation retry queue (`chatmail-delivery`) |
| `pending_notifications/` | Disk-backed push notify jobs (`chatmail-push`) |

Full module map: [`04-storage-layer.md`](04-storage-layer.md).

## Implementation references

| Area | Path |
|------|------|
| Migrations | `crates/chatmail-db/migrations/` |
| Passwords dual-read | `crates/chatmail-db/src/passwords.rs` |
| Settings keys | `crates/chatmail-db/src/settings_keys.rs` |
| Message retention (DB) | `crates/chatmail-db/src/message_retention.rs` — `__MESSAGE_RETENTION_*__` |
| Port overrides | `crates/chatmail-db/src/mail_ports.rs` |
| Dormant accounts | `crates/chatmail-db/src/maintenance.rs` |
| Federation inbound checks | `crates/chatmail-db/src/inbound.rs` |
| IMAP MODSEQ persistence | `crates/chatmail-db/src/modseq.rs` |
| Quota / policy RAM | `crates/chatmail-state/` |
| CLI operators | [`../guide/cli/README.md`](../guide/cli/README.md) |

## Related RFCs

Database tables back protocol behaviour (quotas, federation, credentials), not on-wire encoding. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local file |
|-----|-------|------------|
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Message headers referenced in indexes/stats | [rfc5322.txt](RFC/rfc5322.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP QUOTA / mailbox semantics (`quotas` table) | [rfc3501.txt](RFC/rfc3501.txt) |
| [2087](https://datatracker.ietf.org/doc/html/rfc2087) | QUOTA extension (storage limits) | [rfc2087.txt](RFC/rfc2087.txt) |
| [5464](https://datatracker.ietf.org/doc/html/rfc5464) | `push_tokens` / METADATA-related keys | [rfc5464.txt](RFC/rfc5464.txt) |
