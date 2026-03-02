# Madmail Performance & Memory Analysis

Date: 2026-03-02
Scope: Static code review of runtime paths that affect memory use and concurrency behavior in production.
Goal: Explain likely causes of OOM on a 1GB host under 1000+ concurrent users and provide a concrete fix order.

## Resolution Log

- [~] F1 Full-body buffering in hot paths
  - partial: `4b82540` (`/mxdeliv` now streams to file-backed buffer and enforces max size)
- [x] F2 Uncapped database pools
  - resolved: `0af52fb` (added explicit pool caps for backend and gorm SQL pools)
- [ ] F3 `sqlite3_cache_size` parsed but not applied (commit: pending)
- [ ] F4 Per-connection IMAP UID map amplification (commit: pending)

## Executive Summary

The memory pressure is not caused by a single leak. It is a combination of high-amplification message buffering, uncapped DB pools (especially dangerous with SQLite), and per-connection IMAP state that scales with mailbox size.

Most likely primary causes on a 1GB host are:

- Full-message buffering and re-buffering in multiple hot paths.
- No explicit DB pool caps, plus two separate DB stacks (`go-imap-sql` and GORM) opened for the same storage.
- Per-IMAP-connection UID maps that duplicate mailbox state in RAM.

## Prioritized Findings

## 1) Critical: Full message bodies are loaded into RAM (sometimes multiple times)

Evidence:

- `internal/endpoint/chatmail/chatmail.go:811` reads full HTTP mail body with `io.ReadAll(r.Body)`.
- `internal/endpoint/chatmail/chatmail.go:868` reads remaining body again into `remainingBody`.
- `internal/endpoint/imap/imap.go:705` reads full APPEND body with `io.ReadAll(body)`.
- `internal/pgp_verify/pgp_verify.go:36` reads entire body again for verification.
- `internal/pgp_verify/pgp_verify.go:155` and `:171` read MIME parts fully.
- `internal/endpoint/smtp/session.go:503` and `:588` call `pgp_verify.IsAcceptedMessage(...)`, which triggers the full-body reads above.

Why this is dangerous:

- Message data can be replicated in memory multiple times during a single transaction.
- Under concurrent sends/appends, peak RSS grows roughly with `(message size) x (copies) x (in-flight operations)`.
- A 10MB message with 3 in-memory copies at only 40 in-flight operations is already ~1.2GB before other process memory.

Recommendation:

- Replace full `ReadAll` verification with streaming/limited parsing.
- In `/mxdeliv`, parse headers from stream and spool body directly to file-backed buffer.
- Add strict maximum body size enforcement for `/mxdeliv` (same policy as SMTP `max_message_size`).
- Refactor PGP checks to avoid whole-body copies; inspect MIME envelope incrementally and cap read sizes.

## 2) Critical: Database connection pools are effectively uncapped

Evidence:

- `internal/go-imap-sql/sql.go:43` only sets `SetMaxOpenConns(1)` for `:memory:` test DB.
- No production `SetMaxOpenConns`, `SetMaxIdleConns`, `SetConnMaxLifetime`, or `SetConnMaxIdleTime` for the main backend DB.
- `internal/storage/imapsql/imapsql.go:308` creates backend DB (`go-imap-sql`) and `:318` separately creates another GORM DB.
- `internal/db/db.go:35` opens GORM DB with no pool tuning.

Why this is dangerous:

- Go's default max open conns is unlimited.
- With SQLite, each extra connection has its own caches and internal structures; this can become large fast under concurrency.
- Two independent DB stacks against the same DB roughly doubles base DB-side memory/caching behavior.

Recommendation:

- Add explicit pool caps in config and apply to both backend DB and GORM DB.
- For 1GB SQLite deployments, start conservative (example):
  - Backend DB: max open 4-8, max idle 4-8
  - GORM DB: max open 2-4, max idle 2-4
  - conn max idle time: 1-5 minutes
- Expose these as config directives so operators can tune by host size.

## 3) High: Configured SQLite cache size is parsed but not used

Evidence:

- `internal/storage/imapsql/imapsql.go:165` reads `sqlite3_cache_size` into `opts.CacheSize`.
- `internal/go-imap-sql/backend.go` defines `Opts.CacheSize`.
- No actual use of `CacheSize` in runtime SQL setup paths (`rg` shows only declaration/read).

Why this matters:

- Operators cannot reduce SQLite page cache memory even when configured.
- On constrained hosts this removes a key memory control lever.

Recommendation:

- Apply `PRAGMA cache_size` during connection initialization using `Opts.CacheSize`.
- Validate behavior per-driver and per-connection.

## 4) High: IMAP selected mailbox state scales linearly per connection

Evidence:

- `internal/go-imap-sql/mailbox.go:165-189` loads all message UIDs for selected mailbox into `uids []uint32`.
- `internal/go-imap-sql/user.go:137` passes this into `go-imap-mess` handle.
- `internal/go-imap-mess/mailbox.go:32` stores `uidMap []uint32` per mailbox handle/connection.

Why this is dangerous:

- Memory is duplicated per active selected mailbox connection.
- Rough estimate: `4 bytes x message_count x active_connections` plus slice/object overhead.
- Example: 100k-message mailbox x 1000 active IMAP connections is ~400MB just for raw UID arrays, not counting overhead and other state.

Recommendation:

- Short term: limit IMAP concurrent sessions per user/IP and prune idle sessions aggressively.
- Medium term: use compressed UID ranges or segmented structures instead of full UID slice duplication.
- Long term: redesign seq/uid mapping to avoid loading full mailbox UID sets per connection.

## 5) Medium: `servertracker` maps grow without bounds

Evidence:

- `internal/servertracker/tracker.go:45-47` stores hashed unique connection IPs/domains/IP-literals.
- `internal/servertracker/tracker.go:96-113` inserts on every message.
- No TTL/LRU/size cap.

Why this matters:

- Long-running servers exposed to diverse sender domains/IPs will accumulate entries forever.
- This is slow growth, but permanent.

Recommendation:

- Add max cardinality cap and periodic eviction.
- Optionally keep approximate cardinality (HyperLogLog) instead of exact sets.

## 6) Medium: Template parsing and file loading done on every HTTP request

Evidence:

- `internal/endpoint/chatmail/chatmail.go:614-620` parses templates in `handleStaticFiles` per request.
- `internal/endpoint/chatmail/chatmail.go:1406-1412` parses templates again in `serveTemplate` per request.

Why this matters:

- High allocation churn and avoidable GC work under web traffic.
- Not usually the main OOM source, but contributes to memory pressure and latency spikes.

Recommendation:

- Pre-parse and cache templates at init/reload.
- Serve static files with `http.FileServer` and cache headers where appropriate.

## 7) Medium: Goroutine growth risk in connection fan-out paths

Evidence:

- `internal/endpoint/chatmail/chatmail.go:934` starts one goroutine per Shadowsocks connection, plus nested copy goroutine at `:973`.
- `internal/endpoint/chatmail/chatmail.go:1077` starts one goroutine per ALPN-accepted connection.
- No explicit concurrency limiter for these paths.

Why this matters:

- Under bursts or abuse, goroutine count can spike and increase memory use significantly.

Recommendation:

- Add connection limits/semaphores per service.
- Add read/write deadlines on proxy connections.
- Reject early when overloaded.

## 8) Medium: `/mxdeliv` lacks explicit body size limit enforcement

Evidence:

- `internal/endpoint/chatmail/chatmail.go:811` reads request body fully, no `LimitReader` and no max-size check.

Why this matters:

- Allows oversized payloads on an HTTP path that bypasses SMTP message-size protections.
- Can trigger immediate OOM with a few concurrent large requests.

Recommendation:

- Enforce `max_message_size` on this endpoint before buffering.
- Return `413 Payload Too Large` when exceeded.

## Immediate Remediation Plan (recommended order)

1. Stop full-body `ReadAll` in hot paths (`/mxdeliv`, IMAP APPEND encryption check, SMTP/LMTP encryption check).
2. Add hard DB pool caps for both backend and GORM connections.
3. Implement `sqlite3_cache_size` behavior (currently ignored).
4. Add `/mxdeliv` request-size cap.
5. Add IMAP connection/session limits and idle pruning for 1GB profile.
6. Add caps/eviction to `servertracker`.
7. Cache templates/static responses.

## Suggested 1GB Host Profile

These are starting points, not final truth:

- IMAP + SMTP + HTTP max active sessions tuned aggressively (avoid unbounded defaults).
- Backend DB pool cap for SQLite: 4-8 open.
- GORM DB pool cap for SQLite: 2-4 open.
- Disable optional features that create persistent per-connection state unless required.
- Prefer file-backed buffering for message bodies by default.

## Validation Plan

1. Add heap and goroutine profiling during load test (1000 concurrent mixed idle + send).
2. Capture RSS over time and peak allocation by code path.
3. Re-test after each remediation step to isolate impact.

Key metrics to watch:

- RSS (`/proc/<pid>/status`, cgroup memory)
- goroutine count
- DB open connection counts (both backend and GORM)
- GC pause and heap live size
- In-flight message count and message-size distribution

## Closing Notes

The OOM behavior is consistent with architectural memory amplification, not just a tiny leak. The biggest wins will come from eliminating full-body in-memory validation and enforcing strict connection/pool caps.
