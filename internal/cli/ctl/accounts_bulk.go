package ctl

import (
	"crypto/rand"
	"encoding/json"
	"fmt"
	"os"
	"strings"

	"github.com/themadorg/madmail/framework/module"
	maddycli "github.com/themadorg/madmail/internal/cli"
	clitools2 "github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "accounts",
		Usage: "Bulk account operations: export, import, delete-all",
		Subcommands: []*cli.Command{
			{
				Name:  "export",
				Usage: "Export all usernames to a JSON file",
				Description: `Export all user accounts (usernames only) to a JSON file.
The output format is a JSON array of objects with a "username" field,
compatible with the Admin API export and the import command.

Examples:
  maddy accounts export                       Print to stdout
  maddy accounts export -o accounts.json      Write to file`,
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "cfg-block",
						Usage:   "Module configuration block to use",
						EnvVars: []string{"MADDY_CFGBLOCK"},
						Value:   "local_authdb",
					},
					&cli.StringFlag{
						Name:    "output",
						Aliases: []string{"o"},
						Usage:   "Output file path (default: stdout)",
					},
				},
				Action: func(ctx *cli.Context) error {
					be, err := openUserDB(ctx)
					if err != nil {
						return err
					}
					defer closeIfNeeded(be)
					return accountsExport(be, ctx)
				},
			},
			{
				Name:      "import",
				Usage:     "Import accounts from a JSON file",
				ArgsUsage: "FILE",
				Description: `Import user accounts from a JSON file.
The file must contain a JSON array of objects with a "username" field.
The "password" field is optional — if omitted, a random password is generated.

This makes it compatible with export files (backup/restore flow).

Example file formats:
  Export-compatible (passwords auto-generated):
  [
    {"username": "alice@example.com"},
    {"username": "bob@example.com"}
  ]

  With explicit passwords:
  [
    {"username": "alice@example.com", "password": "secret123"},
    {"username": "bob@example.com", "password": "hunter2"}
  ]

Existing accounts are skipped (not overwritten).

Examples:
  maddy accounts import accounts.json`,
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "cfg-block",
						Usage:   "Module configuration block to use",
						EnvVars: []string{"MADDY_CFGBLOCK"},
						Value:   "local_authdb",
					},
				},
				Action: func(ctx *cli.Context) error {
					be, err := openUserDB(ctx)
					if err != nil {
						return err
					}
					defer closeIfNeeded(be)
					return accountsImport(be, ctx)
				},
			},
			{
				Name:  "delete-all",
				Usage: "Delete ALL user accounts",
				Description: `Delete ALL user accounts from the credentials database.
This is a destructive operation that cannot be undone.
Internal settings keys (like __REGISTRATION__, etc.) are preserved.

Examples:
  maddy accounts delete-all
  maddy accounts delete-all --yes`,
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "cfg-block",
						Usage:   "Module configuration block to use",
						EnvVars: []string{"MADDY_CFGBLOCK"},
						Value:   "local_authdb",
					},
					&cli.BoolFlag{
						Name:    "yes",
						Aliases: []string{"y"},
						Usage:   "Don't ask for confirmation",
					},
				},
				Action: func(ctx *cli.Context) error {
					be, err := openUserDB(ctx)
					if err != nil {
						return err
					}
					defer closeIfNeeded(be)
					return accountsDeleteAll(be, ctx)
				},
			},
		},
	})
}

type exportUserEntry struct {
	Username string `json:"username"`
}

func accountsExport(be module.PlainUserDB, ctx *cli.Context) error {
	users, err := be.ListUsers()
	if err != nil {
		return fmt.Errorf("failed to list users: %v", err)
	}

	entries := make([]exportUserEntry, 0, len(users))
	for _, u := range users {
		// Skip internal settings keys
		if strings.HasPrefix(u, "__") && strings.HasSuffix(u, "__") {
			continue
		}
		entries = append(entries, exportUserEntry{Username: u})
	}

	data, err := json.MarshalIndent(entries, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal JSON: %v", err)
	}

	output := ctx.String("output")
	if output != "" {
		if err := os.WriteFile(output, data, 0644); err != nil {
			return fmt.Errorf("failed to write file: %v", err)
		}
		fmt.Printf("✅ Exported %d accounts to %s\n", len(entries), output)
	} else {
		fmt.Println(string(data))
	}

	return nil
}

type importUserEntry struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

func accountsImport(be module.PlainUserDB, ctx *cli.Context) error {
	file := ctx.Args().First()
	if file == "" {
		return fmt.Errorf("file path is required")
	}

	data, err := os.ReadFile(file)
	if err != nil {
		return fmt.Errorf("failed to read file: %v", err)
	}

	var users []importUserEntry
	if err := json.Unmarshal(data, &users); err != nil {
		return fmt.Errorf("invalid JSON: %v", err)
	}

	if len(users) == 0 {
		fmt.Println("No accounts to import.")
		return nil
	}

	imported, skipped := 0, 0
	for _, u := range users {
		if u.Username == "" {
			skipped++
			continue
		}
		if strings.HasPrefix(u.Username, "__") && strings.HasSuffix(u.Username, "__") {
			skipped++
			continue
		}

		// Auto-generate password if not provided (backup/restore flow)
		password := u.Password
		if password == "" {
			p, err := generateCLIPassword(24)
			if err != nil {
				fmt.Fprintf(os.Stderr, "⚠ %s: failed to generate password: %v\n", u.Username, err)
				continue
			}
			password = p
		}

		if err := be.CreateUser(u.Username, password); err != nil {
			if strings.Contains(err.Error(), "already exist") {
				skipped++
				continue
			}
			fmt.Fprintf(os.Stderr, "⚠ %s: %v\n", u.Username, err)
			continue
		}
		imported++
	}

	fmt.Printf("✅ Imported: %d, Skipped: %d (of %d total)\n", imported, skipped, len(users))
	return nil
}

func accountsDeleteAll(be module.PlainUserDB, ctx *cli.Context) error {
	users, err := be.ListUsers()
	if err != nil {
		return fmt.Errorf("failed to list users: %v", err)
	}

	// Count real accounts (not internal keys)
	var realUsers []string
	for _, u := range users {
		if strings.HasPrefix(u, "__") && strings.HasSuffix(u, "__") {
			continue
		}
		realUsers = append(realUsers, u)
	}

	if len(realUsers) == 0 {
		fmt.Println("No accounts to delete.")
		return nil
	}

	if !ctx.Bool("yes") {
		msg := fmt.Sprintf("Are you sure you want to delete ALL %d accounts? This cannot be undone!", len(realUsers))
		if !clitools2.Confirmation(msg, false) {
			return fmt.Errorf("cancelled")
		}
	}

	deleted := 0
	for _, u := range realUsers {
		if err := be.DeleteUser(u); err != nil {
			fmt.Fprintf(os.Stderr, "⚠ %s: %v\n", u, err)
			continue
		}
		deleted++
	}

	fmt.Printf("🗑️  Deleted %d accounts\n", deleted)
	return nil
}

// generateCLIPassword generates a random password with mixed characters.
func generateCLIPassword(length int) (string, error) {
	const charset = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*"
	b := make([]byte, length)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	for i := range b {
		b[i] = charset[b[i]%byte(len(charset))]
	}
	return string(b), nil
}
