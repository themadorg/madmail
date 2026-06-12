# Testing Strategy

## Philosophy
Chatmail's correctness is defined by **real Delta Chat client behavior**, not just protocol compliance. Therefore, **E2E testing with actual Delta Chat RPC clients** is the primary validation method.

## Test Pyramid

```
          E2E (Delta Chat RPC)
        ▲
       / \
      /   \     Integration (SMTP/IMAP/Federation handlers)
     /     \
    /       \   Unit (PGP verify, quota cache, federation policy, etc.)
   Unit     Integration
```

## 1. Unit Tests
- `cargo test`
- Focus on pure logic:
  - PGP packet verification (many edge cases)
  - Quota cache concurrency safety
  - Federation policy evaluation + normalization (IP brackets, case)
  - Endpoint cache resolution
  - Settings toggle logic
  - **Auth cache** (credentials, blocklist, JIT flag hydrate + write-through)
  - **dclogin URL** shape (`build_dclogin_link`, `POST /new`, cmping IP URLs)
  - **STARTTLS** (IMAP LOGIN gate, SMTP AUTH-after-TLS, TLS cert load for plain listeners)
  - **Autoconfig** (SSL/STARTTLS entries, no fake HTTPS ALPN IMAP)

### madmail-v2 unit test index (parity fixes)

| Area | Crate / path | Key tests |
|------|--------------|-----------|
| dclogin / `/new` | `chatmail-config`, `chatmail-www` | `build_dclogin_link_matches_www_shape`, `new_account_returns_dclogin_url_with_ssl_hints` |
| cmping IP setup | `context/cmping/test_cmping_dclogin.py` | `test_ip_dclogin_includes_ssl_host_hints` |
| Auth cache + JIT | `chatmail-state`, `chatmail-auth` | `hydrate_loads_blocklist_and_jit_flag`, `jit_coalesces_concurrent_creates_for_same_user` |
| TLS for STARTTLS | `chatmail-config` | `listeners_need_tls_cert_for_starttls_only_ports` |
| Autoconfig | `chatmail-config`, `chatmail-www` | `autoconfig_omits_https_alpn_even_when_http_tls_bound` |
| IMAP caps | `chatmail-imap` | `p5_ut01_test_capability_includes_chatmail_extensions` (`XDELTAPUSH`) |
| Push notify | `chatmail-push`, `chatmail-admin` | `push_mode_and_circuit_breaker`, `successful_delivery_increments_push_stats`, `p9_push_service_toggle` |
| IMAP push E2E | `tests/imap_e2e.rs` | `imap_e2e_push_devicetoken_setmetadata`, `imap_e2e_push_disabled_hides_capabilities` |
| SMTP submission | `chatmail-smtp` | `submission_starttls_upgrade_then_auth_allowed` |

Run cmping unit tests: `cd context/cmping && uv run python -m unittest test_cmping_dclogin.py -v`

## 2. Integration Tests
- Use `tokio::test` + test containers or in-memory SQLite
- Test full pipelines:
  - Submission → PGP check → local delivery
  - `/mxdeliv` receive path
  - Admin API authentication + all resources
  - IMAP IDLE + push notifications

## 3. End-to-End Tests (Primary)
Replicate and extend the existing Python test suite (`tests/deltachat-test/`).

**Key Scenarios** (must all pass):
1. Account creation (JIT + /new)
2. Unencrypted message rejection (523)
3. Secure Join + verified contact
4. P2P encrypted messaging
5. Group creation & messaging
6. File transfer (hash verification)
7. **Federation** (cross-server, port blocking analysis)
8. No-Logging verification (`journalctl` check)
9. Large file transfers + quota
10. Binary upgrade mechanism (signature verification)
11. JIT registration
12. IMAP IDLE responsiveness
13. Concurrent profiles
14. Message purging (admin)
15. Iroh discovery + WebXDC realtime
16. Admin API (all major endpoints)
17. Quota enforcement
18. **TURN** — IMAP `GETMETADATA /shared/vendor/deltachat/turn`, credentials work on local turn-rs, Core `ice_servers()` without fallback ([`plans/b9/`](../plans/b9/README.md))

**Test Infrastructure**:
- Python + `uv` + `deltachat-rpc-server`
- Optional LXC mode for clean federation testing (`--lxc`)
- Keep containers alive for debugging (`--keep-lxc`)

## 4. Performance / Load Tests (Future)
- Concurrent IMAP connections + IDLE
- High volume federation delivery
- Quota cache under heavy delivery load

## 5. Security-Focused Tests
- Timing attacks on admin token
- Federation policy bypass attempts (subdomains, IP literals, case)
- PGP structure fuzzing / malformed messages

## Continuous Integration
- GitHub Actions:
  - `cargo test`
  - `cargo clippy`
  - `cargo fmt -- --check`
  - Run core E2E suite on every PR (or nightly for long-running ones)

## Documentation Tests
All public Admin API examples and CLI examples should be validated.

---

## cmdeploy online tests (legacy stack black-box)

Path: `context/cmdeploy/src/cmdeploy/tests/online/`

These tests target a **live deployed** Chatmail host (env `CHATMAIL_DOMAIN`, optional `chatmail.ini`). Historically that stack is **Dovecot + Postfix**, not Madmail or madmail-v2. Use them as an **external behavioural spec** when validating a new Rust IMAP/SMTP server on the same domain.

| File | Validates |
|------|-----------|
| `test_0_login.py` | IMAP/SMTP login, JIT auto-create, password rules, concurrent logins |
| `test_0_login.py` (`test_capabilities`) | IMAP caps: `XCHATMAIL`, `XDELTAPUSH` |
| `test_0_qr.py` | QR / invite flows (HTTP, not IMAP) |
| `test_1_basic.py` | SSH + SMTP send (`swaks`), DNS, deliverability smoke |
| `test_2_deltachat.py` | Delta Chat RPC: 1:1 chat, metadata SET/GET on INBOX |
| `test_3_status.py` | Service health on remote host |

**Run example:**

```bash
export CHATMAIL_DOMAIN=your.test.domain
cd context/cmdeploy && pytest src/cmdeploy/tests/online/test_0_login.py -v
```

**Mapping to TDD:**

| cmdeploy test | Primary TDD section |
|---------------|---------------------|
| Login / JIT | `05-authentication.md`, `03-imap-server.md` |
| `XCHATMAIL` / `XDELTAPUSH` | `03-imap-server.md` |
| Delta Chat send/receive | `02-smtp-server.md` + `03-imap-server.md` + `16-testing.md` (deltachat-test) |

**Gaps:** cmdeploy does **not** cover `523 Encryption Needed`, federation ACCEPT/REJECT, or Admin API — use `context/madmail/tests/deltachat-test/` for those.

When madmail-v2 replaces Dovecot/Madmail, re-run cmdeploy online suite against the new host before declaring protocol parity.

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Suite | Path | Role |
|-------|------|------|
| **madmail E2E (primary)** | [`context/madmail/tests/deltachat-test/`](../../context/madmail/tests/deltachat-test/) | Must-pass scenarios: PGP, federation, JIT, IDLE, Admin API, No-Log |
| **madmail LXC drivers** | [`context/madmail/tests/cmlxc/`](../../context/madmail/tests/cmlxc/) | [`driver_madmail.py`](../../context/madmail/tests/cmlxc/src/cmlxc/driver_madmail.py), [`driver_cmdeploy.py`](../../context/madmail/tests/cmlxc/src/cmlxc/driver_cmdeploy.py) |
| **cmdeploy online** | [`context/cmdeploy/src/cmdeploy/tests/online/`](../../context/cmdeploy/src/cmdeploy/tests/online/) | Black-box against Dovecot/Postfix deploy |
| **cmrelay** | [`context/cmrelay/src/filtermail/`](../../context/cmrelay/src/filtermail/) | Component tests under `python/chatmaild/tests/` |
| **stalwart** | [`context/stalwart/tests/`](../../context/stalwart/tests/) (if present) | Protocol-level Rust tests — not Chatmail-specific |

| Scenario area | madmail test file |
|---------------|-------------------|
| JIT | [`test_11_jit_registration.py`](../../context/madmail/tests/deltachat-test/scenarios/test_11_jit_registration.py) |
| Federation | [`test_07_federation.py`](../../context/madmail/tests/deltachat-test/scenarios/test_07_federation.py) |
| PGP reject | [`test_02_unencrypted_rejection.py`](../../context/madmail/tests/deltachat-test/scenarios/test_02_unencrypted_rejection.py) |
| IDLE | [`test_12_smtp_imap_idle.py`](../../context/madmail/tests/deltachat-test/scenarios/test_12_smtp_imap_idle.py) (if present) |
| Admin API | scenarios under [`deltachat-test/scenarios/`](../../context/madmail/tests/deltachat-test/scenarios/) |

## Related RFCs

E2E and integration tests assert behaviour defined by these specs. Offline copies and per-section mapping: [`RFC/README.md`](RFC/README.md). Regenerate: [`RFC/download-rfcs.sh`](RFC/download-rfcs.sh).

| Area | Primary RFCs (local) | IETF |
|------|----------------------|------|
| SMTP / submission | [5321](RFC/rfc5321.txt), [6409](RFC/rfc6409.txt), [3156](RFC/rfc3156.txt) | [5321](https://datatracker.ietf.org/doc/html/rfc5321), [6409](https://datatracker.ietf.org/doc/html/rfc6409), [3156](https://datatracker.ietf.org/doc/html/rfc3156) |
| IMAP | [3501](RFC/rfc3501.txt), [2177](RFC/rfc2177.txt), [6851](RFC/rfc6851.txt), [5464](RFC/rfc5464.txt) | [3501](https://datatracker.ietf.org/doc/html/rfc3501), [2177](https://datatracker.ietf.org/doc/html/rfc2177), [6851](https://datatracker.ietf.org/doc/html/rfc6851), [5464](https://datatracker.ietf.org/doc/html/rfc5464) |
| Federation | [5322](RFC/rfc5322.txt), [9110](RFC/rfc9110.txt) | [5322](https://datatracker.ietf.org/doc/html/rfc5322), [9110](https://datatracker.ietf.org/doc/html/rfc9110) |
| Security (PGP) | [9580](RFC/rfc9580.txt), [3156](RFC/rfc3156.txt) | [9580](https://datatracker.ietf.org/doc/html/rfc9580), [3156](https://datatracker.ietf.org/doc/html/rfc3156) |
| TURN / calls | [8656](RFC/rfc8656.txt), [8489](RFC/rfc8489.txt), [8445](RFC/rfc8445.txt) | [8656](https://datatracker.ietf.org/doc/html/rfc8656), [8489](https://datatracker.ietf.org/doc/html/rfc8489), [8445](https://datatracker.ietf.org/doc/html/rfc8445) |
| Admin API | [9110](RFC/rfc9110.txt), [8259](RFC/rfc8259.txt) | [9110](https://datatracker.ietf.org/doc/html/rfc9110), [8259](https://datatracker.ietf.org/doc/html/rfc8259) |