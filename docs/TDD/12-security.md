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

## Cross-Protocol Attacks (ALPACA)

Every TLS listener presents the same certificate, because there is one hostname.
That removes the certificate-separation defence against [ALPACA](https://alpaca-attack.com/)
(USENIX Security 2021): an attacker redirects a victim's HTTPS connection to a
different TLS service that presents an acceptable certificate for the same name,
then abuses that service's parser. TLS does not bind a connection to an intended
port, so a lax mail listener stays a valid substitute server for the web origin
however strict the HTTPS port is.

Strict ALPN is therefore the only TLS-layer defence available, and RFC 9325
(BCP 195) §3.8 makes it the recommendation. Each listener advertises exactly the
protocols it will speak, so rustls answers a client whose offers do not overlap
with a fatal `no_application_protocol`:

| Listener | Advertised ALPN |
|----------|-----------------|
| IMAP implicit TLS (993) | `imap` |
| Submission implicit TLS (465) | `smtp` |
| HTTPS sharing mail (443, `alpn_imap` / `alpn_smtp`) | enabled mail tokens + `http/1.1` |
| HTTPS alone (443) | none |
| STARTTLS upgrades (143 / 587), inbound SMTP (25) | none |

Built by `load_mail_tls_configs` in `crates/chatmail/src/shared_listener.rs`.

On the shared HTTPS port, ALPN is also checked **before** the SNI hostname. That
ordering is what makes hostname routing safe: SNI names a host rather than a
protocol and a browser sets it from its URL bar, so consulting SNI first would send
a visitor to `https://imap.example.org/` into the IMAP parser with no attacker
involved. Browsers always offer ALPN in order to negotiate HTTP/2, so they are
decided before the hostname is ever read. Tests: `p12_it07_alpn_outranks_sni`,
`p12_ut20_configured_sni_replaces_the_prefix_default`.

Two limits are deliberate. STARTTLS ports advertise nothing because the client
negotiates TLS after a plaintext greeting and has no ClientHello to offer ALPN in.
And a client that sends no ALPN extension is never rejected — rustls only fails
when the client offered ALPN and nothing overlapped — which is what keeps
Thunderbird, Apple Mail and Delta Chat on the standard ports working. That also
means an ALPN-less client remains unattributable, so on the shared HTTPS port such
a connection is served as HTTPS rather than guessed into a mail parser.

Tests: `p12_it04_alpn_less_client_still_reaches_imap`,
`p12_it05_browser_alpn_refused_on_imap_port`,
`p12_it02_unoffered_alpn_is_refused`,
`p12_ut17_dedicated_mail_ports_get_their_own_alpn`.

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
| [7301](https://datatracker.ietf.org/doc/html/rfc7301) | TLS ALPN extension | — |
| [9325](https://datatracker.ietf.org/doc/html/rfc9325) | TLS BCP 195 — strict ALPN/SNI vs ALPACA | — |
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | ACME (Let's Encrypt / DNS-01) | [rfc8555.txt](RFC/rfc8555.txt) |