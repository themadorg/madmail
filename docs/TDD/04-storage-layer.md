# Storage Layer – High Throughput Design

**Implementation:** on-disk mail — `crates/chatmail-storage` (Maildir under `{state_dir}/mail/`). Hot caches and flush — `crates/chatmail-state` (`quota`, `tracker`, `policy`, `flusher`). Persistence — `crates/chatmail-db` (settings, stats, policy rows; not message bodies).

## Design Goals for High Throughput

- **Mail storage**: Must be filesystem-based (files + symlinks), similar to Dovecot + Postfix. Avoid storing full message bodies in the database.
- **Hot data in RAM**: Users, credentials, quotas, federation rules, endpoint overrides, and all metrics must live primarily in memory.
- **Low-latency operations**: Most reads and many writes should be served from memory with O(1) or O(log n) complexity.
- **Durability**: Periodic flushing / write-ahead logging to database for persistence and recovery.
- **Scalability**: Designed to handle thousands of concurrent connections and high message throughput.

## 1. Mail Storage (Filesystem-based)

### Recommended Layout (Maildir + Symlinks style)

```
{var}/mail/
├── {user_hash}/
│   ├── Maildir/
│   │   ├── cur/
│   │   ├── new/
│   │   └── tmp/
│   ├── index/               # Optional fast index (SQLite per user or shared)
│   └── quota
└── symlinks/                # For efficient folder sharing / virtual mailboxes
    └── {folder_id} -> ../../{user_hash}/Maildir/...
```

**Why this model?**
- Extremely fast appends (just write file + fsync or rename).
- Excellent cacheability by OS page cache.
- Easy to implement `fsync`, quota checks via `du` or inode tracking.
- Compatible with existing Dovecot-style tools and migration paths.
- Message bodies are **never** stored in the main SQL database.

### Message Metadata
Only lightweight metadata is kept in the central database / in-memory cache:
- UID, flags, size, date, mailbox
- Message-ID (for threading)
- Internal message pointer (path on disk)

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

This pattern dramatically reduces database write pressure.

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

This design moves the system much closer to how high-performance mail servers (Postfix + Dovecot) operate while keeping the rich admin/federation features of Chatmail.

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Concern | madmail | cmrelay | cmdeploy | stalwart |
|---------|---------|---------|----------|----------|
| Maildir / filesystem store | [`go-imap-sql/fsstore.go`](../../context/madmail/internal/go-imap-sql/fsstore.go), [`external_store.go`](../../context/madmail/internal/go-imap-sql/external_store.go) | Dovecot maildir (deployed) | [`dovecot.conf.j2`](../../context/cmdeploy/src/cmdeploy/dovecot/dovecot.conf.j2) | [`crates/email/src/message/`](../../context/stalwart/crates/email/src/message/) |
| Delivery → mailbox | [`go-imap-sql/delivery.go`](../../context/madmail/internal/go-imap-sql/delivery.go), [`storage/imapsql/delivery.go`](../../context/madmail/internal/storage/imapsql/delivery.go) | [`inbound.rs`](../../context/cmrelay/src/filtermail/src/inbound.rs) | LMTP path in Dovecot template | [`crates/email/src/message/delivery.rs`](../../context/stalwart/crates/email/src/message/delivery.rs) |
| In-memory quota | [`internal/quota/cache.go`](../../context/madmail/internal/quota/cache.go) | — | Dovecot `quota` plugin in template | [`crates/smtp/src/queue/quota.rs`](../../context/stalwart/crates/smtp/src/queue/quota.rs) |
| Federation rules RAM | [`federationtracker/`](../../context/madmail/internal/federationtracker/) | — | — | — |
| Endpoint cache | [`endpoint_cache/`](../../context/madmail/internal/endpoint_cache/) | — | — | — |
| DB models | [`internal/db/models.go`](../../context/madmail/internal/db/models.go) | [`migrate_db.py`](../../context/cmrelay/src/filtermail/python/chatmaild/migrate_db.py) | — | [`crates/store/`](../../context/stalwart/crates/store/) |

## Related RFCs

Message and mailbox semantics (Maildir itself is de-facto standard, not an RFC). Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Message headers/metadata (Message-ID, dates) | [rfc5322.txt](RFC/rfc5322.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP mailbox model (UID, flags, APPEND) | [rfc3501.txt](RFC/rfc3501.txt) |

All local files: [`RFC/README.md`](RFC/README.md). Regenerate: [`RFC/download-rfcs.sh`](RFC/download-rfcs.sh).