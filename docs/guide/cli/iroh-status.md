# `madmail iroh status`

Parent: [`iroh`](iroh.md)

Show Iroh configuration, admin toggle, relay URL, port, and a localhost listen probe.

## Synopsis

```bash
madmail iroh status [OPTIONS]
```

Omitting a subcommand is the same as `status`.

## Notes

`runtime_enabled` is true only when the config has Iroh **and** `__IROH_ENABLED__` is not false (default on). `listening` is a best-effort TCP probe to the effective port on loopback.

## JSON output (`--json`)

```bash
madmail iroh status --json
```

Success stdout:

```json
{"ok": true, "command": "iroh status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#iroh-status).


---
[← `iroh`](iroh.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/iroh.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/iroh.rs)
