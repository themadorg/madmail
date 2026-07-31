# `openrelay`

Allow or deny non-local `RCPT TO` on **unauthenticated inbound SMTP** (port 25 open-relay-class).

Default is **denied**. Setting maps to:

- DB: `__ALLOW_INBOUND_REMOTE_RCPT__`
- Config file: `allow_inbound_remote_rcpt` (used when the DB key is unset)

Authenticated submission (587/465) is **not** affected and always may use remote RCPT.


## Synopsis

```bash
madmail openrelay <status|enable|disable>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show effective allow/deny, file default, and DB override |
| `enable` | Allow non-local RCPT on inbound SMTP (lab/special only) |
| `disable` | Deny non-local RCPT on inbound SMTP (default-safe) |

```bash
madmail openrelay status
madmail openrelay enable
madmail reload
madmail openrelay disable
madmail reload
```

After `enable` / `disable`, run:

```bash
madmail reload
```

so a running server re-hydrates the live inbound-remote-RCPT flag (soft reload). The admin API toggle updates the live flag immediately; the CLI writes the DB setting only.

## Subcommand pages

- [`disable`](openrelay-disable.md) — `madmail openrelay disable`
- [`enable`](openrelay-enable.md) — `madmail openrelay enable`
- [`status`](openrelay-status.md) — `madmail openrelay status`

## JSON output (`--json`)

```bash
madmail openrelay status --json
```

Success stdout:

```json
{"ok": true, "command": "openrelay status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#openrelay-status).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/openrelay.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/openrelay.rs)
