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
| Federation & routing | [`federation.md`](../guide/cli/federation.md) · [`dkim.md`](../guide/cli/dkim.md) · [`endpoint-cache.md`](../guide/cli/endpoint-cache.md) |
| Services & ports | [`port.md`](../guide/cli/port.md) · [`proxy.md`](../guide/cli/proxy.md) · [`iroh.md`](../guide/cli/iroh.md) · [`push.md`](../guide/cli/push.md) · [`webimap.md`](../guide/cli/webimap.md) · [`websmtp.md`](../guide/cli/websmtp.md) |
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
| `monitor` | [monitor.md](../guide/cli/monitor.md) | `monitor.rs` | **done** |
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
| `openrelay` | [openrelay.md](../guide/cli/openrelay.md) | `openrelay.rs` | **done** (`status` / `enable` / `disable`; `__ALLOW_INBOUND_REMOTE_RCPT__`) |
| `federation` | [federation.md](../guide/cli/federation.md) | `federation.rs` | **done** (+ `dismiss`, `undismiss`, `dismiss-list`, `dismiss-flush`) |
| `dkim` | [dkim.md](../guide/cli/dkim.md) | `dkim.rs` | **done** (`show` / `check` / `status`; `{state}/dkim/default.{private,txt}`) |
| `endpoint-cache` / `dns-cache` | [endpoint-cache.md](../guide/cli/endpoint-cache.md) | `endpoint_cache.rs` | **done** |
| `sharing` | [sharing.md](../guide/cli/sharing.md) | `sharing.rs` | **done** |
| `port` | [port.md](../guide/cli/port.md) | `port.rs` | **done** |
| `message-size` | [message-size.md](../guide/cli/message-size.md) | `message_size.rs` | **done** |
| `language` | [language.md](../guide/cli/language.md) | `language.rs` | **done** |
| `push` | [push.md](../guide/cli/push.md) | `push.rs` | **done** |
| `proxy` / `pr` | [proxy.md](../guide/cli/proxy.md) | `proxy.rs` | **done** |
| `iroh` | [iroh.md](../guide/cli/iroh.md) | `iroh.rs` | **done** (`status` / `install` / `enable` / `disable`; `__IROH_ENABLED__`) |
| `db` | [db.md](../guide/cli/db.md) | `db.rs` | **done** (`sqlite-to-postgres` — copy application SQLite tables into Postgres) |
| `webimap` | [webimap.md](../guide/cli/webimap.md) | `service_toggle.rs` | **done** |
| `websmtp` | [websmtp.md](../guide/cli/websmtp.md) | `service_toggle.rs` | **done** |
| `tasks` | [tasks.md](../guide/cli/tasks.md) | `tasks.rs` | **done** |
| `html-export` | [html-export.md](../guide/cli/html-export.md) | `html.rs` | **done** |
| `html-serve` | [html-serve.md](../guide/cli/html-serve.md) | `html.rs` | **done** |
| `creds` | [creds.md](../guide/cli/creds.md) | — | **planned** |
| `hash` | [hash.md](../guide/cli/hash.md) | — | **planned** |
| `submission-access` | [submission-access.md](../guide/cli/submission-access.md) | — | **planned** |
| `queue` | [queue.md](../guide/cli/queue.md) | `queue_cmd.rs` | **done** — outbound `remote_queue` status/list/show/remove/purge |
| `exchanger` | [exchanger.md](../guide/cli/exchanger.md) | — | **defer** |
| `imap-acct` | [imap-acct.md](../guide/cli/imap-acct.md) | — | **planned** |
| `imap-mboxes` | [imap-mboxes.md](../guide/cli/imap-mboxes.md) | — | **planned** |
| `imap-msgs` | [imap-msgs.md](../guide/cli/imap-msgs.md) | — | **defer** |
| `migrate-pgp-config` | [migrate-pgp-config.md](../guide/cli/migrate-pgp-config.md) | — | **planned** |

`dispatch.rs` `not_implemented` list (parsed but no handler): `creds`, `hash`, `submission-access`, `exchanger`, `imap-acct`, `imap-mboxes`, `imap-msgs`, `migrate-pgp-config`.

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
| `monitor` | [monitor.md](../guide/cli/monitor.md) | — (v2; #19) | **done** (`--interval`, `--count`, `--addr`; `status`, `enable`, `disable`) |
| `certificate` | [certificate.md](../guide/cli/certificate.md) | — (instant-acme) | **done** — `get`, `regenerate`, `status`, [`autocert`](../guide/cli/certificate-autocert.md) |
| `db` | [db.md](../guide/cli/db.md) | — | **done** — `sqlite-to-postgres` |

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
| `openrelay` | [openrelay.md](../guide/cli/openrelay.md) | — (v2; inbound anti open-relay) | **done** — `__ALLOW_INBOUND_REMOTE_RCPT__` / `allow_inbound_remote_rcpt` |
| `federation` | [federation.md](../guide/cli/federation.md) | `ctl/federation.go` | **done** — includes silent dismiss (`chatmail-state::silent_dismiss`) |
| `dkim` | [dkim.md](../guide/cli/dkim.md) | — (v2; outbound federation DKIM) | **done** — `show` / `check` / `status` |
| `endpoint-cache` | [endpoint-cache.md](../guide/cli/endpoint-cache.md) | `ctl/dnscache.go` | **done** |
| `sharing` | [sharing.md](../guide/cli/sharing.md) | `ctl/sharing.go` | **done** |
| `port` | [port.md](../guide/cli/port.md) | `ctl/port.go` | **done** |
| `message-size` | [message-size.md](../guide/cli/message-size.md) | `appendlimit` / SMTP size | **done** — `__APPENDLIMIT__`, `__MAX_MESSAGE_SIZE__` |
| `language` | [language.md](../guide/cli/language.md) | `ctl/language.go` | **done** |
| `submission-access` | [submission-access.md](../guide/cli/submission-access.md) | `ctl/submission_access.go` | **planned** |
| `tasks` | [tasks.md](../guide/cli/tasks.md) | imapsql cleanup | **done** — see [21-scheduled-maintenance.md](21-scheduled-maintenance.md) |
| `queue` | [queue.md](../guide/cli/queue.md) | `ctl/queue.go` | **done** — `queue_cmd.rs`; disk store `chatmail-delivery::QueueStore` |
| `exchanger` | [exchanger.md](../guide/cli/exchanger.md) | `ctl/exchanger.go` | **defer** |

### Services (DB toggles)

| Command | Guide | Settings keys | madmail-v2 |
|---------|-------|---------------|-------------|
| `push` | [push.md](../guide/cli/push.md) | `__PUSH_MODE__` | **done** — [23-push-notifications.md](23-push-notifications.md) |
| `proxy` | [proxy.md](../guide/cli/proxy.md) | `__SS_ENABLED__`, `__SS_CIPHER__`, `__SS_PASSWORD__` | **done** — [11-proxy-services.md](11-proxy-services.md) |
| `iroh` | [iroh.md](../guide/cli/iroh.md) | `__IROH_ENABLED__` (+ config `iroh_relay_url`; port via `port iroh` / `__IROH_PORT__`) | **done** — [11-proxy-services.md](11-proxy-services.md) |
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
| `ctl/queue.go` | `queue` | [queue.md](../guide/cli/queue.md) | **done** (`queue_cmd.rs`) |
| `ctl/exchanger.go` | `exchanger` | [exchanger.md](../guide/cli/exchanger.md) | defer |

---

## Deviations from Madmail

- Binary name **`chatmail`** in development; production installs use **`madmail`** (`cli.rs` `name = "madmail"`).
- **`--json`** on all ctl commands (see [`json-output.md`](../guide/cli/json-output.md)).
- **`upgrade` / `update`:** HTTP(S) download (100 MB cap); `.tar.gz` / `.tgz` URLs extract the binary first, then signed replace.
- **`certificate autocert`:** writes `tls_mode autocert` + `acme_email` to config; optional immediate `get` ([`certificate-autocert-enable.md`](../guide/cli/certificate-autocert-enable.md)).
- **`federation dismiss`:** silent-dismiss cache (`chatmail-state::silent_dismiss`) — extra vs base Madmail CLI surface.
- **`dkim show` / `dkim check` / `dkim status`:** print / generate outbound federation DKIM (`default._domainkey`), verify the published TXT, or summarize without creating a key — extra vs base Madmail CLI surface.

## Related RFCs

| RFC | Topic | Local file |
|-----|-------|------------|
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | `certificate get` / ACME | [rfc8555.txt](RFC/rfc8555.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS material | [rfc8446.txt](RFC/rfc8446.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP ctl (ports, queue) | [rfc5321.txt](RFC/rfc5321.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP ctl | [rfc3501.txt](RFC/rfc3501.txt) |

See also: [02-smtp-server.md](02-smtp-server.md), [03-imap-server.md](03-imap-server.md), [19-certificates.md](19-certificates.md).
## Operator webhook configuration

Operator webhooks are configured through the Services dashboard or authenticated
`/admin/services/webhooks` RPC, rather than a dedicated CLI command.
`madmail admin-web --help` describes dashboard access; `madmail admin-token`
supplies API credentials. See [the operator guide](../guide/operator-webhooks.md)
for the complete API and delivery contract. The maintained man page source also
documents this workflow.

## Explicit Unix installation mode (issue #216)

Directory choice and installation intent are independent. `install --system`
requests root-only service-account creation, ownership setup, binary installation,
and systemd unit installation even with custom paths. `--local` requests only local
configuration/certificate/database bootstrap; no host docs, binary, account or unit
changes. Flags conflict. Without either flag, preserve the legacy FHS/default-path
heuristic and explain local selection with a hint to use `--system`.

`--skip-user` and `--skip-systemd` remain independent opt-outs within system mode.
Explicit Unix system mode is rejected on Windows (use Windows service flags).
System paths must be absolute. Custom systemd units retain RuntimeDirectory while
avoiding StateDirectory/ConfigurationDirectory aliases for custom persistent paths.
Dry-run lists planned actions without creating files, accounts or invoking systemctl.
Before its first host change, a system install that writes a unit refuses a
symlinked or masked unit path. For an existing account whose home differs from the
state directory, the home is moved with `usermod -d -m` when the state directory does
not exist yet. When it exists, the full install repoints the home without `-m` only if
the previous home is gone; otherwise it refuses before `usermod`, so data in the old
home is not left behind while the install continues with an empty state directory
(`--user-only` adopts an existing state directory on purpose). Units are written
with mode 0644 regardless of the process umask.
Tests cover CLI conflicts, default/auto/explicit modes, custom unit paths, dry-run
side-effect absence, root enforcement and isolated installation/idempotence.

### Isolated Unix installation repair

`install --user-only` and `install --systemd-only` are mutually exclusive,
non-interactive system operations for existing installations. They imply `--system`
and bypass domain resolution, TLS setup, configuration rendering, database seeding,
binary copying, and documentation installation. Local, certificate-only, skip,
Windows service and firewall options conflict with these repair modes.

User-only creates or aligns the service account and applies existing config/TLS/state
ownership using installer rules. Existing config and state directories are required;
content is preserved, but ownership and config mode may change. An existing account
with another home is repointed with `usermod -d` and no `-m` when the state directory
exists (`usermod -m` refuses an existing target); `-m` is used only when it does not.
Root works next to a service account that owns the state directory and may be
compromised: while root runs, it can create, rename (including
`renameat2(RENAME_EXCHANGE)` loops) and link anything in that tree. During a
system install (also with `--skip-user` when the account exists) or a
`--user-only` repair, every root write into the state directory, and every write
or ownership or mode change of the config file, the TLS files and their
directories and the binary, therefore goes through file descriptors
(`ctl/secure_fs.rs`), never through a path the service account can change. The
systemd unit, the man page and shell completions are written by path into
root-owned system directories. The parent directories of the state directory are
created before the account step with the checked resolver below (`mkdirat` in
trusted directories, the parent itself checked too), so `useradd` and `usermod`
never run when a component is refused:

- Paths are resolved by a checked resolver. It opens `/` and then each component
  relative to the previous directory with `O_NOFOLLOW`; a symbolic link is read
  with `readlinkat` and resolution goes on from its target, at most 40 times.
  Every directory and link on the way is checked with `fstat` /
  `fstatat(AT_SYMLINK_NOFOLLOW)` and refused when it is owned by the service uid
  (or by the previous owner of the state directory), is a directory writable by
  a group of the service account (primary or supplementary, from `id -G`), or is
  world-writable without the sticky bit. Uid 0 and gid 0 never count as the
  service. Only mode bits are read, so a POSIX ACL entry for the service account
  is caught only when the directory's group is one of its groups. Because every directory passed through is trusted, an
  entry cannot be swapped between its check and the next `openat`.
- The state directory is opened once with this resolver (an operator link to a
  data volume works) and recognized by device and inode. When resolving any other
  path reaches it, the resolver stops and returns the rest of the path. Inside the
  tree, every component is opened with `O_NOFOLLOW`.
- Files are written by create-new-then-rename: a temporary name opened with
  `O_CREAT | O_EXCL | O_NOFOLLOW`, then `fchmod`, `fchown`, write, `fsync` and
  `renameat`. A planted link or hard link at the target is replaced, not written
  through. This covers the config file, the DKIM key and TXT record, self-signed
  and ACME certificates and keys, the ACME account file and the binary (written
  `root:root` mode 0755 into a trusted directory). A file written inside the
  state tree gets its owner on the new descriptor before the rename: the service
  account, or with `--skip-user` the service account when the file it replaces
  was a regular file of that account, and root otherwise. The ownership walk
  therefore never needs to know which files install wrote. The directories
  install creates are made with `mkdirat` in trusted directories.
- A file inside the tree is read as root (the DKIM private key, the ACME account
  file) only when it is a regular file with one link that belongs to the service
  uid or the previous service uid (with `--skip-user` also root); otherwise
  install refuses to read it and prints the `chown` command to run after
  checking it. On Linux the entry is opened with `O_PATH | O_NOFOLLOW`, checked
  with `fstat`, and only a regular file is reopened for reading through
  `/proc/self/fd`, so a device node placed there is never opened. On other
  systems it is checked with `fstatat` and opened with
  `O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_NOCTTY`; the type is checked again on
  the descriptor.
- `--binary-path` inside `--state-dir`, or whose directory resolves into the
  state directory or has a refused component, stops install before it changes
  anything.
- The ACME client writes into a private staging directory (root, mode 0700, in a
  root-owned or sticky temporary directory). Install copies its results into place
  with the writes above.
- The install language is written to the SQLite database with the effective uid
  and gid of the service account (`setegid` / `seteuid`, supplementary groups
  reduced to the service group), after the ownership walk, whenever the state
  directory is controlled by the service account under the trust rule above; any
  other state directory (for example a new root-owned one with `--skip-user`) is
  used as root. Only the effective ids
  change; the saved uid stays 0 (see `12-security.md`).
- The config file is changed through its descriptor after the ownership walk:
  outside the state tree it ends `root:<group>` with mode 0640. A config path with
  a refused component, or a config file with more than one hard link (also one
  the service account made during the walk), stops install with an error. Inside
  the tree install writes it owned by the service account and sets mode 0640; an
  in-tree config file that belongs to another account is refused, and so is a
  config path outside `--state-dir` that resolves into the tree.
- A TLS file outside the tree is given to the service account through its
  descriptor. A TLS path with a refused component is not changed; the output
  notes `Skipping ownership of <path>`. A TLS path that reaches the tree is not
  changed by path: a file there that belongs to the service uid or the previous
  service uid was handled by the walk (install writes its own TLS files there
  with that owner). For a link there, or a file of another account (for example a
  root-owned certificate the operator copied in), the output notes
  `Skipping ownership of <path>` with the reason and, for a file, the `chown`
  command to run after checking it. A TLS file whose final object is owned by the
  service uid is accepted, so repair and reinstall keep working. Outside the tree a TLS file with more than one hard link
  is changed only when it belongs to the service uid or the previous service uid;
  its directory is trusted, so the service account cannot have placed it there.
- The config directory and the certs directory are re-owned to `root:<group>`,
  and their group and other write bits are cleared, only when they are dedicated
  to madmail (named `madmail`, `chatmail`, either followed by `-` or `_` and a
  suffix, or the binary name, and not a well-known system directory such as `/etc`
  or `/usr/local/etc`; a `certs` directory directly inside such a directory also
  counts). Both the path as given and the physical path the resolver walked must
  pass, and the opened directory must not be a well-known directory by device and
  inode, so a madmail-named link to `/etc/ssl/private` or `/etc` is not followed
  into a change. Any other directory keeps its owner and mode.

The state tree is chowned by a descriptor walk (`ctl/install/chown_walk.rs`). The
owner of the state directory before the walk is recorded as the previous service
uid, unless `id -nu` names another account for that uid; the trust rule above
counts the previous owner as untrusted in both cases. Each directory is opened relative to its parent with
`O_NOFOLLOW | O_DIRECTORY`, checked with `fstat` and changed with `fchown` when it
belongs to the service uid, the previous service uid or root; a directory of
another account is reported and not descended into. The state directory itself
is changed last, so a run that stops half way can be repeated with the same
previous uid. Every other entry is opened
by itself and checked with `fstat` on that descriptor before it is changed: on
Linux with `openat(O_PATH | O_NOFOLLOW)` and `fchownat(fd, "", AT_EMPTY_PATH)`; on
other systems regular files are opened with
`O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_NOCTTY` and changed with `fchown`, and other
non-directory entries are left unchanged. Links are skipped. Any other
non-directory is changed only when it belongs to the service uid or the previous
service uid, whatever its link count: a hard link to another account's file (for
example `/etc/shadow`, or the config file root just wrote, when
`fs.protected_hardlinks` is 0) becomes the inode's only link once the original
name is replaced by a rename. The mail store hard-links every message into the
maildirs, and those files belong to the service. Skipped entries are listed as
`Ownership of <path> left unchanged: <reason>` (the first 50, then a count), each
file of another account followed by the `chown <user>:<group> <path>` command; a
file a root command created in the tree is among them, and the operator runs the
command after checking that it is madmail's own file.
An entry that is replaced or removed during the walk (`ELOOP`, `ENOTDIR`,
`ENOENT`) is skipped with a warning. The walk is iterative and stops descending
after 64 levels, each open level holding one descriptor.
TLS ownership is applied only to the given `--cert-path`/`--key-path` files, so
custom locations must be repeated.

`--skip-user` skips only the account step and the ownership changes (config,
TLS, the walk); when the account exists every write above still goes through
the descriptor handle. Outside the tree a rewritten config file keeps the
previous file's owner; inside it, a rewritten file is the service account's when
the file it replaces was, and root's otherwise. With `--skip-user` and no account, files are written by path as root and
the config file is written with mode 0640 and `O_NOFOLLOW`; that is refused when
the state directory exists and is not owned by root. A local install writes by
path as the calling user.
A passwd
home that resolves to the same directory as `--state-dir` counts as equal to it. Systemd-only requires an existing config
file, state directory, executable binary and service account; it writes the unit
through a temporary file and `rename`, refuses a symlinked or masked unit path, and
runs daemon-reload, without enabling or starting it. The cert/key paths also feed the
unit's `ProtectHome=` decision. Existing unit customizations are replaced, so
operators should use systemd drop-ins.

Paths remain explicit installer inputs, not values inferred from config contents;
use the same binary name and paths as the original install. Dry runs need no root
and report the selected operation without mutation. Live prerequisites are checked
before provisioning; failures return a nonzero exit status. This is a deliberate
extension to the full installer, preserving its default behavior.

### Uninstall path safety

`uninstall` collects config and state directories from every source: the state
directory in use, `/etc/madmail*` and `/etc/chatmail*`, `/var/lib/madmail*` and
`/var/lib/chatmail*`, and `User=`, `WorkingDirectory=` and the `ExecStart=` binary
and `--config` path of every `madmail*`/`chatmail*` unit. The state directory in
use is `state_dir` from the config file when the config file sets it, and
`--state-dir` otherwise. `state_dir` from the config file is used only when the
file and every directory and link on its path pass `Trust::root_or_caller` and
the file has one hard link. That trust needs root as owner, refuses a file
group-writable by a group other than root, and refuses a directory writable by
a group of the service, writable by any other group except root without the
sticky bit, or world-writable without the sticky bit. `/var/log` and `/var/mail`,
matched by `(dev, ino)`, may be group-writable by a group that is not a group of
the service. The groups of the service are the primary and supplementary groups
of every account found (the `User=` accounts of the units and the accounts named
like the binary, `madmail` or `chatmail`; read with `id -G`, and when an account
exists but its groups cannot be read, the config file is not used), the group of
the config file's directory and the groups of the state directories (from the
config file and from `--state-dir`). Otherwise uninstall uses
`--state-dir` and lists the ignored value as left in place, unless another source
plans that directory for removal.

The same rule applies to every directory, whatever found it. A directory is
removed with all its contents only when its basename is a family name
(`madmail`, `chatmail`, or either followed by `-` or `_` and a suffix) or the
running binary's name, and it is not denied. From any other config directory only
`<binary>.conf` is removed, and TLS files in its `certs/` subdirectory are reported
but kept, since nothing records whether install generated them. Any other state
directory is kept, also when it is given with `--state-dir`. Well-known system
directories (`/`, `/etc`, `/usr`, `/usr/local`, `/usr/local/etc`, `/opt`, `/srv`,
`/var`, `/var/lib`, `/home`, `/root`, `/tmp` and others), every other direct child
of `/` (`system_dir_reason`, shared with install), every `/home/<name>`, and
the homes of root, regular accounts (uid 1000 or more except `nobody`), `$HOME` and
`$SUDO_USER` are denied, checked both lexically and after resolution.

A directory is also not removed as a whole when it is or contains a path that
stays: a kept path, a state directory under `--keep-data`, or a config directory
or file under `--keep-config`. Containment is checked on the paths as written and
after resolving links in both (the longest existing prefix canonicalized), so a
config directory that is a link into the state directory counts as inside it; a
path that cannot be resolved counts as contained. Such a config directory loses
only its config file; such a state, log, cache or runtime directory is kept.
After that, `prune_config_in_state_dirs` drops every config directory and file
that is inside a state directory both as written and after resolution: it goes
with the state directory, and removing it by path would pass through the state
directory, which the service account owns. Under `--keep-data` such a path is
listed as kept (`inside state directory <path>, which --keep-data keeps`).
Before each removal the parent of the path is resolved with the checked resolver
under the same rule (Ubuntu's `/var/log`, root:syslog 0775, passes through the
`(dev, ino)` allowance above); a path with a component that fails it is not
removed: `remove_path` returns it as a `KeptKind::Refused` entry, the output
prints `⚠️  Left in place: <path> (<component> is <reason>; check it and remove the path yourself)`,
and the run goes on. Refused entries are shown at the end and in `left_in_place`
whatever the keep flags. The last
component is never followed: a link is removed itself (the output says
`Removed link <path> (its target <target> was left in place)`, the target read
with `readlinkat`), a file is unlinked through the parent's descriptor, and a
directory is removed by `secure_fs::remove_dir_tree`: it opens each directory
with `openat(O_NOFOLLOW | O_DIRECTORY)` relative to its parent, checks that the
first one is the `(dev, ino)` just looked at, unlinks entries with `unlinkat`,
re-lists a directory up to 8 times when entries keep appearing, and stops at a
depth of 256. `/run`, `/var/log` and `/var/cache` entries are
derived only from family names, and accounts are deleted with `userdel` without
`-r`. The plan and, on Unix, the JSON result (`left_in_place`) list every path left
in place with its reason.
