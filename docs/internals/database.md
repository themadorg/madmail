# Database Abstraction Layer

Madmail uses [GORM](https://gorm.io) as its ORM (Object-Relational Mapper) to abstract
database operations across multiple backends. The implementation lives in `internal/db/`.

## Supported Drivers

| Driver     | GORM Dialector         | Config Value   | Typical DSN                                      |
|------------|------------------------|----------------|--------------------------------------------------|
| SQLite     | `gorm.io/driver/sqlite`   | `sqlite3`      | `imapsql.db` (filename, can be relative)         |
| PostgreSQL | `gorm.io/driver/postgres` | `postgres`     | `host=localhost user=maddy dbname=maddy sslmode=disable` |
| MySQL      | `gorm.io/driver/mysql`    | `mysql`        | `maddy:password@tcp(127.0.0.1:3306)/maddy`       |

The driver and DSN are configured in the `storage.imapsql` block of `maddy.conf`:

```
storage.imapsql local_mailboxes {
    driver sqlite3
    dsn imapsql.db
}
```

For PostgreSQL:
```
storage.imapsql local_mailboxes {
    driver postgres
    dsn host=localhost user=maddy dbname=maddy sslmode=disable
}
```

## Package Structure

### `internal/db/db.go` — Connection Factory

```go
func New(driver string, dsn []string, debug bool) (*gorm.DB, error)
```

Creates a new GORM database connection. The `dsn` parts are joined with spaces
before passing to the GORM dialector.

- **`driver`**: One of `sqlite3`, `sqlite`, `postgres`, or `mysql`.
- **`dsn`**: DSN string parts (joined with spaces). For SQLite, this is typically
  a filename. For PostgreSQL/MySQL, it's the connection string.
- **`debug`**: When `false`, GORM logging is suppressed (Silent mode).
  This is important for the [No Log Policy](../chatmail/nolog.md).

### `internal/db/models.go` — GORM Models

Three models are defined:

#### `Quota`
Tracks per-user storage quotas and account creation/login timestamps.

```go
type Quota struct {
    Username     string `gorm:"primaryKey"`
    MaxStorage   int64
    CreatedAt    int64
    FirstLoginAt int64
}
```

- **`Username`**: Primary key — the full email address (e.g. `user@example.com`).
- **`MaxStorage`**: Per-user storage limit in bytes. `0` means inherit global default.
- **`CreatedAt`**: Unix timestamp of account creation.
- **`FirstLoginAt`**: Unix timestamp of first IMAP login. Value `1` means "never logged in"
  (used by `prune-unused` to identify dormant accounts).

A special row with `Username = "__GLOBAL_DEFAULT__"` stores the server-wide default quota,
set via `maddy imap-acct quota set-default`.

#### `Contact`
Used by the contact sharing feature.

```go
type Contact struct {
    Slug      string    `gorm:"primaryKey"`
    URL       string    `gorm:"column:url;not null"`
    Name      string
    CreatedAt time.Time `gorm:"autoCreateTime"`
}
```

#### `TableEntry`
Generic key-value store for the `sql_table` module.

```go
type TableEntry struct {
    Key   string `gorm:"primaryKey"`
    Value string `gorm:"not null"`
}
```

## Usage Across the Codebase

### Storage Layer (`internal/storage/imapsql/`)

The main consumer. `Storage.GORMDB` is initialized during `Init()`:

```go
store.GORMDB, err = mdb.New(driver, dsn, store.Log.Debug)
```

Used for:
- Quota management (get/set/reset per-user and global)
- Storage statistics (`GetStat()` — total bytes, user count)
- Message purging (all, read-only, per-user, by age)
- First-login tracking and auto-migration
- Unused account pruning

### Chatmail Endpoint (`internal/endpoint/chatmail/`)

Opens its own GORM connection for registration controls:

```go
gdb, err := mdb.New(driver, dsn, e.logger.Debug)
```

### Contact Sharing CLI (`internal/cli/ctl/sharing.go`)

Opens a separate GORM connection for the sharing database:

```go
db, err := mdb.New(driver, dsn, ctx.Bool("debug"))
```

For SQLite, this defaults to `{state_dir}/sharing.db`.

### Status Command (`internal/cli/ctl/online.go`)

Opens a read-only GORM connection to query user count:

```go
db, err := mdb.New(driver, []string{dsn}, false)
db.Table("users").Count(&count)
```

### SQL Table Module (`internal/table/sql_query.go`)

Opens a GORM connection for generic key-value lookups.

### Update Pipe (`internal/updatepipe/pubsub/`)

For PostgreSQL setups, opens a GORM connection for PubSub-based update notifications
between multiple maddy instances.

## Schema Management

GORM's `AutoMigrate` is used for the `quotas` table:

```go
store.GORMDB.AutoMigrate(&mdb.Quota{})
```

The core IMAP tables (`users`, `mboxes`, `msgs`, etc.) are managed by
[go-imap-sql](https://github.com/foxcpp/go-imap-sql), not by GORM.
GORM only manages the Madmail-specific extension tables.

## SQLite Notes

- For SQLite, the DSN is a filename relative to `state_dir` (default: `/var/lib/maddy`).
- When `MADDY_SQLITE_UNSAFE_SYNC_OFF=1` is set, WAL mode and synchronous=OFF are enabled
  for performance (at the cost of durability on crash).
- SQLite uses Unix domain sockets for inter-process update notifications.

## PostgreSQL Notes

- For PostgreSQL, the DSN is a standard libpq connection string.
- PubSub (LISTEN/NOTIFY) is used for inter-process update notifications,
  allowing multiple maddy instances to share the same database.
