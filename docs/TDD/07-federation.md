# Federation & HTTP Delivery (`/mxdeliv`)

**Implementation:** inbound HTTP — `crates/chatmail-fed` (`mxdeliv`, `security`); outbound — `crates/chatmail-delivery`; stats — `chatmail-state::tracker` + `chatmail-db`.

**Operator CLI:** [`../guide/cli/federation.md`](../guide/cli/federation.md) · [`endpoint-cache.md`](../guide/cli/endpoint-cache.md) · TDD [14-cli-tools.md](14-cli-tools.md).

## Overview
Chatmail uses **HTTP-based federation** as the primary delivery method between servers, with traditional SMTP as fallback. This enables reliable delivery even for IP-only deployments without DNS MX records.

## Wire Protocol

### Sending (Outbound)
```
POST /mxdeliv HTTP/1.1
Host: recipient-server
X-Mail-From: sender@domain
X-Mail-To: recipient@domain
X-Mail-To: another@domain
Content-Type: application/octet-stream

<complete RFC 5322 message>
```

*(Historically called “RFC 822”; use [RFC 5322](RFC/rfc5322.txt) — [datatracker](https://datatracker.ietf.org/doc/html/rfc5322).)*

### Receiving (Inbound)
Server must:
1. Validate `X-Mail-From` and at least one `X-Mail-To`
2. Parse RFC 5322 body
3. Run federation policy check on sender domain
4. Run PGP enforcement
5. Deliver to each recipient's mailbox via `DeliveryTarget`
6. Return `403 Forbidden` when federation policy rejects the sender (Madmail); do not accept and answer `200` for blocked mail

### Response Codes
| Code | Meaning |
|------|---------|
| 200  | Accepted for all valid recipients |
| 400  | Bad request (missing headers, unparseable body) |
| 403  | Federation policy rejected |
| 404  | No valid recipients |
| 413  | Message too large |
| 500  | Internal error |

## Outbound retry queue (`target.queue`)

Madmail persists failed remote deliveries under `target.queue remote_queue` and retries with exponential backoff. madmail-v2 mirrors most of this, with one deliberate difference:

| Setting (`maddy.conf`) | Default | Meaning |
|------------------------|---------|---------|
| `max_tries` | 3 | Attempts per recipient before drop |
| `max_parallelism` | 16 | Concurrent deliveries |
| `initial_retry` | 1m | First retry delay (Go duration: `1m`, `15m`, `1h`, …) |
| `retry_time_scale` | 1.25 | Backoff multiplier |
| `post_init_delay` | 10s | Startup grace before processing loaded entries |
| `max_delivery_time` / `delivery_timeout` | **10m** | **madmail-v2 only:** max wall-clock time in queue; older messages are logged as failed and removed (Madmail has no equivalent cap and may retry for days) |
| `location` | `{state_dir}/remote_queue` | On-disk queue directory |

Each queued message: `{id}.meta` (JSON, includes `queued_at_unix`) + `{id}.body` (RFC 5322 bytes). Remote SMTP/IMAP accept enqueues immediately; the worker delivers via HTTPS/HTTP `/mxdeliv` (same as before). Temporary failures requeue until `max_tries` or `max_delivery_time`; HTTP 4xx → immediate permanent drop.

**Not yet:** DSN/bounce pipeline (`bounce {}` in Madmail), full MX lookup for SMTP (madmail-v2 uses direct `:25` to resolved host).

## Delivery Priority (target.remote)
1. **HTTPS** `POST https://domain/mxdeliv` (InsecureSkipVerify for self-signed)
2. **HTTP**  `POST http://domain/mxdeliv` (fallback)
3. **SMTP**  Direct delivery to recipient host port 25 (last resort after HTTP failures)

## Endpoint Override System
Database table `dns_overrides` + in-memory cache.

Used to:
- Route traffic through exchangers (`madexchanger`)
- Redirect during migrations
- Override IP literals

CLI: [`madmail endpoint-cache set`](../guide/cli/endpoint-cache-set.md) (alias `dns-cache`); federation policy: [`madmail federation`](../guide/cli/federation.md) including `dismiss` / `undismiss` for silent-dismiss cache (`chatmail-state::silent_dismiss`)

## FederationTracker (In-Memory)
Critical for diagnostics:
- Per-domain queue depth
- Success/failure counts per transport (HTTP/HTTPS/SMTP)
- Mean latency
- Last active timestamp

Flushed to DB every 30s. Exposed via `/admin/federation/servers`

## Implementation modules (`crates/`)

| Madmail path | madmail-v2 crate | Notes |
|--------------|-------------------|-------|
| `internal/target/remote/` | `chatmail-delivery` | `queue`, `router`, `transport`, `federation_http` — shared `reqwest` client for `/mxdeliv` POSTs |
| `internal/target/queue/queue.go` | `chatmail-config::queue` | `target.queue` settings parsed into `AppConfig.queue` |
| `internal/endpoint/chatmail/` (`/mxdeliv`) | `chatmail-fed` | `mxdeliv.rs`, `security.rs`; listener in `server.rs` |
| `internal/federationtracker/` | `chatmail-state::tracker` | Flushed via `chatmail-state::flusher` → `chatmail-db` |
| `internal/endpoint_cache/` | `chatmail-db::endpoint_cache` | Overrides read on outbound routing |
| Federation policy / silent dismiss | `chatmail-state::policy`, `silent_dismiss` | Hydrated from `chatmail-db::federation_policy` |
| PGP on receive | `chatmail-pgp` | Called from `mxdeliv` and SMTP ingest paths |

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Concern | madmail | cmrelay | cmdeploy | stalwart |
|---------|---------|---------|----------|----------|
| `/mxdeliv` receive | [`chatmail.go`](../../context/madmail/internal/endpoint/chatmail/chatmail.go) (`handleReceiveEmail`), [`mxdeliv_security.go`](../../context/madmail/internal/endpoint/chatmail/mxdeliv_security.go) | [`mxdeliv.rs`](../../context/cmrelay/src/filtermail/src/mxdeliv.rs) | — | — |
| Outbound HTTP/SMTP | [`target/remote/remote.go`](../../context/madmail/internal/target/remote/remote.go), [`connect.go`](../../context/madmail/internal/target/remote/connect.go) | [`outbound.rs`](../../context/cmrelay/src/filtermail/src/outbound.rs), [`transport.rs`](../../context/cmrelay/src/filtermail/src/transport.rs) | Postfix routing | [`smtp/outbound/`](../../context/stalwart/crates/smtp/src/outbound/) |
| Federation policy | [`federationtracker/policy.go`](../../context/madmail/internal/federationtracker/policy.go), [`tracker.go`](../../context/madmail/internal/federationtracker/tracker.go) | — | — | — |
| DNS overrides | [`endpoint_cache/`](../../context/madmail/internal/endpoint_cache/) | — | — | — |
| Exchanger | [`exchangers/madexchanger/`](../../context/madmail/exchangers/madexchanger/) | — | — | — |
| Tests | [`test_07_federation.py`](../../context/madmail/tests/deltachat-test/scenarios/test_07_federation.py), [`test_22_mxdeliv_security.py`](../../context/madmail/tests/deltachat-test/scenarios/test_22_mxdeliv_security.py) | — | — | — |

## Security Notes
- TLS verification **disabled** for federation HTTP (self-signed certs are normal in Chatmail)
- Message authenticity comes from end-to-end **PGP** encryption
- Admin protection: never deliver to `admin@`, `postmaster@`, etc. via federation

## Related RFCs

Federation wire format and HTTP delivery. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Internet Message Format (`/mxdeliv` body) | [rfc5322.txt](RFC/rfc5322.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP fallback delivery | [rfc5321.txt](RFC/rfc5321.txt) |
| [9110](https://datatracker.ietf.org/doc/html/rfc9110) | HTTP semantics (`POST /mxdeliv`) | [rfc9110.txt](RFC/rfc9110.txt) |

Regenerate offline copies: [`RFC/download-rfcs.sh`](RFC/download-rfcs.sh).

## Testing
Must pass `test_07_federation.py` (Parts A–D), including port blocking scenarios (HTTPS only, HTTP only, SMTP only).