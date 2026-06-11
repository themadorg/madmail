# `madmail language status`

Parent: [`language`](language.md)

Show current language (default subcommand)

## Synopsis

```bash
madmail language status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail language status --json
```

Success stdout:

```json
{"ok": true, "command": "language status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#language-status).


---
[← `language`](language.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/language.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/language.rs)
