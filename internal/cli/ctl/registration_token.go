package ctl

import (
	"crypto/rand"
	"encoding/base64"
	"fmt"
	"os"
	"strings"
	"time"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/db"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:    "registration-tokens",
		Aliases: []string{"reg-tokens", "tokens"},
		Usage:   "Manage registration tokens for account creation",
		Description: `Manage registration tokens that control account creation on this server.

When registration_token_required is enabled, users must provide a valid
token to create an account via /new. Tokens have a max_uses limit and
optional expiration. Token consumption is deferred to first login.

Usage examples:
  maddy registration-tokens create --max-uses 10 --comment "Team onboarding"
  maddy registration-tokens create --token my-custom-token --max-uses 5 --expires 72h
  maddy registration-tokens list
  maddy registration-tokens status my-token-string
  maddy registration-tokens delete my-token-string`,
		Subcommands: []*cli.Command{
			{
				Name:  "create",
				Usage: "Create a new registration token",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:  "token",
						Usage: "Custom token string (auto-generated if empty)",
					},
					&cli.IntFlag{
						Name:  "max-uses",
						Usage: "Maximum number of uses",
						Value: 1,
					},
					&cli.StringFlag{
						Name:  "comment",
						Usage: "Human-readable comment for this token",
					},
					&cli.StringFlag{
						Name:  "expires",
						Usage: "Expiration duration (e.g. 72h, 168h, 720h)",
					},
				},
				Action: func(c *cli.Context) error {
					cfg := getDBConfig(c)
					database, err := openDB(cfg)
					if err != nil {
						return fmt.Errorf("failed to open database: %v", err)
					}
					defer closeDB(database)

					if err := database.AutoMigrate(&db.RegistrationToken{}); err != nil {
						return fmt.Errorf("failed to migrate table: %v", err)
					}

					token := c.String("token")
					if token == "" {
						b := make([]byte, 18)
						if _, err := rand.Read(b); err != nil {
							return fmt.Errorf("failed to generate token: %v", err)
						}
						token = base64.RawURLEncoding.EncodeToString(b)
					}

					maxUses := c.Int("max-uses")
					if maxUses <= 0 {
						maxUses = 1
					}

					t := db.RegistrationToken{
						Token:   token,
						MaxUses: maxUses,
						Comment: c.String("comment"),
					}

					if expiresStr := c.String("expires"); expiresStr != "" {
						dur, err := time.ParseDuration(expiresStr)
						if err != nil {
							return fmt.Errorf("invalid expiration duration: %v", err)
						}
						exp := time.Now().Add(dur)
						t.ExpiresAt = &exp
					}

					if err := database.Create(&t).Error; err != nil {
						return fmt.Errorf("failed to create token: %v", err)
					}

					if isTerminal(os.Stdout) {
						fmt.Println()
						fmt.Printf("  Token:      %s\n", t.Token)
						fmt.Printf("  Max Uses:   %d\n", t.MaxUses)
						if t.Comment != "" {
							fmt.Printf("  Comment:    %s\n", t.Comment)
						}
						if t.ExpiresAt != nil {
							fmt.Printf("  Expires At: %s\n", t.ExpiresAt.Format(time.RFC3339))
						}
						fmt.Println()
					} else {
						fmt.Print(t.Token)
					}
					return nil
				},
			},
			{
				Name:  "list",
				Usage: "List all registration tokens",
				Action: func(c *cli.Context) error {
					cfg := getDBConfig(c)
					database, err := openDB(cfg)
					if err != nil {
						return fmt.Errorf("failed to open database: %v", err)
					}
					defer closeDB(database)

					if err := database.AutoMigrate(&db.RegistrationToken{}); err != nil {
						return fmt.Errorf("failed to migrate table: %v", err)
					}
					if err := database.AutoMigrate(&db.Quota{}); err != nil {
						return fmt.Errorf("failed to migrate quota table: %v", err)
					}

					var tokens []db.RegistrationToken
					if err := database.Order("created_at DESC").Find(&tokens).Error; err != nil {
						return fmt.Errorf("failed to list tokens: %v", err)
					}

					if len(tokens) == 0 {
						fmt.Println("No registration tokens found.")
						return nil
					}

					now := time.Now()
					fmt.Printf("\n%-28s %-8s %-10s %-10s %-10s %s\n",
						"TOKEN", "MAX", "CONSUMED", "PENDING", "STATUS", "COMMENT")
					fmt.Println(strings.Repeat("-", 90))

					for _, t := range tokens {
						var pending int64
						database.Model(&db.Quota{}).
							Where("used_token = ? AND first_login_at = 1", t.Token).
							Count(&pending)

						status := "active"
						if t.ExpiresAt != nil && t.ExpiresAt.Before(now) {
							status = "expired"
						} else if int64(t.UsedCount)+pending >= int64(t.MaxUses) {
							status = "exhausted"
						}

						comment := t.Comment
						if len(comment) > 20 {
							comment = comment[:17] + "..."
						}

						fmt.Printf("%-28s %-8d %-10d %-10d %-10s %s\n",
							truncate(t.Token, 28), t.MaxUses, t.UsedCount, pending, status, comment)
					}
					fmt.Println()
					return nil
				},
			},
			{
				Name:      "status",
				Usage:     "Show detailed status for a specific token",
				ArgsUsage: "<token>",
				Action: func(c *cli.Context) error {
					if c.NArg() < 1 {
						return fmt.Errorf("token string is required")
					}
					tokenStr := c.Args().First()

					cfg := getDBConfig(c)
					database, err := openDB(cfg)
					if err != nil {
						return fmt.Errorf("failed to open database: %v", err)
					}
					defer closeDB(database)

					if err := database.AutoMigrate(&db.RegistrationToken{}); err != nil {
						return fmt.Errorf("failed to migrate table: %v", err)
					}
					if err := database.AutoMigrate(&db.Quota{}); err != nil {
						return fmt.Errorf("failed to migrate quota table: %v", err)
					}

					var t db.RegistrationToken
					if err := database.Where("token = ?", tokenStr).First(&t).Error; err != nil {
						return fmt.Errorf("token not found: %v", err)
					}

					var pending int64
					database.Model(&db.Quota{}).
						Where("used_token = ? AND first_login_at = 1", t.Token).
						Count(&pending)

					now := time.Now()
					status := "active"
					if t.ExpiresAt != nil && t.ExpiresAt.Before(now) {
						status = "expired"
					} else if int64(t.UsedCount)+pending >= int64(t.MaxUses) {
						status = "exhausted"
					}

					fmt.Println()
					fmt.Printf("  Token:      %s\n", t.Token)
					fmt.Printf("  Status:     %s\n", status)
					fmt.Printf("  Max Uses:   %d\n", t.MaxUses)
					fmt.Printf("  Consumed:   %d (confirmed first logins)\n", t.UsedCount)
					fmt.Printf("  Pending:    %d (reserved, awaiting first login)\n", pending)
					fmt.Printf("  Available:  %d\n", int64(t.MaxUses)-int64(t.UsedCount)-pending)
					if t.Comment != "" {
						fmt.Printf("  Comment:    %s\n", t.Comment)
					}
					fmt.Printf("  Created At: %s\n", t.CreatedAt.Format(time.RFC3339))
					if t.ExpiresAt != nil {
						fmt.Printf("  Expires At: %s\n", t.ExpiresAt.Format(time.RFC3339))
						if t.ExpiresAt.After(now) {
							fmt.Printf("  Expires In: %s\n", t.ExpiresAt.Sub(now).Round(time.Minute))
						} else {
							fmt.Printf("  Expired:    %s ago\n", now.Sub(*t.ExpiresAt).Round(time.Minute))
						}
					}

					// Show accounts that used this token
					var quotas []db.Quota
					database.Where("used_token = ?", t.Token).Find(&quotas)
					if len(quotas) > 0 {
						fmt.Printf("\n  Pending Accounts (%d):\n", len(quotas))
						for _, q := range quotas {
							loginStatus := "awaiting first login"
							if q.FirstLoginAt > 1 {
								loginStatus = "consumed"
							}
							fmt.Printf("    - %s (%s)\n", q.Username, loginStatus)
						}
					}
					fmt.Println()
					return nil
				},
			},
			{
				Name:      "delete",
				Usage:     "Delete a registration token",
				ArgsUsage: "<token>",
				Action: func(c *cli.Context) error {
					if c.NArg() < 1 {
						return fmt.Errorf("token string is required")
					}
					tokenStr := c.Args().First()

					cfg := getDBConfig(c)
					database, err := openDB(cfg)
					if err != nil {
						return fmt.Errorf("failed to open database: %v", err)
					}
					defer closeDB(database)

					if err := database.AutoMigrate(&db.RegistrationToken{}); err != nil {
						return fmt.Errorf("failed to migrate table: %v", err)
					}

					result := database.Where("token = ?", tokenStr).Delete(&db.RegistrationToken{})
					if result.Error != nil {
						return fmt.Errorf("failed to delete token: %v", result.Error)
					}
					if result.RowsAffected == 0 {
						return fmt.Errorf("token not found: %s", tokenStr)
					}

					fmt.Printf("Deleted token: %s\n", tokenStr)
					return nil
				},
			},
		},
	})
}

// truncate truncates s to maxLen, adding "..." if truncated.
func truncate(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	if maxLen <= 3 {
		return s[:maxLen]
	}
	return s[:maxLen-3] + "..."
}
