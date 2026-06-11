# `madmail language reset`

Parent: [`language`](language.md)

Remove DB override (use config default)

## Synopsis

```bash
madmail language reset [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail language reset --json
```

Success stdout:

```json
{"ok": true, "command": "language reset", "data": { ... }}
```

Schema: [json-output.md](json-output.md#language-reset).


---
[← `language`](language.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/language.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/language.rs)
