# CLI tools (Madmail parity)

madmail-v2 exposes the same **single binary** model as Madmail: one executable (`chatmail` in dev, **`madmail`** in production) with global flags and subcommands.

## Operator guide (per-command reference)

**Primary operator docs:** [`../guide/cli/README.md`](../guide/cli/README.md) — one page per command (flags, examples, JSON schemas).

| Topic | Guide index |
|-------|-------------|
| Global flags, `--json` | [`global-flags.md`](../guide/cli/global-flags.md) · [`json-output.md`](../guide/cli/json-output.md) |
| Install / uninstall | [`install.md`](../guide/cli/install.md) · [`uninstall.md`](../guide/cli/uninstall.md) |
| TLS / ACME | [`certificate.md`](../guide/cli/certificate.md) · [`certificate-autocert.md`](../guide/cli/certificate-autocert.md) |
| Accounts & registration | [`accounts.md`](../guide/cli/accounts.md) · [`registration.md`](../guide/cli/registration.md) · [`registration-tokens.md`](../guide/cli/registration-tokens.md) |
| Federation & routing | [`federation.md`](../guide/cli/federation.md) · [`endpoint-cache.md`](../guide/cli/endpoint-cache.md) |
| Services & ports | [`port.md`](../guide/cli/port.md) · [`push.md`](../guide/cli/push.md) · [`webimap.md`](../guide/cli/webimap.md) · [`websmtp.md`](../guide/cli/websmtp.md) |
| Maintenance | [`tasks.md`](../guide/cli/tasks.md) · [`tasks-run.md`](../guide/cli/tasks-run.md) |
| Message limits | [`message-size.md`](../guide/cli/message-size.md) |

**Design / parity (this file):** implementation status, `ctl/` module map, Madmail Go references.

**Madmail Go reference:** [`context/madmail/docs/chatmail/commands.md`](../../context/madmail/docs/chatmail/commands.md).

**Rust layout:**

| Crate / path | Role |
|--------------|------|
| `chatmail-config::cli` | `clap` root: global flags + subcommand enum (`cli.rs`) |
| `chatmail::ctl::*` | One module per command family (`dispatch.rs` routes here) |
| `chatmail::upgrade` | Signed binary replacement + systemd |
| `chatmail::boot` | Server start (`run`) |

After on-disk DB changes while the daemon runs, operators call **`madmail reload`** or **`POST /admin/reload`** (Admin API).

---

## Global flags

| Flag | Madmail | madmail-v2 | Guide |
|------|---------|-------------|-------|
| `--config` | `MADDY_CONFIG` | `CHATMAIL_CONFIG`; auto `./data/chatmail.toml` | [`global-flags.md`](../guide/cli/global-flags.md) |
| `--state-dir` / `--libexec` | `/var/lib/<binary>` | `CHATMAIL_STATE_DIR`; auto `./data` when DB present | same |
| `--json` | — | Machine-readable stdout (all ctl commands) | [`json-output.md`](../guide/cli/json-output.md) |
| `--debug` | yes | — | Use `debug` in config file only |
| `run --log` | yes | — | Use `log` in config file only (default off) |

---

## Command index (guide → implementation)

Status: **done** · **planned** (parsed, `not_implemented`) · **defer**

| Command | Guide | `ctl/` module | Status |
|---------|-------|---------------|--------|
| `run` | [run.md](../guide/cli/run.md) | `boot` | **done** |
| `install` | [install.md](../guide/cli/install.md) | `install/` | **done** |
| `uninstall` | [uninstall.md](../guide/cli/uninstall.md) | `uninstall.rs` | **done** |
| `upgrade` / `update` | [upgrade.md](../guide/cli/upgrade.md) | `upgrade.rs` | **done** |
| `version` | [version.md](../guide/cli/version.md) | `version.rs` | **done** |
| `reload` | [reload.md](../guide/cli/reload.md) | `reload.rs` | **done** |
| `status` | [status.md](../guide/cli/status.md) | `status_cmd.rs` | **done** |
| `completion` | [completion.md](../guide/cli/completion.md) | `docs.rs` | **done** |
| `admin-token` | [admin-token.md](../guide/cli/admin-token.md) | `admin_token.rs` | **done** |
| `admin-web` | [admin-web.md](../guide/cli/admin-web.md) | `admin_web.rs` | **done** |
| `certificate` | [certificate.md](../guide/cli/certificate.md) | `certificate.rs` | **done** (`get`, `regenerate`, `status`, `autocert`) |
| `accounts` | [accounts.md](../guide/cli/accounts.md) | `accounts.rs` | **done** |
| `ban-list` | [ban-list.md](../guide/cli/ban-list.md) | `accounts.rs` | **done** (alias) |
| `blocklist` | [blocklist.md](../guide/cli/blocklist.md) | `blocklist_cmd.rs` | **done** |
| `create-user` | [create-user.md](../guide/cli/create-user.md) | `accounts.rs` | **done** |
| `delete` | [delete.md](../guide/cli/delete.md) | `delete_cmd.rs` | **done** |
| `registration` | [registration.md](../guide/cli/registration.md) | `registration.rs` | **done** |
| `registration-tokens` | [registration-tokens.md](../guide/cli/registration-tokens.md) | `registration_tokens.rs` | **done** |
| `federation` | [federation.md](../guide/cli/federation.md) | `federation.rs` | **done** (+ `dismiss`, `undismiss`, `dismiss-list`, `dismiss-flush`) |
| `endpoint-cache` / `dns-cache` | [endpoint-cache.md](../guide/cli/endpoint-cache.md) | `endpoint_cache.rs` | **done** |
| `sharing` | [sharing.md](../guide/cli/sharing.md) | `sharing.rs` | **done** |
| `port` | [port.md](../guide/cli/port.md) | `port.rs` | **done** |
| `message-size` | [message-size.md](../guide/cli/message-size.md) | `message_size.rs` | **done** |
| `language` | [language.md](../guide/cli/language.md) | `language.rs` | **done** |
| `push` | [push.md](../guide/cli/push.md) | `push.rs` | **done** |
| `webimap` | [webimap.md](../guide/cli/webimap.md) | `service_toggle.rs` | **done** |
| `websmtp` | [websmtp.md](../guide/cli/websmtp.md) | `service_toggle.rs` | **done** |
| `tasks` | [tasks.md](../guide/cli/tasks.md) | `tasks.rs` | **done** |
| `html-export` | [html-export.md](../guide/cli/html-export.md) | `html.rs` | **done** |
| `html-serve` | [html-serve.md](../guide/cli/html-serve.md) | `html.rs` | **done** |
| `creds` | [creds.md](../guide/cli/creds.md) | — | **planned** |
| `hash` | [hash.md](../guide/cli/hash.md) | — | **planned** |
| `submission-access` | [submission-access.md](../guide/cli/submission-access.md) | — | **planned** |
| `queue` | [queue.md](../guide/cli/queue.md) | — | **defer** (use `tasks` + `/admin/queue`) |
| `exchanger` | [exchanger.md](../guide/cli/exchanger.md) | — | **defer** |
| `imap-acct` | [imap-acct.md](../guide/cli/imap-acct.md) | — | **planned** |
| `imap-mboxes` | [imap-mboxes.md](../guide/cli/imap-mboxes.md) | — | **planned** |
| `imap-msgs` | [imap-msgs.md](../guide/cli/imap-msgs.md) | — | **defer** |
| `migrate-pgp-config` | [migrate-pgp-config.md](../guide/cli/migrate-pgp-config.md) | — | **planned** |

`dispatch.rs` `not_implemented` list (parsed but no handler): `creds`, `hash`, `submission-access`, `queue`, `exchanger`, `imap-acct`, `imap-mboxes`, `imap-msgs`, `migrate-pgp-config`.

---

## Command parity matrix (by category)

### Server lifecycle

| Command | Guide | Madmail source | madmail-v2 |
|---------|-------|----------------|-------------|
| `run` | [run.md](../guide/cli/run.md) | `maddy.go` | **done** |
| `install` | [install.md](../guide/cli/install.md) | `ctl/install.go` | **done** (non-interactive + `--simple`; no DNS-01) |
| `uninstall` | [uninstall.md](../guide/cli/uninstall.md) | `ctl/uninstall.go` | **done** |
| `upgrade` / `update` | [upgrade.md](../guide/cli/upgrade.md) | `ctl/upgrade.go` | **done** |
| `version` | [version.md](../guide/cli/version.md) | `maddy.go` | **done** |
| `reload` | [reload.md](../guide/cli/reload.md) | `ctl/reload_config.go` | **done** |
| `status` | [status.md](../guide/cli/status.md) | `ctl/online.go` | **done** (`--details`) |
| `certificate` | [certificate.md](../guide/cli/certificate.md) | — (lers) | **done** — `get`, `regenerate`, `status`, [`autocert`](../guide/cli/certificate-autocert.md) |

### Accounts & credentials

| Command | Guide | Madmail source | madmail-v2 |
|---------|-------|----------------|-------------|
| `accounts` | [accounts.md](../guide/cli/accounts.md) | `ctl/accounts_*.go` | **done** |
| `ban-list` | [ban-list.md](../guide/cli/ban-list.md) | `ctl/accounts_direct.go` | **done** |
| `blocklist` | [blocklist.md](../guide/cli/blocklist.md) | `ctl/blocklist.go` | **done** |
| `create-user` | [create-user.md](../guide/cli/create-user.md) | `ctl/create_user.go` | **done** |
| `delete` | [delete.md](../guide/cli/delete.md) | `ctl/delete.go` | **done** |
| `registration` | [registration.md](../guide/cli/registration.md) | `ctl/users.go` | **done** |
| `registration-tokens` | [registration-tokens.md](../guide/cli/registration-tokens.md) | `ctl/registration_token.go` | **done** |
| `creds` | [creds.md](../guide/cli/creds.md) | `ctl/users.go` | **planned** |

### Policy & delivery

| Command | Guide | Madmail source | madmail-v2 |
|---------|-------|----------------|-------------|
| `federation` | [federation.md](../guide/cli/federation.md) | `ctl/federation.go` | **done** — includes silent dismiss (`chatmail-state::silent_dismiss`) |
| `endpoint-cache` | [endpoint-cache.md](../guide/cli/endpoint-cache.md) | `ctl/dnscache.go` | **done** |
| `sharing` | [sharing.md](../guide/cli/sharing.md) | `ctl/sharing.go` | **done** |
| `port` | [port.md](../guide/cli/port.md) | `ctl/port.go` | **done** |
| `message-size` | [message-size.md](../guide/cli/message-size.md) | `appendlimit` / SMTP size | **done** — `__APPENDLIMIT__`, `__MAX_MESSAGE_SIZE__` |
| `language` | [language.md](../guide/cli/language.md) | `ctl/language.go` | **done** |
| `submission-access` | [submission-access.md](../guide/cli/submission-access.md) | `ctl/submission_access.go` | **planned** |
| `tasks` | [tasks.md](../guide/cli/tasks.md) | imapsql cleanup | **done** — see [21-scheduled-maintenance.md](21-scheduled-maintenance.md) |
| `queue` | [queue.md](../guide/cli/queue.md) | `ctl/queue.go` | **defer** |
| `exchanger` | [exchanger.md](../guide/cli/exchanger.md) | `ctl/exchanger.go` | **defer** |

### Services (DB toggles)

| Command | Guide | Settings keys | madmail-v2 |
|---------|-------|---------------|-------------|
| `push` | [push.md](../guide/cli/push.md) | `__PUSH_MODE__` | **done** — [23-push-notifications.md](23-push-notifications.md) |
| `webimap` | [webimap.md](../guide/cli/webimap.md) | `__WEBIMAP_ENABLED__` | **done** |
| `websmtp` | [websmtp.md](../guide/cli/websmtp.md) | `__WEBSMTP_ENABLED__` | **done** |
| `admin-web` | [admin-web.md](../guide/cli/admin-web.md) | `__ADMIN_WEB_*__` | **done** |

### Web / HTML

| Command | Guide | madmail-v2 |
|---------|-------|-------------|
| `html-export` | [html-export.md](../guide/cli/html-export.md) | **done** |
| `html-serve` | [html-serve.md](../guide/cli/html-serve.md) | **done** — sets `www_dir` in config |

### IMAP tooling

| Command | Guide | madmail-v2 |
|---------|-------|-------------|
| `imap-acct` | [imap-acct.md](../guide/cli/imap-acct.md) | **planned** (`prune-unused` → `tasks run prune-unused-accounts`) |
| `imap-mboxes` | [imap-mboxes.md](../guide/cli/imap-mboxes.md) | **planned** |
| `imap-msgs` | [imap-msgs.md](../guide/cli/imap-msgs.md) | **defer** |

### Hidden / dev

| Command | madmail-v2 |
|---------|-------------|
| `generate-man` | `docs.rs` (embedded `docs/man/madmail.1.scd`) |
| `generate-fish-completion` | `docs.rs` |
| `completion {bash,zsh,fish}` | `docs.rs` + [`completion.md`](../guide/cli/completion.md) |

---

## Tests

| Layer | Location | Run |
|-------|----------|-----|
| CLI parse | `chatmail-config::cli` tests | `cargo test -p chatmail-config` |
| Unit + dispatch | `chatmail::ctl::{accounts,ops_tests,...}` | `cargo test -p chatmail ctl` |
| E2E (subprocess) | `tests/ctl_cli_e2e.rs`, `tests/ctl_ops_e2e.rs` | `cargo test -p chatmail-integration --test ctl_cli_e2e --test ctl_ops_e2e` |

`CtlContext::open_pool` creates `chatmail.db` when missing so CLI works on a fresh `--state-dir` without a prior `run`.

---

## `internal/cli` file map (Madmail Go)

| Go file | Command(s) | Guide | Priority |
|---------|------------|-------|----------|
| `ctl/install.go` | `install` | [install.md](../guide/cli/install.md) | **done** |
| `ctl/uninstall.go` | `uninstall` | [uninstall.md](../guide/cli/uninstall.md) | **done** |
| `ctl/upgrade.go` | `upgrade`, `update` | [upgrade.md](../guide/cli/upgrade.md) | **done** |
| `ctl/admin_token.go` | `admin-token` | [admin-token.md](../guide/cli/admin-token.md) | **done** |
| `ctl/online.go` | `status` | [status.md](../guide/cli/status.md) | **done** |
| `ctl/reload_config.go` | `reload` | [reload.md](../guide/cli/reload.md) | **done** |
| `ctl/accounts_*.go` | `accounts`, `ban-list` | [accounts.md](../guide/cli/accounts.md) | **done** |
| `ctl/blocklist.go` | `blocklist` | [blocklist.md](../guide/cli/blocklist.md) | **done** |
| `ctl/federation.go` | `federation` | [federation.md](../guide/cli/federation.md) | **done** |
| `ctl/dnscache.go` | `endpoint-cache` | [endpoint-cache.md](../guide/cli/endpoint-cache.md) | **done** |
| `ctl/port.go` | `port` | [port.md](../guide/cli/port.md) | **done** |
| `ctl/language.go` | `language` | [language.md](../guide/cli/language.md) | **done** |
| `ctl/adminweb.go` | `admin-web` | [admin-web.md](../guide/cli/admin-web.md) | **done** |
| `ctl/html.go` | `html-export`, `html-serve` | [html-export.md](../guide/cli/html-export.md) | **done** |
| `ctl/users.go` | `creds` | [creds.md](../guide/cli/creds.md) | planned |
| `ctl/hash.go` | `hash` | [hash.md](../guide/cli/hash.md) | planned |
| `ctl/imapacct.go` | `imap-acct` | [imap-acct.md](../guide/cli/imap-acct.md) | planned |
| `ctl/queue.go` | `queue` | [queue.md](../guide/cli/queue.md) | defer |
| `ctl/exchanger.go` | `exchanger` | [exchanger.md](../guide/cli/exchanger.md) | defer |

---

## Deviations from Madmail

- Binary name **`chatmail`** in development; production installs use **`madmail`** (`cli.rs` `name = "madmail"`).
- **`--json`** on all ctl commands (see [`json-output.md`](../guide/cli/json-output.md)).
- **`upgrade` / `update`:** HTTP(S) download (100 MB cap), then signed replace.
- **`certificate autocert`:** writes `tls_mode autocert` + `acme_email` to config; optional immediate `get` ([`certificate-autocert-enable.md`](../guide/cli/certificate-autocert-enable.md)).
- **`federation dismiss`:** silent-dismiss cache (`chatmail-state::silent_dismiss`) — extra vs base Madmail CLI surface.

## Related RFCs

| RFC | Topic | Local file |
|-----|-------|------------|
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | `certificate get` / ACME | [rfc8555.txt](RFC/rfc8555.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS material | [rfc8446.txt](RFC/rfc8446.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP ctl (ports, queue) | [rfc5321.txt](RFC/rfc5321.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP ctl | [rfc3501.txt](RFC/rfc3501.txt) |

See also: [02-smtp-server.md](02-smtp-server.md), [03-imap-server.md](03-imap-server.md), [19-certificates.md](19-certificates.md).