package db

import (
	"fmt"
	"strings"

	"gorm.io/driver/mysql"
	"gorm.io/driver/postgres"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
)

// New initializes a GORM database connection based on the driver and DSN.
func New(driver string, dsn []string) (*gorm.DB, error) {
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

	db, err := gorm.Open(dialector, &gorm.Config{})
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	return db, nil
}
