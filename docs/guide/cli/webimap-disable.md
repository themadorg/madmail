# `madmail webimap disable`

Parent: [`webimap`](webimap.md)

Disable the API (HTTP 404)

## Synopsis

```bash
madmail webimap disable [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail webimap disable --json
```

Success stdout:

```json
{"ok": true, "command": "webimap disable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#webimap-disable).


---
[← `webimap`](webimap.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/service_toggle.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/service_toggle.rs)
