# `language`

View or change the website/UI language (`__LANGUAGE__` in settings DB).


## Synopsis

```bash
madmail language [status|set|reset]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show current language (default when no subcommand given) |
| `set <LANG>` | Set language code |
| `reset` | Remove DB override (use config default) |

## Supported codes

`en` (English), `fa` (Persian), `ru` (Russian), `es` (Spanish)

## Examples

```bash
madmail language
madmail language set fa
madmail language reset
```

## Subcommand pages

- [`reset`](language-reset.md) — `madmail language reset`
- [`set`](language-set.md) — `madmail language set`
- [`status`](language-status.md) — `madmail language status`

## JSON output (`--json`)

```bash
madmail language --json
```

Success stdout:

```json
{"ok": true, "command": "language", "data": { ... }}
```

Schema: [json-output.md](json-output.md#language).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/language.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/language.rs)
