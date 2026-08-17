# Federation bugs: IP address normalize + endpoint-cache host:port

**Status:** fixed on branch `fix/federation-ip-normalize-endpoint-port`  
**Affects:** IP-literal chatmail domains (`user@[1.2.3.4]`), madexchanger / endpoint-cache relays on non-443 ports  
**Crates:** `chatmail-fed`, `chatmail-state`, `chatmail-delivery`

## Summary

Two independent bugs broke (or silently dropped) federation between IP-based Madmail servers and/or when routing via an intermediate **madexchanger** on a custom port:

1. **Inbound `/mxdeliv` did not normalize addresses** before account lookup and maildir delivery.
2. **Outbound `endpoint-cache` treated `host:port` as IPv6**, producing invalid rewrite targets.

Both returned outcomes that looked “healthy” (HTTP 200 from peers / successful relay counters) while mail never reached the correct mailbox or never used the intended HTTP path.

---

## Bug 1 — Bare IP vs bracketed IP on inbound `/mxdeliv`

### Symptom

- Peer (or madexchanger) POSTs `/mxdeliv` and receives **HTTP 200 OK**.
- Recipient maildir stays empty.
- Logs (when enabled) may show: `mxdeliv: silently dropped (no account or reserved rcpt)`.

### Root cause

Accounts on IP-mode installs are registered as RFC 5321 address-literals:

```text
alice@[203.0.113.50]
```

Auth cache / `passwords.username` / maildir paths use that **exact** string.

Inbound federation only did an **exact** `user_exists(rcpt)` check on the raw `X-Mail-To` header. Clients, relays, or peers may send either:

| Form | Typical source |
|------|----------------|
| `alice@[203.0.113.50]` | Delta Chat / registration canonical form |
| `alice@203.0.113.50` | Some relays, manual tests, non-normalized peers |

`alice@203.0.113.50` failed the exact match → recipient list emptied → handler returned **`Ok(())` → HTTP 200** (anti-enumeration style silent drop). The remote server and madexchanger both treat this as successful delivery.

### Fix

1. **`chatmail-fed` / `mxdeliv.rs`**  
   Normalize every `X-Mail-From` / `X-Mail-To` with case-fold localpart + `wrap_ip_domain()` before domain checks, account lookup, and local delivery so storage paths match registration.

2. **`chatmail-state` / `auth.rs` — `local_recipient_allowed`**  
   Also try the IP-literal-normalized form if the raw key is missing (defense in depth for other callers).

### Operator note

Prefer documenting contacts as `user@[ip]` for IP servers. After this fix, bare `user@ip` is accepted and stored under the bracketed mailbox.

---

## Bug 2 — `endpoint-cache` `host:port` rewritten as fake IPv6

### Symptom

- Operator sets:  
  `madmail endpoint-cache set peer.example.org 127.0.0.1:19080`
- Outbound federation does **not** hit the madexchanger (or hits a nonsense URL).
- Traffic falls through to SMTP / direct IP and fails when the direct path is firewalled (isolation / NAT tests).
- Federation stats may show only **Failed (SMTP)** for the peer domain.

### Root cause

`mxdeliv_host_for_url()` did:

1. Strip brackets.
2. If not a pure IPv4 literal and the string **contains `:`**, wrap as IPv6: `[…]`.

So `127.0.0.1:19080` became **`[127.0.0.1:19080]`**, then:

```text
https://[127.0.0.1:19080]/mxdeliv
```

which is invalid for IPv4+port. HTTPS/HTTP to the exchanger failed; SMTP fallback targeted the wrong host.

When `TARGET_HOST` is a full URL (`http://127.0.0.1:19080/mxdeliv`), the `MxdelivUrl` path already worked — only the bare `host:port` form was broken.

### Fix

**`chatmail-delivery` / `transport.rs` — `mxdeliv_host_for_url`**

- Detect **IPv4 + numeric port** (`a.b.c.d:port`) and keep it as-is for URL construction (`https://127.0.0.1:19080/mxdeliv`).
- Still wrap bare IPv6 with brackets.
- Unit test: `ipv4_with_port_not_wrapped_as_ipv6`.

### Operator recommendation

Prefer full URL overrides for relays:

```bash
madmail endpoint-cache set peer.example.org "http://127.0.0.1:19080/mxdeliv" "via madexchanger tunnel"
madmail endpoint-cache set 203.0.113.50 "http://127.0.0.1:19080/mxdeliv"
madmail endpoint-cache set "[203.0.113.50]" "http://127.0.0.1:19080/mxdeliv"
```

After this fix, `127.0.0.1:19080` (without scheme) is also valid as a Host target.

---

## How we validated (field)

Setup: internal DNS chatmail (`delta.example`) ↔ external IP chatmail (`[203.0.113.50]`) with **madexchanger** on a third host, reverse SSH tunnels to `127.0.0.1:19080`, and iptables blocking direct host↔host reachability.

| Check | Result after fix |
|-------|------------------|
| Direct HTTPS between servers | Blocked (by design) |
| POST via tunnel → madexchanger → peer `/mxdeliv` | Forward success |
| Outbound queue → endpoint rewrite → exchanger | Delivered (HTTP) |
| `user@ip` and `user@[ip]` inbound store | Both land in bracketed maildir |

---

## Related docs

- TDD: [`docs/TDD/07-federation.md`](../TDD/07-federation.md)
- CLI: [`docs/guide/cli/endpoint-cache-set.md`](../guide/cli/endpoint-cache-set.md)
- CLI: [`docs/guide/cli/endpoint-cache.md`](../guide/cli/endpoint-cache.md)

## Code touch points

| File | Change |
|------|--------|
| `crates/chatmail-fed/src/mxdeliv.rs` | `normalize_addr` on envelope headers |
| `crates/chatmail-state/src/auth.rs` | IP-normalized `local_recipient_allowed` |
| `crates/chatmail-delivery/src/transport.rs` | `mxdeliv_host_for_url` host:port (IPv4, DNS, peel `[ipv4]:port`) + tests |
| `crates/chatmail-fed/src/mxdeliv.rs` | reuse `normalize_username`; bare-IP delivery test |
| `crates/chatmail-state/src/auth.rs` | bare↔bracketed IPv4 account lookup |
