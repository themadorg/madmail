# `version`

Print the madmail crate version and exit.


## Synopsis

```bash
madmail version
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Output

Prints the product name and semantic version compiled into the binary (no server connection required).

Example:

```text
madmail-v2 2.4.0
```

## JSON output (`--json`)

```bash
madmail version --json
```

Success stdout:

```json
{"ok": true, "command": "version", "data": { ... }}
```

Schema: [json-output.md](json-output.md#version).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/version.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/version.rs)
