package imapsql

import (
	"database/sql"
	"testing"

	_ "github.com/mattn/go-sqlite3"
)

func TestConfigureEngineAppliesSQLiteCacheSize(t *testing.T) {
	t.Parallel()

	sqlDB, err := sql.Open("sqlite3", ":memory:")
	if err != nil {
		t.Fatalf("failed to open sqlite3: %v", err)
	}
	t.Cleanup(func() {
		_ = sqlDB.Close()
	})

	b := &Backend{
		Opts: Opts{
			CacheSize: -2048,
		},
		db: db{
			DB:     sqlDB,
			driver: "sqlite3",
			dsn:    ":memory:",
		},
	}

	if err := b.configureEngine(); err != nil {
		t.Fatalf("configureEngine failed: %v", err)
	}

	if got := sqlDB.Stats().MaxOpenConnections; got != 1 {
		t.Fatalf("unexpected max open conns for in-memory sqlite: got %d, want 1", got)
	}

	var cacheSize int
	if err := sqlDB.QueryRow(`PRAGMA cache_size`).Scan(&cacheSize); err != nil {
		t.Fatalf("failed to read sqlite cache_size pragma: %v", err)
	}
	if cacheSize != -2048 {
		t.Fatalf("unexpected sqlite cache_size: got %d, want -2048", cacheSize)
	}
}
