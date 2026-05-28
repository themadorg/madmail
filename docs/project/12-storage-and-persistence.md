# 12 — Storage, Quota, and the Persistence Model

This document explains where mail lives, how quota is enforced, and how the fast in-memory world stays in sync with durable disk.

## The Two Kinds of Data

### 1. Structured / Small / Hot Data → SQLite

`chatmail.db` (or `credentials.db` in some paths) via `chatmail-db`.

Tables for:
- Settings (dynamic config)
- Password hashes
- Quotas (per-user max + used token for accounting)
- Blocklist
- Registration tokens
- Federation stats & policy
- Message counters (sent/received per user)
- DNS overrides
- Mail port overrides
- etc.

Accessed via SQLx (compile-time checked queries in many places).

### 2. Message Bodies + Flags → Maildir on Disk

Location (in dev): `data/mail/<normalized-user>/Maildir/`

Layout (standard + Delta Chat extensions):
- `cur/`, `new/`, `tmp/`
- `folders/DeltaChat/Maildir/...` (and potentially other folders)
- Each message is a single file whose name encodes the IMAP UID + flags (the classic Maildir `:2,FRS` suffix).

`chatmail-storage` provides the abstraction:
- `write_blob`, `read_blob`
- `deliver_local_messages`
- `list_mailbox_messages`, `store_add_flags`, `expunge_deleted`
- `move_message`, `copy_message`
- Purge jobs for retention

## The In-Memory Hot Layer (`AppState`)

Defined in `chatmail-state`.

Contains several `Arc<...Cache>` objects that are consulted on **every** message path:

- `QuotaCache` — current used bytes + max per user. Checked before accepting DATA or /mxdeliv.
- `FederationPolicyCache`
- `FederationSilentDismissCache`
- `MessageSizeLimit`
- `FederationTracker` (per-destination stats)
- `EventBus` (for waking IDLE sessions)
- `ListenerPortsStore`
- `MailboxStore` (lightweight handle to the Maildir root)

These are populated at boot during `hydrate()` and then kept up to date by the hot path + a background flusher.

## Hydration at Boot

`AppState::hydrate` walks:
- The `settings` table
- The `quotas` table
- The on-disk Maildir to recompute actual used storage per user (because the `used` column is not the source of truth — the filesystem is)

This can take a few seconds on a server with many users or large mailboxes, but it only happens at start or reload.

## Write Path (the important one)

When a message arrives (SMTP DATA, IMAP APPEND, or inbound /mxdeliv):

1. Size check (`AppState::check_message_size`)
2. Quota check (`quota.check_quota` — reads the in-memory cache)
3. PGP gate
4. Policy / blocklist checks
5. **Write to disk** via `chatmail_storage::write_blob` / `deliver_local_messages`
6. **Update RAM quota** (`quota.record_write`)
7. **Notify IDLE** via EventBus (so connected clients see the new mail instantly)
8. **Record stats** (increment sent/received counters in RAM)
9. The background flusher will later persist the stats and any quota checkpoints to SQLite

The disk write is the slow / durable step. Everything before it is fast RAM checks.

## Quota Model

- Each user has a row in `quotas` with `max_storage`.
- Default comes from config or the `DEFAULT_QUOTA_BYTES` setting.
- Used bytes are **not** stored per-message in the DB. They are computed from the filesystem (at hydrate) and then maintained incrementally in RAM.
- The `used_token` column is used for some accounting / first-login logic.

This design means that if the server crashes between a delivery and the next flusher run, the RAM quota is lost and will be recomputed on next boot by walking Maildir. Correct, just not instantaneous.

## The Flusher

`chatmail_state::flusher`

- Started once per `AppState`
- Runs on a timer (roughly 30s in current code)
- Calls `flush_federation_stats`, message stats flush, etc.
- On graceful shutdown it is explicitly flushed one last time.

This is the component that makes "write-through RAM + eventual durable persistence" safe.

## Retention & Purging (`chatmail-tasks` + `chatmail-storage::purge`)

Background jobs (scheduled via `chatmail-tasks`):

- `purge_read_messages` / `purge_unread_older`
- `purge_user_messages` (when an account is deleted)
- Dormant account detection and cleanup (`maintenance.rs` in db)

These walk the Maildir and remove old blobs according to the retention policy stored in settings.

## Folder / Delta Chat Special Case

Delta Chat clients create a folder called `DeltaChat` (or similar) and put most chat messages there instead of INBOX.

The storage layer treats subfolders under `folders/` as first-class Maildir hierarchies.

IMAP `LIST`, `SELECT`, `MOVE` etc. all understand this layout.

## Backups & Durability

- SQLite WAL mode is used.
- Maildir is append-only for new messages (classic safety property).
- No special backup tool is shipped; operators are expected to snapshot the entire `state_dir` (or at least `chatmail.db` + the `mail/` tree) with normal filesystem tools (rsync, zfs snapshot, restic, etc.).

## Common Operational Questions

**"Why is quota wrong after a crash?"**
→ On next boot `hydrate` will re-walk the Maildir and correct it. You can also trigger a manual re-hydration via reload or restart.

**"A user is over quota but the admin UI shows room"**
→ The UI reads the RAM cache or the last flushed value. Force a flush or restart.

**"Where do the actual .eml files live?"**
→ `<state_dir>/mail/<user>/Maildir/cur/` and `new/`, plus under `folders/*/Maildir/`.

## Testing Storage

- `cargo test -p chatmail-storage`
- Maintenance tests in `chatmail-db`
- Purge tests
- E2E tests that deliver mail and then inspect the resulting Maildir on disk

## Next

You now understand the full core server.

The remaining documents cover the **build system, deployment, the context/ reference trees, development workflow, testing, and how to extend the project**.

→ [13-build-test-deploy.md](./13-build-test-deploy.md)
