# P10-S10: Full E2E validation with relay-ping + cmping 60-person media + durability

## Action

Run the complete battery across a freshly deployed test server (or the reference good server) after all previous stages:

- Full unit test suite for P10
- relay-ping full matrix (connectivity, dclogin with media, throughput, latency_matrix)
- Multiple cmping -g 60 runs, including with real image + video attachments
- Simulated crash + recovery durability test (deliver, kill -9, restart, verify clients see everything)

Document results and compare against both previous stage and known-good cmdeploy baseline.

## Files touched

- This plan document (record results)
- Any final bugfixes discovered during E2E

## TDD references

All previous TDD docs for phases 2,4,5,6.

## Madmail / context references

Full parity targets from Go madmail + Dovecot behavior on identical hardware.

## RFC references

All relevant IMAP/SMTP RFCs.

## Verification (this stage is the final gate)

```bash
# 1. Unit tests
cargo test --workspace

# 2. Protocol (relay-ping)
context/relay-ping/bin/relay-ping -test connectivity -domain https://<server>/ -log-file - -vvv
context/relay-ping/bin/relay-ping -test dclogin ...
context/relay-ping/bin/relay-ping -test throughput -count 30 -workers 8

# 3. 60-person group (multiple runs, with media)
cd context/cmping
uv run cmping --reset -c 1 -g 60 -i 0 https://<server>/
# Repeat with media attachments in the group message

# 4. Durability
# Deliver messages, kill server, restart, wait for clients to catch up via cmping or manual check
```

All 60/60 must succeed with full media integrity on every run, with no regression vs. a known-good reference server.

## Linked tests

| Test ID                    | Step      |
|----------------------------|-----------|
| Full P10 unit suite        | P10-S10   |
| relay-ping full suite      | P10-S10   |
| cmping -g 60 media x3      | P10-S10   |
| Crash + recovery + cmping  | P10-S10   |

## Completion Criteria for Phase 10

- All previous stages passed their mandatory unit + relay-ping + cmping -g 60 gates.
- This final E2E stage shows parity (or better) with cmdeploy/Dovecot and Go madmail on the exact 60-person media workload that exposed the original two problems.
- No open P10-UT* or E2E failures.
- Updated top-level plans README and TDD index.

## Next

Mark Phase 10 complete. Update main project tracking. Begin any follow-on phases (e.g. exposing modseq to clients for QRESYNC).