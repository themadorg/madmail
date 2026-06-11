# `ban-list`

Top-level alias for `madmail accounts ban-list` — lists all blocklisted usernames.


## Synopsis

```bash
madmail ban-list
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


Equivalent to:

```bash
madmail accounts ban-list
```

See [`accounts ban-list`](accounts-ban-list.md) for full details.

## JSON output (`--json`)

```bash
madmail ban list --json
```

Success stdout:

```json
{"ok": true, "command": "ban list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#ban-list).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
