# `madmail iroh disable`

Parent: [`iroh`](iroh.md)

Disable runtime Iroh (`__IROH_ENABLED__` = false).

## Synopsis

```bash
madmail iroh disable [OPTIONS]
```

## Notes

Does not remove `iroh_relay_url` from the config file. After changes, run `madmail reload`.

## JSON output (`--json`)

```bash
madmail iroh disable --json
```

Success stdout:

```json
{"ok": true, "command": "iroh disable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#iroh-enabledisable).


---
[← `iroh`](iroh.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/iroh.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/iroh.rs)
