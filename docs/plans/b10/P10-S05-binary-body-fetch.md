# P10-S05: Correct binary body handling in IMAP FETCH (fix UTF-8 corruption)

## Action

In `crates/chatmail-imap/src/session.rs`:
- `handle_fetch` FullBody path: stop using `std::str::from_utf8(&body).unwrap_or("")`.
- Treat body as raw `&[u8]`. Build the IMAP literal response using proper byte length and binary-safe transmission.
- Same treatment for header extraction paths when they feed into full-body responses.
- Update any response formatting helpers.

Add tests with binary (non-UTF-8) payloads that simulate PGP-encrypted media.

## Files touched

- `crates/chatmail-imap/src/session.rs` (handle_fetch, related helpers)
- `crates/chatmail-storage/src/blob.rs` (if any UTF-8 assumptions exist)
- Unit tests in chatmail-imap

## TDD references

- [03-imap-server.md](../../TDD/03-imap-server.md) — FETCH BODY[]
- [04-storage-layer.md](../../TDD/04-storage-layer.md)

## Madmail / context references

- Go madmail: treats bodies as `[]byte` via go-imap library + `extStore.Open`
- Stalwart: `DataItem::Binary` / raw bytes, no UTF-8 assumptions on body content in `fetch.rs`

## RFC references

- RFC 3501 §6.4.5 (FETCH BODY[] literals — must be 8-bit clean)

## Verification (mandatory before considering stage complete)

```bash
# Unit tests (include binary payload roundtrips)
cargo test -p chatmail-imap fetch_binary

# Protocol (use attachments)
context/relay-ping/bin/relay-ping -test dclogin ...  # force media in test if possible
context/relay-ping/bin/relay-ping -test throughput -count 5

# 60-person group with real image/video
cd context/cmping && uv run cmping --reset -c 1 -g 60 -i 0 https://<test-server>/
# Verify: 60/60 received + media integrity (no corruption, full size match)
```

## Linked tests

| Test ID              | Step     |
|----------------------|----------|
| P10-UT05-binary-body | P10-S05  |
| cmping media integrity under -g 60 | P10-S05 |

## Next

P10-S06 (External Blob Store abstraction).