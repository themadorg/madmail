# `madmail websmtp status`

Parent: [`websmtp`](websmtp.md)

Show whether the API is enabled

## Synopsis

```bash
madmail websmtp status [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail websmtp status --json
```

Success stdout:

```json
{"ok": true, "command": "websmtp status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#websmtp-status).


---
[← `websmtp`](websmtp.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/service_toggle.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/service_toggle.rs)
