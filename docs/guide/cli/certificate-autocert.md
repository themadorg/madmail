# `madmail certificate autocert`

Parent: [`certificate`](certificate.md)

Enable or inspect in-process Let's Encrypt auto-renewal

## Synopsis

```bash
madmail certificate autocert [OPTIONS] <COMMAND>
```

## JSON output (`--json`)

```bash
madmail certificate autocert --json
```

Success stdout:

```json
{"ok": true, "command": "certificate autocert", "data": { ... }}
```

Schema: [json-output.md](json-output.md#certificate-autocert).


---
[← `certificate`](certificate.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/certificate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/certificate.rs)
