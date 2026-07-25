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

### Linux

```bash
sudo madmail html-serve /var/lib/madmail/www
sudo madmail html-serve embedded
sudo madmail reload
# or: sudo systemctl restart madmail
```

### Windows

```powershell
$cfg = "$env:ProgramData\Madmail\config\madmail.conf"
$st  = "$env:ProgramData\Madmail\data"
$www = "$env:ProgramData\Madmail\www"

& "C:\Program Files\Madmail\madmail.exe" --config $cfg --state-dir $st html-serve $www
# Revert to built-in pages:
# & "C:\Program Files\Madmail\madmail.exe" --config $cfg --state-dir $st html-serve embedded
Restart-Service Madmail
```

After changing settings stored in the database, run `madmail reload` and/or restart the service to remount HTTP routes.

Operator walkthrough (export → edit → serve): [Customizing HTML pages](../../project/user-guide/17-customizing-html-pages.md).

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
