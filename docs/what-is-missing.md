# CLI gaps: Madmail (`context/madmail`) vs madmail-v2

This document lists **Madmail / `maddy` operator CLI** commands defined under [`context/madmail`](../context/madmail) (symlink to the Madmail Go tree) that are **not implemented** in madmail-v2 today, plus commands that exist in both but differ in behavior.

**madmail-v2 binary:** `chatmail` (often installed as `/usr/local/bin/madmail`).  
**Reference code:** `context/madmail/internal/cli/ctl/*.go`, `context/madmail/maddy.go`.  
**Reference docs:** [`context/madmail/docs/chatmail/cli.md`](../context/madmail/docs/chatmail/cli.md) (full subcommand index), [`context/madmail/docs/chatmail/commands.md`](../context/madmail/docs/chatmail/commands.md) (install/flags detail).  
**Parity matrix (maintained):** [`docs/TDD/14-cli-tools.md`](TDD/14-cli-tools.md).

When a command is missing from the CLI, the same feature may still be available via the **Admin HTTP API** (noted below where applicable).

---

## Summary

| Status | Count (top-level) | Commands |
|--------|-------------------|----------|
| **Implemented** | 29 | `run`, `version`, `upgrade`, `update`, `admin-token`, `admin-web`, `install`*, `certificate`, `accounts`, `ban-list`, `blocklist`, `create-user`*, `delete`, `registration`, `language`, `html-export`, `html-serve`, `webimap`, `websmtp`, `federation`, `registration-tokens`, `sharing`, `status`, `uninstall`, `reload`, `endpoint-cache`, `port`, `message-size`, `tasks` |
| **Stub only** (clap parses; `dispatch` → not implemented) | 9 | `creds`, `hash`, `submission-access`, `imap-acct`, `imap-mboxes`, `imap-msgs`, `queue`, `exchanger`, `migrate-pgp-config` |
| **Madmail-only / not in chatmail clap** | — | (none material; see partial gaps) |
| **madmail-v2-only / reshaped** | — | See [madmail-v2 extensions](#madmail-v2-extensions-not-in-madmail-cli) |

\*Partial — see [Partial parity](#partial-parity-implemented-but-incomplete).

**Madmail top-level** (from `internal/cli/ctl/*.go` + `maddy.go`; `context/madmail/docs/chatmail/cli.md` quick index omits `migrate-pgp-config` but `ctl/migrate_pgp_config.go` registers it):  
`run`, `version`, `status`, `reload`, `install`, `uninstall`, `upgrade`, `update`, `hash`, `html-export`, `html-serve`, `admin-token`, `admin-web`, `port`, `submission-access`, `language`, `webimap`, `websmtp`, `accounts`, `ban-list`, `create-user`, `creds`, `imap-acct`, `imap-mboxes`, `imap-msgs`, `delete`, `blocklist`, `queue`, `federation`, `exchanger`, `registration-tokens`, `sharing`, `endpoint-cache`, `migrate-pgp-config`, plus hidden `generate-man`, `generate-fish-completion`.

**chatmail clap** adds: `certificate`, `registration` (top-level), `message-size`, `tasks` — same binary also lists all Madmail stubs above.

---

## Implemented in madmail-v2

These match Madmail closely enough for day-to-day ops on `credentials.db` + maildir:

| Command | Madmail source | madmail-v2 notes |
|---------|----------------|-------------------|
| `run` | `maddy.go` | Default server start |
| `version` | `maddy.go` | Crate version (no full Madmail build metadata) |
| `upgrade` / `update` | `ctl/upgrade.go` | Local signed file or `http(s)://` URL; Ed25519 verify; systemd stop/replace/start |
| `admin-token` | `ctl/admin_token.go` | `--raw` |
| `admin-web` | `ctl/adminweb.go` | `status`, `enable`, `disable`, `path` / `--reset` |
| `install` | `ctl/install.go` | Non-interactive + `--simple`; not full interactive/DNS-01 installer |
| `certificate` | — | **madmail-v2 addition** (HTTP-01 / lers), not in Go Madmail CLI |
| `accounts` | `ctl/accounts_bulk.go` | All subcommands below |
| `ban-list` | `ctl/accounts_direct.go` | Alias of `accounts ban-list` |
| `blocklist` | `ctl/blocklist.go` | `list`, `add`, `remove` |
| `create-user` | `ctl/create_user.go` | Random account JSON — **`dclogin` only** (Madmail also prints `email`, `password`; see partial) |
| `delete` | `ctl/delete.go` | Full delete + blocklist; fewer module flags |
| `registration` | `ctl/users.go` (Madmail: `creds registration`) | Top-level `open`, `close`, `status` on `__REGISTRATION_OPEN__` |
| `language` | `ctl/language.go` | `status`, `set`, `reset` on `__LANGUAGE__` (en, fa, ru, es) |
| `webimap` / `websmtp` | `ctl/webmail_services.go` | `status`, `enable`, `disable` on `__WEBIMAP_ENABLED__` / `__WEBSMTP_ENABLED__` |
| `html-export` / `html-serve` | `ctl/html.go` | See [HTML www overrides](#html-www-overrides-done) |
| `federation` | `ctl/federation.go` | Madmail: `policy`, `block`, `allow`, `remove`, `flush`, `list`, `status`. **Also:** `dismiss`, `undismiss`, `dismiss-list`, `dismiss-flush` (madmail-v2; `federation_silent_dismiss` table) |
| `registration-tokens` | `ctl/registration_token.go` | `create`, `list`, `status`, `delete` |
| `sharing` | `ctl/sharing.go` | `list`, `create`, `reserve`, `remove`, `edit` (`{state_dir}/sharing.db`) |
| `uninstall` | `ctl/uninstall.go` | `--force`, `--keep-data` / `--keep-user` / `--keep-config` / `--keep-binary`, `--dry-run`, `--log-file` |
| `status` | `ctl/online.go` | `--details` / `-d`; `ss` connection counts, registered users, `server_tracker.json` |
| `reload` | `ctl/reload_config.go` | POST admin envelope to `/admin/reload`; `--url`, `--insecure` |
| `endpoint-cache` | `ctl/dnscache.go` | Alias `dns-cache`; `list`, `set`, `get`, `remove` on `dns_overrides` |
| `port` | `ctl/port.go` | `status` + per-service `status`/`set`/`reset`/`local`/`public` on `__*_PORT__` / `__*_LOCAL_ONLY__` |
| `message-size` | `ctl/appendlimit.go` (per-user) + install `--max-message-size` | **madmail-v2 top-level** — `status`, `set`, `reset` on `__APPENDLIMIT__` / `__MAX_MESSAGE_SIZE__` |
| `tasks` | imapsql periodic cleanup (no dedicated ctl in Go) | **madmail-v2** — `list`, `run`, `run-all`; see [`TDD/21-scheduled-maintenance.md`](TDD/21-scheduled-maintenance.md) |

### `accounts` subcommands (done)

- `status`, `info`, `create`, `create-random`, `delete`, `ban`, `unban`, `ban-list`, `export`, `import`, `delete-all`

Tests: `cargo test -p chatmail ctl`, `cargo test -p chatmail-integration --test ctl_cli_e2e`, `--test ctl_ops_e2e`.

### HTML www overrides (done)

Madmail parity for operator-owned www trees (`ctl/html.go`):

| Piece | Location |
|-------|----------|
| CLI | `crates/chatmail/src/ctl/html.rs` — `html-export DEST`, `html-serve DIR` / `embedded` |
| Export | `crates/chatmail-www/src/export.rs` — all embedded `www/` assets → disk (`WwwAssets::iter()` count) |
| Config | `crates/chatmail-config/src/config_www.rs` — `www_dir` in `chatmail { }` blocks (`maddy.conf`) or `chatmail.toml` |
| Runtime | `chatmail-www`: `TemplateEngine::from_config` loads `.html` from `www_dir`; `read_asset_bytes` serves CSS/JS/SVG from disk first, then embed |

**Default site (no `www_dir`):** HTML templates and all static files are served from **embedded RAM** in the binary (`rust_embed`) — preloaded at startup, no disk I/O.

**Operator override:** `html-export` → edit files → `html-serve /path/to/www` → `systemctl restart madmail` (once, to set `www_dir`). After that, files are read from disk on each request (live reload; `Cache-Control: no-cache`). `html-serve embedded` clears `www_dir` and restores the RAM default.

**Manually verified:** export count, config `www_dir`, journal line `www: serving HTML from external directory`, custom homepage + static file over HTTP/HTTPS, revert to embedded.

**Note:** Configs with two `chatmail` listeners (e.g. `:80` and `:443`) get `www_dir` set in **both** blocks — same as needing both endpoints to see the same www root.

### `message-size` (done)

Madmail sets limits via install (`--max-message-size`) and per-account `imap-acct appendlimit USERNAME`. madmail-v2 exposes a **server-wide** CLI backed by the settings DB:

| Subcommand | Settings keys |
|------------|---------------|
| `status` (default) | Shows effective bytes, config default, DB overrides |
| `set SIZE` | Writes `__APPENDLIMIT__` and `__MAX_MESSAGE_SIZE__` |
| `reset` | Clears DB overrides |

Code: `crates/chatmail/src/ctl/message_size.rs`. Apply to a running server: `chatmail reload`.

### `tasks` (done — madmail-v2 only)

On-demand maintenance (Madmail runs similar jobs inside `imapsql` when the server is up; there is no `maddy tasks` command):

| Subcommand | Purpose |
|------------|---------|
| `list` | Jobs + whether retention is enabled in config |
| `run TASK` | One job; optional `--retention`. Names: `prune-old-messages` (alias `retention`), `prune-unused-accounts` (alias `prune-unused`), `purge-seen` / `purge-read`, `prune-unread-older` |
| `run-all` | All jobs enabled by `storage.imapsql` retention in config |

Code: `crates/chatmail/src/ctl/tasks.rs`, `chatmail-tasks` crate. Prefer `tasks` + Admin `/admin/queue` over a future `queue purge` for operators.

### madmail-v2 extensions (not in Madmail CLI)

| Piece | Madmail | madmail-v2 |
|-------|---------|-------------|
| `certificate` | — (TLS via install / autocert) | `get`, `regenerate` (lers HTTP-01) |
| `registration` | `creds registration {open\|close\|status}` | Top-level `registration` (same `__REGISTRATION_OPEN__` DB key) |
| `message-size` | Install `--max-message-size`; per-user `imap-acct appendlimit` | Top-level `status` / `set` / `reset` on settings DB |
| `tasks` | Periodic jobs inside running server only | `list`, `run`, `run-all` on demand |
| `federation dismiss-*` | Not in `ctl/federation.go` | `dismiss`, `undismiss`, `dismiss-list`, `dismiss-flush` |

---

## Missing top-level commands

Invoking these today prints: *`'chatmail <name>' is not implemented in madmail-v2 yet`*.

### `creds` — `ctl/users.go`

Manage credentials and runtime toggles via the auth module (Madmail loads `pass_table` / config blocks).

| Subcommand | Further subcommands |
|------------|---------------------|
| `list` | — |
| `create` | — |
| `remove` | — |
| `password` | — |
| `registration` | `open`, `close`, `status` — **use top-level** `chatmail registration` (done); not under `creds` yet |
| `jit` | `enable`, `disable`, `status` |
| `turn` | `on`, `off`, `status` |
| `logging` | `on`, `off`, `status` |

**Admin API overlap:** registration/JIT/TURN/logging toggles exist under `/admin/...` (e.g. toggles resources). No single CLI equivalent.

**Flags (all subcommands):** `--cfg-block` (default `local_authdb`).

---

### `hash` — `ctl/hash.go`

Print password hashes for `pass_table` (`bcrypt`, `argon2`, cost flags). Madmail reads password from stdin unless `--password` / `-p`.

---

### `submission-access` — `ctl/submission_access.go`

| Subcommand |
|------------|
| `status` |
| `local` |
| `public` |

---

### `imap-acct` — `ctl/imapacct.go`, `appendlimit.go`

Storage-account tooling (module framework + `local_mailboxes`).

| Subcommand | Notes |
|------------|--------|
| `list` | |
| `create` | `--no-specialuse`, folder name overrides |
| `remove` | |
| `quota` | `get`, `set`, `reset`, `list`, `set-default` |
| `purge-msgs` | |
| `purge-all` | |
| `purge-read` | |
| `prune-unread` | |
| `stat` | |
| `appendlimit` | per-user APPENDLIMIT (server-wide limit → use **`message-size`** in madmail-v2) |
| `prune-unused` | age argument (e.g. `720h`) — overlap with **`tasks run prune-unused-accounts`** |

**Flags:** `--cfg-block` (default `local_mailboxes`).

**madmail-v2:** mail is maildir under `state_dir/mail/`; no separate imapsql account CLI.

---

### `imap-mboxes` — `ctl/imap.go`

| Subcommand |
|------------|
| `list` |
| `create` |
| `remove` |
| `rename` |

---

### `imap-msgs` — `ctl/imap.go`

Debug / migration tooling.

| Subcommand |
|------------|
| `add` |
| `add-flags` |
| `rem-flags` |
| `set-flags` |
| `remove` |
| `copy` |
| `move` |
| `list` |
| `dump` |

---

### `queue` — `ctl/queue.go`

| Subcommand | Notes |
|------------|--------|
| `purge USERNAME` | Required arg. `--sender` / `--recipient` are **boolean** flags (default `true`) controlling which direction to purge; `--cfg-block` (default `remote_queue`) |

**madmail-v2:** use **`tasks`** for retention; queue purge via Admin API when exposed.

---

### `exchanger` — `ctl/exchanger.go`

Pull-based relay (exchanger) management.

| Subcommand |
|------------|
| `list` |
| `add` | `--interval` |
| `remove` |
| `enable` |
| `disable` |

---

### `migrate-pgp-config` — `ctl/migrate_pgp_config.go`

One-time config rewrite: move submission PGP policy from `check.pgp_encryption` to endpoint `pgp_*` directives (creates `.bak`). Flags: `--config`, `--dry-run`.

**madmail-v2:** declared in clap; **not implemented**. Documented in Madmail: `context/madmail/docs/code/message-checks-pipeline.md`, `pgp-verification.md`.

---

## Partial parity (implemented but incomplete)

| Area | Madmail | madmail-v2 gap |
|------|---------|-----------------|
| **`run`** | `--log` targets | Logging via config / tracing only |
| **`install`** | Full interactive installer, Cloudflare/DNS, many flags (`--enable-iroh`, `--lang`, …) | Subset (`--simple`, non-interactive); no DNS-01 in install |
| **`accounts status` / `info`** | Human sizes (GB), RFC3339 times, dual DB via `--cfg-block` + `--storage-cfg-block` | Plain text / unix timestamps; single SQLite path |
| **`accounts create`** | `--hash`, `--bcrypt-cost`; interactive password | `--password` only; fixed hash path in Rust |
| **`create-user`** / **`accounts create-random`** | JSON: `email`, `password`, `dclogin`; `--json-only` suppresses extra stdout | JSON: **`dclogin` only** (URI query keys `ih`/`ip`/`is`/`sh`/`sp`/`ss`); `--json-only` flag **ignored** in Rust |
| **`delete`** | `--auth-block`, `--storage-block` | `--reason`, `-y`; unified `--state-dir` |
| **`blocklist`** | `--cfg-block` | Direct `credentials.db` |
| **Session kick on ban/delete** | Reload / signal active IMAP sessions | Not implemented (DB block only) |
| **`version`** | Full build info (git, etc.) | Crate `0.1.0` style version |
| **Message limits** | Per-user `imap-acct appendlimit` + install flag | Server-wide `message-size` only (no per-account CLI) |
| **`webimap` / `websmtp` enable/disable** | DB write + `reloadRunningDaemons()` → **SIGUSR2** on Linux | DB write only; needs `reload` / restart for in-process effect |

---

## Madmail global / hidden (not ported)

| Item | Notes |
|------|--------|
| `generate-man`, `generate-fish-completion` | Hidden dev helpers in `app.go` |
| `debug.pprof`, `debug.blockprofrate`, … | Build-tag debug flags on `run` |
| `--cfg-block` / `MADDY_CFGBLOCK` | Used on most Madmail ctl commands |
| `--storage-cfg-block` / `MADDY_STORAGE_CFGBLOCK` | Used on `accounts` / storage commands |
| Deprecated `./maddy` without `run` | Still supported in Go; chatmail uses explicit `run` or default |

---

## Suggested implementation order

Aligned with [`docs/TDD/14-cli-tools.md`](TDD/14-cli-tools.md):

1. **`hash`** — operators need this often.
2. **`creds`** (`jit`, `turn`, `logging`, list/create/remove/password) — mirror admin toggles; `registration` is already top-level.
3. **`submission-access`** — thin wrapper over `settings` table + shared admin helpers.
4. **`migrate-pgp-config`** — port `ctl/migrate_pgp_config.go` for legacy installs (config-only, no DB).
5. **`imap-acct`** (quota/purge) — map to maildir + `quotas` / message stats where possible; retention largely covered by **`tasks`**.
6. **`queue`**, **`exchanger`**, **`imap-mboxes`**, **`imap-msgs`** — lower priority or defer.

**Done (no longer on this list):** `registration`, `language`, `webimap`, `websmtp`, `html-export`, `html-serve`, `federation`, `registration-tokens`, `sharing`, `status`, `uninstall`, `reload`, `endpoint-cache`, `port`, `message-size`, `tasks`.

Reuse logic from `chatmail-admin` and `chatmail-db` for every command that already has an admin route, so CLI and HTTP stay in sync.

---

## Quick check

```bash
# Implemented (examples)
chatmail accounts status --state-dir /var/lib/madmail
chatmail registration status --config /etc/madmail/madmail.conf
chatmail language status --state-dir /var/lib/madmail
chatmail webimap status --state-dir /var/lib/madmail
chatmail html-export /tmp/www-test
chatmail html-serve /var/lib/madmail/www-custom   # then: systemctl restart madmail
chatmail html-serve embedded                      # revert www_dir
chatmail federation policy accept --state-dir /var/lib/madmail
chatmail registration-tokens create --max-uses 10 --comment "onboarding"
chatmail sharing list --state-dir /var/lib/madmail
chatmail status --details --state-dir /var/lib/madmail
chatmail uninstall --dry-run                      # preview removal (root)

chatmail port status --state-dir /var/lib/madmail
chatmail endpoint-cache list --state-dir /var/lib/madmail
chatmail reload --state-dir /var/lib/madmail   # needs running server + admin token

chatmail message-size status --state-dir /var/lib/madmail
chatmail message-size set 100M --state-dir /var/lib/madmail
chatmail tasks list --state-dir /var/lib/madmail
chatmail tasks run prune-old-messages --state-dir /var/lib/madmail

# Not implemented (will error)
chatmail creds list --state-dir /var/lib/madmail
chatmail hash -p 'secret'
chatmail migrate-pgp-config --config /etc/madmail/madmail.conf --dry-run
```
