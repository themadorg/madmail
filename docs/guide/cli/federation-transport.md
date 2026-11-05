# `madmail federation transport`

Parent: [`federation`](federation.md)

Allow or block outbound federation transports. With no subcommand, this shows the current mode.

Unset keys stay **allow**, which is the historical order: HTTPS `/mxdeliv`, then plain HTTP, then SMTP. Blocking a transport skips it. Blocking all three makes outbound delivery to that recipient fail without dialing, with `delivery blocked by outbound transport policy: https, http and smtp are all blocked (recipient domain <domain>)`. A delivery blocked by this policy is a permanent failure: the queue drops the entry and does not retry it, so unblocking later does not bring it back. A local sender gets a `mailer-daemon` delivery report with the reason. With `smtp` blocked, a 4xx from `/mxdeliv` is permanent too. With `http` blocked, an HTTPS redirect to `http://` is refused.

`--domain` stores an override for one destination domain. A transport omitted from that override inherits the global mode. An override wins in both directions: `allow` re-enables a transport that is blocked globally, and `block` disables one that is allowed. `inherit` clears one override. The override matches the recipient domain on the message. If the stored overrides value is not a JSON object (for example after a bad hand edit), `set --domain` fails with an error and leaves it in place instead of replacing it; delete the `__FEDERATION_TRANSPORT_DOMAINS__` setting by hand first.

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
