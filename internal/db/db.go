package db

import (
	"fmt"
	"strings"

	"gorm.io/driver/mysql"
	"gorm.io/driver/postgres"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
	"gorm.io/gorm/logger"
)

// New initializes a GORM database connection based on the driver and DSN.
func New(driver string, dsn []string, debug bool) (*gorm.DB, error) {
	dsnStr := strings.Join(dsn, " ")

	var dialector gorm.Dialector
	switch driver {
	case "sqlite3", "sqlite":
		dialector = sqlite.Open(dsnStr)
	case "postgres":
		dialector = postgres.Open(dsnStr)
	case "mysql":
		dialector = mysql.Open(dsnStr)
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

	// Configure connection pool for better scalability with many concurrent users
	sqlDB, err := db.DB()
	if err != nil {
		return nil, fmt.Errorf("failed to get underlying sql.DB: %w", err)
	}

	// Support up to 1000 concurrent IMAP connections
	// Each connection may need 1-2 DB connections for operations
	sqlDB.SetMaxOpenConns(2000)
	// Keep 100 idle connections ready to reduce connection overhead
	sqlDB.SetMaxIdleConns(100)

	return db, nil
}
