package ctl

import (
	"fmt"
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
// TableName is the GORM/physical table for __*__ key-value settings.
// It must match the settings_table block when present, or the main table
// block otherwise (see pass_table.Auth GetSetting/SetSetting).
type dbConfig struct {
	Driver    string
	DSN       string
	TableName string
}

// sqlTableConfig describes a single `table sql_table { driver; dsn; table_name }`
// block. It's used by the accounts CLI to talk to the passwords and
// (optional) settings tables directly via GORM — no module registration.
type sqlTableConfig struct {
	Driver    string
	DSN       string
	TableName string
}

// storageConfig describes the driver/dsn of a `storage.imapsql <name> { ... }` block.
type storageConfig struct {
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

	// Default: SQLite credentials.db in state dir, table "passwords"
	cfg := dbConfig{
		Driver:    "sqlite3",
		DSN:       filepath.Join(stateDir, "credentials.db"),
		TableName: "passwords",
	}

	confPath := frameworkconfig.ConfigFile()
	data, err := os.ReadFile(confPath)
	if err != nil {
		return cfg
	}

	driver, dsn, tableName := parseAuthTableKVStore(string(data))
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
	if tableName != "" {
		cfg.TableName = tableName
	}

	return cfg
}

// parseAuthTableKVStore returns the driver, DSN, and table_name used for
// key-value __*__ settings in the first auth.pass_table block. It prefers
// settings_table when present, otherwise the main table block (same as
// internal/auth/pass_table).
// tableName defaults to "passwords" in sql_table when the directive is empty.
func parseAuthTableKVStore(conf string) (driver, dsn, tableName string) {
	block, ok := firstAuthPassTableBlockLines(conf)
	if !ok {
		return "", "", ""
	}
	if d, s, t := extractSQLTableBlockFull(block, "settings_table"); d != "" && s != "" {
		if t == "" {
			t = "passwords"
		}
		return d, s, t
	}
	if d, s, t := extractSQLTableBlockFull(block, "table"); d != "" && s != "" {
		if t == "" {
			t = "passwords"
		}
		return d, s, t
	}
	return "", "", ""
}

// firstAuthPassTableBlockLines returns the first auth.pass_table { ... } block as lines.
func firstAuthPassTableBlockLines(conf string) ([]string, bool) {
	lines := strings.Split(conf, "\n")

	inAuthPassTable := false
	braceDepth := 0
	authBraceStart := 0

	authStart := -1
	authEnd := -1
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)

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
		return nil, false
	}
	return lines[authStart : authEnd+1], true
}

// settingsKVTable is the GORM table name for __*__ keys (defaults to "passwords").
func settingsKVTable(cfg dbConfig) string {
	if cfg.TableName != "" {
		return cfg.TableName
	}
	return "passwords"
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

// parseAuthPassTableByName locates a specific `auth.pass_table <blockName> { ... }`
// block inside the given maddy.conf contents and returns its primary
// `table sql_table { ... }` sub-block plus the optional
// `settings_table sql_table { ... }` sub-block. Returned sqlTableConfigs have
// empty Driver/DSN if the respective block isn't present.
func parseAuthPassTableByName(conf, blockName string) (main, settings sqlTableConfig) {
	authBlock, ok := findNamedBlock(conf, "auth.pass_table", blockName)
	if !ok {
		return
	}
	if d, s, t := extractSQLTableBlockFull(authBlock, "table"); d != "" {
		main = sqlTableConfig{Driver: d, DSN: s, TableName: t}
	}
	if d, s, t := extractSQLTableBlockFull(authBlock, "settings_table"); d != "" {
		settings = sqlTableConfig{Driver: d, DSN: s, TableName: t}
	}
	return
}

// parseStorageImapsqlByName locates `storage.imapsql <blockName> { ... }` and
// returns its driver and dsn directives.
func parseStorageImapsqlByName(conf, blockName string) storageConfig {
	block, ok := findNamedBlock(conf, "storage.imapsql", blockName)
	if !ok {
		return storageConfig{}
	}
	var cfg storageConfig
	for _, line := range block {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "#") || strings.HasPrefix(trimmed, "//") {
			continue
		}
		fields := strings.Fields(trimmed)
		if len(fields) < 2 {
			continue
		}
		switch fields[0] {
		case "driver":
			cfg.Driver = fields[1]
		case "dsn":
			rest := strings.TrimSpace(strings.TrimPrefix(trimmed, "dsn"))
			// Strip trailing comments
			if idx := strings.Index(rest, "#"); idx >= 0 {
				rest = strings.TrimSpace(rest[:idx])
			}
			cfg.DSN = strings.Trim(rest, `"'`)
		}
	}
	return cfg
}

// findNamedBlock returns the body lines (including opening and closing braces)
// of a top-level `<prefix> <name> { ... }` block in the config.
func findNamedBlock(conf, prefix, name string) ([]string, bool) {
	lines := strings.Split(conf, "\n")
	header := prefix + " " + name

	start := -1
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "#") || strings.HasPrefix(trimmed, "//") {
			continue
		}
		// Accept either "prefix name {" or "prefix name" on its own line.
		if trimmed == header || strings.HasPrefix(trimmed, header+" ") || strings.HasPrefix(trimmed, header+"\t") {
			start = i
			break
		}
	}
	if start < 0 {
		return nil, false
	}

	braceDepth := 0
	opened := false
	for i := start; i < len(lines); i++ {
		trimmed := strings.TrimSpace(lines[i])
		if strings.HasPrefix(trimmed, "#") || strings.HasPrefix(trimmed, "//") {
			continue
		}
		braceDepth += strings.Count(trimmed, "{") - strings.Count(trimmed, "}")
		if strings.Contains(trimmed, "{") {
			opened = true
		}
		if opened && braceDepth <= 0 {
			return lines[start : i+1], true
		}
	}
	return nil, false
}

// extractSQLTableBlockFull finds a `<prefix> sql_table { ... }` block within
// the given lines and extracts driver, dsn, and table_name.
func extractSQLTableBlockFull(lines []string, prefix string) (driver, dsn, tableName string) {
	inBlock := false
	braceDepth := 0

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "#") || strings.HasPrefix(trimmed, "//") {
			continue
		}

		if !inBlock {
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

		fields := strings.Fields(trimmed)
		if len(fields) < 2 {
			continue
		}
		switch fields[0] {
		case "driver":
			driver = fields[1]
		case "dsn":
			rest := strings.TrimSpace(strings.TrimPrefix(trimmed, "dsn"))
			if idx := strings.Index(rest, "#"); idx >= 0 {
				rest = strings.TrimSpace(rest[:idx])
			}
			dsn = strings.Trim(rest, `"'`)
		case "table_name":
			tableName = fields[1]
		}
	}
	return driver, dsn, tableName
}

// resolveSQLiteDSN makes relative sqlite paths absolute by prepending stateDir.
func resolveSQLiteDSN(driver, dsn, stateDir string) string {
	if driver != "sqlite3" && driver != "sqlite" {
		return dsn
	}
	if filepath.IsAbs(dsn) {
		return dsn
	}
	return filepath.Join(stateDir, dsn)
}

// openSQLTableDB opens a GORM connection to an auth sql_table backend.
// If the sqlite file doesn't exist yet, we still proceed (the DB may be
// created on first write — matches what sql_table does at runtime).
func openSQLTableDB(cfg sqlTableConfig) (*gorm.DB, error) {
	if cfg.Driver == "" || cfg.DSN == "" {
		return nil, fmt.Errorf("missing driver/dsn")
	}
	return db.New(cfg.Driver, []string{cfg.DSN}, false)
}
