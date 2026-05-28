# P9-S09: E2E — relay-ping-style IMAP (and SMTP optional)

## Action

Full wire E2E using the same harness as existing mail tests (not go-imap library — raw TCP like relay-ping documents):

### In-process (`chatmail-integration`)

New tests in `tests/turn_e2e.rs`:

| Test | Steps |
|------|--------|
| `turn_imap_e2e_capability_metadata` | CAPABILITY includes `METADATA` when TURN on |
| `turn_imap_e2e_getmetadata_deltachat` | `GETMETADATA "" /shared/vendor/deltachat/turn` → four-field line |
| `turn_imap_e2e_getmetadata_requires_auth` | PREAUTH/NOT AUTHENTICATED → no leak |
| `turn_imap_e2e_turn_disabled` | `turn_enable=no` → key absent |

Fix/remove stub [`imap_e2e_getmetadata_turn_relay`](../../../tests/imap_e2e.rs) (`/private/turn/relay`, `turn://`).

### SMTP (relay-ping parity)

Optional: after IMAP metadata fetch, `smtp_submit` from [`tests/support/mod.rs`](../../../tests/support/mod.rs) to prove mail stack still healthy — same session as Secure Join E2E (no TURN on SMTP, regression guard only).

### External relay-ping binary (running server)

Document Makefile target:

```makefile
test-turn-relay-ping: relay-ping-build run-bg
	# Future: relay-ping imap probe + metadata step
```

Reference: [`context/relay-ping/internal/check/imapcheck/imapcheck.go`](../../../context/relay-ping/internal/check/imapcheck/imapcheck.go) — LOGIN, SELECT, IDLE sequence; extend with GETMETADATA when upstream adds check.

## Files touched

- `tests/turn_e2e.rs`
- `tests/imap_e2e.rs` — remove wrong key
- `Makefile` — `test-turn`, `test-turn-relay-ping`
- `docs/TDD/03-imap-server.md` — cross-link b9

## TDD references

- [03-imap-server.md](../../TDD/03-imap-server.md) — relay-ping IDLE table
- [11-proxy-services.md](../../TDD/11-proxy-services.md)

## Madmail / context references

- `context/relay-ping/README.md`
- `tests/support/imap_client.rs` — "relay-ping style dialog"

## RFC references

- [RFC 5464](../../TDD/RFC/rfc5464.txt)
- [RFC 3501](../../TDD/RFC/rfc3501.txt) — LOGIN, authenticated state

## Verification

**P9-E2E01**

```bash
cargo test -p chatmail-integration turn_imap_e2e
make test-turn   # alias: unit + smoke + integration + e2e
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-E2E01** | `P9-S09` |

## Next

[P9-S10-e2e-core-ice-servers.md](P9-S10-e2e-core-ice-servers.md)
