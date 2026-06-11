# `html-export`

Export the embedded default HTML (registration UI, static pages) to a directory for customization.


## Synopsis

```bash
madmail html-export <DEST_DIR>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Arguments

| Argument | Description |
|----------|-------------|
| `DEST_DIR` | Destination directory (created if needed) |

## Example

```bash
madmail html-export /opt/madmail-www-backup
```

Edit exported files, then point the server at them with [`html-serve`](html-serve.md).

## JSON output (`--json`)

```bash
madmail html export --json
```

Success stdout:

```json
{"ok": true, "command": "html export", "data": { ... }}
```

Schema: [json-output.md](json-output.md#html-export).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/html.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/html.rs)
