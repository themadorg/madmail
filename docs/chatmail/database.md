# Madmail Database Architecture

This document describes the database architecture of Madmail, including the in-memory SQLite support and synchronization mechanisms.

## Overview

Madmail uses [GORM](https://gorm.io/) as its ORM, providing a driver-agnostic layer for database operations. It supports SQLite, PostgreSQL, and MySQL.

### Core Components

- **Quota Management**: Handled via the `quotas` table, tracking user storage limits and account creation dates.
- **Contact Sharing**: Handled via the `contacts` table for shareable DeltaChat invitation links.
- **Generic Key-Value**: Modules like `sql_table` use the `TableEntry` model for simple storage.

## In-Memory SQLite Support

To enhance performance, Madmail supports running SQLite databases entirely in RAM. This significantly reduces disk I/O latency for high-traffic servers.

### How it Works

When `sqlite_in_memory` is habilitated:
1. **Startup**: If a persistent database file exists, its tables and data are loaded into a shared in-memory SQLite instance (`file::memory:?cache=shared`).
2. **Runtime**: All reads and writes occur in memory.
3. **Synchronization**: Every `sqlite_sync_interval` (e.g., 20 minutes), the in-memory state is dumped to the original disk path.

### Sync Logic (VACUUM INTO)

The synchronization process uses SQLite's `VACUUM INTO` command, which:
- Creates a consistent, non-blocked snapshots of the database.
- Performed to a temporary file (`.tmp`) and then renamed to the original path for atomic replacement.

### Transaction Locking

During the synchronization window, new database transactions are briefly blocked to ensure data consistency. This is managed by the `SyncLockPlugin` in `internal/db/db.go`, which implements a `sync.RWMutex`:
- **Read Operations**: Take a `RLock` (shared).
- **Background Sync**: Takes a `Lock` (exclusive).

## Configuration

To enable in-memory SQLite, add the following to your `storage.imapsql` or `auth.pass_table` blocks:

```maddy
storage.imapsql local_mailboxes {
    driver sqlite3
    dsn imapsql.db
    sqlite_in_memory yes
    sqlite_sync_interval 20m
}
```

Options:
- `sqlite_in_memory`: (bool) Set to `yes` to enable.
- `sqlite_sync_interval`: (duration) How often to sync to disk (e.g., `20m`, `1h`, `1d`). Set to `0` to disable syncing (RAM-only mode, data lost on restart).

## Implementation Details

The core logic is implemented in `internal/db/db.go`. The standard `New` function now handles the initialization of the sync loop and the registration of the `SyncLockPlugin`.

CLI tools (like `maddy ctl`) automatically detect when they are running as a non-server process and bypass the in-memory mode, directly accessing the disk file to provide inspection of the last synced state.
