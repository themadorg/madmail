package ctl

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/themadorg/madmail/framework/config"
	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/db"
	"github.com/urfave/cli/v2"
	"golang.org/x/sys/unix"
	"gorm.io/gorm"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "admin-token",
		Usage: "Display the admin API credentials",
		Description: `Display the admin API credentials for this server.

The token is automatically generated on first chatmail startup and stored
in the state directory (default: /var/lib/maddy/admin_token).

If admin_token is set explicitly in maddy.conf, that value is used instead.
If admin_token is set to "disabled", the admin API is not available.

Usage example:
  TOKEN=$(maddy admin-token --raw)
  curl -X POST https://your-server/api/admin \
    -H 'Content-Type: application/json' \
    -d "{\"method\":\"GET\",\"resource\":\"/admin/status\",\"headers\":{\"Authorization\":\"Bearer $TOKEN\"}}"`,
		Flags: []cli.Flag{
			&cli.StringFlag{
				Name:    "state-dir",
				Usage:   "Path to the state directory",
				EnvVars: []string{"MADDY_STATE_DIR"},
			},
			&cli.BoolFlag{
				Name:  "raw",
				Usage: "Print only the raw token (for use in scripts)",
			},
		},
		Action: func(c *cli.Context) error {
			stateDir := c.String("state-dir")
			if stateDir == "" {
				stateDir = config.StateDirectory
			}
			if stateDir == "" {
				stateDir = "/var/lib/maddy"
			}

			// Check the config file for an explicit admin_token
			confToken := getTokenFromConfig("/etc/maddy/maddy.conf")
			if confToken == "disabled" {
				return fmt.Errorf("admin API is explicitly disabled in config (admin_token disabled)")
			}

			var token string
			if confToken != "" {
				token = confToken
			} else {
				// Read from state directory
				tokenPath := filepath.Join(stateDir, "admin_token")
				data, err := os.ReadFile(tokenPath)
				if err != nil {
					if os.IsNotExist(err) {
						return fmt.Errorf("admin token not found at %s\nThe token is generated on first chatmail startup. Start maddy first.", tokenPath)
					}
					return fmt.Errorf("failed to read admin token: %v", err)
				}
				token = strings.TrimSpace(string(data))
				if token == "" {
					return fmt.Errorf("admin token file is empty: %s", tokenPath)
				}
			}

			// Raw mode: print only the token (for piping / scripts)
			if c.Bool("raw") || !isTerminal(os.Stdout) {
				fmt.Print(token)
				return nil
			}

			// Pretty mode: show labeled credentials
			dbPath := filepath.Join(stateDir, "credentials.db")
			apiURL := buildAdminURL(dbPath)
			fmt.Println()
			fmt.Printf("  Admin API URL:   %s\n", apiURL)
			fmt.Printf("  Admin Token:     %s\n", token)
			fmt.Println()
			return nil
		},
	})
}

// isTerminal returns true if f is connected to a terminal.
func isTerminal(f *os.File) bool {
	_, err := unix.IoctlGetTermios(int(f.Fd()), unix.TCGETS)
	return err == nil
}

// buildAdminURL constructs the admin API URL by reading host from
// maddy.conf and overriding with DB-stored settings via GORM.
func buildAdminURL(dbPath string) string {
	// 1. Read hostname from maddy.conf $(hostname) = ...
	host := getHostnameFromConfig("/etc/maddy/maddy.conf")
	if host == "" {
		host = "your-server"
	}

	// Defaults
	port := "443"
	adminPath := "/api/admin"

	// 2. Read DB overrides using GORM (same pattern as the rest of the codebase)
	settings := readSettingsFromDB(dbPath)
	if v, ok := settings["__SMTP_HOSTNAME__"]; ok && v != "" {
		host = v
	}
	if v, ok := settings["__HTTPS_PORT__"]; ok && v != "" {
		port = v
	}
	if v, ok := settings["__ADMIN_PATH__"]; ok && v != "" {
		adminPath = v
	}

	// Clean up host: remove brackets if present (e.g. [1.2.3.4])
	host = strings.Trim(host, "[]")

	// Build URL
	if port == "443" {
		return fmt.Sprintf("https://%s%s", host, adminPath)
	}
	return fmt.Sprintf("https://%s:%s%s", host, port, adminPath)
}

// readSettingsFromDB opens the credentials.db via GORM and reads all
// settings keys (those starting with "__") from the passwords table.
func readSettingsFromDB(dbPath string) map[string]string {
	result := make(map[string]string)

	if _, err := os.Stat(dbPath); err != nil {
		return result
	}

	database, err := db.New("sqlite3", []string{dbPath}, false)
	if err != nil {
		return result
	}
	defer func() {
		if sqlDB, err := database.DB(); err == nil {
			sqlDB.Close()
		}
	}()

	// Query settings keys from the passwords table
	var entries []db.TableEntry
	err = database.Table("passwords").
		Where("\"key\" LIKE ?", "__%__").
		Find(&entries).Error
	if err != nil && err != gorm.ErrRecordNotFound {
		return result
	}

	for _, e := range entries {
		result[e.Key] = e.Value
	}
	return result
}

// getHostnameFromConfig reads $(hostname) = ... from maddy.conf.
func getHostnameFromConfig(confPath string) string {
	data, err := os.ReadFile(confPath)
	if err != nil {
		return ""
	}

	lines := strings.Split(string(data), "\n")
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "$(hostname)") && strings.Contains(trimmed, "=") {
			parts := strings.SplitN(trimmed, "=", 2)
			if len(parts) == 2 {
				return strings.TrimSpace(parts[1])
			}
		}
	}
	return ""
}

// getTokenFromConfig reads the maddy.conf file and extracts the admin_token value
// from the first chatmail block that has it set.
func getTokenFromConfig(confPath string) string {
	data, err := os.ReadFile(confPath)
	if err != nil {
		return ""
	}

	lines := strings.Split(string(data), "\n")
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "admin_token ") {
			parts := strings.Fields(trimmed)
			if len(parts) >= 2 {
				return parts[1]
			}
		}
	}
	return ""
}
