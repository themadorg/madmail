# `madmail language set`

Parent: [`language`](language.md)

Set language code

## Synopsis

```bash
madmail language set [OPTIONS] <LANG>
```

## JSON output (`--json`)

```bash
madmail language set --json
```

Success stdout:

```json
{"ok": true, "command": "language set", "data": { ... }}
```

Schema: [json-output.md](json-output.md#language-set).


---
[← `language`](language.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/language.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/language.rs)
