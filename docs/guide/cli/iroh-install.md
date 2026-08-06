# `madmail iroh install`

Parent: [`iroh`](iroh.md)

Enable Iroh on an existing deployment without a full reinstall (same idea as `madmail install --enable-iroh`).

## Synopsis

```bash
madmail iroh install [OPTIONS]
```

## Notes

Writes `iroh_relay_url` into the config file when missing (inside an `imap` block for Madmail-style conf), and sets `__IROH_ENABLED__` to true. If the config already has Iroh, only the admin toggle is set.

Requires a real config file (`--config`). Afterward: `madmail reload`.

## JSON output (`--json`)

```bash
madmail iroh install --json
```

Success stdout:

```json
{"ok": true, "command": "iroh install", "data": { ... }}
```

Schema: [json-output.md](json-output.md#iroh-install).


---
[← `iroh`](iroh.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/iroh.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/iroh.rs)
