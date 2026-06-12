# RFC reference library (madmail-v2 TDD)

Plain-text copies of IETF RFCs referenced by the Technical Design Document. Use these for offline implementation work; canonical specs remain on the IETF site.

**Regenerate:** `./download-rfcs.sh` (from this directory).

**Canonical index:** [IETF Datatracker](https://datatracker.ietf.org/) — e.g. [RFC 5321](https://datatracker.ietf.org/doc/html/rfc5321).

## By TDD section

Each numbered TDD file has a **Related RFCs** section linking IETF Datatracker URLs and local `*.txt` files in this directory.

| TDD | Local RFC files (in this folder) | Topics |
|-----|----------------------------------|--------|
| [00-intro](../00-intro.md) | [rfc5321.txt](rfc5321.txt), [rfc5322.txt](rfc5322.txt), [rfc3501.txt](rfc3501.txt) | Core mail protocols |
| [01-architecture](../01-architecture.md) | [rfc5321.txt](rfc5321.txt), [rfc5322.txt](rfc5322.txt), [rfc3501.txt](rfc3501.txt), [rfc9110.txt](rfc9110.txt), [rfc8446.txt](rfc8446.txt) | SMTP, IMAP, HTTP, TLS |
| [02-smtp-server](../02-smtp-server.md) | [rfc5321.txt](rfc5321.txt), [rfc6409.txt](rfc6409.txt), [rfc8314.txt](rfc8314.txt), [rfc3207.txt](rfc3207.txt), [rfc4954.txt](rfc4954.txt), [rfc6531.txt](rfc6531.txt), [rfc5322.txt](rfc5322.txt), [rfc2045.txt](rfc2045.txt)–[rfc2049.txt](rfc2049.txt), [rfc3156.txt](rfc3156.txt), [rfc4880.txt](rfc4880.txt), [rfc9580.txt](rfc9580.txt) | SMTP, submission, STARTTLS, AUTH, MIME, PGP |
| [03-imap-server](../03-imap-server.md) | [rfc3501.txt](rfc3501.txt), [rfc2595.txt](rfc2595.txt), [rfc8314.txt](rfc8314.txt), [rfc2177.txt](rfc2177.txt), [rfc5464.txt](rfc5464.txt), [rfc2087.txt](rfc2087.txt), [rfc6851.txt](rfc6851.txt), [rfc7889.txt](rfc7889.txt), [rfc3348.txt](rfc3348.txt), [rfc6154.txt](rfc6154.txt), [rfc5256.txt](rfc5256.txt), [rfc4978.txt](rfc4978.txt), [rfc2342.txt](rfc2342.txt), [rfc7162.txt](rfc7162.txt), [rfc2971.txt](rfc2971.txt), [rfc5322.txt](rfc5322.txt), [rfc3156.txt](rfc3156.txt) | IMAP4rev1, STARTTLS, extensions |
| [04-storage-layer](../04-storage-layer.md) | [rfc5322.txt](rfc5322.txt), [rfc3501.txt](rfc3501.txt) | Message format, mailbox semantics |
| [05-authentication](../05-authentication.md) | [rfc4616.txt](rfc4616.txt), [rfc4954.txt](rfc4954.txt), [rfc8264.txt](rfc8264.txt), [rfc8265.txt](rfc8265.txt), [rfc3501.txt](rfc3501.txt) | SASL PLAIN, PRECIS, IMAP/SMTP AUTH |
| [07-federation](../07-federation.md) | [rfc5322.txt](rfc5322.txt), [rfc5321.txt](rfc5321.txt), [rfc9110.txt](rfc9110.txt) | `/mxdeliv` body, SMTP fallback, HTTP |
| [09-admin-api](../09-admin-api.md) | [rfc9110.txt](rfc9110.txt), [rfc8259.txt](rfc8259.txt), [rfc6750.txt](rfc6750.txt) | JSON, Bearer token, HTTP |
| [10-webimap](../10-webimap.md) | [rfc9110.txt](rfc9110.txt), [rfc3501.txt](rfc3501.txt), [rfc5322.txt](rfc5322.txt), [rfc3156.txt](rfc3156.txt), [rfc5321.txt](rfc5321.txt), [rfc8446.txt](rfc8446.txt) | WebIMAP/WebSMTP over HTTP |
| [11-proxy-services](../11-proxy-services.md) | [rfc5464.txt](rfc5464.txt), [rfc8489.txt](rfc8489.txt), [rfc8656.txt](rfc8656.txt), [rfc5766.txt](rfc5766.txt), [rfc5769.txt](rfc5769.txt), [rfc6062.txt](rfc6062.txt), [rfc6156.txt](rfc6156.txt), [rfc8445.txt](rfc8445.txt), [draft-uberti-behave-turn-rest-00.txt](draft-uberti-behave-turn-rest-00.txt), [rfc5389.txt](rfc5389.txt), [rfc3489.txt](rfc3489.txt) | TURN/STUN/ICE for calls |
| [12-security](../12-security.md) | [rfc3156.txt](rfc3156.txt), [rfc9580.txt](rfc9580.txt), [rfc4880.txt](rfc4880.txt), [rfc2045.txt](rfc2045.txt)–[rfc2049.txt](rfc2049.txt), [rfc5321.txt](rfc5321.txt), [rfc5322.txt](rfc5322.txt), [rfc8446.txt](rfc8446.txt), [rfc8555.txt](rfc8555.txt) | PGP/MIME, TLS, ACME |
| [13-configuration](../13-configuration.md) | [rfc8314.txt](rfc8314.txt), [rfc8446.txt](rfc8446.txt), [rfc8555.txt](rfc8555.txt), [rfc6409.txt](rfc6409.txt), [rfc3501.txt](rfc3501.txt), [rfc5321.txt](rfc5321.txt), [rfc9110.txt](rfc9110.txt), [rfc8615.txt](rfc8615.txt) | Listeners, TLS, ACME, `/.well-known` |
| [14-cli-tools](../14-cli-tools.md) | [rfc8555.txt](rfc8555.txt), [rfc8446.txt](rfc8446.txt), [rfc5321.txt](rfc5321.txt), [rfc3501.txt](rfc3501.txt) | Certificate + protocol ctl |
| [16-testing](../16-testing.md) | *(subset below; full set per E2E scenario)* | Protocol compliance under test |
| [17-data-models](../17-data-models.md) | [rfc5322.txt](rfc5322.txt), [rfc3501.txt](rfc3501.txt), [rfc2087.txt](rfc2087.txt), [rfc5464.txt](rfc5464.txt) | Schema ↔ protocol semantics |
| [19-certificates](../19-certificates.md) | [rfc8446.txt](rfc8446.txt), [rfc8555.txt](rfc8555.txt), [rfc8314.txt](rfc8314.txt), [rfc9110.txt](rfc9110.txt) | TLS + ACME |
| [20-deltachat-calls](../20-deltachat-calls.md) | STUN/TURN/ICE table below + [rfc5464.txt](rfc5464.txt), [draft-uberti-behave-turn-rest-00.txt](draft-uberti-behave-turn-rest-00.txt) | Delta Chat calls |
| [21-scheduled-maintenance](../21-scheduled-maintenance.md) | [rfc3501.txt](rfc3501.txt), [rfc5322.txt](rfc5322.txt), [rfc2087.txt](rfc2087.txt) | Retention, purge, quotas |
| [22-bandwidth-monitoring](../22-bandwidth-monitoring.md) | [rfc5321.txt](rfc5321.txt), [rfc3501.txt](rfc3501.txt), [rfc9110.txt](rfc9110.txt), [rfc8656.txt](rfc8656.txt), [rfc8489.txt](rfc8489.txt), [rfc8445.txt](rfc8445.txt), [rfc5464.txt](rfc5464.txt), [rfc8259.txt](rfc8259.txt) | Bandwidth spec (planned) |

## STUN / TURN / ICE (Delta Chat calls)

Used by **Delta Chat core** (IMAP METADATA + WebRTC ICE JSON) and **turn-rs** / Madmail `pion/turn`.

| RFC / draft | Title (short) | File |
|-------------|----------------|------|
| 3489 | STUN (classic; historic) | [rfc3489.txt](rfc3489.txt) |
| 5389 | Session Traversal Utilities for NAT (STUN) | [rfc5389.txt](rfc5389.txt) |
| 8489 | STUN (bis; current STUN spec) | [rfc8489.txt](rfc8489.txt) |
| 5766 | TURN (historic; see 8656) | [rfc5766.txt](rfc5766.txt) |
| 8656 | TURN (current) | [rfc8656.txt](rfc8656.txt) |
| 5769 | STUN test vectors | [rfc5769.txt](rfc5769.txt) |
| 6062 | TURN extension for TCP relaying | [rfc6062.txt](rfc6062.txt) |
| 6156 | TURN IPv6 extension | [rfc6156.txt](rfc6156.txt) |
| 6263 | ICE bandwidth management | [rfc6263.txt](rfc6263.txt) |
| 8445 | Interactive Connectivity Establishment (ICE) | [rfc8445.txt](rfc8445.txt) |
| draft-uberti-behave-turn-rest-00 | TURN REST API (shared secret credentials) | [draft-uberti-behave-turn-rest-00.txt](draft-uberti-behave-turn-rest-00.txt) |

**Discovery:** [RFC 5464](rfc5464.txt) — `/shared/vendor/deltachat/turn` metadata (already in mail inventory below).

## Full inventory

| RFC | Title (short) | File |
|-----|-----------------|------|
| 2045 | MIME Part One: Format of Internet Message Bodies | [rfc2045.txt](rfc2045.txt) |
| 2046 | MIME Part Two: Media Types | [rfc2046.txt](rfc2046.txt) |
| 2047 | MIME Part Three: Message Header Extensions | [rfc2047.txt](rfc2047.txt) |
| 2048 | MIME Part Four: Registration Procedures | [rfc2048.txt](rfc2048.txt) |
| 2049 | MIME Part Five: Conformance Criteria | [rfc2049.txt](rfc2049.txt) |
| 2087 | IMAP QUOTA extension | [rfc2087.txt](rfc2087.txt) |
| 2177 | IMAP IDLE | [rfc2177.txt](rfc2177.txt) |
| 2342 | IMAP NAMESPACE | [rfc2342.txt](rfc2342.txt) |
| 2595 | Using TLS with IMAP, POP3 and ACAP (STARTTLS) | [rfc2595.txt](rfc2595.txt) |
| 2971 | IMAP ID extension | [rfc2971.txt](rfc2971.txt) |
| 3156 | MIME Security with OpenPGP | [rfc3156.txt](rfc3156.txt) |
| 3207 | SMTP Service Extension for Secure SMTP over TLS (STARTTLS) | [rfc3207.txt](rfc3207.txt) |
| 3348 | IMAP CHILDREN extension | [rfc3348.txt](rfc3348.txt) |
| 3501 | INTERNET MESSAGE ACCESS PROTOCOL - VERSION 4rev1 | [rfc3501.txt](rfc3501.txt) |
| 4616 | The PLAIN SASL Mechanism | [rfc4616.txt](rfc4616.txt) |
| 4880 | OpenPGP Message Format (historic) | [rfc4880.txt](rfc4880.txt) |
| 4954 | SMTP Service Extension for Authentication | [rfc4954.txt](rfc4954.txt) |
| 4978 | IMAP COMPRESS extension | [rfc4978.txt](rfc4978.txt) |
| 5256 | IMAP SORT and THREAD | [rfc5256.txt](rfc5256.txt) |
| 5321 | Simple Mail Transfer Protocol | [rfc5321.txt](rfc5321.txt) |
| 5322 | Internet Message Format | [rfc5322.txt](rfc5322.txt) |
| 5464 | IMAP METADATA Extension | [rfc5464.txt](rfc5464.txt) |
| 6154 | IMAP SPECIAL-USE extension | [rfc6154.txt](rfc6154.txt) |
| 6409 | Message Submission for Mail | [rfc6409.txt](rfc6409.txt) |
| 6531 | SMTP Extension for Internationalized Email | [rfc6531.txt](rfc6531.txt) |
| 6750 | OAuth 2.0 Bearer Token Usage | [rfc6750.txt](rfc6750.txt) |
| 6851 | IMAP MOVE Extension | [rfc6851.txt](rfc6851.txt) |
| 7162 | IMAP CONDSTORE | [rfc7162.txt](rfc7162.txt) |
| 7889 | IMAP APPENDLIMIT Extension | [rfc7889.txt](rfc7889.txt) |
| 8259 | The JavaScript Object Notation (JSON) Data Interchange Format | [rfc8259.txt](rfc8259.txt) |
| 8264 | PRECIS Framework | [rfc8264.txt](rfc8264.txt) |
| 8265 | Preparation, Enforcement, and Comparison of Internationalized Strings (PRECIS) | [rfc8265.txt](rfc8265.txt) |
| 8314 | Cleartext Considered Obsolete: Use of TLS for SMTP Submission | [rfc8314.txt](rfc8314.txt) |
| 8446 | The Transport Layer Security (TLS) Protocol Version 1.3 | [rfc8446.txt](rfc8446.txt) |
| 8555 | Automatic Certificate Management Environment (ACME) | [rfc8555.txt](rfc8555.txt) |
| 8615 | Well-Known Uniform Resource Identifiers (URIs) | [rfc8615.txt](rfc8615.txt) |
| 9110 | HTTP Semantics | [rfc9110.txt](rfc9110.txt) |
| 9580 | OpenPGP Message Format | [rfc9580.txt](rfc9580.txt) |

**Note:** RFC 822 is obsoleted by [RFC 5322](rfc5322.txt); TDD text that says “RFC 822 message” means the Internet Message Format in RFC 5322.

## Autoconfig (not an IETF RFC)

Delta Chat / Thunderbird mail autoconfig at `GET /.well-known/autoconfig/mail/config-v1.1.xml` follows the **Mozilla ISPDB** XML format (same as cmdeploy / Dovecot autoconfig), not an RFC.

| Spec | Role | Where |
|------|------|--------|
| [RFC 8615](rfc8615.txt) | `/.well-known/` URI prefix on HTTPS | Local: [rfc8615.txt](rfc8615.txt) |
| Mozilla autoconfig | `<clientConfig>` XML (`incomingServer` / `outgoingServer`, `socketType` SSL/STARTTLS) | [Thunderbird autoconfig](https://github.com/thunderbird/autoconfig) (online); implementation: `crates/chatmail-config/src/autoconfig.rs`, route in `chatmail-www` |

**Related RFCs already local:** [8314](rfc8314.txt) (TLS for mail access/submission), [2595](rfc2595.txt) (IMAP STARTTLS), [3207](rfc3207.txt) (SMTP STARTTLS), [6409](rfc6409.txt) (submission ports).
