# Phase 2 — Storage & hot caches

## Goal

Maildir I/O, RAM quota/federation caches, 30s flusher.

## TDD index

- [docs/TDD/README.md](../../TDD/README.md)
- [RFC library](../../TDD/RFC/README.md)

## Steps

| Step | File | Summary |
|------|------|---------|
| P2-S01 | [P2-S01-storage-crate-init.md](P2-S01-storage-crate-init.md) | storage crate init |
| P2-S02 | [P2-S02-maildir-init.md](P2-S02-maildir-init.md) | maildir init |
| P2-S03 | [P2-S03-write-blob.md](P2-S03-write-blob.md) | write blob |
| P2-S04 | [P2-S04-read-blob.md](P2-S04-read-blob.md) | read blob |
| P2-S05 | [P2-S05-delete-blob.md](P2-S05-delete-blob.md) | delete blob |
| P2-S06 | [P2-S06-state-crate-init.md](P2-S06-state-crate-init.md) | state crate init |
| P2-S07 | [P2-S07-quota-cache.md](P2-S07-quota-cache.md) | quota cache |
| P2-S08 | [P2-S08-quota-hydrate.md](P2-S08-quota-hydrate.md) | quota hydrate |
| P2-S09 | [P2-S09-quota-check.md](P2-S09-quota-check.md) | quota check |
| P2-S10 | [P2-S10-fed-tracker.md](P2-S10-fed-tracker.md) | fed tracker |
| P2-S11 | [P2-S11-tracker-methods.md](P2-S11-tracker-methods.md) | tracker methods |
| P2-S12 | [P2-S12-policy-cache.md](P2-S12-policy-cache.md) | policy cache |
| P2-S13 | [P2-S13-flusher-start.md](P2-S13-flusher-start.md) | flusher start |
| P2-S14 | [P2-S14-flusher-interval.md](P2-S14-flusher-interval.md) | flusher interval |
| P2-S15 | [P2-S15-flusher-upsert.md](P2-S15-flusher-upsert.md) | flusher upsert |
| P2-S16 | [P2-S16-wire-boot.md](P2-S16-wire-boot.md) | wire boot |

## Tests

See step files for P*-UT* / E2E mappings.
