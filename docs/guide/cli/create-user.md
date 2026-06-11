# `create-user`

Create a random account and print JSON credentials. Alias for [`madmail accounts create-random`](accounts-create-random.md).


## Synopsis

```bash
madmail create-user [--json-only]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Options

| Flag | Description |
|------|-------------|
| `--json-only` | Print only JSON (for scripts) |

## Output

JSON object with a `dclogin` field containing a Delta Chat login URI.

```bash
madmail create-user --json-only
```

## JSON output (`--json`)

```bash
madmail create user --json
```

Success stdout:

```json
{"ok": true, "command": "create user", "data": { ... }}
```

Schema: [json-output.md](json-output.md#create-user).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
