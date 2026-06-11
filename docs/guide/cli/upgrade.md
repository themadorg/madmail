# `upgrade`

Replace the running binary with a **signed** build from a local path or `http://` / `https://` URL.


## Synopsis

```bash
madmail upgrade <PATH_OR_URL>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Arguments

| Argument | Description |
|----------|-------------|
| `PATH_OR_URL` | Local path to signed binary, or URL to download one (max 100 MB) |

## How it works

1. Downloads the file if a URL is given (TLS verification is skipped for self-signed peers).
2. Verifies an **Ed25519 signature** in the last 64 bytes of the file.
3. Replaces the current executable and restarts the systemd service when applicable.

## Examples

```bash
madmail upgrade /tmp/madmail-signed
madmail upgrade https://relay.example/releases/madmail
```

## Notes

- Only binaries signed with the official release key are accepted.
- Requires appropriate permissions to replace `/usr/local/bin/madmail`.

## JSON output (`--json`)

```bash
madmail upgrade --json
```

Success stdout:

```json
{"ok": true, "command": "upgrade", "data": { ... }}
```

Schema: [json-output.md](json-output.md#upgrade).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/upgrade.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/upgrade.rs)
