# `madmail federation policy`

Parent: [`federation`](federation.md)

Set global federation posture (`accept` or `reject`)

## Synopsis

```bash
madmail federation policy [OPTIONS] <accept|reject>
```

## Examples

```bash
madmail federation policy accept
```

## JSON output (`--json`)

```bash
madmail federation policy --json
```

Success stdout:

```json
{"ok": true, "command": "federation policy", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-policy).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
