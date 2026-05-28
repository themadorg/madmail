# CLI tools (Madmail parity)

chatmail-rs exposes the same **single binary** model as Madmail: one executable (`chatmail`, deployed as `/usr/local/bin/madmail` on test servers) with global flags and subcommands.

**Reference docs:** [`context/madmail/docs/chatmail/commands.md`](../../context/madmail/docs/chatmail/commands.md) (operator guide; incomplete vs full Go tree).

**Reference code:** [`context/madmail/internal/cli/`](../../context/madmail/internal/cli/) — `app.go` registers subcommands from `ctl/*.go` via `init()` + `AddSubcommand`.

**Rust layout (target):**

| Crate / path | Role |
|--------------|------|
| `chatmail-config::cli` | `clap` root: global flags + subcommand enum |
| `chatmail::ctl::*` | One module per Madmail ctl file (direct DB / no full module framework where possible) |
| `chatmail::upgrade` | Signed binary replacement + systemd (mirrors `ctl/upgrade.go`) |
| `chatmail::boot` | Server start (`run`) |

Madmail ctl commands use **`--cfg-block`** / **`--storage-cfg-block`** when config blocks are not `local_authdb` / `local_mailboxes`. Rust ports should accept the same flags and talk to `chatmail.db` / maildir via `chatmail-db` + `chatmail-storage`, not load the Go module framework.

After on-disk DB changes while the daemon runs, operators call **`POST /admin/cache/reload`** (Admin API) — same as Madmail.

---

## Global flags

| Flag | Madmail | chatmail-rs | Notes |
|------|---------|-------------|-------|
| `--config` | `MADDY_CONFIG` → `/etc/<binary>/<binary>.conf` | `CHATMAIL_CONFIG`; auto **`./data/chatmail.toml`** when present | Parses `maddy.conf` + TOML |
| `--state-dir` | `/var/lib/<binary>` | `CHATMAIL_STATE_DIR`; auto **`./data`** when it contains `chatmail.db` / `admin_token` | Overridden by `state_dir` in config |
| `--debug` | yes | — | Use `debug` in config file only |
| `run --libexec` | yes | `--libexec` (alias of `--state-dir`) | **done** (Madmail systemd `ExecStart` compat) |
| `run --log` | yes | — | Use `log` in config file only (default off) |

---

## Command parity matrix

Status legend: **done** · **planned** (TDD scope) · **defer** (post-MVP) · **n/a** (Madmail-only / removed)

### Server lifecycle

| Command | Subcommands / notes | Madmail source | chatmail-rs |
|---------|---------------------|----------------|-------------|
| *(default)* / `run` | Start SMTP/IMAP/HTTP/federation | `maddy.go` | **done** |
| `upgrade` | Signed file or URL | `ctl/upgrade.go` | **done** |
| `update` | Alias of `upgrade` (URL or local path) | `ctl/upgrade.go` | **done** |
| `version` | Build metadata | `maddy.go` | **done** (crate version) |
| `install` | Interactive / simple / non-interactive setup | `ctl/install.go`, `maddy.conf.j2` | **done** (non-interactive + `--simple`; no DNS-01 install yet) |
| `certificate` | `get`, `regenerate` (Let's Encrypt HTTP-01) | — (chatmail-rs + lers) | **done** |
| `uninstall` | `--keep-data`, `--keep-config`, … | `ctl/uninstall.go` | **done** |
| `reload` | POST `/admin/reload` (process restart) | `ctl/reload_config.go` | **done** (`--url`, `--insecure`) |
| `status` | Connections, users, uptime (`--details`) | `ctl/online.go` | **done** |

### Deploy & signing

| Mechanism | Madmail | chatmail-rs |
|-----------|---------|-------------|
| `make push` | `Makefile` + `sign.py` | **done** (`madmailv2/Makefile`) |
| Ed25519 pubkey | `internal/auth/signature_key.go` | **done** (same hex in `upgrade.rs`) |

### Operator / token

| Command | Madmail source | chatmail-rs |
|---------|----------------|-------------|
| `admin-token` | `ctl/admin_token.go` | **done** — pretty URL + token; `--raw` for scripts; reads `__SMTP_HOSTNAME__`, `__HTTPS_PORT__`, `__ADMIN_PATH__` from DB |
| `admin-web` | `ctl/adminweb.go` | **done** — CLI toggles `__ADMIN_WEB_*__`; HTTP serves embedded `context/madmail/admin-web/build` at `admin_web_path` |
| `admin-web` | `ctl/adminweb.go` | **planned** (enable/disable www override) |
| `hash` | `ctl/hash.go` | **planned** (password hashing helper) |

### Accounts & credentials (direct DB)

| Command | Subcommands | Madmail source | chatmail-rs |
|---------|-------------|----------------|-------------|
| `accounts` | `status`, `info`, `create`, `create-random`, `delete`, `ban`, `unban`, `ban-list`, `export`, `import`, `delete-all` | `ctl/accounts_bulk.go`, `accounts_direct.go` | **done** — direct DB via `chatmail-db` + maildir |
| `ban-list` | Top-level alias | `ctl/accounts_direct.go` | **done** (alias of `accounts ban-list`) |
| `creds` | `list`, `create`, `remove`, `password`, `registration`, `jit`, `turn`, `logging` | `ctl/users.go` | **planned** |
| `create-user` | Random account JSON | `ctl/create_user.go` | **done** (same as `accounts create-random`) |
| `delete` | Remove user + mail + blocklist | `ctl/delete.go` | **done** |

### IMAP tooling

| Command | Subcommands | Madmail source | chatmail-rs |
|---------|-------------|----------------|-------------|
| `imap-acct` | `list`, `create`, `remove`, `quota`, `purge-*`, `stat`, `appendlimit`, `prune-unused` | `ctl/imapacct.go`, `appendlimit.go` | **planned** (`prune-unused` → `tasks run prune-unused-accounts`) |
| `imap-mboxes` | `list`, `create`, `remove`, `rename` | `ctl/imap.go` | **planned** |
| `imap-msgs` | `add`, flags, `remove`, `copy`, `move`, `list`, `dump` | `ctl/imap.go` | **defer** (debug / migration) |

### Policy & runtime settings (DB `settings` table)

| Command | Subcommands | Madmail source | chatmail-rs |
|---------|-------------|----------------|-------------|
| `federation` | `policy`, `block`, `allow`, `remove`, `flush`, `list`, `status` | `ctl/federation.go` | **done** |
| `blocklist` | `list`, `add`, `remove` | `ctl/blocklist.go` | **done** |
| `sharing` | `list`, `create`, `remove`, `edit`, `reserve` | `ctl/sharing.go` | **done** |
| `endpoint-cache` | `list`, `set`, `get`, `remove` | `ctl/dnscache.go` | **done** (alias `dns-cache`) |
| `registration` | `open`, `close`, `status` | `ctl/users.go` (`creds registration`) | **done** (top-level `chatmail registration`) |
| `registration-tokens` | `create`, `list`, `status`, `delete` | `ctl/registration_token.go` | **done** |
| `port` | `status`, `set`, `reset`, `local`, `public` | `ctl/port.go` | **done** (per-service subcommands) |
| `submission-access` | `status`, `local`, `public` | `ctl/submission_access.go` | **planned** |
| `language` | `status`, `set`, `reset` | `ctl/language.go` | **done** |
| `exchanger` | `list`, `add`, `remove`, `enable`, `disable` | `ctl/exchanger.go` | **defer** |
| `queue` | `purge` | `ctl/queue.go` | **defer** (use `tasks` + `/admin/queue`) |
| `tasks` | `list`, `run`, `run-all` | `imapsql` cleanup loops | **done** — [`21-scheduled-maintenance.md`](21-scheduled-maintenance.md) |

### Web / HTML

| Command | Madmail source | chatmail-rs |
|---------|----------------|-------------|
| `html-export` | `ctl/html.go` | **done** |
| `html-serve` | `ctl/html.go` | **done** |
| `webimap` | — (admin API only in Go) | **done** (`enable` / `disable` / `status`) |
| `websmtp` | — | **done** |

### Hidden / dev

| Command | Madmail source | chatmail-rs |
|---------|----------------|-------------|
| `generate-man` | `app.go` | **n/a** |
| `generate-fish-completion` | `app.go` | **n/a** |
| `debug.pprof` flags | `maddy.go` (build tag) | **defer** |

---

## Tests

| Layer | Location | Run |
|-------|----------|-----|
| CLI parse | `chatmail-config::cli` tests | `cargo test -p chatmail-config accounts_subcommands blocklist_subcommands` |
| Unit + in-process dispatch | `chatmail::ctl::{accounts,ops_tests,...}` | `cargo test -p chatmail ctl` |
| E2E (subprocess) | `tests/ctl_cli_e2e.rs`, `tests/ctl_ops_e2e.rs` | `cargo test -p chatmail-integration --test ctl_cli_e2e --test ctl_ops_e2e` |

`CtlContext::open_pool` creates `credentials.db` when missing so CLI works on a fresh `--state-dir` without a prior `run`.

---

## Implementation phases

Aligned with [`../plans/`](../plans/) and Admin API coverage:

1. **Phase 1 (done):** `run`, global flags, `upgrade`, `version`, `admin-token`, `make push` / `sign`.
2. **Phase 2 — ops:** `status`, `reload`, `hash`; improve `version` (git SHA). (`update` alias **done**.)
3. **Phase 3 — accounts:** `accounts`, `creds`, `create-user`, `delete`, `ban-list` (direct SQLite; silent CLI per Madmail nolog policy).
4. **Phase 4 — mailboxes:** `imap-acct`, `blocklist`, `registration-tokens`.
5. **Phase 5 — policy:** `federation`, `sharing`, `endpoint-cache`, `port`, `submission-access` (`language` done).
6. **Phase 6+:** `install` / `uninstall`, `imap-msgs`, `queue`, `exchanger`, HTML overrides.

Where Admin API already implements a resource (`/admin/accounts`, `/admin/blocklist`, …), CLI commands should call the same logic in a shared library crate (e.g. `chatmail-admin` helpers) to avoid drift.

---

## `internal/cli` file map

| Go file | Top-level command(s) | Priority |
|---------|----------------------|----------|
| `app.go` | Registration, global flags, `run` hack | — |
| `extflag.go` | Extended flag helpers | — |
| `ctl/upgrade.go` | `upgrade`, `update` | P1 |
| `ctl/admin_token.go` | `admin-token` | P1 ✓ |
| `ctl/online.go` | `status` | P2 |
| `ctl/reload_config.go` | `reload` | **done** |
| `ctl/install.go` | `install` | P6 |
| `ctl/uninstall.go` | `uninstall` | P6 |
| `ctl/accounts_bulk.go` | `accounts` | P3 |
| `ctl/accounts_direct.go` | `ban-list`, direct DB helpers | P3 |
| `ctl/users.go` | `creds` | P3 |
| `ctl/create_user.go` | `create-user` | P3 |
| `ctl/delete.go` | `delete` | P3 |
| `ctl/imapacct.go` | `imap-acct` | P4 |
| `ctl/appendlimit.go` | (used by imap-acct) | P4 |
| `ctl/blocklist.go` | `blocklist` | P4 |
| `ctl/federation.go` | `federation` | P5 |
| `ctl/sharing.go` | `sharing` | P5 |
| `ctl/dnscache.go` | `endpoint-cache` | **done** |
| `ctl/registration_token.go` | `registration-tokens` | P4 |
| `ctl/port.go` | `port` | **done** |
| `ctl/submission_access.go` | `submission-access` | P5 |
| `ctl/language.go` | `language` | P5 |
| `ctl/adminweb.go` | `admin-web` | P5 |
| `ctl/exchanger.go` | `exchanger` | defer |
| `ctl/queue.go` | `queue` | defer |
| `ctl/imap.go` | `imap-mboxes`, `imap-msgs` | defer |
| `ctl/html.go` | `html-export`, `html-serve` | **done** (config + runtime `www_dir`) |
| `ctl/hash.go` | `hash` | P2 |
| `ctl/dbconfig.go` | Shared DB open helpers | internal |
| `ctl/moduleinit.go` | Module init (Go-only) | n/a |
| `ctl/maddy.conf.j2` | Install template | P6 |
| `ctl/dns.zone.j2` | Install DNS template | P6 |

---

## Testing

| Layer | Approach |
|-------|----------|
| Unit | `clap` parsing (`chatmail-config::cli` tests), `verify_signature` on fixture binary |
| Integration | `chatmail upgrade` against temp signed file; `admin-token` with temp state dir |
| E2E | Reuse Madmail scenarios in `context/madmail/tests/deltachat-test/scenarios/test_10_upgrade_mechanism.py` against chatmail binary |

---

## Deviations from Madmail

- Binary name **`chatmail`** in development; production test servers still invoke **`madmail`** path and unit name derived from basename (`madmail.service`).
- **`upgrade` / `update`:** `http(s)://` download (100 MB cap, TLS verify skipped for self-signed peers), then same signed replace flow as Madmail.
- **`install`** will likely remain a shell/Makefile wrapper around `chatmail install` once ported, reusing `maddy.conf.j2` generation logic in Rust or templating.

## Related RFCs

CLI commands configure and operate protocol endpoints defined by these specs. Full library: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local file |
|-----|-------|------------|
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | `certificate get` / ACME | [rfc8555.txt](RFC/rfc8555.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | `certificate`, TLS material | [rfc8446.txt](RFC/rfc8446.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP-related ctl (ports, queue) | [rfc5321.txt](RFC/rfc5321.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP-related ctl | [rfc3501.txt](RFC/rfc3501.txt) |

See also section-specific RFC tables: [02-smtp-server.md](02-smtp-server.md), [03-imap-server.md](03-imap-server.md), [19-certificates.md](19-certificates.md).
