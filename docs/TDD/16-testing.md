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

## Docker tests

`make test-postgres-restart-docker` builds Madmail's Docker image and starts a disposable PostgreSQL 18 container. It verifies empty cold start, populated restart, forward migration from the original INT4 federation statistics schema, and the legacy ensure path with Go HTTPS column names. Every counter is checked for preservation and values above the INT4 limit. A PostgreSQL-backed Rust test also exercises runtime tracker updates, flushing, reconnecting, and hydration. Containers and database volumes are removed; logs remain under `target/postgres-restart-tests/`. Requirements: Docker, Cargo, OpenSSL, and curl. To test an existing image, set `MADMAIL_POSTGRES_TEST_IMAGE` and `MADMAIL_POSTGRES_TEST_SKIP_BUILD=1`; `MADMAIL_POSTGRES_TEST_DB_IMAGE` overrides the PostgreSQL image.

`make test-docker` runs the full default Rust workspace suite and landing-site tests inside Docker, then builds the shipping image and installs an isolated self-signed relay. Docker clients check browser-selected DNS/IP account domains, foreign-host fallback, multi-domain JIT login and wrong-password rejection, metrics disabled by default, enable/disable across restarts, custom listen IP/port, HTTP Basic authentication success and rejection, saved CLI credentials, password removal, HTTPS and dashboard serving, admin authentication, SMTP TLS/STARTTLS delivery, IMAP retrieval and login rejection, plaintext rejection, restart persistence, and live IMAP TURN metadata plus UDP allocation. The runner removes its containers and volumes and retains logs under `target/docker-tests/`. See [Docker test instructions](../guide/docker.md#automated-local-docker-tests) for prerequisites, ignored/external tests, and port overrides. The PGP/MIME fixture and TURN allocation checks do not replace real Delta Chat client or media round-trip tests.

`make test-deltachat-docker` runs the existing 11 cmlxc relay_minitest checks and extensive Delta Chat RPC scenarios against two Docker relays, including encrypted messaging, groups, cross-relay delivery, files, and large-file SHA-256 verification. `make test-full-docker` combines both Docker suites. See [extensive Docker tests](../guide/docker.md#extensive-delta-chat-and-cmlxc-tests-in-docker) for selection, artifacts, and the explicit signing-key/Go-camouflage/LXC-exchanger exclusions.

## 1. Unit Tests

- `cargo test`
- Focus on pure logic:
  - PGP MIME/header policy (`chatmail-pgp::enforce_encryption` — Secure-Join, bounces, PGP-MIME)
  - Quota cache concurrency safety
  - Federation policy evaluation + normalization (IP brackets, case)
  - **Federation HTTP body limit** (`FederationSizeLimit`, Axum `DefaultBodyLimit` on `/mxdeliv`, default 70M)
  - Endpoint cache resolution
  - Settings toggle logic
  - **Auth cache** (credentials, blocklist, JIT flag hydrate + write-through)
  - **dclogin URL** shape (`build_dclogin_link`, `POST /new`, cmping IP URLs)
  - **STARTTLS** (IMAP LOGIN gate, SMTP AUTH-after-TLS, TLS cert load for plain listeners)
  - **Autoconfig** (SSL/STARTTLS entries; HTTPS ALPN entries only when `alpn_imap`/`alpn_smtp` are configured)

### madmail-v2 unit test index (parity fixes)

| Area | Crate / path | Key tests |
|------|--------------|-----------|
| dclogin / `/new` | `chatmail-config`, `chatmail-www` | `build_dclogin_link_matches_www_shape`, `new_account_returns_dclogin_url_with_ssl_hints` |
| Custom www migrate (Go→Minijinja + legacy `/qr`) | `chatmail-www`, `chatmail` ctl | `www_migrate::*`, `e2e_html_migrate_go_templates_and_legacy_qr`, `e2e_html_migrate_warns_on_literal_brace_url` |
| cmping IP setup | `context/cmping/test_cmping_dclogin.py` | `test_ip_dclogin_includes_ssl_host_hints` |
| Auth cache + JIT | `chatmail-state`, `chatmail-auth` | `hydrate_loads_blocklist_and_jit_flag`, `jit_coalesces_concurrent_creates_for_same_user` |
| TLS for STARTTLS | `chatmail-config` | `listeners_need_tls_cert_for_starttls_only_ports` |
| Autoconfig | `chatmail-config`, `chatmail-www` | `autoconfig_omits_https_alpn_when_not_configured`, `p12_ut12_autoconfig_advertises_alpn_https_when_enabled` |
| Shared HTTPS port (ALPN demux) | `chatmail` | `p12_it01_one_port_serves_imap_smtp_and_http` (real TLS: `imap`/`smtp`/no-ALPN over one listener), `p12_it02_unoffered_alpn_is_refused` (strict ALPN / ALPACA), `p12_ut16_legacy_directive_value_never_reaches_the_wire` |
| IMAP caps (pre/post-auth) | `chatmail-imap` | `p5_ut01_test_capability_includes_chatmail_extensions` — pre-auth omits `XCHATMAIL`/`XDELTAPUSH`/`METADATA`; post-auth includes chatmail tokens (#120) |
| Push notify | `chatmail-push`, `chatmail-admin` | `push_mode_and_circuit_breaker`, `successful_delivery_increments_push_stats`, `p9_push_service_toggle` |
| IMAP push E2E | `tests/imap_e2e.rs` | `imap_e2e_push_devicetoken_setmetadata` (post-auth caps), `imap_e2e_push_disabled_hides_capabilities`, `imap_e2e_greeting_and_capability` |
| TURN CAPABILITY | `tests/turn_e2e.rs` | `turn_imap_e2e_capability_metadata` — `METADATA` only after LOGIN |
| SMTP submission | `chatmail-smtp` | `submission_starttls_upgrade_then_auth_allowed` |
| Federation body limit | `chatmail-fed`, `chatmail-state`, `chatmail-config`, `chatmail-admin` | `p7_ut04`–`p7_ut06`, `federation_size_*`, `admin_federation_size_*` |
| Bigfile roundtrip E2E | `tests/deltachat-test` | `test_23_bigfile_roundtrip.py` (5 MiB SHA-256, two relays, &lt;60s) |

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
18. **TURN** — IMAP `GETMETADATA /shared/vendor/deltachat/turn`, credentials work on local webrtc-rs TURN (`chatmail-turn`), Core `ice_servers()` without fallback ([`plans/b9/`](../plans/b9/README.md))

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

## Release automation

`python3 -m unittest discover -s scripts -p 'test_*.py'` covers Conventional Commit version selection, manifest and changelog updates, dry runs, reruns, rejected Git pushes, unsigned prerelease retries, and registry checks. The tests use temporary repositories and fake GitHub or registry responses. They do not publish tags, images, or releases. See [release automation](release-automation.md).

## 6. Network-Condition Smoke Test (manual, off-CI)

The shared-port demux (443 serving HTTPS + IMAP + submission) exists for networks
that block 993/465 and inspect TLS. CI cannot exercise that: it needs a client
sitting on such a network, so it is a manual run with a control.

[`tests/highdpi-smoke/`](../../tests/highdpi-smoke/README.md) holds the harness:

| Tier | What it establishes |
|------|---------------------|
| A | Which ports are reachable, and that the demux routes ALPN / SNI / first bytes correctly — classifying failures as blocked port, reset after ClientHello, alert 120, or cert mismatch |
| B | Whether a given ClientHello shape is singled out, by comparing kill rates across seven shapes against the same IP and minute |
| C | Authenticated IMAP LOGIN, IMAP IDLE survival over minutes, and submission upload throughput |
| D | A real Delta Chat client pinned to `:443` (`deltachat-rpc-client`) |

Rules that make the output meaningful:

- Always pair a run on the network under test with a control run from an
  unfiltered host; a lone report cannot separate filtering from misconfiguration.
- Tiers A and B accept any certificate on purpose (they measure routing); tier C
  verifies it, because credentials travel on that socket.
- `A6` (SNI routing) reports SKIP unless the certificate covers `imap.<domain>`.
  `madmail install` orders a single-name certificate, so that path needs a
  certbot cert with the extra SANs — see `server-setup.sh`.

The harness is validated against a local instance on high ports (recipe in the
README); a correct build gives 13 PASS / 0 FAIL in tier A.

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
| `test_0_login.py` (`test_capabilities`) | IMAP caps: `XCHATMAIL`, `XDELTAPUSH` (cmdeploy/Dovecot often pre-auth; madmail-v2 exposes these **post-auth only** — #120) |
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
| `XCHATMAIL` / `XDELTAPUSH` (post-auth on madmail-v2) | `03-imap-server.md` (#120 pre-auth fingerprinting) |
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

## Operator webhook coverage

`cargo test -p chatmail-state webhooks` tests signatures, policy/gating, secret
redaction, bounded delivery/dedup, timeouts/retries, redirect rejection and reload.
`cargo test -p chatmail-integration --test operator_webhooks_e2e` triggers actual
admin, web and JIT registrations and IMAP/SMTP quota rejections through a mock
receiver, verifies concurrent admin imports emit one event, verifies authorization
and checks that public pages expose no webhook
settings. See [the design](25-operator-webhooks.md) for the complete contract.

### JIT domain retirement and CLI provisioning

`cli_creation_and_existing_logins_survive_jit_domain_restriction` exercises the
real account-creation CLI with a multi-domain config, explicit DNS/IP addresses,
and a bare username defaulting to the first primary domain. It then narrows JIT
to one domain and verifies existing accounts continue authenticating through
both database fallback and the hydrated cache, incorrect passwords stay
rejected, and unknown accounts on retired domains are never created. Explicit
admin provisioning remains independent of the first-login JIT allowlist.

## Configuration safeguards (#217)

See [the test matrix](26-configuration-precedence.md#verification-plan). Coverage includes parser parity, explicit false, invalid input, single/batch/list resolution, protected set/reset, compound mutation preflight, service snapshot isolation, retention consumers, admin path/auth routing, and UI source metadata.

### Live configuration-source matrix

`python3 tests/configuration-sources-docker.py --image madmail-local:test-docker`
checks the shipping image with both file formats and environment overrides. Build
the image from the current source first. The script reads the actual examples from
[the configuration guide](../guide/configuration.md), seeds conflicting DB values,
and checks precedence, explicit false, CLI/API protections, restart, restored DB
values, and invalid inputs. Containers and volumes are isolated and removed.

## Custom-path installer regression (#216)

Cover mutually exclusive `--system`/`--local`, legacy automatic selection, explicit
custom-path system mode, skip flags, absolute-path validation, Windows rejection of
Unix system mode, custom systemd paths and RuntimeDirectory. CLI dry-run integration
tests assert action reporting and absence of filesystem writes. Verify real account,
ownership, binary and unit installation in an isolated container; never exercise
host provisioning against the developer machine as a test.

Run the provisioning regression with `cargo build -p chatmail`, followed by
`bash tests/install-system-docker.sh`. The script builds a disposable Debian 13
image and mounts only the test binary read-only. It validates real account creation,
config/DB ownership, binary installation, unit syntax (`systemd-analyze verify`),
boot as the service user, repeated installation and independent skip flags. It
stubs only `systemctl daemon-reload` because systemd is not PID 1; it does not verify
an actual systemd service start.

Verification for #216: full workspace 1,067 passed / zero failed / three ignored;
final installer subset 17 passed, plus the CLI dry-run integration test; strict
workspace clippy, fmt, man-page generation/groff, and the Docker provisioning
regression passed. Windows-specific behavior is covered by a cfg-gated regression
for the Windows CI runner; it was not executed locally.

### Expanded installer E2E matrix

`make test-install-docker` builds the installer with dev debug symbols and incremental
compilation disabled and two build jobs, then runs the isolated Debian test. This
limits local build storage and memory without changing release or normal dev profiles.
The image receives only its Dockerfile as build context; source/build directories
are not uploaded to Docker. The binary is mounted read-only for the test and must
be a Linux ELF build; the script refuses other binaries (such as a macOS host build)
before starting Docker, so on macOS set `MADMAIL_INSTALL_TEST_BINARY` to a Linux
build. CI does not run this suite.

Coverage includes flag conflicts; automatic FHS/custom and explicit modes; JSON
plans (including certificate-only and skip flags); rejected relative/control-character
paths; dry-run side-effect absence; root enforcement; root and non-root local
bootstrap; real account, ownership, binary and host-doc installation; generated-unit
verification; boot as the service user; repeated installation; each skip flag
independently and combined; an escaped binary path; a state directory with a
space (verbatim `WorkingDirectory=`, quoted `ReadWritePaths=`); and propagation of
daemon-reload failure. Negative assertions explicitly fail the suite when an unexpected user or
unit directive is present. An actual systemd PID-1 start remains outside this test;
only daemon-reload is stubbed. Windows service installation remains a separate flow.

Expanded matrix verification: `make test-install-docker` passed after a clean
compact build, and the installer unit tests (17 at the time) passed with the same
compact profiles. After the #220 review fixes there were 19 `ctl::install` unit tests
and 18 `ctl::uninstall` unit tests. The test Docker build context was 2 KiB. No production/release profile
settings were changed. Failure-path checks include invalid mode/path input,
non-root system installation, and a nonzero daemon-reload result.

Installer repair coverage (`make test-install-docker`) additionally checks
`--user-only` and `--systemd-only`: no-domain offline operation, incompatible flags,
absolute/control-character path validation, non-root rejection, non-root dry-run
isolation, missing installation/binary/account prerequisites, deleted-account and
unit restoration, repeated execution, JSON actions, preserved config/TLS/DKIM/DB/
binary hashes, unchanged accounts and file modes during unit-only repair, no unit
or daemon changes during user-only repair, and propagated daemon-reload failures.
It also repoints an account whose home differs at the existing state directory
(home, config `0640` and state ownership asserted), checks that links planted in the
state directory to `/etc/shadow` and `/etc`, and a TLS path through a linked
directory there, leave their targets' ownership unchanged, and refuses a masked
(`/dev/null`) or otherwise symlinked unit without writing through it or reloading
systemd. A full install over a masked unit fails before changing accounts, file
modes or contents, and a unit written under `umask 077` is still `0644`. A full
install into a pre-created state directory, while the account's home still holds
data elsewhere, is refused with the home and data left in place. These cases passed
in the Docker suite with a Linux (aarch64 musl) build of the #222 review fixes; the
pre-created state directory case fails against the build before them. Unit tests
cover the `usermod` arguments (no `-m` into an existing state directory, full-install
refusal to abandon a previous home), the descriptor-based `chown_tree` including a
state directory root reached through a link, planted-link detection on any component below the state
directory, symlink refusal, mode and atomic replacement of the unit file, and repair
`from_args` without a domain.

The state-tree and uninstall path-safety fixes add Docker cases for links planted
in nested state directories (`nested/deeper/planted-passwd -> /etc/passwd`,
`nested/planted-etc -> /etc`), which keep their targets' owners while the real
directories get the service owner; for a `--cert-path` outside the state
directory that reaches the tree through an operator link and ends in a link the
service planted to `/etc/shadow`, which is skipped with a `Skipping ownership of`
note and leaves `/etc/shadow` unchanged; and for `uninstall` after an install with
`--config-dir /usr/local/etc`. That last case checks the dry-run plan and the
`left_in_place` JSON, then that uninstall removes `madmail.conf`, the dedicated
state directory, unit, binary and account, and keeps `/usr/local/etc`, an
unrelated `other.conf` and the generated `certs/` files it reports. Unit tests
cover the descriptor walk (nested links, a root replaced by a link, entries that
change or vanish, the depth limit, listing without `.`/`..`), service-tree
detection (a link in an intermediate component, canonical paths under a
symlinked state root, an ancestor resolving into the tree with a final link out
of it), the canonical-equal passwd home, and uninstall classification (dedicated
directories removed whole, a shared config directory losing only its config file,
denied system and home directories, and a shared `WorkingDirectory=` kept). There
were 32 Unix `ctl::install` unit tests and 22 `ctl::uninstall` unit tests after
those fixes.

The full Docker suite, including the space-containing state directory case, passed
with a Linux aarch64 musl debug build of these fixes (`PASS: complete Unix
installer E2E matrix`). Run against a binary built before them, it fails at the
`--cert-path` case.

The descriptor-based install and uninstall fixes (checked path resolver in
`ctl/secure_fs.rs`, shared naming rule in `ctl/host_paths.rs`) add Docker cases
for: a file outside the tree, owned by root and hard-linked into the state
directory (`hl-planted`), which keeps `root:root` and mode `0600` and is reported as
`Ownership of ... left unchanged: belongs to uid 0, not to the service account
and has 2 hard links`; a planted
link to `/etc/shadow`, after which the hash, owner and mode of `/etc/shadow` are
unchanged; account repair with a new uid while another account holds the old one,
after which the service's own file with two hard links belongs to the new uid; an
operator link from `/var/xyzzy/etc/madmail/certs/fullchain.pem` to a file in the
state tree that the service account replaced with a link to `/etc/shadow`, through
a full reinstall and an account repair, after which the hash, owner and mode of
`/etc/shadow` are unchanged and repair notes
`Skipping ownership of ...: inside the service-writable state directory`; and an
uninstall dry run with `--state-dir /srv/shared-data`, which leaves that directory
in place as a shared directory. Unit tests cover the trust rules, refusal of a
service-owned directory or link anywhere on a path (also on a second link hop and
when the target is outside the tree), the hop limit, recognition of the state
directory by device and inode, writes in the tree that replace planted links
instead of following them, directory creation only in trusted directories, the
hard-link decision for the walk (links skipped, foreign hard links skipped, the
service's hard links changed for the current and the previous uid), TLS files
reached through the tree, config files outside the tree behind a service
directory, directories that are not dedicated to madmail, and, for uninstall, a
config `state_dir` naming a shared directory, a `state_dir` from a world-writable
directory that is ignored, `--keep-data` with a config directory that is or
contains the state directory, `--keep-config` with a state directory that
contains the config, and removal that refuses a path through a world-writable
directory and removes a link instead of its target.

The follow-up fixes (the walk changes a non-directory only when it belongs to the
current or previous service uid; the binary and
`--skip-user` writes go through descriptors; supplementary groups count as service
groups) add Docker cases for: `/etc/shadow` hard-linked into the tree and then
renamed over by a copy, so the inode has one link, after which `/etc/shadow`
stays `root` and repair prints `left unchanged: belongs to uid 0, not to the
service account;`; account repair with a new uid on a pre-created account; a
reused uid (the old service uid given to another account, `uidsquatter`), after
which install then printed `belonged to uid <n>, which is now account uidsquatter`
and left that uid's files unchanged (the later fixes below refuse that state
directory instead, until the operator gives it to root); `--binary-path` inside the state directory,
which is refused before anything changes; `--skip-user` with a DNS domain and the
account present, where `dkim/default.txt` is planted as a link to `/etc/shadow`:
the hash, owner and mode of `/etc/shadow` are unchanged, the record is written
(`v=DKIM1`), the config file is `root:madmail` mode 0640, and a later normal
install gives the record to `madmail`; `--skip-user` with no account and a state
directory owned by `nobody`, which is refused (`does not exist, but state
directory`); and uninstall with `/var/log` set to a non-root group with mode 0775
(as on Ubuntu, `root:syslog`), which still removes `/var/log/madmail`. New unit
tests cover the walk decision (foreign files skipped whatever their link count),
supplementary groups, root-only trust of directories
group-writable by system groups, a config path outside `--state-dir` that
resolves into it, a madmail-named link to a shared directory, a `--binary-path`
inside or resolving into the state tree, keep-flag containment through links, the
`Removed link` message, and removal under a parent writable by a system group.

The next fixes remove the list of files install wrote in the run: the walk gave
any such inode away, so a hard link the service account made into the tree to
the config file root had just written outside it (`/etc/madmail/madmail.conf`)
handed that file to the service. Files install writes inside the tree now get the
service account as owner on the new descriptor, the walk changes only files of
the current or previous service uid and directories of those or root (another
account's directory is reported and not entered), the state directory itself is
changed last, the config file's owner and mode are set after the walk, in-tree
reads require a service-owned file (on Linux opened with `O_PATH` and reopened
through `/proc/self/fd`), and the parents of `--state-dir` are created with the
checked resolver before `useradd`. Docker cases: the config file hard-linked into
the tree before a reinstall, and a loop that waits for the walk to change the
ctime of a sentinel file and then hard-links the new config file into 400
directories while the walk runs; afterwards the config file is `root:madmail`
mode 0640, no linked name belongs to `madmail`, and install either succeeds or
stops with `refusing to change config file ...: it has <n> hard links`; a run that fails at the certificate step
(`--tls-mode file` without the files) leaves `dkim/default.private` as
`madmail:madmail` mode 0600, and after the next run `madmail` can still read it;
root-owned TLS files under `--state-dir` with `--tls-mode file`, for which install
prints `Skipping ownership of <path>: inside the state directory and belongs to
uid 0, not to the service account` and a command to change its owner, and
leaves the key `root:root` mode 0600; and a config directory under
`--state-dir`, where the config file is `madmail:madmail` mode 0640 after an
install and still after a `--skip-user` reinstall. The repair case with a planted
link from the config directory into the tree now expects `a link inside the
state directory; links there are not followed`. New unit tests cover the walk
decision for directories (root and service directories changed, another
account's not entered), a config file
written outside the tree and hard-linked into it (not changed by the walk, then
refused for its two links), the in-tree owner on creation and its `--skip-user`
rule, and in-tree reads of another account's file. There are 44 `ctl::install`,
30 `ctl::uninstall`, 10 `ctl::secure_fs` and 1 `ctl::host_paths` unit tests on a
Unix host after these fixes.

The next fixes refuse, before `useradd` or `usermod`, a `--state-dir` that is a
system, top-level or home directory (also through a link) or that belongs to
another named account; print every path escaped and every printed command
shell-quoted, with steps that stop the service and its processes before
`chown -h`, and say that another account's directory was not entered; give an
operator's TLS file of another account to the service only in a madmail or Let's
Encrypt directory; make uninstall fail closed on groups it cannot read, refuse a
hard-linked config file and group-writable directories without the sticky bit
(except `/var/log` and `/var/mail` by device and inode), remove trees through
descriptors, leave a path whose way is not trusted in place and go on, and let a
config directory inside the state directory go with it. New unit tests cover the
state directory refusals, `shown` and `shell_quote` (no raw control bytes, JSON
escaping), the TLS directory rule, the recorded chown order of the walk (the root
is changed once and last, and not at all after a failure in the middle, through
an injected `chown_walk::Chown`), descriptor-based tree removal, the uninstall
trust groups and allowlist, refused removals and in-tree config planning. The
order in which install runs `setup_permissions` (the walk) before
`setup_config_permissions` (the config file's final owner and mode) is covered
only by the Docker suite, through the config file hard-linked into the tree during
the walk. Docker cases: the refused state directories (`/var/lib`, `/`, `/etc`,
`/data`, `/home/alice`, a link to `/var/lib` and another account's directory) with
the account and owners unchanged, the squatted state directory given to root and
then repaired, a root-owned file whose name has a newline (printed as `\n`, with
`$'...\x0a...'` in the command), another account's directory (not entered, no
command), a root-owned operator TLS file in `/srv` (left `root:root` with the
command printed) and one in `/etc/letsencrypt/archive` (changed), and an
uninstall with `--config-dir` inside the state directory (listed as kept with
`--keep-data`, removed with the state directory otherwise). The `--skip-user`
case without an account now expects the refusal of a `nobody`-owned state
directory by account name, and the old refusal for a state directory of a uid
with no name. There are 47 `ctl::install`, 33 `ctl::uninstall`, 11
`ctl::secure_fs` and 3 `ctl::host_paths` unit tests on a Unix host after these
fixes.

The next fixes look up account names with `getpwuid_r` (BusyBox `id` takes no
numeric uid, and an NSS failure looked like "no account") and refuse a state
directory whose owner cannot be looked up; stop install when the service
account's groups cannot be read; refuse a state directory below `/etc`, `/usr`
and the other system directories unless it is named for madmail; print steps
that also lock and stop the account that now has a file's uid, that say a link
count of 1 proves nothing, and that end with the command; print no command for
a file inside the tree reached through another account's directory; escape the
paths of uninstall's JSON `left_in_place`; and in uninstall turn off the
`/var/log` and `/var/mail` allowance when groups cannot be read and leave a path
whose removal fails in place. New unit tests cover `account_name` and
`account_groups_checked` (a missing account against a lookup error), a lookup
error that refuses the state directory (`check_state_root_with` with an injected
lookup), the system-tree rule (`/etc/systemd`, `/lib/systemd/system` refused,
`/etc/madmail/data`, `/srv/m2` allowed), the printed steps (`file_chown_hint`
and `in_tree_file_hint`), JSON escaping of DEL, C1 and bidirectional characters
through `shown`, an injected group lookup error in `uninstall_state_dir_with`
and `set_service_groups`, the allowance turned off by `without_group_dirs`, and
a removal that fails on a read-only directory. Docker cases: `--state-dir
/etc/systemd` and `/usr/lib/systemd/system` refused by install and by
`--user-only` with passwd, group and the owners and modes of `/etc/systemd`,
`/etc/systemd/system` and `/usr/lib/systemd` unchanged; whole printed lines
for the walk's and the in-tree TLS steps (`grep -qxF`); the steps for a file of
the reused uid naming `uidsquatter`; and the documented recovery (lock the
account, check `/proc` for its processes, list with `find -xdev -uid <n> -ls`,
then change directories and single-link files). There are 48 `ctl::install`,
35 `ctl::uninstall`, 12 `ctl::secure_fs` and 3 `ctl::host_paths` unit tests on
a Unix host after these fixes. Uninstall's removal error path and the
unreadable-groups path are covered only by unit tests.

The next fixes keep the in-process `getpwuid_r` lookup (BusyBox `id` takes no
numeric uid) and add `getent passwd` when it finds no entry. The release and
test binaries are linked statically with musl, whose `getpwuid_r` and
`getpwnam_r` read only `/etc/passwd` and never ask NSS modules, so without
`getent` an LDAP or sssd account looked like a uid without a name and its
directory was taken for the previous service uid's. `getent` uses the host's
NSS configuration; a host without it (BusyBox) has no NSS modules either. The
trade-off is that `getent` exits with 2 both for a missing entry and for an NSS
service that is down, so that case still looks like "no account". Only
`ENOENT` and a NULL result from the C library now mean "not found" (`EPERM`,
`EBADF` and `ESRCH` are errors). The fixes also refuse a root-owned state
directory that is not empty and not named for madmail (unless `--skip-user`),
a dangling `--state-dir` link into a system tree, a groups lookup that finds no
account `id` found, and a state directory that changes between the two opens
in `SecureInstall::open`; reword the refusal of a config path that resolves
into the state directory; print steps that lock and expire each account,
remove its cron and at jobs and kill its processes with `SIGKILL`; print
`chown -h` for an operator's TLS file; give uninstall's JSON `left_in_place`
entries the raw `path` and an escaped `path_shown`; and build the uninstall
trust accounts with one helper. New unit tests cover `parse_getent` (exit
status 0, 2 and others), the lookup codes that are now errors (also through
`check_state_root_with`), `service_groups_with` with an injected lookup, the
non-empty root-owned rule (refused, empty, named for madmail, `--skip-user`),
the dangling link to `/usr/newstate-for-test`, the config refusal for a
`--state-dir` link to the physical config path, the new printed steps and
`trust_accounts`. Docker cases: `--state-dir /var/lib/dpkg` refused by install
and by `--user-only` with passwd, group, shadow and the owner, mode and inode
of `/var/lib/dpkg` and `/var/lib/dpkg/info` unchanged; a dangling link
`/srv/m2` to `/etc/ssl/newstate` refused with nothing created; a state
directory of `ldapuser`, an account served only by `libnss-extrausers`
(checked with `getent passwd 4242` first), refused by install and by
`--user-only` with its owners unchanged; whole printed lines for the new
steps; the recovery with `usermod -L -e 1970-01-02`, a third account's directory in the
tree and the `find ... -prune` recipe, which leaves that directory's file
unchanged; and `path_shown` in uninstall's JSON. There are 51
`ctl::install`, 36 `ctl::uninstall`, 13 `ctl::secure_fs` and 3
`ctl::host_paths` unit tests on a Unix host after these fixes.

The next fixes let install take a state directory it made itself. A
`--cert-only` run, or an install that stopped at the certificate step, leaves a
root-owned state directory with `messages/`, `dkim/` and so on when install
created it (an existing account whose home is the missing directory), and the
rule above refused that directory on the next run when it was not named for
madmail; a fresh file system holding only `lost+found` was refused too. Install
now creates a root-owned `.madmail-state` file in a state directory it creates
or in a root-owned one it accepts, before anything else, and takes a directory
that holds only a root-owned `lost+found` or has that file as a root-owned
regular file with one link; the walk leaves it root's. The madmail-name
exemption looks at the physical path only, and the rule applies to every owner
other than the service uid (so a deleted package account's directory, owned by
a uid without a name, is refused unless marked). `getent` is run as
`getent passwd -- <key>` from `/usr/bin`, `/bin`, then `PATH`; its entry must be
the one asked for, and a missing `getent` means "no account" only when
`/etc/nsswitch.conf` is missing or lists only `files` for passwd and group. The
printed steps quote account names for the shell and escape them in the text,
use `usermod -L -e 1970-01-02`, stop the systemd user manager
(`loginctl terminate-user`), say that an account known only to a directory
service must be locked there, and, for the account that now has the old uid,
say to note its expiry with `chage -l` first and to restore it and unlock the
account (`usermod -U`) afterwards. The `find` recipe in the docs uses `-H` and
also prunes directories a group or others can write. New unit tests cover
`ensure_dir_created`, the marker, `lost+found`, hard-linked, linked and
directory markers and the name as given versus physically, `parse_getent`
entries of another account, `nsswitch_is_files_only` and quoted account names.
New Docker cases: a madmail-named link `/srv/madmail` to `/var/lib/dpkg` and a
non-empty `/var/lib/oldapp` of uid 4999 refused; `--cert-only` into
`/srv/mailstate`, then a full install, a repair, and a repair after the account
is recreated with a new uid, all taking it through its root-owned marker (which
the walk leaves root's without a note); a marker of the service account in the
directory, once it is root's, refused with the "interrupted" advice; and a
fresh file system with only `lost+found` accepted and marked by a run that stops
at the certificate step, then taken by a full install; the uid-squatter recovery
also prunes a group-writable directory. There are 52 `ctl::install`, 36
`ctl::uninstall`, 14 `ctl::secure_fs` and 3 `ctl::host_paths` unit tests on a
Unix host after these fixes.

The full Docker suite passed with a Linux aarch64 musl debug build of these fixes.
Its output was:

```text
PASS: mode conflicts, automatic/explicit modes, JSON plans, invalid paths and certificate-only plan
PASS: dry-run side effects, non-root system rejection and local host isolation
PASS: missing parents, home paths, migration data preservation, failed-move rollback and pre-created state refusal
PASS: system, top-level, home, system-tree (such as /etc/systemd) and linked system directories and another account's directory are refused as state directory before the account or any owner changes
PASS: a non-empty root-owned directory not named for madmail (/var/lib/dpkg), a dangling link into a system tree and a directory of an NSS-only account are refused as state directory before the account or any owner changes
PASS: a madmail-named link to another program's directory and a non-empty directory of a uid without a name are refused, a --cert-only state directory not named for madmail is taken by the next install and by repair (also after the account is recreated) through its root-owned .madmail-state marker, which the walk leaves root's (a service-owned one does not count), and a fresh file system holding only lost+found is accepted and marked
PASS: custom-path account, ownership, binary/docs, unit verification, service-user boot and idempotence
PASS: each skip flag independently and combined, JSON results, escaped binary and state paths
PASS: non-root local install and propagated systemd failure
PASS: isolated repair conflicts, prerequisites, dry-run, account/unit restoration, home repointing, planted-link and masked-unit refusal, unit mode, content preservation, repeatability and failure propagation
PASS: state walk skips foreign files (hard-linked or renamed), planted links and files of a reused uid, follows a new service uid, prints escaped names with quoted commands, does not enter another account's directory, never writes through an operator link into the tree, chowns operator TLS files only in madmail or Let's Encrypt directories, and --skip-user writes through descriptors
PASS: config hard-linked into the tree (before and during the walk) stays root:madmail 640, DKIM keys survive a failed run, root-owned in-tree TLS files are reported, and an in-tree config keeps the service owner with --skip-user
PASS: uninstall keeps a shared config directory and its other files and a shared state directory, removes the dedicated state directory
PASS: uninstall removes a config directory inside the state directory with it, lists it as kept with --keep-data, and does not stop
PASS: complete Unix installer E2E matrix
```
Systemd uses the same recording stub and real unit syntax verification as the full
installer suite; running a service under systemd PID 1 is not covered here.

## Raw IMAP/SMTP WebSocket transport

See [the bridge TDD](26-websocket-mail-bridge.md) and [operator/client usage](../guide/cli/websocket-bridge.md). The bridge is opt-in, TLS-only, and reuses native authenticated mail sessions. Coverage lives in `tests/websocket_bridge_e2e.rs`; run the isolated Docker scenario with `bash tests/websocket-bridge-docker.sh`.
