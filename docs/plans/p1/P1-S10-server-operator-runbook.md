# P1-S10: Operator runbook (server side)

## Action

Document for admins (optional `docs/operator/webimap-desktop.md`):

- Enable `webimap` / `websmtp` on server
- Reverse proxy WebSocket settings
- Client desktop toggle (does not enable server)
- Troubleshooting table

No Core code in this step — **regression tests only** prove server still matches plan assumptions.

## Enable on madmail-v2

```bash
chatmail webimap enable
chatmail websmtp enable
chatmail webimap status
```

Admin API: `POST /admin/services/webimap` `{"action":"enable"}`.

## Tests (implement / run with this step)

| Test ID | Tier | Location | Asserts |
|---------|------|----------|---------|
| **P1-IT04** | Integration | `crates/chatmail/src/ctl/ops_tests.rs` | `dispatch_webimap_websmtp_toggle` — enable sets `WEBIMAP_ENABLED`, disable clears |
| **P1-IT05** | Integration | `tests/securejoin_e2e.rs` | `/webimap/send` + mailboxes work with flags on (existing) |
| **P1-IT05b** | Integration | `crates/chatmail-www/src/tests.rs` | WS `list_messages` + `send` round-trip (existing) |

### P1-IT04

```bash
cargo test -p chatmail dispatch_webimap_websmtp_toggle
# or
cargo test -p chatmail p1_it04
```

Rename or alias existing test to `p1_it04` in PR if you want ID alignment.

### P1-IT05

```bash
cargo test -p chatmail-integration securejoin
cargo test -p chatmail-integration p2p -- --test-threads=1
```

### P1-IT05b

```bash
cargo test -p chatmail-www
```

## Verification (full server regression)

```bash
cd madmailv2
cargo test -p chatmail p1_it04
cargo test -p chatmail-www
cargo test -p chatmail-integration securejoin
```

**Step done when:** P1-IT04, P1-IT05, P1-IT05b green; operator doc merged.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-IT04 | P1-S10 |
| P1-IT05 | P1-S10 |
| P1-IT05b | P1-S10 |

## Phase complete

Re-run full matrix from [README.md](README.md) — all **P1-UT\***, **P1-IT\***, **P1-E2E\***, **P1-UI01** checked.
