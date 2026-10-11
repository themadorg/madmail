# `madmail federation transport`

Parent: [`federation`](federation.md)

Allow or block outbound federation transports. With no subcommand, this shows the current mode.

Unset keys stay **allow**, which is the historical order: HTTPS `/mxdeliv`, then plain HTTP, then SMTP. Blocking a transport skips it. Blocking all three makes outbound delivery to that recipient fail immediately with `federation transport blocked by policy`.

A running server picks up CLI changes after `madmail reload`. `POST /admin/settings/federation` with a `transports` object applies in the same process.

## Synopsis

```bash
madmail federation transport [status]
madmail federation transport set <https|http|smtp> <allow|block>
```

## Examples

```bash
madmail federation transport
madmail federation transport set http block
madmail federation transport set smtp block
madmail reload
```

Blocking `http` and `smtp` keeps HTTPS `/mxdeliv` and skips the plaintext fallbacks.

## JSON output (`--json`)

```bash
madmail federation transport status --json
```

Success stdout:

```json
{"ok": true, "command": "federation transport status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-transport-status).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
