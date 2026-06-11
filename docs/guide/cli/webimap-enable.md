# `madmail webimap enable`

Parent: [`webimap`](webimap.md)

Enable the API

## Synopsis

```bash
madmail webimap enable [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail webimap enable --json
```

Success stdout:

```json
{"ok": true, "command": "webimap enable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#webimap-enable).


---
[← `webimap`](webimap.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/service_toggle.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/service_toggle.rs)
