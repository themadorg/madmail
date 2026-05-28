# P9-S01: chatmail-turn credential helper

## Action

Create workspace crate `chatmail-turn` with:

- `turn_metadata_line(server, port, secret, ttl_secs, now_unix) -> String` — `host:port:username:password`
- `hmac_password(secret, username) -> String` — base64 SHA1 HMAC (Madmail-compatible)
- Unit tests with **frozen** `now_unix` and known secret (vectors from Madmail `imap.go`).

## Files touched

- `crates/chatmail-turn/Cargo.toml`
- `crates/chatmail-turn/src/lib.rs`
- `Cargo.toml` (workspace member)

## TDD references

- [11-proxy-services.md](../../TDD/11-proxy-services.md) — metadata contract
- [13-configuration.md](../../TDD/13-configuration.md) — `turn_secret`, `turn_ttl`

## Madmail / context references

- `context/madmail/internal/endpoint/imap/imap.go` — GETMETADATA generation
- `context/turn-rs/src/codec/crypto.rs` — `static_auth_secret`

## RFC references

- [RFC 5464](../../TDD/RFC/rfc5464.txt) — metadata transport (not wire format)
- [draft-uberti-behave-turn-rest-00 §2.2](../../TDD/RFC/draft-uberti-behave-turn-rest-00.txt) — shared-secret username/password

## Verification

**P9-UT01** — `p9_ut01_hmac_matches_madmail_vector`

```bash
cargo test -p chatmail-turn p9_ut01
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-UT01** | `P9-S01` |

## Next

[P9-S02-parser-core-parity.md](P9-S02-parser-core-parity.md)
