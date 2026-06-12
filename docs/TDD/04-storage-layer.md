# Storage Layer – High Throughput Design

**Implementation:** on-disk mail — `crates/chatmail-storage` (Maildir + optional CAS blobs under `{state_dir}/mail/` and `{state_dir}/blobs/`). Hot caches and flush — `crates/chatmail-state` (`quota`, `tracker`, `policy`, `flusher`, `MailboxStore` wired with `StoragePolicy`). Persistence — `crates/chatmail-db` (settings, stats, policy rows; not message bodies).

## Design Goals for High Throughput

- **Mail storage**: Must be filesystem-based (files + symlinks), similar to Dovecot + Postfix. Avoid storing full message bodies in the database.
- **Hot data in RAM**: Users, credentials, quotas, federation rules, endpoint overrides, and all metrics must live primarily in memory.
- **Low-latency operations**: Most reads and many writes should be served from memory with O(1) or O(log n) complexity.
- **Durability**: Periodic flushing / write-ahead logging to database for persistence and recovery.
- **Scalability**: Designed to handle thousands of concurrent connections and high message throughput.

## 1. Mail Storage (Filesystem-based)

### On-disk layout (`chatmail-storage`)

```
{state_dir}/
├── mail/{user}/Maildir/
│   ├── cur/                 # seen messages
│   ├── new/                 # unseen messages
│   ├── tmp/                 # atomic writes + uidlist staging
│   └── chatmail-uidlist     # persistent UID index (Dovecot uidlist parity)
├── blobs/{hh}/{sha256}      # content-addressed payloads (when blob_dedup on)
├── remote_queue/            # outbound federation retry queue (chatmail-delivery)
└── pending_notifications/   # disk-backed push notify jobs (chatmail-push)
```

Per-user mailboxes may also use subfolders (e.g. `folders/DeltaChat/cur|new|tmp`) for IMAP `LIST` / MVBOX.

**Why this model?**
- Fast appends (write file + optional fsync, or hardlink from CAS).
- Good cacheability via the OS page cache.
- `mail_fsync` / `blob_dedup` tunables match Dovecot relay throughput knobs.
- Multi-recipient fan-out can **link** one canonical blob instead of copying bytes.
- Message bodies are **never** stored in the main SQL database.

### `chatmail-storage` modules

| Module | Role |
|--------|------|
| `maildir` | `MailboxStore` — paths, policy, listing cache, uidlist, CAS, fsync coordinator |
| `blob` | Delivery (`deliver_local_messages`), APPEND streaming, multi-recipient link |
| `cas` | `ContentStore` — SHA-256 dedup under `{state_dir}/blobs/` |
| `external_store` | `ExternalStore` trait + `FsStore` default (Madmail `ExternalStore` seam) |
| `storage_policy` | `FsyncMode` (`always` / `optimized` / `never`) + `StoragePolicy` |
| `uidlist` | Stable IMAP UIDs via `chatmail-uidlist` (no renumbering on delete) |
| `maildir_cache` | `MaildirListCache` — skip `readdir` when `new/` + `cur/` mtimes unchanged |
| `fsync_batch` | Coalesce directory fsyncs under `mail_fsync = optimized` |
| `delivery_batch` | Per-mailbox coordinator for `mail_fsync = never` high-concurrency path |
| `maildir_message` | Flags, list, move, copy, expunge |
| `purge` | Retention / seen / unread purge helpers (`chatmail-tasks`) |
| `inbox` | Inbox listing helper |

`AppState` constructs `MailboxStore::with_policy(StoragePolicy::from_config(mail_fsync, blob_dedup))` at boot (`chatmail-state`).

### Storage policy (`mail_fsync`, `blob_dedup`)

Parsed from `storage.imapsql` in `maddy.conf` (see [`13-configuration.md`](13-configuration.md)):

| `mail_fsync` | Behaviour |
|--------------|-----------|
| `always` (default) | `sync_data` + directory fsync on every write |
| `optimized` | Per-file fsync; directory fsyncs batched via `FsyncCoordinator` |
| `never` | Skip fsync (relay throughput); uses `DeliveryBatcher` to serialize visibility steps per mailbox |

| `blob_dedup` | Behaviour |
|--------------|-----------|
| `on` (default) | Identical payloads stored once in `blobs/`; maildir entries hardlink |
| `off` | Every message written as a distinct maildir file |

Large APPEND bodies (≥ 64 KiB) stream socket → `tmp/` instead of buffering in RAM. PGP policy scans the first 64 KiB during streaming (`cas::HEADER_SCAN_PREFIX`).

### Message metadata

UID, flags, size, and internal date are cached in `chatmail-uidlist` on disk and in `MaildirListCache` in RAM. The SQL database holds only account/quota/policy rows — not per-message indexes (Madmail go-imap-sql `msgs` table is **not** replicated).

## 2. In-Memory Hot Data Architecture

All frequently accessed data is loaded into memory at startup and kept consistent via **write-through** or **write-behind** strategies.

### Core In-Memory Structures

| Component              | Structure                          | Update Strategy          | Flush to DB          | Notes |
|------------------------|------------------------------------|--------------------------|----------------------|-------|
| **Users / Credentials**| `HashMap<String, User>`            | Write-through            | On create/delete     | Full user table in RAM |
| **Quotas**             | `QuotaCache` (RwLock<HashMap>)     | Write-through            | Periodic + on change | Already designed |
| **Federation Rules**   | `RwLock<HashSet<String>>`          | Write-through            | On add/remove        | O(1) checks |
| **Endpoint Cache**     | `RwLock<HashMap>`                  | Write-through            | On change            | Delivery routing |
| **FederationTracker**  | `RwLock<HashMap<Domain, Stats>>`   | In-memory increments     | Every 30s            | High-frequency updates |
| **Message Counters**   | Atomic counters + struct           | In-memory                | Every 30s            | `sent`, `received`, etc. |
| **Settings**           | `RwLock<HashMap<String, String>>`  | Write-through            | On change            | Dynamic config |

### Loading Strategy at Startup
1. Load **all users** into memory (credentials + basic profile).
2. Load **all quotas** and compute current usage from filesystem or cached values.
3. Load **federation rules**, **endpoint overrides**, and **settings**.
4. Warm up `FederationTracker` from last flushed DB state.

### Update & Sync Strategy

#### For User Operations (Create / Delete)
- **Create user**:
  1. Insert into in-memory `HashMap` immediately.
  2. Return success to caller.
  3. Asynchronously persist to database (or on next flush).
- **Delete user**:
  1. Mark as deleted in memory (or remove).
  2. Schedule filesystem cleanup + DB delete.
  3. Block re-registration via blocklist (also in memory).

This allows very fast user provisioning under high load.

#### For Metrics & High-Frequency Data
- All counters and `FederationTracker` are updated **purely in memory**.
- A background flusher task runs every **30 seconds** (or configurable) and does batch UPSERTs to the database.
- On graceful shutdown, force flush everything.

This pattern reduces database write pressure compared with per-message SQL writes.

## 3. Database Role (Reduced)

The SQL database (SQLite or PostgreSQL) is used for:
- Durability and recovery after restart/crash
- Complex queries (admin listing, search)
- Long-term audit (if logging enabled)
- Federation rules and endpoint overrides (as source of truth)

**It is not** the primary path for mail delivery or quota checks.

## 4. Concurrency & Safety

- All in-memory structures protected by `tokio::sync::RwLock` or `std::sync::RwLock`.
- Write operations that need durability go through a single writer task or use channels.
- Use `dashmap` or similar for high-concurrency maps if contention becomes an issue.

## 5. Benefits for High Throughput

- Message delivery path touches almost no database after initial load.
- Quota checks are pure memory + filesystem `stat`.
- Federation policy evaluation is O(1) in RAM.
- User authentication can be served from memory cache.
- Background flushing keeps disk I/O predictable and batched.

## Implementation Notes (Rust)

- Use `dashmap` for high-concurrency user/metric maps.
- Consider `notify` crate or inotify for filesystem-based quota if needed.
- Implement a `PersistenceManager` actor/task that handles periodic flushing.
- On startup, have a clear "hydration" phase with progress logging.

This design follows patterns used by traditional mail stacks (Postfix + Dovecot) while keeping chatmail's admin and federation features.

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Concern | madmail-v2 | madmail | cmrelay | cmdeploy | stalwart |
|---------|-------------|---------|---------|----------|----------|
| Maildir / blob store | `crates/chatmail-storage/` (`blob`, `cas`, `external_store`) | [`fsstore.go`](../../context/madmail/internal/go-imap-sql/fsstore.go), [`external_store.go`](../../context/madmail/internal/go-imap-sql/external_store.go) | Dovecot maildir | [`dovecot.conf.j2`](../../context/cmdeploy/src/cmdeploy/dovecot/dovecot.conf.j2) | [`crates/email/src/message/`](../../context/stalwart/crates/email/src/message/) |
| Delivery → mailbox | `blob::deliver_local_messages` | [`delivery.go`](../../context/madmail/internal/go-imap-sql/delivery.go) | [`inbound.rs`](../../context/cmrelay/src/filtermail/src/inbound.rs) | LMTP | [`delivery.rs`](../../context/stalwart/crates/email/src/message/delivery.rs) |
| UID stability | `uidlist::UidListStore` | go-imap-sql positional UIDs | Dovecot uidlist | Dovecot uidlist | — |
| `mail_fsync` / dedup | `storage_policy`, `fsync_batch`, `delivery_batch` | — | Dovecot `mail_fsync` | Dovecot config | CAS store |
| In-memory quota | `chatmail-state::quota` | [`quota/cache.go`](../../context/madmail/internal/quota/cache.go) | — | Dovecot `quota` plugin | [`queue/quota.rs`](../../context/stalwart/crates/smtp/src/queue/quota.rs) |
| Federation rules RAM | `chatmail-state::policy` | [`federationtracker/`](../../context/madmail/internal/federationtracker/) | — | — | — |
| Endpoint cache | `chatmail-db::endpoint_cache` | [`endpoint_cache/`](../../context/madmail/internal/endpoint_cache/) | — | — | — |
| DB models | `chatmail-db/migrations/` | [`models.go`](../../context/madmail/internal/db/models.go) | [`migrate_db.py`](../../context/cmrelay/src/filtermail/python/chatmaild/migrate_db.py) | — | [`crates/store/`](../../context/stalwart/crates/store/) |

## Related RFCs

Message and mailbox semantics (Maildir itself is de-facto standard, not an RFC). Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Message headers/metadata (Message-ID, dates) | [rfc5322.txt](RFC/rfc5322.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP mailbox model (UID, flags, APPEND) | [rfc3501.txt](RFC/rfc3501.txt) |

All local files: [`RFC/README.md`](RFC/README.md). Regenerate: [`RFC/download-rfcs.sh`](RFC/download-rfcs.sh).