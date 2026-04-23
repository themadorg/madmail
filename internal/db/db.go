package db

import (
	"fmt"
	"strings"

	"gorm.io/driver/postgres"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
	"gorm.io/gorm/logger"
	_ "modernc.org/sqlite" // registers database/sql driver name "sqlite" (pure Go; works with CGO_ENABLED=0)
)

// New initializes a GORM database connection based on the driver and DSN.
func New(driver string, dsn []string, debug bool) (*gorm.DB, error) {
	dsnStr := strings.Join(dsn, " ")

	var dialector gorm.Dialector
	switch driver {
	case "sqlite3", "sqlite":
		// gorm's sqlite.Open() defaults to driver "sqlite3" (mattn); use "sqlite" so
		// we use modernc.org/sqlite (imported above), which works with CGO_ENABLED=0.
		dialector = sqlite.Dialector{DSN: dsnStr, DriverName: "sqlite"}
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

	return db, nil
}
