# IMAP Server Implementation

**Implementation:** `crates/chatmail-imap` (`server`, `session`, `connection_stats`). Mailbox backend: `chatmail-storage`. Hot limits and federation side effects: `chatmail-state`. Wired from `chatmail::supervisor`.

**Operator CLI:** [`../guide/cli/port.md`](../guide/cli/port.md) (IMAP ports) · [`push.md`](../guide/cli/push.md) (`XDELTAPUSH`).

This section documents **IMAP commands and extensions** as implemented in the reference codebases under `context/`, for use when designing **madmail-v2**. It covers three layers:

| Layer | Path | Role |
|-------|------|------|
| **Server (Madmail)** | `context/madmail` | Production Chatmail IMAP listener + SQL storage |
| **Client (Delta Chat core)** | `context/core` | What a real Delta Chat client sends and expects |
| **Legacy deploy (cmdeploy)** | `context/cmdeploy` | Dovecot-based Chatmail stack (comparison / migration) |

`context/madmail/chatmail-core` is a submodule copy of Delta Chat core; IMAP **client** behavior is the same as `context/core`.

---

## Design Goals (from `00-intro.md`)

The Rust rewrite must support at minimum:

- **[IMAP4rev1](RFC/rfc3501.txt)** ([RFC 3501](https://datatracker.ietf.org/doc/html/rfc3501)) — Delta Chat sync (SELECT, FETCH, STORE, CLOSE, LIST, STATUS)
- **IDLE** ([RFC 2177](RFC/rfc2177.txt)) — primary push mechanism
- **METADATA** ([RFC 5464](RFC/rfc5464.txt)) — TURN / Iroh relay discovery, push token, server comment/admin
- **QUOTA** ([RFC 2087](RFC/rfc2087.txt)) — `GETQUOTA` / `GETQUOTAROOT`
- **MOVE** ([RFC 6851](RFC/rfc6851.txt)) — move messages to mvbox / trash without COPY+DELETE
- **APPEND** with **PGP enforcement** on submission paths
- **Special-use** mailboxes (`\Inbox`, `\Sent`, etc.)
- **XCHATMAIL** — client detects Chatmail servers
- Optional: **COMPRESS=DEFLATE**, **CONDSTORE**, **XDELTAPUSH** (Dovecot/cmdeploy; **madmail-v2** when push enabled — see [23-push-notifications.md](23-push-notifications.md))

---

## Madmail IMAP Server (`context/madmail`)

### Architecture

```
Delta Chat / other IMAP clients
        │
        ▼
internal/endpoint/imap/imap.go     ← TLS, SASL, extensions (QUOTA, METADATA, XCHATMAIL)
        │  backend interface
        ▼
internal/storage/imapsql/          ← JIT, quota cache, blocklist, delivery
        │  go-imap-sql Backend
        ▼
internal/go-imap-sql/              ← SQL mailboxes, messages, IDLE notify
        │
github.com/foxcpp/go-imap/server   ← IMAP4rev1 command parser & session machine
```

Key files:

- `internal/endpoint/imap/imap.go` — listener, auth, custom extensions
- `internal/storage/imapsql/imapsql.go` — `IMAPExtensions()`, account lifecycle
- `internal/go-imap-sql/` — mailbox/user backend
- `internal/endpoint/webimap/` — REST/WebSocket façade over the same backend (not raw IMAP)

The server uses **`github.com/foxcpp/go-imap`** (fork of emersion/go-imap). Standard commands are implemented by the library and delegated to the **backend** (`go-imap-sql`).

### Advertised capabilities

**From storage** (`Storage.IMAPExtensions()` in `internal/storage/imapsql/imapsql.go`):

| Capability | RFC / notes |
|------------|-------------|
| `APPENDLIMIT` | [RFC 7889](RFC/rfc7889.txt) — max APPEND size via STATUS |
| `MOVE` | [RFC 6851](RFC/rfc6851.txt) |
| `CHILDREN` | [RFC 3348](RFC/rfc3348.txt) |
| `SPECIAL-USE` | [RFC 6154](RFC/rfc6154.txt) |
| `I18NLEVEL=1` | go-imap-i18nlevel |
| `SORT` | [RFC 5256](RFC/rfc5256.txt) |
| `THREAD=ORDEREDSUBJECT` | [RFC 5256](RFC/rfc5256.txt) threading |
| `QUOTA` | [RFC 2087](RFC/rfc2087.txt) (storage + custom handlers) |

**Added by IMAP endpoint** (`enableExtensions()` in `internal/endpoint/imap/imap.go`):

| Capability | Handler / behavior |
|------------|-------------------|
| `COMPRESS=DEFLATE` | [RFC 4978](RFC/rfc4978.txt) — `github.com/emersion/go-imap-compress` |
| `NAMESPACE` | [RFC 2342](RFC/rfc2342.txt) — `github.com/foxcpp/go-imap-namespace` |
| `QUOTA` | `GETQUOTA`, `GETQUOTAROOT`; **`SETQUOTA` rejected** |
| `METADATA` | Only if TURN and/or Iroh relay configured; **`GETMETADATA` only** (no SET at endpoint) |
| `XCHATMAIL` | Non-standard; capability only (no commands) |

**Implicit via go-imap / IMAP4rev1** (not listed in `IMAPExtensions()` but available):

| Capability / command | Notes |
|---------------------|--------|
| `IMAP4rev1`, `LITERAL+`, `UIDPLUS`, … | Library defaults |
| `IDLE` | [RFC 2177](RFC/rfc2177.txt) — `Mailbox.Idle()` in go-imap-sql + delivery notifications |
| `AUTH=PLAIN` (+ optional `LOGIN`) | SASL from `auth` modules |
| `LOGOUT`, `NOOP`, `CAPABILITY`, … | Standard session |

**Not advertised by Madmail storage** (Delta Chat may skip related features):

| Capability | Impact on Delta Chat |
|------------|---------------------|
| `CONDSTORE` | Core skips server-side `\Seen` sync via `CHANGEDSINCE` |
| `XDELTAPUSH` | Not on Madmail Go IMAP; **madmail-v2** advertises when `__PUSH_MODE__` ≠ `off` |

### Standard IMAP4rev1 commands (via go-imap-sql backend)

These map to `backend.User` / `backend.Mailbox` methods and are handled by **go-imap/server** when the backend implements them.

#### Connection & auth (`internal/endpoint/imap/imap.go`)

| Command | Implementation |
|---------|----------------|
| `CAPABILITY` | Server + enabled extensions |
| `NOOP` | Library |
| `LOGOUT` | Library; `User.Logout()` is no-op in go-imap-sql |
| `LOGIN` | `Endpoint.Login()` → storage account + PGP wrapper |
| `AUTHENTICATE` | SASL providers (`PLAIN`, optional `LOGIN`) → `openAccount()` |
| `STARTTLS` | When listening on cleartext port with TLS config |

#### Mailbox management (`internal/go-imap-sql/user.go`)

| Command | Backend method |
|---------|----------------|
| `LIST` | `ListMailboxes()` |
| `LSUB` | `ListMailboxes(subscribed=true)` |
| `CREATE` | `CreateMailbox()`, `CreateMailboxSpecial()` |
| `DELETE` | `DeleteMailbox()` |
| `RENAME` | `RenameMailbox()` |
| `SUBSCRIBE` / `UNSUBSCRIBE` | `SetSubscribed()` |
| `STATUS` | `Status()` — includes `APPENDLIMIT` when configured |
| `NAMESPACE` | `Namespaces()` |

#### Selected mailbox (`internal/go-imap-sql/mailbox.go`)

| Command | Backend method |
|---------|----------------|
| `SELECT` / `EXAMINE` | `initSelected()` |
| `CLOSE` | `Close()` — expunge if `\Deleted` pending |
| `FETCH` / `UID FETCH` | `ListMessages()` |
| `STORE` / `UID STORE` | `UpdateMessagesFlags()` |
| `SEARCH` / `UID SEARCH` | `SearchMessages()` |
| `COPY` / `UID COPY` | `CopyMessages()` |
| `MOVE` / `UID MOVE` | `MoveMessages()` |
| `EXPUNGE` / `UID EXPUNGE` | `Expunge()`, `DelMessages()` |
| `APPEND` | `CreateMessage()` — **PGP check** in `encryptionWrapperUser` |
| `IDLE` | `Idle()` — mess update pipe; triggered on delivery (`delivery.go`) |

#### Not implemented as custom commands

| Command | Notes |
|---------|--------|
| `SORT` / `THREAD` | Advertised; provided by go-imap-sortthread against backend |
| `SETMETADATA` | Not on Madmail Go IMAP; Dovecot/cmdeploy + **madmail-v2** (`/private/devicetoken`) |
| `SETQUOTA` | Explicitly rejected |

### Madmail-specific: QUOTA

Handlers in `internal/endpoint/imap/imap.go`:

| Command | Behavior |
|---------|----------|
| `GETQUOTA "ROOT"` | Returns `STORAGE (used_kb max_kb)` from `Storage.GetQuota()` |
| `GETQUOTAROOT <mailbox>` | Quota root `ROOT` + quota response |
| `SETQUOTA` | **Not allowed** — error at parse and handle |

Quota data comes from `internal/quota/cache.go` (in-memory, DB-backed).

### Madmail-specific: METADATA (GET only)

Enabled when `turn_enable` + TURN credentials **or** `iroh_relay_url` is set.

| Key | When returned |
|-----|----------------|
| `/shared/vendor/deltachat/turn` | TURN enabled; value `server:port:username:password` (HMAC-SHA1) |
| `/shared/vendor/deltachat/turns` | Same as turn (TLS variant key) |
| `/shared/vendor/deltachat/irohrelay` | `iroh_relay_url` configured |

Requires authenticated state. Empty mailbox argument in handler.

### Madmail-specific: APPEND + PGP

`encryptionWrapperUser.CreateMessage()` reads the full message and runs `pgp_verify.EnforceEncryption()` before storage — same policy as SMTP submission.

### Madmail-specific: XCHATMAIL

Capability only. Delta Chat sets `is_chatmail` and adjusts UI (hide non-chatmail options).

### IDLE & push to clients

1. Client sends `IDLE` in selected folder.
2. On new mail (SMTP, federation, delivery), `go-imap-sql/delivery.go` commits then notifies IDLE waiters / update pipe.
3. Client receives `EXISTS` (or related unsolicited data) and fetches.

See `docs/imap-connection-lifecycle.md` for connection/goroutine lifecycle.

`auto_logout` config must not go below go-imap `MinAutoLogout` (30m) without breaking IDLE (documented in `imap.go`).

### WebIMAP (non-IMAP API)

`internal/endpoint/webimap/` exposes HTTP/WebSocket operations that call the **same** backend interfaces:

| REST / WS operation | Backend equivalent |
|--------------------|-------------------|
| List mailboxes | `ListMailboxes` + `Status` |
| List / get messages | `ListMessages` (FETCH items) |
| Set flags | `UpdateMessagesFlags` |
| Delete messages | `UpdateMessagesFlags` + `\Deleted` |
| Mark seen/unseen | `UpdateMessagesFlags` |

Controlled by `__WEBIMAP_ENABLED__` in settings DB.

---

## Delta Chat IMAP Client (`context/core`)

Delta Chat is an **IMAP client** (async-imap). It does not implement server commands; this section lists what **madmail-v2 must support** for real clients.

### Module layout

| File | Responsibility |
|------|----------------|
| `src/imap.rs` | Connect, fetch/move/delete, metadata, quota, folder config |
| `src/imap/client.rs` | TCP/TLS/STARTTLS, LOGIN, AUTH XOAUTH2, CAPABILITY, ID |
| `src/imap/session.rs` | LIST, prefetch UID FETCH |
| `src/imap/select_folder.rs` | SELECT / CLOSE, UIDVALIDITY, UIDNEXT |
| `src/imap/idle.rs` | IDLE |
| `src/imap/capabilities.rs` | Capability flags from CAPABILITY |
| `src/quota.rs` | GETQUOTAROOT / GETQUOTA |

### Connection sequence

1. TCP (+ proxy optional)
2. Read greeting
3. `STARTTLS` (if StartTLS transport)
4. `LOGIN` or `AUTHENTICATE XOAUTH2`
5. `CAPABILITY` → build `Capabilities`
6. `ID` (if `ID` capability) — sends `name=Delta Chat`
7. `COMPRESS DEFLATE` (if `COMPRESS=DEFLATE`)

### Commands used in normal operation

| Command | Purpose |
|---------|---------|
| `LIST "" "*"` | Discover folders + SPECIAL-USE / name heuristics |
| `SELECT` / `SELECT ... CONDSTORE` | Open folder; track UIDNEXT / UIDVALIDITY |
| `EXAMINE` | Probe mvbox candidates without selecting |
| `CLOSE` | Expunge `\Deleted` after batch deletes |
| `STATUS (UIDNEXT)` | Fallback when SELECT omits UIDNEXT |
| `UID FETCH` | Prefetch headers, download bodies, resync, CONDSTORE flags |
| `UID STORE` | Set `\Seen`, `\Deleted` (+ triggers CLOSE) |
| `UID MOVE` | Move to mvbox / trash (preferred over COPY+DELETE) |
| `UID COPY` | Fallback move path when MOVE unavailable |
| `IDLE` | Wait for new mail (or **fake IDLE** = 60s sleep if no IDLE) |
| `GETMETADATA` | `/shared/comment`, `/shared/admin`, `/shared/vendor/deltachat/turn`, `/shared/vendor/deltachat/irohrelay` |
| `SETMETADATA` | `/private/devicetoken` on INBOX (if `METADATA` + `XDELTAPUSH`) |
| `GETQUOTAROOT` + quota responses | Mailbox usage warnings |

### FETCH items (representative)

| Use case | FETCH attributes |
|----------|------------------|
| Prefetch / classify | `UID INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER.FIELDS (...)]` |
| Download | `FLAGS BODY.PEEK[]` |
| Message-ID resync ([RFC 5322](RFC/rfc5322.txt) §3.6.4) | `UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID ...)]` |
| CONDSTORE seen sync | `(FLAGS) (CHANGEDSINCE <modseq>)` |

### Capabilities the client checks

From `src/imap/capabilities.rs` / `client.rs`:

| Capability | Client behavior if present |
|------------|---------------------------|
| `IDLE` | Real IDLE; else poll every 60s |
| `MOVE` | `UID MOVE`; else COPY + DELETE |
| `QUOTA` | Periodic quota check + device messages |
| `CONDSTORE` | `sync_seen_flags()` with CHANGEDSINCE |
| `METADATA` | TURN/Iroh/comment/admin |
| `COMPRESS=DEFLATE` | Enable compression after login |
| `XDELTAPUSH` | SETMETADATA devicetoken + push service |
| `XCHATMAIL` | Chatmail UI defaults |

### Commands the client does **not** rely on

| Command | Notes |
|---------|--------|
| `APPEND` | Outbound mail uses SMTP; APPEND only in tests (`python/direct_imap.py`) |
| `CREATE` / `DELETE` / `RENAME` mailbox | Folders created by server delivery or admin |
| `SEARCH` | Not used in core scheduler path |
| `SORT` / `THREAD` | Not used by Delta Chat |
| `SUBSCRIBE` | Not used |

---

## cmdeploy / Dovecot (`context/cmdeploy`)

Legacy Chatmail deployments use **Dovecot** instead of Madmail’s go-imap stack. Relevant config: `src/cmdeploy/dovecot/dovecot.conf.j2`.

### Protocols & plugins

| Setting | Effect |
|---------|--------|
| `protocols = imap lmtp` | IMAP + LMTP delivery |
| `mail_plugins = zlib quota` | Compressed maildir + quota |
| `protocol imap { mail_plugins = ... imap_quota last_login [imap_zlib] }` | QUOTA, last-login dict, optional COMPRESS |
| `imap_metadata = yes` | METADATA on IMAP |
| `mail_attribute_dict = proxy:.../metadata.socket` | METADATA backed by **chatmail-metadata** service |

### Advertised IMAP capabilities (config)

```
imap_capability = +XDELTAPUSH XCHATMAIL
```

| Capability | Notes |
|------------|--------|
| `XDELTAPUSH` | SETMETADATA `/private/devicetoken` → push notifications |
| `XCHATMAIL` | Same semantic as Madmail |
| `imap_zlib` | COMPRESS=DEFLATE when `config.imap_compress` |
| Quota plugin | `GETQUOTA` / limits via maildir quota rules |
| METADATA | TURN/Iroh via dict proxy (`chatmail-metadata.service`) |

Dovecot also provides full IMAP4rev1, IDLE, MOVE, SPECIAL-USE, etc., per stock Dovecot.

### cmdeploy tests (`test_2_deltachat.py`)

Online tests exercise raw `SETMETADATA` / `GETMETADATA` on the dict socket (not only Dovecot binary).

---

## Feature matrix (Madmail vs Dovecot vs Delta Chat needs)

| Feature | Madmail | Dovecot (cmdeploy) | Delta Chat core |
|---------|---------|-------------------|-----------------|
| IDLE | Yes | Yes | **Required** |
| UID FETCH / STORE | Yes | Yes | **Required** |
| MOVE | Yes | Yes | **Required** |
| SPECIAL-USE / LIST | Yes | Yes | **Required** |
| METADATA GET (TURN/Iroh) | Yes (endpoint) | Yes (dict proxy) | **Required** for calls |
| METADATA SET (push token) | Yes (`XDELTAPUSH`) | Yes (`XDELTAPUSH`) | Optional (push) |
| QUOTA | Yes | Yes | Used |
| CONDSTORE | No | Yes (Dovecot) | Used if present |
| COMPRESS | Yes | Optional | Used if present |
| XCHATMAIL | Yes | Yes | Detection |
| APPEND + PGP | Yes (wrapper) | Policy differs | Tests only |
| XDELTAPUSH | **Yes** (cap + SETMETADATA + notify proxy) | Yes | Optional mobile wake-up; see [23-push-notifications.md](23-push-notifications.md) |

---

## Stalwart reference (`context/stalwart/crates/imap`, `imap-proto`)

AGPL community code — **study for protocol design**, not a drop-in library (tied to `store`, `common::Server`, JMAP mail model).

### Crate split (recommended pattern for madmail-v2)

| Crate | Path | Role |
|-------|------|------|
| **`imap-proto`** | `context/stalwart/crates/imap-proto` | Parse commands, build responses, capabilities, UTF-7 mailbox names |
| **`imap`** | `context/stalwart/crates/imap` | Tokio session manager, per-command `op/*` handlers, links to storage |

```
imap-proto/
├── parser/       # Command → AST (LIST, FETCH, STORE, …)
├── protocol/     # Response serialization, Capability enum
└── receiver.rs   # Incremental line reader

imap/
├── core/         # Session, mailbox state, client I/O
│   ├── session.rs
│   ├── mailbox.rs
│   └── message.rs
└── op/           # One module per IMAP command
    ├── select.rs, fetch.rs, idle.rs, append.rs, …
```

### Commands implemented in Stalwart (community)

From `imap-proto/src/parser/mod.rs` + `imap/src/op/mod.rs`:

| Command | Stalwart `op/` | Needed for Delta Chat / Madmail |
|---------|----------------|----------------------------------|
| `CAPABILITY`, `NOOP`, `LOGOUT` | yes | Yes |
| `STARTTLS`, `LOGIN`, `AUTHENTICATE` | yes | Yes |
| `SELECT`, `EXAMINE`, `CLOSE`, `UNSELECT` | yes | SELECT, EXAMINE, CLOSE |
| `LIST`, `LSUB`, `NAMESPACE` | yes | LIST (LSUB optional) |
| `STATUS` | yes | Yes (UIDNEXT fallback) |
| `FETCH`, `STORE`, `SEARCH` | yes | UID FETCH, UID STORE |
| `COPY`, `MOVE`, `EXPUNGE` | yes | UID MOVE, CLOSE expunge |
| `APPEND` | yes | Yes (+ PGP wrapper in madmail-v2) |
| `IDLE` | yes | **Required** |
| `ENABLE` | yes | CONDSTORE (if advertised) |
| `GETQUOTA`, `GETQUOTAROOT` | yes | Yes |
| `ID` | yes | Yes (Delta Chat sends client ID) |
| `CREATE`, `DELETE`, `RENAME`, `SUBSCRIBE` | yes | Server-side / admin (not DC scheduler) |
| `SORT`, `THREAD` | yes | No (DC does not use) |
| `ACL` (`GETACL`, …) | yes | No (MVP) |
| `CHECK` | yes | No |

### Stalwart capabilities vs Chatmail needs

| Capability | Stalwart (typical) | Madmail | cmdeploy Dovecot |
|------------|-------------------|---------|------------------|
| `IDLE` | Yes | Yes | Yes |
| `MOVE` | Yes | Yes | Yes |
| `CONDSTORE` | Yes | No | Yes |
| `QUOTA` | Yes | Yes | Yes |
| `SPECIAL-USE` | Yes | Yes | Yes |
| `COMPRESS=DEFLATE` | Optional | Yes | Optional (`imap_zlib`) |
| `METADATA` | Not Chatmail keys | GET TURN/Iroh | dict proxy |
| `XCHATMAIL` | No | Yes | Yes |
| `XDELTAPUSH` | Yes (madmail-v2) | No | Yes |
| `IMAP4rev2` | Yes (greeting) | IMAP4rev1 | IMAP4rev1 |

**Conclusion:** The **command checklist in this document (Madmail + Delta Chat client) is complete** for requirements. Stalwart implements a **superset** of commands; madmail-v2 must add **Chatmail-specific** extensions (METADATA keys, `XCHATMAIL`, PGP on APPEND, IDLE notify on deliver) on top of a Stalwart-like `imap-proto` + `op` layout.

### What to copy vs reimplement

| Approach | Recommendation |
|----------|----------------|
| Vendor `imap` + `imap-proto` crates | **No** — hard dependency on Stalwart `store` / account model |
| Copy `imap-proto` parser/response code | Possible (AGPL); attribute Stalwart Labs |
| Use `async-imap` only on **client** side | Already in Delta Chat core — not for server |
| Fresh `imap-codec` + thin handlers | OK if smaller; Stalwart proves command coverage |

### IDLE in Stalwart

`op/idle.rs` + mailbox change notifications from storage layer. For madmail-v2, mirror Madmail: **signal IDLE waiters after SMTP/`/mxdeliv`/APPEND commit** (see `go-imap-sql/delivery.go`).

---

## Rust implementation notes (`madmail-v2`)

### `crates/chatmail-storage` — mailbox backend

IMAP sessions use `AppState::mailbox_store` (`MailboxStore`):

| Concern | Implementation |
|---------|----------------|
| Stable UIDs | `uidlist::UidListStore` — `chatmail-uidlist` file; UIDs never reused on delete |
| Listing | `maildir_cache::MaildirListCache` — skip `readdir` when directory mtimes unchanged |
| APPEND / delivery | `blob` + optional `cas` hardlink fan-out; `mail_fsync` / `blob_dedup` from config |
| IDLE EXISTS | `list_mailbox_messages` after delivery; unsolicited updates in `session.rs` |

See [`04-storage-layer.md`](04-storage-layer.md) for `StoragePolicy` and on-disk layout.

### `crates/chatmail-imap` — IDLE (implemented)

Mirrors Madmail `go-imap-sql/delivery.go` and Delta Chat `context/core/src/imap/idle.rs`:

| Step | Server (`session.rs`) | Client (Delta Chat / relay-ping) |
|------|----------------------|----------------------------------|
| 1 | `CAPABILITY` includes `IDLE` | Skip IDLE if no `IDLE` (60s fake poll) |
| 2 | `SELECT INBOX` → `* N EXISTS` | `select_folder` / baseline count |
| 3 | `IDLE` → `+ idling` | `client.Idle()` + disable short read timeout |
| 4 | SMTP/APPEND → `EventBus::notify_new_message` | (other connection submits mail) |
| 5 | IDLE loop → `* N EXISTS` + `* M RECENT` | `UnilateralDataHandler.Mailbox` (`NumMessages`) |
| 6 | Client `DONE` → `tag OK IDLE terminated` | `idleCmd.Close()` then `Wait()` → fetch |

**Server details:**

- `handle_idle`: `tokio::select!` — client `DONE` (biased first) or `EventBus` delivery for authenticated user.
- `emit_idle_updates`: reload maildir (mtime-sorted sequence), send unsolicited EXISTS/RECENT when count grows.
- `handle_fetch`: reload maildir on each FETCH; **sequence** `FETCH n` uses 1-based index; **`UID FETCH`** uses UID (was a common bug).
- FETCH literals: close with `)\r\n` immediately after literal (go-imap compatible).

**Tests:** `p5_ut01_test_capability_includes_chatmail_extensions` (includes `XDELTAPUSH`), `p6_ut01_test_idle_receives_delivery_event`, `p6_imap_idle_unsolicited_exists`, `imap_starttls_capability_and_login_gate`, `imap_starttls_upgrade_then_login` in `crates/chatmail-imap/src/session.rs`.

**relay-ping:** `internal/check/imapcheck/idle.go` — `waitInboxGrow` (IDLE + EXISTS), `probeIdleDelivery` (IDLE + second-session APPEND). Cross-delivery and Secure Join start IDLE **before** SMTP submit (core lifecycle).

### `crates/chatmail-imap` — XDELTAPUSH / SETMETADATA (implemented)

When `__PUSH_MODE__` is `auto` or `on`, `CAPABILITY` includes `METADATA` + `XDELTAPUSH`. When `off` (**default**), neither is advertised and `SETMETADATA` is rejected.

| Command | Behavior |
|---------|----------|
| `SETMETADATA INBOX (/private/devicetoken "…")` | Upsert token in SQLite `push_tokens` (per user, multiple devices); NIL removes |
| `GETMETADATA INBOX /private/devicetoken` | Return stored token (RFC 5464 entry list) |

Inbound mail triggers `AppState::notify_inbound_push()` → `chatmail-push` POSTs raw token to `notifications.delta.chat`. Full flow: [23-push-notifications.md](23-push-notifications.md).

**Tests:** `setmetadata_and_getmetadata_devicetoken_roundtrip`, `imap_e2e_push_devicetoken_setmetadata`, `imap_e2e_push_disabled_hides_capabilities`, `setmetadata-devicetoken` (relay-ping).

### Delta Chat desktop blockers (fixed in `chatmail-imap`)

| Symptom | Cause | Fix |
|---------|--------|-----|
| UI stuck on **“updating …”** | `configure_mvbox` calls `EXAMINE` → **`CLOSE`** → `SELECT`; `CLOSE` was missing (`command not supported`) | Implement `CLOSE` |
| MVBOX mis-detected | `SELECT`/`EXAMINE` ignored mailbox name (always INBOX) | Parse quoted mailbox; per-folder maildir (`INBOX`, `folders/DeltaChat/`) |
| No `DeltaChat` folder | `LIST` only returned INBOX | Init `DeltaChat` on LOGIN; `LIST`/`LSUB` both folders |
| `update_metadata` parse error / UI reconnect loop | `GETMETADATA` used per-key lines; **async-imap** expects RFC 5464 entry lists (`* METADATA "" (/shared/comment NIL …)`) | Single solicited line with parenthesized keys; NIL for empty comment/admin; `GETQUOTAROOT` + `GETQUOTA "ROOT"` |
| Mail not received (prefetch fails) | FETCH answered with bare `HEADER.FIELDS` instead of `BODY.PEEK[HEADER.FIELDS (…)]`; missing `INTERNALDATE` | Echo client section name; include `INTERNALDATE`; literal ends with `)\r\n` |

After rebuilding chatmail, Delta Chat should pass folder configure and enter **IDLE** on INBOX (see log: `IDLE entering wait-on-remote state`).

### Minimum viable server (Delta Chat parity with Madmail)

1. **Session**: AUTH, SELECT, UID FETCH, UID STORE, UID MOVE, CLOSE, LIST, STATUS, IDLE.
2. **Extensions**: MOVE, SPECIAL-USE, QUOTA (`GETQUOTA`/`GETQUOTAROOT`), METADATA GET (Chatmail keys), `XCHATMAIL`, **`XDELTAPUSH`** + `SETMETADATA /private/devicetoken` when push enabled (implemented in madmail-v2; default **off**).
3. **APPEND**: With PGP enforcement (mirror `encryptionWrapperUser`).
4. **IDLE**: Unsolicited `EXISTS` on delivery (SMTP + `/mxdeliv` + local append) — **see table above**.
5. **JIT**: Account/mailbox creation on first LOGIN (see `05-authentication.md`).

### Recommended follow-ups

- `COMPRESS=DEFLATE` (bandwidth)
- `CONDSTORE` (multi-device seen sync)
- ~~`SETMETADATA` + `XDELTAPUSH`~~ — **done** in madmail-v2 ([23-push-notifications.md](23-push-notifications.md))
- `APPENDLIMIT` in STATUS (large attachment policy)

### Crate strategy

Per `00-intro.md` / `01-architecture.md`: evaluate `imap-codec` for parsing and a custom Tokio state machine, or a maintained async IMAP server crate. **Do not assume** go-imap behavior — replicate capabilities above explicitly.

### Reference map

See also [`CONTEXT.md`](CONTEXT.md).

| Topic | madmail | cmrelay | cmdeploy | stalwart |
|-------|---------|---------|----------|----------|
| IMAP endpoint + extensions | [`endpoint/imap/imap.go`](../../context/madmail/internal/endpoint/imap/imap.go) | — (Dovecot) | [`dovecot.conf.j2`](../../context/cmdeploy/src/cmdeploy/dovecot/dovecot.conf.j2) | [`crates/imap/src/`](../../context/stalwart/crates/imap/src/) |
| Capabilities / storage | [`storage/imapsql/imapsql.go`](../../context/madmail/internal/storage/imapsql/imapsql.go) | — | Dovecot plugins in template | [`imap-proto/`](../../context/stalwart/crates/imap-proto/) |
| Mailbox ops | [`go-imap-sql/`](../../context/madmail/internal/go-imap-sql/) | — | — | [`imap/src/op/`](../../context/stalwart/crates/imap/src/op/) |
| IDLE notify | [`go-imap-sql/delivery.go`](../../context/madmail/internal/go-imap-sql/delivery.go) | [`notifier.rs`](../../context/cmrelay/src/filtermail/src/notifier.rs) | — | [`imap/src/op/idle.rs`](../../context/stalwart/crates/imap/src/op/idle.rs) |
| METADATA (TURN/Iroh) | [`imap.go`](../../context/madmail/internal/endpoint/imap/imap.go) | [`metadata.rs`](../../context/cmrelay/src/filtermail/src/metadata.rs), [`chatmail-metadata.service.f`](../../context/cmdeploy/src/cmdeploy/service/chatmail-metadata.service.f) | dict proxy in Dovecot config | — |
| Client expectations | [`madmail/chatmail-core/src/imap.rs`](../../context/madmail/chatmail-core/src/imap.rs) | — | [`test_2_deltachat.py`](../../context/cmdeploy/src/cmdeploy/tests/online/test_2_deltachat.py) | — |
| WebIMAP | [`docs/chatmail/webimap.md`](../../context/madmail/docs/chatmail/webimap.md), [`endpoint/webimap/`](../../context/madmail/internal/endpoint/webimap/) | — | — | — |

---

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md). **madmail** is the server to replicate; **cmdeploy** is the legacy Dovecot reference; **stalwart** is the Rust protocol engine to study; **cmrelay** covers metadata/TURN hooks on the old stack.

| Concern | Primary example |
|---------|-----------------|
| Full IMAP section above | This document § Madmail / cmdeploy / Stalwart |
| PGP on APPEND | [`encryptionWrapper`](../../context/madmail/internal/endpoint/imap/imap.go) + [`pgp_verify/`](../../context/madmail/internal/pgp_verify/) |
| E2E IDLE | [`test_12_smtp_imap_idle.py`](../../context/madmail/tests/deltachat-test/scenarios/test_12_smtp_imap_idle.py) (if present in tree) |
| Capabilities (cmdeploy) | [`test_0_login.py`](../../context/cmdeploy/src/cmdeploy/tests/online/test_0_login.py) (`XCHATMAIL`, `XDELTAPUSH`) |

---

## Related RFCs

IMAP core and extensions used by Madmail / Delta Chat. Full index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP4rev1 (base protocol) | [rfc3501.txt](RFC/rfc3501.txt) |
| [2595](https://datatracker.ietf.org/doc/html/rfc2595) | IMAP STARTTLS (`STARTTLS`, `[PRIVACYREQUIRED]`) | [rfc2595.txt](RFC/rfc2595.txt) |
| [8314](https://datatracker.ietf.org/doc/html/rfc8314) | TLS for IMAP access (993/143) | [rfc8314.txt](RFC/rfc8314.txt) |
| [2177](https://datatracker.ietf.org/doc/html/rfc2177) | IDLE | [rfc2177.txt](RFC/rfc2177.txt) |
| [5464](https://datatracker.ietf.org/doc/html/rfc5464) | METADATA | [rfc5464.txt](RFC/rfc5464.txt) |
| [2087](https://datatracker.ietf.org/doc/html/rfc2087) | QUOTA | [rfc2087.txt](RFC/rfc2087.txt) |
| [6851](https://datatracker.ietf.org/doc/html/rfc6851) | MOVE | [rfc6851.txt](RFC/rfc6851.txt) |
| [7889](https://datatracker.ietf.org/doc/html/rfc7889) | APPENDLIMIT | [rfc7889.txt](RFC/rfc7889.txt) |
| [3348](https://datatracker.ietf.org/doc/html/rfc3348) | CHILDREN | [rfc3348.txt](RFC/rfc3348.txt) |
| [6154](https://datatracker.ietf.org/doc/html/rfc6154) | SPECIAL-USE | [rfc6154.txt](RFC/rfc6154.txt) |
| [5256](https://datatracker.ietf.org/doc/html/rfc5256) | SORT / THREAD | [rfc5256.txt](RFC/rfc5256.txt) |
| [4978](https://datatracker.ietf.org/doc/html/rfc4978) | COMPRESS=DEFLATE | [rfc4978.txt](RFC/rfc4978.txt) |
| [2342](https://datatracker.ietf.org/doc/html/rfc2342) | NAMESPACE | [rfc2342.txt](RFC/rfc2342.txt) |
| [7162](https://datatracker.ietf.org/doc/html/rfc7162) | CONDSTORE (optional) | [rfc7162.txt](RFC/rfc7162.txt) |
| [2971](https://datatracker.ietf.org/doc/html/rfc2971) | ID (client identification) | [rfc2971.txt](RFC/rfc2971.txt) |
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Message-ID and headers in FETCH | [rfc5322.txt](RFC/rfc5322.txt) |
| [3156](https://datatracker.ietf.org/doc/html/rfc3156) | PGP on APPEND (with `12-security.md`) | [rfc3156.txt](RFC/rfc3156.txt) |

## Related TDD sections

- `02-smtp-server.md` — inbound mail triggers IDLE
- `05-authentication.md` — JIT / LOGIN (to be written)
- [`10-webimap.md`](10-webimap.md) — HTTP/WebSocket mapping + enable toggles
- [`11-proxy-services.md`](11-proxy-services.md) — TURN / Iroh metadata values
- [`plans/b9/`](../plans/b9/README.md) — TURN implementation + test matrix
- `12-security.md` — PGP-only policy
