# 06 — Configuration System (Static + Dynamic + Effective Values)

Chatmail has a **three-layer** configuration model. Understanding it is essential for debugging "why is it listening on that port?" or "why did my setting change not take effect?"

## The Three Layers

1. **Static file** (`chatmail.toml` or legacy `maddy.conf` syntax)
2. **Database settings table** (dynamic, most things here are reloadable)
3. **CLI / environment overrides** (highest precedence for some values)

The `chatmail-config` crate is responsible for turning all of this into `AppConfig` and the various `effective_*` values used at runtime.

## Static Config (File)

Loaded in `boot.rs` via `load_config` (or defaults if file missing).

Supports two syntaxes:
- Modern: TOML (`chatmail.toml`)
- Legacy: maddy.conf style (the parser in `madmail_parse.rs` + `maddy.rs` can read the old blocks like `tls file ...`, `listen ...`, etc.)

Important keys (see `AppConfig` struct):
- `hostname`, `primary_domain`, `local_domains`
- `state_dir`
- `tls_mode` ("autocert", "file", "acme", etc.)
- `admin_token`, `admin_path`
- `turn_*` (enable, secret, port, etc.)
- `iroh_*`, `ss_*` (shadowsocks)
- `queue.*` (outbound retry settings)
- `debug`, `log_target`
- Many more (see the full struct and the parser)

The file is read **once at boot**. Changes require a reload or restart (depending on the key).

## Dynamic Settings (the `settings` table)

This is the heart of "change without restart".

Table (from migration):
```sql
CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
```

Accessed via:
- `chatmail_db::settings::*`
- `get_bool_setting(pool, "REGISTRATION_OPEN", true)`
- `set_setting(...)`

**Common keys** (see `settings_keys.rs`):
- `REGISTRATION_OPEN`, `JIT_REGISTRATION_ENABLED`
- `__LOG_DISABLED__` (No-Log)
- `__TURN_ENABLED__`
- `MESSAGE_SIZE_LIMIT`
- `DEFAULT_QUOTA_BYTES`
- Federation policy rows (separate tables but related)
- Many admin toggles

These are read at hydration time and also on demand. Many have in-memory caches (`FederationPolicyCache`, etc.) that are invalidated on admin changes.

## CLI Layer (`clap`)

`chatmail-config/src/cli.rs` defines the global flags and subcommands.

Global flags that affect boot:
- `--config <path>`
- `--state-dir <path>`
- `--debug`
- `--boot-once` (tests)

Every `chatmail <subcommand>` also has its own flags.

The CLI values are merged on top of file + DB in various `effective_*` functions.

## The `effective_*` Pattern (most important API)

In `chatmail-config` you will see dozens of functions like:

- `effective_imap_plain_listen(...)`
- `effective_submission_tls_listen(...)`
- `effective_default_quota_bytes(...)`
- `effective_registration_domain(...)`
- `effective_local_domains(...)`
- `effective_http_plain_listen(...)`

These are the **single source of truth** that the rest of the code should use.

They combine:
- File config
- DB overrides (e.g. `mail_ports` table for per-service port overrides)
- CLI args
- Sensible defaults
- Dev-mode aliases (`localhost`, `127.0.0.1`, etc.)

**Rule**: If you hard-code a port or path somewhere outside `chatmail-config`, you are probably doing it wrong.

## Credential Policy

`CredentialPolicy` (from file or default) controls:
- Minimum password length
- Allowed characters in localpart
- Whether to allow importing existing hashes

Used during JIT registration and `validate_localpart_and_password`.

## Multi-Domain / JIT Domain Handling

- `primary_domain` — the one users usually see
- `local_domains` — additional domains accepted for local delivery
- `jit_domain` — often set to an IP literal (`[203.0.113.50]`) so that clients connecting by IP can still register/login

Dev mode automatically adds `localhost` / `127.0.0.1` variants.

## Admin Token Resolution

See `crates/chatmail/src/admin.rs` and `boot.rs`.

Order of precedence:
1. `CHATMAIL_ADMIN_TOKEN` env?
2. Explicit `admin_token = "..."` in static config (or the literal string "disabled")
3. The file `state_dir/admin_token` (created on first boot with strong random value)

The file is never stored in the DB (security).

## Reload & Dynamic Behavior

- `POST /admin/reload` (or equivalent CLI) tells the supervisor to:
  - Re-read some DB tables (ports, etc.)
  - Rebuild HTTP routers (admin path or token can move)
  - Rebind listeners
- Not everything is reloadable (some TLS material, core listener sockets in some OSes). Full restart is still sometimes needed.

## Example: "Why is my server listening on port 1143?"

Trace the path:
1. `chatmail-imap` calls `effective_imap_plain_listen(config, db_ports_override)`
2. That function looks at CLI, then static config `imap_listen`, then DB `mail_ports` table, then default 143/993.
3. In local dev people often set `state_dir = "./data"` and a custom toml that maps to 1143 for testing without root.

## Debugging Config

Useful commands / techniques:
- `chatmail status` or the admin status resource
- `sqlite3 data/chatmail.db "SELECT * FROM settings;"`
- `sqlite3 data/chatmail.db "SELECT * FROM mail_ports;"`
- `cat data/chatmail.toml`
- `RUST_LOG=debug cargo run -p chatmail -- --debug ...` (with a config that has `log stderr`)

## Next

Now that you know how knobs flow into the system, see how authentication and first-account creation use those knobs:

→ [07-authentication-and-jit.md](./07-authentication-and-jit.md)
