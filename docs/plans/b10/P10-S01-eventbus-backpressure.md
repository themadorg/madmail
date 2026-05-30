# P10-S01: EventBus backpressure and overflow detection

## Action

Add bounded queuing / overflow detection / logging to the current per-user `broadcast` in `chatmail-state/src/events.rs`. Provide a path for resync on overflow (instead of silent drop). Log when drops occur so operators can detect slow clients.

Introduce a small wrapper or metrics around `notify_new_message`.

## Files touched

- `crates/chatmail-state/src/events.rs`
- `crates/chatmail-imap/src/session.rs` (consume the new overflow signal if using resync path)
- New or extended unit tests in the events module

## TDD references

- [03-imap-server.md](../../TDD/03-imap-server.md) — IDLE + push
- [04-storage-layer.md](../../TDD/04-storage-layer.md)
- [16-testing.md](../../TDD/16-testing.md)

## Madmail / context references

- Go madmail: `go-imap-mess` Manager + UpdatePipe (avoids simple broadcast overflow)
- Stalwart: per-subscriber `send_timeout` + purge in `state_manager/manager.rs`

## RFC references

- RFC 2177 (IDLE)

## Verification (mandatory before considering stage complete)

```bash
# Unit tests
cargo test events -- --test-threads=1

# Protocol
context/relay-ping/bin/relay-ping -test connectivity -domain https://<test-server>/ -log-file - -vv
context/relay-ping/bin/relay-ping -test dclogin ...  # with verbose to exercise notifications

# 60-person group (with media where possible)
cd context/cmping && uv run cmping --reset -c 1 -g 60 -i 0 https://<test-server>/
# Record: notification success rate, any logged overflows, p95 fetch cycle time
```

## Linked tests

| Test ID          | Step     |
|------------------|----------|
| P10-UT01         | P10-S01  |
| events overflow + resync | P10-S01 |

## Next

P10-S02 (post-durability notify + directory fsync).