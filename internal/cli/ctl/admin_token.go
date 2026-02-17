package ctl

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/themadorg/madmail/framework/config"
	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "admin-token",
		Usage: "Display the admin API token",
		Description: `Display the admin API token for this server.

The token is automatically generated on first chatmail startup and stored
in the state directory (default: /var/lib/maddy/admin_token).

If admin_token is set explicitly in maddy.conf, that value is used instead.
If admin_token is set to "disabled", the admin API is not available.

Usage example:
  TOKEN=$(maddy admin-token)
  curl -X POST https://your-server/api/admin \
    -H 'Content-Type: application/json' \
    -d "{\"method\":\"GET\",\"resource\":\"/admin/status\",\"headers\":{\"Authorization\":\"Bearer $TOKEN\"}}"`,
		Flags: []cli.Flag{
			&cli.StringFlag{
				Name:    "state-dir",
				Usage:   "Path to the state directory",
				EnvVars: []string{"MADDY_STATE_DIR"},
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
			if confToken != "" {
				fmt.Println(confToken)
				return nil
			}

			// Read from state directory
			tokenPath := filepath.Join(stateDir, "admin_token")
			data, err := os.ReadFile(tokenPath)
			if err != nil {
				if os.IsNotExist(err) {
					return fmt.Errorf("admin token not found at %s\nThe token is generated on first chatmail startup. Start maddy first.", tokenPath)
				}
				return fmt.Errorf("failed to read admin token: %v", err)
			}

			token := strings.TrimSpace(string(data))
			if token == "" {
				return fmt.Errorf("admin token file is empty: %s", tokenPath)
			}

			fmt.Println(token)
			return nil
		},
	})
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
