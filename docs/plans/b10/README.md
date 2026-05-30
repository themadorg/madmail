# Phase 10 — Concurrency Hardening: Reliable Notifications & Media Body Delivery (b10)

## Goal

Fix the two production issues identified through deep analysis and reference comparisons (Dovecot/cmrelay, original Go madmail, Stalwart):

1. **Message loss / never-delivered under concurrency** — EventBus drops, notification-before-durability races, partial fan-out in group delivery, subscription windows.
2. **Images and videos do not load correctly** — Inefficient `read_blob` (full readdir + full load on every FETCH), UTF-8 corruption on binary bodies, lack of body path caching and range support, no streaming for large literals.

These directly impact 60-person groups with media attachments (the exact workload exercised by `context/cmping -g 60`).

This phase builds on:
- b2 (storage / blobs)
- b4 (SMTP delivery & fan-out)
- b5 (IMAP FETCH / APPEND)
- b6 (EventBus + IDLE notifications)

## The Two Things (Core Problems)

**Thing 1: Reliable concurrent notification + durable delivery fan-out**
- Silent drops on broadcast full
- Notify before file is durable (no dir fsync)
- Partial success in `deliver_local_messages` + hardlinks
- Remaining IDLE subscription gaps

**Thing 2: Correct + efficient large binary body serving**
- `read_blob` always does full directory scan + full `fs::read`
- `str::from_utf8(...).unwrap_or("")` on PGP-encrypted / MIME binary bodies in FETCH
- No body path caching (only listing cache exists)
- No range/partial body support
- Everything fully materialized in RAM (no streaming)

## TDD index

- [docs/TDD/README.md](../../TDD/README.md)
- [03-imap-server.md](../../TDD/03-imap-server.md) (IDLE, FETCH)
- [04-storage-layer.md](../../TDD/04-storage-layer.md) (blobs)
- [02-smtp-server.md](../../TDD/02-smtp-server.md) (delivery + notify)

## Implementation status (code + unit tests)

Phase 0 + Phase 1 (correctness, durability, reliability, media efficiency foundations) are
implemented with unit tests. The mandatory live validation (`relay-ping` protocol matrix and
`cmping --reset -c 1 -g 60` against a test server) **must still be run by an operator** — it
cannot run in CI / offline and is the gate for marking each stage fully complete.

| Step | Status | Code + unit tests |
|------|--------|-------------------|
| P10-S05 (binary body FETCH) | ✅ code + `p10_ut05_fetch_binary_body_roundtrip` | FETCH builds a single `Vec<u8>`; body/header sections appended as raw bytes (no `str::from_utf8`); response written + flushed once. Fixes media corruption + literal desync. |
| P10-S02 (durability + post-commit notify) | ✅ code + storage suite | `fsync_dir` after every `rename`/`hard_link` in `write_blob_mailbox`, `link_into_inbox`, `write_message`, `move_message`. SMTP/IMAP/router notify only after the durable write. |
| P10-S03 (partial fan-out) | ✅ code + `p10_ut03_partial_fanout_reports_per_recipient`, `deliver_local_messages_all_failed_is_error` | `deliver_local_messages` returns `DeliveryOutcome { delivered, failed }`; callers notify only durably-delivered recipients and log failures. Hard error only when all recipients fail. |
| P10-S01 (EventBus backpressure) | ✅ code + `p10_ut01_notify_without_subscriber_is_tracked`, `p10_ut01_slow_receiver_lags_then_resyncs` | No-receiver drops counted (`no_receiver_drops()`), `subscriber_count()`, capacity 64→256, version bump before send so drops stay recoverable. |
| P10-S04 (body path cache) | ✅ code + `p10_ut04_read_blob_known_direct_and_fallback` | `MailMessage` carries the listing-discovered filename; `read_blob_known` opens the body directly (falls back to the scanning `read_blob`), removing the per-FETCH readdir thundering herd. |
| P10-S06..S09 | ⏳ not yet implemented | External blob store, range/streaming body serving, centralized push manager, durable modseq — larger architectural changes (Phase 2/3). |
| P10-S10 (E2E) | ⏳ pending live validation | Run after operator validation of S01–S05. |

### Live validation still required (per stage, by operator)

```bash
# Protocol correctness
context/relay-ping/bin/relay-ping -test connectivity -domain https://<server>/ -log-file - -vv
context/relay-ping/bin/relay-ping -test dclogin   -domain https://<server>/
context/relay-ping/bin/relay-ping -test throughput -count 5

# 60-person group with media (the killer workload)
cd context/cmping && uv run cmping --reset -c 1 -g 60 -i 0 https://<server>/
# Record: 60/60 notification success, media byte integrity, p95 fetch-cycle time, any loss.
```

## Steps (Staged with Mandatory Validation at Each Stage)

Each stage **must** include:
- Unit tests (new or extended P*-UT* tests)
- Protocol correctness via `context/relay-ping` (connectivity, dclogin, throughput where relevant)
- Real-world 60-person group validation via `context/cmping --reset -c 1 -g 60 -i 0 <server>` (or equivalent) **before** marking the stage complete

| Step | File | Summary | Validation Tools (mandatory at stage end) |
|------|------|---------|-------------------------------------------|
| P10-S01 | [P10-S01-eventbus-backpressure.md](P10-S01-eventbus-backpressure.md) | Bounded backpressure + overflow detection in EventBus | cargo test, relay-ping connectivity + dclogin, cmping -g 60 (notification success rate) |
| P10-S02 | [P10-S02-post-durability-notify.md](P10-S02-post-durability-notify.md) | Notify strictly after commit + directory fsync | cargo test, relay-ping, cmping -g 60 + crash/recovery test |
| P10-S03 | [P10-S03-partial-fanout.md](P10-S03-partial-fanout.md) | Per-recipient tracking + graceful partial delivery in group fan-out | cargo test, relay-ping throughput, cmping -g 60 (partial success logging + integrity) |
| P10-S04 | [P10-S04-body-path-cache.md](P10-S04-body-path-cache.md) | Body location/path caching on top of inbox_version | cargo test, relay-ping, cmping -g 60 with media (reduced readdir count) |
| P10-S05 | [P10-S05-binary-body-fetch.md](P10-S05-binary-body-fetch.md) | Fix UTF-8 corruption; treat bodies as raw bytes in IMAP FETCH | cargo test (binary roundtrip), relay-ping dclogin with attachments, cmping -g 60 media integrity |
| P10-S06 | [P10-S06-external-blob-store.md](P10-S06-external-blob-store.md) | Introduce ExternalStore trait + FS backend (Go madmail model) with Link + Sync | cargo test, relay-ping, cmping -g 60 (hardlink efficiency + durability) |
| P10-S07 | [P10-S07-range-body-serving.md](P10-S07-range-body-serving.md) | Range/partial body reads + streaming support for large literals | cargo test, relay-ping, cmping -g 60 large media (lower memory, faster partial fetches) |
| P10-S08 | [P10-S08-centralized-push.md](P10-S08-centralized-push.md) | Centralized Push Manager with per-subscriber isolation (Stalwart pattern) | cargo test, relay-ping latency_matrix, cmping -g 60 under slow clients |
| P10-S09 | [P10-S09-durable-modseq.md](P10-S09-durable-modseq.md) | Persistent per-mailbox modseq / change-id for efficient deltas | cargo test, relay-ping, cmping -g 60 (reduced work per burst) |
| P10-S10 | [P10-S10-e2e-validation.md](P10-S10-e2e-validation.md) | Full end-to-end with relay-ping + cmping 60-person media + durability | Full relay-ping matrix + cmping -g 60 media + simulated crash tests |

## Testing Strategy (Enforced at Every Stage)

**Unit Tests (always first)**
- Every step file must specify new `cargo test` commands (P10-UTxx or extension of existing).
- Tests must cover happy path, error paths, concurrent access, and binary data.

**Protocol Correctness** (see `context/relay-ping/cmd/relay-ping/main.go` and `internal/check/`):
- `context/relay-ping -test connectivity` → uses `internal/check/smtpcheck` + `imapcheck`
- `context/relay-ping -test dclogin` → `internal/check/dclogincheck` + `securejoininit`
- `context/relay-ping -test throughput` → `internal/check/throughput/throughput.go` (good for media load)
- `context/relay-ping -test latency_matrix` → `internal/check/latencymatrix/latencymatrix.go`
- Look at `internal/check/imapcheck/idle.go` for existing IDLE/push verification patterns that should be extended for reliable 60-way notification testing under media load.

**Realistic 60-Person Group Load (the killer workload)**
- At the **end of every stage**, run (see `context/cmping/cmping.py`):
  - `GroupPing` class (lines ~634+), `receive()` method using one `receiver_thread` per account calling `wait_for_event()` (from `deltachat_rpc_client`)
  - `setup_accounts` + `AccountMaker.get_relay_account(..., defer_online=True)` + `start_pending_online()`
  ```bash
  cd context/cmping
  uv run cmping --reset -c 1 -g 60 -i 0 <target-server>
  ```
- Measure and record (current tool emits per-seq RTT + loss; future advancement needed in `cmping.py` for explicit media integrity hashes, notification success counters, and body corruption detection):
  - Notification delivery success rate (60/60)
  - Body integrity for text + at least one image/video attachment (currently relies on Delta Chat successful decrypt/display)
  - p95 time for the full client fetch cycle
  - Any "lost" messages or media failures

**Specific files to examine/extend later**:
- `context/cmping/cmping.py` (receive loop, GroupPing class, defer_online logic)
- `context/cmping/test_cmping_dclogin.py` (existing dclogin test pattern)

**Reference Baselines**
- Always compare against a known-good cmdeploy server (cmdeploy_example or equivalent) running the same cmping command.

## Verification Command Examples (per stage)

See individual step files. Typical pattern:

```bash
# Unit
cargo test p10_ut01

# Protocol
context/relay-ping/bin/relay-ping -test connectivity -domain https://<your-test-server>/ -log-file - -vv

# 60-person group (run after every stage)
cd context/cmping && uv run cmping --reset -c 1 -g 60 -i 0 https://<your-test-server>/
```

## Madmail / context references

- Go madmail: `internal/go-imap-sql/delivery.go` (post-commit notify), `external_store.go` + `fsstore.go` (ExternalStore + Link), `fetch.go` (body extraction via extStore)
- Stalwart: `crates/services/src/state_manager/manager.rs` (per-sub spawn + send_timeout), `crates/store/src/dispatch/blob.rs` (get_blob with range), `crates/imap/src/op/fetch.rs` (ChainedBytes + write_bytes batching)
- cmrelay/Dovecot: `dist/config/dovecot/dovecot.conf` (mail_fsync, imap_hibernate, process limits), push_notification.lua (sync before notify)

## RFC references

- RFC 2177 (IDLE)
- RFC 3501 / 4466 (FETCH, BODY sections, literals)
- RFC 5322 / 5321 (message format, durability considerations)

## Next

See individual step files for detailed actions, files touched, and exact test commands.

After P10-S10, update the top-level `docs/plans/README.md` and the main TDD index.

---

**This plan must be executed stage-by-stage. No stage is considered complete until the unit tests pass + relay-ping protocol checks pass + cmping -g 60 (with media) shows no regression in loss or media loading compared to the previous stage + a known-good reference server.**