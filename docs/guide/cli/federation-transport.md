# `madmail federation transport`

Parent: [`federation`](federation.md)

Allow or block outbound federation transports. With no subcommand, this shows the current mode.

Unset keys stay **allow**, which is the historical order: HTTPS `/mxdeliv`, then plain HTTP, then SMTP. Blocking a transport skips it. Blocking all three makes outbound delivery to that recipient fail immediately with `federation transport blocked by policy`.

`--domain` stores an override for one destination domain. A transport omitted from that override inherits the global mode. `inherit` clears one override. The override matches the recipient domain on the message.

A running server picks up CLI changes after `madmail reload`. `POST /admin/settings/federation` with a `transports` object applies in the same process. Add `"domain"` to that POST to write an override instead of the global mode.

## Synopsis

```bash
madmail federation transport [status [DOMAIN]]
madmail federation transport set <https|http|smtp> <allow|block> [--domain DOMAIN]
madmail federation transport set <https|http|smtp> inherit --domain DOMAIN
```

## Examples

```bash
madmail federation transport
madmail federation transport set http block
madmail federation transport set smtp block --domain evil.test
madmail federation transport status evil.test
madmail reload
```

Blocking `http` and `smtp` keeps HTTPS `/mxdeliv` and skips the plaintext fallbacks. The domain example blocks SMTP on port 25 toward `evil.test` and leaves every other destination on the global mode.

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
