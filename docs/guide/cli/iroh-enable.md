# `madmail iroh enable`

Parent: [`iroh`](iroh.md)

Enable runtime Iroh (`__IROH_ENABLED__` = true).

## Synopsis

```bash
madmail iroh enable [OPTIONS]
```

## Notes

Requires Iroh in the static config (`iroh_relay_url` / `iroh_enable`). If not configured, use [`iroh install`](iroh-install.md) first.

After changes, run `madmail reload`.

## JSON output (`--json`)

```bash
madmail iroh enable --json
```

Success stdout:

```json
{"ok": true, "command": "iroh enable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#iroh-enabledisable).


---
[← `iroh`](iroh.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/iroh.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/iroh.rs)
