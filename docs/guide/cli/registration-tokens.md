# `registration-tokens`

Create and manage invite tokens for `/new` registration. Aliases: [`reg-tokens`](reg-tokens.md), [`tokens`](tokens.md).


## Synopsis

```bash
madmail registration-tokens <create|list|status|delete>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

### `create`

```bash
madmail registration-tokens create [--token TOKEN] [--max-uses N] [--comment TEXT] [--expires DURATION]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--token` | random | Custom token string |
| `--max-uses` | `1` | Maximum registrations per token |
| `--comment` | empty | Operator note |
| `--expires` | none | Duration e.g. `72h`, `168h` |

### `list`, `status <TOKEN>`, `delete <TOKEN>`

## Examples

```bash
madmail registration-tokens create --max-uses 5 --comment "Team Berlin" --expires 72h
madmail registration-tokens list
madmail registration-tokens status abc123
madmail registration-tokens delete abc123
```

## Subcommand pages

- [`create`](registration-tokens-create.md) — `madmail registration-tokens create`
- [`delete`](registration-tokens-delete.md) — `madmail registration-tokens delete`
- [`list`](registration-tokens-list.md) — `madmail registration-tokens list`
- [`status`](registration-tokens-status.md) — `madmail registration-tokens status`

## JSON output (`--json`)

```bash
madmail registration tokens --json
```

Success stdout:

```json
{"ok": true, "command": "registration tokens", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-tokens).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration_tokens.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration_tokens.rs)
