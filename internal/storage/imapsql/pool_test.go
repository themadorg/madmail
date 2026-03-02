package imapsql

import (
	"database/sql"
	"testing"
	"time"
)

func openSQLiteTestDB(t *testing.T) *sql.DB {
	t.Helper()

	if sqliteImpl == "missing" {
		t.Skip("sqlite is not available in this build")
	}

	driver := "sqlite3"
	if sqliteImpl == "modernc" {
		driver = "sqlite"
	}

	db, err := sql.Open(driver, ":memory:")
	if err != nil {
		t.Fatalf("failed to open sqlite db: %v", err)
	}
	t.Cleanup(func() {
		_ = db.Close()
	})
	return db
}

func TestApplySQLPoolLimitsSetsMaxOpenConns(t *testing.T) {
	t.Parallel()

	db := openSQLiteTestDB(t)
	applySQLPoolLimits(db, 3, 9, time.Minute, time.Minute)

	if got := db.Stats().MaxOpenConnections; got != 3 {
		t.Fatalf("unexpected max open connections: got %d, want %d", got, 3)
	}
}

func TestApplySQLPoolLimitsNilDB(t *testing.T) {
	t.Parallel()

	applySQLPoolLimits(nil, 3, 3, time.Minute, time.Minute)
}
