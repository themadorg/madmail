# `madmail sharing create`

Parent: [`sharing`](sharing.md)

Create a new share link (`SLUG URL [NAME]`)

## Synopsis

```bash
madmail sharing create [OPTIONS] <SLUG> <URL> [NAME]
```

## Examples

```bash
madmail sharing create alice https://example.org/a.vcf Alice
```

## JSON output (`--json`)

```bash
madmail sharing create --json
```

Success stdout:

```json
{"ok": true, "command": "sharing create", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-create).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
