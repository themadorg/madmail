# Implementation reference codebases

Reference implementations for **madmail-v2** live under `context/` (sibling of `docs/`). Paths below are relative to this TDD directory (`../../context/...`).

| Tree | Role | When to read |
|------|------|--------------|
| [**madmail**](../../context/madmail/) | **Target behaviour** — production Chatmail server (Go, single binary) | Default for feature parity: SMTP, IMAP, `/mxdeliv`, Admin API, PGP, JIT, quota |
| [**cmrelay**](../../context/cmrelay/) | **Legacy Dovecot stack** — Rust `filtermail` + Python `chatmaild` + installer | Dovecot/Postfix-era federation, metadata, JIT hooks; CLI parity stubs vs madmail |
| [**cmdeploy**](../../context/cmdeploy/) | **Deployment & black-box tests** — Postfix + Dovecot + nginx | Install templates, online pytest spec before/after Rust migration |
| [**stalwart**](../../context/stalwart/) | **Protocol engine study** (Rust, AGPL) — SMTP/IMAP parsers & sessions | Session structure, `smtp-proto` / `imap-proto` usage; not a drop-in library |
| [**turn-rs**](../../context/turn-rs/) | **TURN/STUN server** (Rust) — WebRTC relay | Embed or sidecar; `static-auth-secret` matches Madmail HMAC credentials |

## madmail (primary)

| Area | Path |
|------|------|
| Entry / wiring | [`maddy.go`](../../context/madmail/maddy.go) |
| HTTP Chatmail (`/new`, `/mxdeliv`, registration) | [`internal/endpoint/chatmail/`](../../context/madmail/internal/endpoint/chatmail/) — [`chatmail.go`](../../context/madmail/internal/endpoint/chatmail/chatmail.go), [`mxdeliv_security.go`](../../context/madmail/internal/endpoint/chatmail/mxdeliv_security.go) |
| SMTP | [`internal/endpoint/smtp/`](../../context/madmail/internal/endpoint/smtp/) |
| IMAP | [`internal/endpoint/imap/imap.go`](../../context/madmail/internal/endpoint/imap/imap.go) |
| IMAP storage backend | [`internal/storage/imapsql/`](../../context/madmail/internal/storage/imapsql/), [`internal/go-imap-sql/`](../../context/madmail/internal/go-imap-sql/) |
| Outbound federation | [`internal/target/remote/`](../../context/madmail/internal/target/remote/) |
| Federation policy / tracker | [`internal/federationtracker/`](../../context/madmail/internal/federationtracker/) |
| PGP enforcement | [`internal/pgp_verify/`](../../context/madmail/internal/pgp_verify/) |
| Auth / JIT | [`internal/auth/pass_table/`](../../context/madmail/internal/auth/pass_table/), [`internal/auth/sasl.go`](../../context/madmail/internal/auth/sasl.go) |
| Quota cache | [`internal/quota/cache.go`](../../context/madmail/internal/quota/cache.go) |
| Admin API | [`internal/api/admin/`](../../context/madmail/internal/api/admin/) |
| WebIMAP | [`internal/endpoint/webimap/`](../../context/madmail/internal/endpoint/webimap/) |
| TURN (Go/pion) | [`internal/endpoint/turn/turn.go`](../../context/madmail/internal/endpoint/turn/turn.go) |
| TURN (Rust) | [`context/turn-rs/`](../../context/turn-rs/) — see [`11-proxy-services.md`](11-proxy-services.md) |
| E2E tests | [`tests/deltachat-test/`](../../context/madmail/tests/deltachat-test/) |
| Operator docs | [`docs/chatmail/`](../../context/madmail/docs/chatmail/) |

## cmrelay (Dovecot-era Rust/Python)

| Area | Path |
|------|------|
| Overview | [`doc/index.md`](../../context/cmrelay/doc/index.md), [`src/filtermail/README.md`](../../context/cmrelay/src/filtermail/README.md) |
| `/mxdeliv` + inbound/outbound | [`src/filtermail/src/mxdeliv.rs`](../../context/cmrelay/src/filtermail/src/mxdeliv.rs), [`inbound.rs`](../../context/cmrelay/src/filtermail/src/inbound.rs), [`outbound.rs`](../../context/cmrelay/src/filtermail/src/outbound.rs) |
| SMTP server/client | [`smtp_server.rs`](../../context/cmrelay/src/filtermail/src/smtp_server.rs), [`smtp_client.rs`](../../context/cmrelay/src/filtermail/src/smtp_client.rs) |
| OpenPGP check | [`openpgp.rs`](../../context/cmrelay/src/filtermail/src/openpgp.rs) |
| IMAP metadata (TURN/Iroh) | [`metadata.rs`](../../context/cmrelay/src/filtermail/src/metadata.rs) |
| Dovecot auth bridge | [`doveauth.rs`](../../context/cmrelay/src/filtermail/src/doveauth.rs), [`python/chatmaild/doveauth.py`](../../context/cmrelay/src/filtermail/python/chatmaild/doveauth.py) |
| JIT / users | [`python/chatmaild/user.py`](../../context/cmrelay/src/filtermail/python/chatmaild/user.py), [`newemail.py`](../../context/cmrelay/src/filtermail/python/chatmaild/newemail.py) |
| Installer / systemd | [`src/manager/internal/install/`](../../context/cmrelay/src/manager/internal/install/) |
| CLI parity (stubs → madmail) | [`src/manager/internal/madmailctl/`](../../context/cmrelay/src/manager/internal/madmailctl/), [`doc/madmail/cmrelay-parity.md`](../../context/cmrelay/doc/madmail/cmrelay-parity.md) |

## cmdeploy (deploy + pytest)

| Area | Path |
|------|------|
| Orchestrator | [`src/cmdeploy/cmdeploy.py`](../../context/cmdeploy/src/cmdeploy/cmdeploy.py) |
| Dovecot | [`src/cmdeploy/dovecot/dovecot.conf.j2`](../../context/cmdeploy/src/cmdeploy/dovecot/dovecot.conf.j2), [`deployer.py`](../../context/cmdeploy/src/cmdeploy/dovecot/deployer.py) |
| Postfix | [`src/cmdeploy/postfix/`](../../context/cmdeploy/src/cmdeploy/postfix/) |
| Metadata service template | [`src/cmdeploy/service/chatmail-metadata.service.f`](../../context/cmdeploy/src/cmdeploy/service/chatmail-metadata.service.f) |
| Online tests | [`src/cmdeploy/tests/online/`](../../context/cmdeploy/src/cmdeploy/tests/online/) |

## stalwart (protocol reference, Rust)

| Area | Path |
|------|------|
| Repo overview | [`README.md`](../../context/stalwart/README.md) |
| SMTP session | [`crates/smtp/src/inbound/`](../../context/stalwart/crates/smtp/src/inbound/) — [`session.rs`](../../context/stalwart/crates/smtp/src/inbound/session.rs), [`mail.rs`](../../context/stalwart/crates/smtp/src/inbound/mail.rs), [`data.rs`](../../context/stalwart/crates/smtp/src/inbound/data.rs), [`auth.rs`](../../context/stalwart/crates/smtp/src/inbound/auth.rs) |
| SMTP outbound | [`crates/smtp/src/outbound/`](../../context/stalwart/crates/smtp/src/outbound/) |
| IMAP | [`crates/imap/src/`](../../context/stalwart/crates/imap/src/) — [`core/session.rs`](../../context/stalwart/crates/imap/src/core/session.rs), [`op/`](../../context/stalwart/crates/imap/src/op/) |
| IMAP parser | [`crates/imap-proto/`](../../context/stalwart/crates/imap-proto/) |
| Mail store (design only) | [`crates/email/`](../../context/stalwart/crates/email/), [`crates/store/`](../../context/stalwart/crates/store/) |
| HTTP / management API | [`crates/http/`](../../context/stalwart/crates/http/), [`api/v1/openapi.yml`](../../context/stalwart/api/v1/openapi.yml) |
