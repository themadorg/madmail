# P1-S05: Static Configuration Parsing

## Action

Add `parse.rs` with `load_config(path)` supporting `chatmail.toml` (serde/toml) and Madmail `*.conf` subset. Define `AppConfig` (`primary_domain`, `hostname`, `state_dir`, `tls_mode`).

## Files touched

- `crates/chatmail-config/src/parse.rs`
- `crates/chatmail-config/src/lib.rs`

## TDD references

- [13-configuration.md](../../TDD/13-configuration.md) *(planned)* — static + dynamic config
- [01-architecture.md](../../TDD/01-architecture.md) — config subsystem
- [12-security.md](../../TDD/12-security.md) — TLS mode (`autocert`, `file`, …)

## Madmail / context references

- `context/madmail/maddy.conf` — directive reference
- `context/madmail/docs/chatmail/certificate.md` — TLS loaders

## RFC references

- [RFC 8314](../../TDD/RFC/rfc8314.txt) — TLS for mail (STARTTLS policy, future phases)
- [RFC 8446](../../TDD/RFC/rfc8446.txt) — TLS 1.3
- [RFC 8555](../../TDD/RFC/rfc8555.txt) — ACME (autocert/acme loaders)

## Verification

**P1-UT02** `test_load_config_valid_toml` in `parse.rs`.

```bash
cargo test -p chatmail-config test_load_config_valid_toml
```
