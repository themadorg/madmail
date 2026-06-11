# `admin-web`

Control the embedded admin web dashboard (`__ADMIN_WEB_*__` settings).


## Synopsis

```bash
madmail admin-web <status|enable|disable|path>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show whether the admin web UI is enabled and its path |
| `enable` | Enable the admin web dashboard |
| `disable` | Disable it (HTTP 404) |
| `path [PATH]` | Set custom URL path (e.g. `/admin-secret`) |
| `path --reset` | Reset path to default `/admin` |

## Examples

```bash
madmail admin-web status
madmail admin-web enable
madmail admin-web path /admin-xyz123
madmail admin-web path --reset
madmail reload
```

After changing settings stored in the database, run:

```bash
madmail reload
```

to apply listener and HTTP route changes without a full process restart.


## Security

On public servers, change the default `/admin` path to reduce discovery risk.

## Subcommand pages

- [`disable`](admin-web-disable.md) — `madmail admin-web disable`
- [`enable`](admin-web-enable.md) — `madmail admin-web enable`
- [`path`](admin-web-path.md) — `madmail admin-web path`
- [`status`](admin-web-status.md) — `madmail admin-web status`

## JSON output (`--json`)

```bash
madmail admin web --json
```

Success stdout:

```json
{"ok": true, "command": "admin web", "data": { ... }}
```

Schema: [json-output.md](json-output.md#admin-web).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/admin_web.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/admin_web.rs)
