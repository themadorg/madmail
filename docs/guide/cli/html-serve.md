# `html-serve`

Serve website HTML from an external directory instead of embedded defaults.


## Synopsis

```bash
madmail html-serve <WWW_DIR>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Arguments

| Argument | Description |
|----------|-------------|
| `WWW_DIR` | Path to HTML directory, or `embedded` to revert to built-in files |

## Example

```bash
madmail html-serve /opt/custom-www
madmail html-serve embedded
madmail reload
```

After changing settings stored in the database, run:

```bash
madmail reload
```

to apply listener and HTTP route changes without a full process restart.

## JSON output (`--json`)

```bash
madmail html serve --json
```

Success stdout:

```json
{"ok": true, "command": "html serve", "data": { ... }}
```

Schema: [json-output.md](json-output.md#html-serve).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/html.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/html.rs)
