package ctl

import (
	"os"
	"path/filepath"
	"strings"

	frameworkconfig "github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/internal/db"
	"github.com/urfave/cli/v2"
	"gorm.io/gorm"
)

// dbConfig holds the driver and DSN parsed from maddy.conf for the
// credentials / settings table (inside the auth.pass_table block).
type dbConfig struct {
	Driver string
	DSN    string
}

// getStateDir returns the effective state directory from the CLI flag,
// framework config, or the default.
func getStateDir(c *cli.Context) string {
	stateDir := c.String("state-dir")
	if stateDir == "" {
		stateDir = frameworkconfig.StateDirectory
	}
	if stateDir == "" {
		stateDir = "/var/lib/" + frameworkconfig.BinaryName()
	}
	return stateDir
}

// getDBConfig reads maddy.conf and extracts the driver + DSN for the
// credentials table (the first `table sql_table { ... }` block inside
// `auth.pass_table`). Falls back to sqlite3 + credentials.db if parsing
// fails or the config block is not found.
func getDBConfig(c *cli.Context) dbConfig {
	stateDir := getStateDir(c)

	// Default: SQLite credentials.db in state dir
	cfg := dbConfig{
		Driver: "sqlite3",
		DSN:    filepath.Join(stateDir, "credentials.db"),
	}

	confPath := frameworkconfig.ConfigFile()
	data, err := os.ReadFile(confPath)
	if err != nil {
		return cfg
	}

	driver, dsn := parseAuthTableDriverDSN(string(data))
	if driver == "" || dsn == "" {
		return cfg
	}

	cfg.Driver = driver

	// For SQLite, resolve relative DSN paths against state_dir
	if driver == "sqlite3" || driver == "sqlite" {
		if !filepath.IsAbs(dsn) {
			dsn = filepath.Join(stateDir, dsn)
		}
	}
	cfg.DSN = dsn

	return cfg
}

// parseAuthTableDriverDSN extracts the driver and dsn from the first
// `table sql_table { ... }` block inside an `auth.pass_table` block in
// the maddy config. It also checks for a `settings_table sql_table` block
// inside the same auth.pass_table, and if found, returns its driver/dsn
// instead (since that's where settings keys are stored).
func parseAuthTableDriverDSN(conf string) (driver, dsn string) {
	lines := strings.Split(conf, "\n")

	inAuthPassTable := false
	braceDepth := 0
	authBraceStart := 0

	// Phase 1: find the auth.pass_table block boundaries
	authStart := -1
	authEnd := -1
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)

		// Skip comments
		if strings.HasPrefix(trimmed, "#") || strings.HasPrefix(trimmed, "//") {
			continue
		}

		if !inAuthPassTable {
			if strings.HasPrefix(trimmed, "auth.pass_table ") {
				inAuthPassTable = true
				authStart = i
				braceDepth = 0
				authBraceStart = 0
				if strings.Contains(trimmed, "{") {
					braceDepth++
					authBraceStart = 1
				}
			}
			continue
		}

		// Inside auth.pass_table
		braceDepth += strings.Count(trimmed, "{") - strings.Count(trimmed, "}")
		if authBraceStart > 0 && braceDepth <= 0 {
			authEnd = i
			break
		}
		if authBraceStart == 0 && braceDepth > 0 {
			authBraceStart = braceDepth
		}
	}

	if authStart < 0 || authEnd < 0 {
		return "", ""
	}

	authBlock := lines[authStart : authEnd+1]

	// Phase 2: Look for settings_table sql_table block first, then fall back to table sql_table
	if d, s := extractSQLTableBlock(authBlock, "settings_table"); d != "" && s != "" {
		return d, s
	}
	return extractSQLTableBlock(authBlock, "table")
}

// extractSQLTableBlock finds a `<prefix> sql_table { ... }` block within the
// given lines and extracts the driver and dsn directives.
func extractSQLTableBlock(lines []string, prefix string) (driver, dsn string) {
	inBlock := false
	braceDepth := 0

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)

		// Skip comments
		if strings.HasPrefix(trimmed, "#") || strings.HasPrefix(trimmed, "//") {
			continue
		}

		if !inBlock {
			// Match e.g. "table sql_table {" or "settings_table sql_table {"
			if strings.HasPrefix(trimmed, prefix+" sql_table") {
				inBlock = true
				braceDepth = 0
				if strings.Contains(trimmed, "{") {
					braceDepth++
				}
			}
			continue
		}

		braceDepth += strings.Count(trimmed, "{") - strings.Count(trimmed, "}")
		if braceDepth <= 0 {
			break
		}

		// Parse directives within the sql_table block
		fields := strings.Fields(trimmed)
		if len(fields) >= 2 {
			switch fields[0] {
			case "driver":
				driver = fields[1]
			case "dsn":
				// DSN may be a quoted string or a bare word
				rest := strings.TrimPrefix(trimmed, "dsn")
				rest = strings.TrimSpace(rest)
				dsn = strings.Trim(rest, `"'`)
			}
		}
	}

	return driver, dsn
}

// openDB opens a GORM database connection using the driver/DSN from the
// config. For SQLite, it verifies the file exists first.
func openDB(cfg dbConfig) (*gorm.DB, error) {
	if cfg.Driver == "sqlite3" || cfg.Driver == "sqlite" {
		if _, err := os.Stat(cfg.DSN); err != nil {
			return nil, err
		}
	}
	return db.New(cfg.Driver, []string{cfg.DSN}, false)
}

// closeDB cleanly closes a GORM database connection.
func closeDB(database *gorm.DB) {
	if sqlDB, err := database.DB(); err == nil {
		sqlDB.Close()
	}
}
