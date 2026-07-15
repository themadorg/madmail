# Security Architecture

## Core Security Policies

### 1. PGP-Only Enforcement
**Mandatory** for all user-submitted messages (SMTP submission and IMAP APPEND).

Accepted:
- Properly formed `multipart/encrypted` PGP/MIME messages
- Secure Join handshake messages (`Secure-Join: vc-request`)
- Certain automated bounces (`mailer-daemon@` + `multipart/report`)

Rejected with `523 Encryption Needed`:
- Plain text
- Invalid PGP structure
- Header mismatches (MIME From != Envelope From)

**Implementation**: Deep packet inspection in `pgp_verify` module (see Madmail reference).

### 2. No-Log Policy
When `log off` in config (the default if `log` is omitted):
- Use `tracing` with filter `off` (no protocol/DB chatter)
- No log destinations opened (no stderr fan-out, no log files)
- Only critical boot errors go to stderr via `boot_error` (not tracing)

Operators opt in with the static `log` directive (maddy-compatible):
- `log stderr` / `log on` / `log stderr_ts` — write tracing to stderr
- `log /path/to/file` — append to a file (create parents as needed)
- `log stderr /var/lib/madmail/madmail.log` — both
- `log syslog` — currently maps to stderr (dedicated syslog backend not wired yet)

Debug mode (`debug true` / `yes` / `1` / `enable` / …) **overrides** No-Log: forces `debug` filter level and stderr if no `log` target is set. Logging is not toggled via CLI or admin API.

### 3. Federation Policy Engine
- `ACCEPT` (default) + blocklist rules
- `REJECT` + allowlist rules
- Enforced on **every** inbound SMTP and `/mxdeliv` request
- Enforced on **every** outbound delivery attempt
- In-memory `HashSet` for O(1) checks + `RwLock`

### 4. TLS & Certificate Modes
Support all four modes from Madmail:
- `autocert` (Let's Encrypt HTTP-01)
- `acme` (DNS-01)
- `file` (user provided)
- `self_signed`

Use `rustls` + `rustls-acme` or `instant-acme` crates.

### 5. Admin API Security
- Shared secret Bearer token
- Constant-time comparison
- Rate limiting (10 attempts/min/IP)
- Always HTTP 200 + status in body
- Request size limit (1MB)
- No sensitive data leakage

### 6. Rate Limiting & DoS Protection
- Per-IP auth failure rate limit on Admin API
- Connection limits per service
- Early size checks on message submission

### 7. Quota Enforcement
Checked on every delivery and IMAP quota command.
In-memory cache with write-through updates.

## Threat Model Considerations
- User enumeration prevention (silent drop on non-existent users during federation)
- Timing attack prevention (constant-time token compare)
- Configuration injection prevention (sanitize values written to config)
- Self-signed cert acceptance only for federation (not for submission)

## Rust-Specific Security
- Prefer crates with good audit history (`rustls`, `tokio`, `axum`, `sqlx`)
- Use `zeroize` for sensitive data in memory where appropriate
- Avoid `unwrap()` in production paths
- Structured logging with `tracing` (can be completely disabled)

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Concern | madmail | cmrelay | cmdeploy | stalwart |
|---------|---------|---------|----------|----------|
| PGP-only enforcement | [`pgp_verify/pgp_verify.go`](../../context/madmail/internal/pgp_verify/pgp_verify.go), tests in [`pgp_verify/`](../../context/madmail/internal/pgp_verify/) | [`openpgp.rs`](../../context/cmrelay/src/filtermail/src/openpgp.rs) | — | — |
| Submission check | [`endpoint/smtp/submission.go`](../../context/madmail/internal/endpoint/smtp/submission.go) | [`inbound.rs`](../../context/cmrelay/src/filtermail/src/inbound.rs) | — | — |
| `/mxdeliv` security | [`mxdeliv_security.go`](../../context/madmail/internal/endpoint/chatmail/mxdeliv_security.go) | [`mxdeliv.rs`](../../context/cmrelay/src/filtermail/src/mxdeliv.rs) | — | — |
| Federation policy | [`federationtracker/policy.go`](../../context/madmail/internal/federationtracker/policy.go) | — | — | — |
| No-Log | [`docs/chatmail/nolog.md`](../../context/madmail/docs/chatmail/nolog.md) | — | — | [`crates/trc/`](../../context/stalwart/crates/trc/) (tracing design) |
| TLS / ACME | [`internal/tls/`](../../context/madmail/internal/tls/) | [`manager/internal/install/tls.go`](../../context/cmrelay/src/manager/internal/install/tls.go) | [`acmetool/`](../../context/cmdeploy/src/cmdeploy/acmetool/) | TLS in server crates + install |
| E2E security tests | [`test_02_unencrypted_rejection.py`](../../context/madmail/tests/deltachat-test/scenarios/test_02_unencrypted_rejection.py), [`test_08_no_logging.py`](../../context/madmail/tests/deltachat-test/scenarios/test_08_no_logging.py), [`test_22_mxdeliv_security.py`](../../context/madmail/tests/deltachat-test/scenarios/test_22_mxdeliv_security.py) | — | — | — |

## Related RFCs

PGP policy, MIME, TLS, and certificate automation. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [3156](https://datatracker.ietf.org/doc/html/rfc3156) | PGP/MIME (`multipart/encrypted`) | [rfc3156.txt](RFC/rfc3156.txt) |
| [9580](https://datatracker.ietf.org/doc/html/rfc9580) | OpenPGP message format | [rfc9580.txt](RFC/rfc9580.txt) |
| [4880](https://datatracker.ietf.org/doc/html/rfc4880) | OpenPGP (legacy reference) | [rfc4880.txt](RFC/rfc4880.txt) |
| [2045](https://datatracker.ietf.org/doc/html/rfc2045)–[2049](https://datatracker.ietf.org/doc/html/rfc2049) | MIME structure | [rfc2045.txt](RFC/rfc2045.txt) … [rfc2049.txt](RFC/rfc2049.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP error semantics (`523`, `554`) | [rfc5321.txt](RFC/rfc5321.txt) |
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Header / envelope matching | [rfc5322.txt](RFC/rfc5322.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS 1.3 | [rfc8446.txt](RFC/rfc8446.txt) |
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | ACME (Let's Encrypt / DNS-01) | [rfc8555.txt](RFC/rfc8555.txt) |