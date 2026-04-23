package db

import (
	"fmt"
	"strings"

	// gormsqlite is our in-tree GORM dialector wired directly to
	// modernc.org/sqlite — no mattn/go-sqlite3, no glebarez fork.
	gormsqlite "github.com/themadorg/madmail/internal/db/gormsqlite"
	"gorm.io/driver/postgres"
	"gorm.io/gorm"
	"gorm.io/gorm/logger"
)

// augmentModerncSqliteDSN appends PRAGMAs the modernc driver applies at connect time.
// Without a non-zero busy_timeout, concurrent IMAP+SMTP (e.g. Delta Chat bring_online)
// can hit SQLITE_BUSY on the same credentials file during JIT user creation.
func augmentModerncSqliteDSN(dsn string) string {
	s := strings.TrimSpace(dsn)
	if s == "" {
		return s
	}
	// Already tuned or user-specified; do not override.
	lower := strings.ToLower(s)
	if strings.Contains(lower, "busy_timeout") {
		return s
	}
	sep := "?"
	if strings.Contains(s, "?") {
		sep = "&"
	}
	// See modernc.org/sqlite Open (DSN) docs: _pragma=… runs at connect.
	return s + sep + "_pragma=busy_timeout(30000)&_pragma=journal_mode(WAL)"
}

// New initializes a GORM database connection based on the driver and DSN.
func New(driver string, dsn []string, debug bool) (*gorm.DB, error) {
	dsnStr := strings.Join(dsn, " ")

	var dialector gorm.Dialector
	isSQLite := false
	switch driver {
	case "sqlite3", "sqlite":
		isSQLite = true
		// gormsqlite internally calls sql.Open("sqlite", …), which hits
		// the modernc.org/sqlite driver registered by the gormsqlite
		// package's own import — no cgo, no mattn/go-sqlite3.
		dialector = gormsqlite.Open(augmentModerncSqliteDSN(dsnStr))
	case "postgres":
		dialector = postgres.Open(dsnStr)
	default:
		return nil, fmt.Errorf("unsupported database driver: %s", driver)
	}

	gormCfg := &gorm.Config{}
	if !debug {
		gormCfg.Logger = logger.Default.LogMode(logger.Silent)
	}

	db, err := gorm.Open(dialector, gormCfg)
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	if isSQLite {
		// SQLite supports exactly one writer at a time. Capping the
		// database/sql pool to a single connection turns the pool into
		// a FIFO write queue, which, together with WAL + busy_timeout,
		// eliminates SQLITE_BUSY under concurrent IMAP+SMTP JIT auth.
		// Reads remain fast because the hot credentials are served from
		// pass_table.credCache (in-memory) and never hit this pool.
		if sqlDB, err := db.DB(); err == nil {
			sqlDB.SetMaxOpenConns(1)
			sqlDB.SetMaxIdleConns(1)
		}
	}

	return db, nil
}
