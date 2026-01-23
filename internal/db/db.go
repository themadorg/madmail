package db

import (
	"fmt"
	"os"
	"strings"
	"sync"
	"time"

	"gorm.io/driver/mysql"
	"gorm.io/driver/postgres"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
	"gorm.io/gorm/logger"

	"github.com/themadorg/madmail/framework/module"
)

type Config struct {
	Driver       string
	DSN          []string
	Debug        bool
	InMemory     bool
	SyncInterval time.Duration
}

type SyncLockPlugin struct {
	mu *sync.RWMutex
}

func (p *SyncLockPlugin) Name() string { return "sync_lock" }

func (p *SyncLockPlugin) Initialize(db *gorm.DB) error {
	if err := db.Callback().Create().Before("*").Register("sync_lock:before", p.lock); err != nil {
		return err
	}
	if err := db.Callback().Query().Before("*").Register("sync_lock:before", p.lock); err != nil {
		return err
	}
	if err := db.Callback().Update().Before("*").Register("sync_lock:before", p.lock); err != nil {
		return err
	}
	if err := db.Callback().Delete().Before("*").Register("sync_lock:before", p.lock); err != nil {
		return err
	}
	if err := db.Callback().Row().Before("*").Register("sync_lock:before", p.lock); err != nil {
		return err
	}
	if err := db.Callback().Raw().Before("*").Register("sync_lock:before", p.lock); err != nil {
		return err
	}

	if err := db.Callback().Create().After("*").Register("sync_lock:after", p.unlock); err != nil {
		return err
	}
	if err := db.Callback().Query().After("*").Register("sync_lock:after", p.unlock); err != nil {
		return err
	}
	if err := db.Callback().Update().After("*").Register("sync_lock:after", p.unlock); err != nil {
		return err
	}
	if err := db.Callback().Delete().After("*").Register("sync_lock:after", p.unlock); err != nil {
		return err
	}
	if err := db.Callback().Row().After("*").Register("sync_lock:after", p.unlock); err != nil {
		return err
	}
	if err := db.Callback().Raw().After("*").Register("sync_lock:after", p.unlock); err != nil {
		return err
	}
	return nil
}

func (p *SyncLockPlugin) lock(db *gorm.DB) {
	p.mu.RLock()
}

func (p *SyncLockPlugin) unlock(db *gorm.DB) {
	p.mu.RUnlock()
}

// New initializes a GORM database connection based on the driver and DSN.
func New(cfg Config) (*gorm.DB, error) {
	dsnStr := strings.Join(cfg.DSN, " ")
	originalDSN := dsnStr

	var dialector gorm.Dialector
	if (cfg.Driver == "sqlite3" || cfg.Driver == "sqlite") && cfg.InMemory && !module.NoRun {
		// Use shared in-memory database
		dsnStr = "file::memory:?cache=shared"
	}

	switch cfg.Driver {
	case "sqlite3", "sqlite":
		dialector = sqlite.Open(dsnStr)
	case "postgres":
		dialector = postgres.Open(dsnStr)
	case "mysql":
		dialector = mysql.Open(dsnStr)
	default:
		return nil, fmt.Errorf("unsupported database driver: %s", cfg.Driver)
	}

	gormCfg := &gorm.Config{}
	if !cfg.Debug {
		gormCfg.Logger = logger.Default.LogMode(logger.Silent)
	}

	db, err := gorm.Open(dialector, gormCfg)
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	if (cfg.Driver == "sqlite3" || cfg.Driver == "sqlite") && cfg.InMemory && !module.NoRun {
		mu := &sync.RWMutex{}
		if err := db.Use(&SyncLockPlugin{mu: mu}); err != nil {
			return nil, fmt.Errorf("failed to register sync lock plugin: %w", err)
		}

		// Initial load from disk if file exists
		if originalDSN != "" && originalDSN != ":memory:" {
			if _, err := os.Stat(originalDSN); err == nil {
				if err := loadFromDisk(db, originalDSN); err != nil {
					return nil, fmt.Errorf("failed to load database from disk: %w", err)
				}
			}
		}

		if cfg.SyncInterval > 0 && originalDSN != "" && originalDSN != ":memory:" {
			go backgroundSync(db, originalDSN, cfg.SyncInterval, mu)
		}
	}

	return db, nil
}

func loadFromDisk(db *gorm.DB, path string) error {
	return db.Connection(func(tx *gorm.DB) error {
		// Use ATTACH to copy data from disk to memory
		err := tx.Exec(fmt.Sprintf("ATTACH DATABASE '%s' AS disk", path)).Error
		if err != nil {
			return err
		}
		defer tx.Exec("DETACH DATABASE disk")

		// Get table names from disk
		var tables []string
		err = tx.Raw("SELECT name FROM disk.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").Scan(&tables).Error
		if err != nil {
			return err
		}

		for _, table := range tables {
			// Drop memory table if exists (though it should be empty)
			tx.Exec(fmt.Sprintf("DROP TABLE IF EXISTS main.%s", table))
			// Create and copy
			if err := tx.Exec(fmt.Sprintf("CREATE TABLE main.%s AS SELECT * FROM disk.%s", table, table)).Error; err != nil {
				return err
			}
		}
		return nil
	})
}

func backgroundSync(db *gorm.DB, path string, interval time.Duration, mu *sync.RWMutex) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for range ticker.C {
		mu.Lock()
		// Sync memory to disk using VACUUM INTO
		// We use a temporary file then rename it to ensure atomicity
		tempPath := path + ".tmp"
		os.Remove(tempPath)

		err := db.Exec(fmt.Sprintf("VACUUM INTO '%s'", tempPath)).Error
		if err != nil {
			fmt.Fprintf(os.Stderr, "failed to sync in-memory database to disk: %v\n", err)
			mu.Unlock()
			continue
		}

		if err := os.Rename(tempPath, path); err != nil {
			fmt.Fprintf(os.Stderr, "failed to rename synced database: %v\n", err)
		}
		mu.Unlock()
	}
}
