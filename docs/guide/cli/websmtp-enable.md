# `madmail websmtp enable`

Parent: [`websmtp`](websmtp.md)

Enable the API

## Synopsis

```bash
madmail websmtp enable [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail websmtp enable --json
```

Success stdout:

```json
{"ok": true, "command": "websmtp enable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#websmtp-enable).


---
[← `websmtp`](websmtp.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/service_toggle.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/service_toggle.rs)
