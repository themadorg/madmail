# `update`

Alias for [`upgrade`](upgrade.md). Accepts the same signed local path or URL.


## Synopsis

```bash
madmail update <PATH_OR_URL>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Arguments

| Argument | Description |
|----------|-------------|
| `PATH_OR_URL` | Same as `upgrade` — local signed binary, or `http://` / `https://` URL to a raw binary or `.tar.gz` / `.tgz` archive |

## Examples

```bash
madmail update /tmp/madmail-signed
madmail update https://relay.example/releases/madmail
madmail update https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64.tar.gz
```

See [upgrade](upgrade.md) for full behavior (archive extract, signature verify, systemd stop/replace/start).

## JSON output (`--json`)

```bash
madmail update --json
```

Success stdout:

```json
{"ok": true, "command": "update", "data": { ... }}
```

Schema: [json-output.md](json-output.md#update).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/upgrade.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/upgrade.rs)
