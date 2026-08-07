# `iroh`

Manage Iroh relay and IMAP METADATA discovery (`__IROH_*__`).


## Synopsis

```bash
madmail iroh [status|install|enable|disable]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show config, admin toggle, relay URL, port, local-only, listen probe (default) |
| `install` | Add Iroh to an existing config (`iroh_relay_url`, same as `install --enable-iroh`) and enable the admin toggle |
| `enable` | Set `__IROH_ENABLED__` = true (requires config) |
| `disable` | Set `__IROH_ENABLED__` = false |

## Examples

```bash
madmail iroh status
madmail iroh install
madmail iroh enable
madmail iroh disable
madmail port iroh set 3340
madmail reload
```

## Notes

Iroh is configured when the static config has `iroh_relay_url` (typically inside an `imap` block) or `iroh_enable`. Without that, `status` reports *not configured* and `enable` fails — use `iroh install` (or `madmail install --enable-iroh` on first install).

`install` writes the config file and sets `__IROH_ENABLED__`. Runtime active state is **configured AND admin enabled** (admin default is on when the setting is unset).

Listener port and local-only bind are managed with [`port iroh`](port-iroh.md), not this command.

After DB or config changes, run `madmail reload` so a running server reloads Iroh.

Same admin toggle as `/admin/services/iroh`.

**Windows:** releases do not ship `iroh-relay.exe`. `iroh status` and `iroh disable` work; `iroh install` and `iroh enable` refuse with an error so a Windows service is not configured for a missing relay binary.

## Subcommand pages

- [`status`](iroh-status.md) — `madmail iroh status`
- [`install`](iroh-install.md) — `madmail iroh install`
- [`enable`](iroh-enable.md) — `madmail iroh enable`
- [`disable`](iroh-disable.md) — `madmail iroh disable`

## JSON output (`--json`)

```bash
madmail iroh status --json
```

Success stdout:

```json
{"ok": true, "command": "iroh status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#iroh-status).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/iroh.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/iroh.rs)
