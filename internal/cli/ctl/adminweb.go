package ctl

import (
	"fmt"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/db"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "admin-web",
		Usage: "Enable, disable, or configure the admin web dashboard",
		Description: `Manage the admin web dashboard served by the chatmail endpoint.

When enabled, the admin web SPA is served at the configured path (default: /admin).
When disabled, requests to the admin web path return 404.

The toggle takes effect immediately without a restart.
Changing the path requires a service restart.

Examples:
  maddy admin-web status           Show current status
  maddy admin-web enable           Enable the admin web dashboard
  maddy admin-web disable          Disable the admin web dashboard
  maddy admin-web path /admin      Set the admin web path (requires restart)
  maddy admin-web path --reset     Reset path to config default`,
		Subcommands: []*cli.Command{
			{
				Name:  "status",
				Usage: "Show admin web dashboard status",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: adminWebStatus,
			},
			{
				Name:  "enable",
				Usage: "Enable the admin web dashboard",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: adminWebEnable,
			},
			{
				Name:  "disable",
				Usage: "Disable the admin web dashboard",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: adminWebDisable,
			},
			{
				Name:      "path",
				Usage:     "Set or reset the admin web path",
				ArgsUsage: "[PATH]",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
					&cli.BoolFlag{
						Name:  "reset",
						Usage: "Reset path to config default",
					},
				},
				Action: adminWebPath,
			},
		},
	})
}

const (
	dbKeyAdminWebEnabled = "__ADMIN_WEB_ENABLED__"
	dbKeyAdminWebPath    = "__ADMIN_WEB_PATH__"
)

func adminWebStatus(c *cli.Context) error {
	cfg := getDBConfig(c)
	settings := readSettingsFromDB(cfg)

	enabled := "enabled"
	if v, ok := settings[dbKeyAdminWebEnabled]; ok && v == "false" {
		enabled = "disabled"
	}

	path := "(config default)"
	if v, ok := settings[dbKeyAdminWebPath]; ok && v != "" {
		path = v
	}

	fmt.Println()
	fmt.Printf("  Admin Web Dashboard:  %s\n", enabled)
	fmt.Printf("  Admin Web Path:       %s\n", path)
	fmt.Println()
	return nil
}

func adminWebEnable(c *cli.Context) error {
	cfg := getDBConfig(c)
	if err := setSetting(cfg, dbKeyAdminWebEnabled, "true"); err != nil {
		return fmt.Errorf("failed to enable admin web: %v", err)
	}
	fmt.Println("✅ Admin web dashboard enabled (effective immediately)")
	return nil
}

func adminWebDisable(c *cli.Context) error {
	cfg := getDBConfig(c)
	if err := setSetting(cfg, dbKeyAdminWebEnabled, "false"); err != nil {
		return fmt.Errorf("failed to disable admin web: %v", err)
	}
	fmt.Println("🚫 Admin web dashboard disabled (effective immediately)")
	return nil
}

func adminWebPath(c *cli.Context) error {
	cfg := getDBConfig(c)

	if c.Bool("reset") {
		if err := deleteSetting(cfg, dbKeyAdminWebPath); err != nil {
			return fmt.Errorf("failed to reset admin web path: %v", err)
		}
		fmt.Println("🔄 Admin web path reset to config default (restart required)")
		return nil
	}

	newPath := c.Args().First()
	if newPath == "" {
		// Show current path
		settings := readSettingsFromDB(cfg)
		if v, ok := settings[dbKeyAdminWebPath]; ok && v != "" {
			fmt.Printf("Current admin web path: %s (DB override)\n", v)
		} else {
			fmt.Println("Current admin web path: (config default)")
		}
		return nil
	}

	if err := setSetting(cfg, dbKeyAdminWebPath, newPath); err != nil {
		return fmt.Errorf("failed to set admin web path: %v", err)
	}
	fmt.Printf("✅ Admin web path set to %s (restart required)\n", newPath)
	return nil
}

// setSetting writes a key-value pair to the credentials database.
func setSetting(cfg dbConfig, key, value string) error {
	database, err := openDB(cfg)
	if err != nil {
		return fmt.Errorf("failed to open database: %v", err)
	}
	defer closeDB(database)

	entry := db.TableEntry{Key: key, Value: value}
	result := database.Table(settingsKVTable(cfg)).Where("\"key\" = ?", key).Assign(entry).FirstOrCreate(&entry)
	return result.Error
}

// deleteSetting removes a setting key from the credentials database.
func deleteSetting(cfg dbConfig, key string) error {
	database, err := openDB(cfg)
	if err != nil {
		return fmt.Errorf("failed to open database: %v", err)
	}
	defer closeDB(database)

	result := database.Table(settingsKVTable(cfg)).Where("\"key\" = ?", key).Delete(&db.TableEntry{})
	return result.Error
}
