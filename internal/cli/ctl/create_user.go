package ctl

import (
	"crypto/rand"
	"encoding/json"
	"fmt"
	"strings"

	frameworkconfig "github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth/pass_table"
	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "create-user",
		Usage: "Create a new random user account and output credentials",
		Description: `Create a new user account with a random username and password,
similar to the /new HTTP endpoint. Outputs JSON with the email, password,
and a DCLOGIN URI that can be used to configure a Delta Chat account.

The output format is:
  {"email": "xyz@[1.1.1.1]", "password": "...", "dclogin": "dclogin:..."}

Use --json-only to suppress everything except the JSON output (for scripts).

Examples:
  maddy create-user
  maddy create-user --json-only
  maddy create-user --cfg-block local_authdb`,
		Flags: []cli.Flag{
			&cli.StringFlag{
				Name:    "cfg-block",
				Usage:   "Module configuration block to use for auth DB",
				EnvVars: []string{"MADDY_CFGBLOCK"},
				Value:   "local_authdb",
			},
			&cli.BoolFlag{
				Name:  "json-only",
				Usage: "Print only the JSON output (for use in scripts)",
			},
		},
		Action: func(ctx *cli.Context) error {
			be, err := openUserDB(ctx)
			if err != nil {
				return err
			}
			defer closeIfNeeded(be)
			return createRandomUser(be, ctx)
		},
	})
}

func createRandomUser(be module.PlainUserDB, ctx *cli.Context) error {
	// Read hostname from the active config to build the email domain.
	hostname := getHostnameFromConfig(frameworkconfig.ConfigFile())
	if hostname == "" {
		return fmt.Errorf("could not determine hostname from config")
	}

	const maxAttempts = 5
	for attempt := 0; attempt < maxAttempts; attempt++ {
		// Generate random username (10 chars, lowercase alpha)
		username, err := generateRandomUsername(10)
		if err != nil {
			return fmt.Errorf("failed to generate username: %v", err)
		}

		// Generate random password (24 chars)
		password, err := generateCLIPassword(24)
		if err != nil {
			return fmt.Errorf("failed to generate password: %v", err)
		}

		email := username + "@" + hostname

		// Create user with SHA256 hashing if supported (fast for server-generated passwords)
		if authHash, ok := be.(*pass_table.Auth); ok {
			err = authHash.CreateUserHash(email, password, pass_table.DefaultHash, pass_table.HashOpts{})
		} else {
			err = be.CreateUser(email, password)
		}

		if err != nil {
			if strings.Contains(err.Error(), "already exist") {
				continue // retry with new username
			}
			return fmt.Errorf("failed to create user: %v", err)
		}

		// Build DCLOGIN URI
		// Strip brackets from hostname for the URI host
		uriHost := strings.Trim(hostname, "[]")
		dclogin := fmt.Sprintf("dclogin:%s:%s::%s", email, password, uriHost)

		type result struct {
			Email    string `json:"email"`
			Password string `json:"password"`
			DCLogin  string `json:"dclogin"`
		}

		r := result{
			Email:    email,
			Password: password,
			DCLogin:  dclogin,
		}

		data, err := json.MarshalIndent(r, "", "  ")
		if err != nil {
			return fmt.Errorf("failed to marshal JSON: %v", err)
		}
		fmt.Println(string(data))
		return nil
	}

	return fmt.Errorf("failed to create user after %d attempts", maxAttempts)
}

// generateRandomUsername creates a random lowercase alpha string.
func generateRandomUsername(length int) (string, error) {
	const charset = "abcdefghijklmnopqrstuvwxyz0123456789"
	b := make([]byte, length)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	for i := range b {
		b[i] = charset[b[i]%byte(len(charset))]
	}
	return string(b), nil
}
