# Phase 2 — Storage layer & hot caches

Sprint steps: [plans/b2/README.md](plans/b2/README.md)

## Delivered

- `chatmail-storage` — Maildir layout + atomic blob I/O
- `chatmail-state` — `QuotaCache`, `FederationTracker`, `FederationPolicyCache`, 30s flusher
- Migration `20240201000000_federation_stats.sql`
- Boot wires hydrate + flusher; `Ctrl+C` triggers final flush

## TDD

- [04-storage-layer.md](TDD/04-storage-layer.md)
- [07-federation.md](TDD/07-federation.md)
- [12-security.md](TDD/12-security.md)

## Tests

```bash
cargo test -p chatmail-storage -p chatmail-state
cargo test --workspace
```
