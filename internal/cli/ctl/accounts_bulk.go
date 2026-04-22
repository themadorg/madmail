package ctl

import (
	"crypto/rand"
	"encoding/json"
	"fmt"
	"os"
	"strings"

	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	"github.com/themadorg/madmail/internal/auth/pass_table"
	maddycli "github.com/themadorg/madmail/internal/cli"
	clitools2 "github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/urfave/cli/v2"
	"golang.org/x/crypto/bcrypt"
)

func accountsAuthStorageFlags() []cli.Flag {
	return []cli.Flag{
		&cli.StringFlag{
			Name:    "cfg-block",
			Usage:   "Credentials (auth.pass_table) configuration block",
			EnvVars: []string{"MADDY_CFGBLOCK"},
			Value:   "local_authdb",
		},
		&cli.StringFlag{
			Name:    "storage-cfg-block",
			Usage:   "IMAP storage configuration block",
			EnvVars: []string{"MADDY_STORAGE_CFGBLOCK"},
			Value:   "local_mailboxes",
		},
	}
}

func isInternalSettingsKey(u string) bool {
	return strings.HasPrefix(u, "__") && strings.HasSuffix(u, "__")
}

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "accounts",
		Usage: "Account management: status, info, create, ban, delete, bulk tools",
		Description: `All subcommands open your config file and talk to the databases directly
(on-disk SQLite/PostgreSQL), not a running server.

Use --cfg-block and --storage-cfg-block when your auth and IMAP blocks are not
local_authdb / local_mailboxes.`,
		Subcommands: []*cli.Command{
			{
				Name:  "status",
				Usage: "Show credentials and storage summary",
				Flags: accountsAuthStorageFlags(),
				Action: func(ctx *cli.Context) error {
					return accountsStatus(ctx)
				},
			},
			{
				Name:      "info",
				Usage:     "Show one account (credentials, IMAP, quota, blocklist)",
				ArgsUsage: "USERNAME",
				Flags:     accountsAuthStorageFlags(),
				Action: func(ctx *cli.Context) error {
					return accountsInfo(ctx)
				},
			},
			{
				Name:  "create",
				Usage: "Create login credentials and IMAP mailbox",
				Description: `Creates a password hash in the credentials database and an IMAP storage
account (same as admin API / chatmail flow). Password is read interactively
unless --password is set.`,
				ArgsUsage: "USERNAME",
				Flags: append(accountsAuthStorageFlags(),
					&cli.StringFlag{
						Name:    "password",
						Aliases: []string{"p"},
						Usage:   "Password (otherwise read interactively; avoid shell history)",
					},
					&cli.StringFlag{
						Name:  "hash",
						Usage: "Hash algorithm (auth.pass_table only): " + strings.Join(pass_table.Hashes, ", "),
						Value: "bcrypt",
					},
					&cli.IntFlag{
						Name:  "bcrypt-cost",
						Usage: "bcrypt cost",
						Value: bcrypt.DefaultCost,
					},
				),
				Action: func(ctx *cli.Context) error {
					return accountsCreateFull(ctx)
				},
			},
			{
				Name:  "create-random",
				Usage: "Create a random login + IMAP account; print JSON credentials",
				Description: `Generates a random localpart and password, full email = localpart@$(hostname)
from maddy.conf. Same idea as the admin panel "create account".

Examples:
  maddy accounts create-random
  maddy accounts create-random --json-only`,
				Flags: append(accountsAuthStorageFlags(),
					&cli.BoolFlag{
						Name:  "json-only",
						Usage: "Print only JSON (for scripts)",
					},
				),
				Action: func(ctx *cli.Context) error {
					return accountsCreateRandomFull(ctx)
				},
			},
			{
				Name:  "delete",
				Usage: "Remove credentials, delete IMAP data, and block re-registration",
				Description: `Matches admin API delete: drops the password row, runs full storage cleanup
(quota, mailboxes), and adds the address to the blocklist.`,
				ArgsUsage: "USERNAME",
				Flags: append(accountsAuthStorageFlags(),
					&cli.BoolFlag{Name: "yes", Aliases: []string{"y"}, Usage: "Skip confirmation"},
				),
				Action: func(ctx *cli.Context) error {
					return accountsRemoveFull(ctx, "deleted via maddy accounts delete", "Deleted")
				},
			},
			{
				Name:  "ban",
				Usage: "Same as delete — full removal and blocklist (moderation)",
				Description: `Removes login, deletes mail storage, and blocklists the username.
Optional second argument sets the blocklist reason.`,
				ArgsUsage: "USERNAME [REASON]",
				Flags: append(accountsAuthStorageFlags(),
					&cli.BoolFlag{Name: "yes", Aliases: []string{"y"}, Usage: "Skip confirmation"},
				),
				Action: func(ctx *cli.Context) error {
					reason := ctx.Args().Get(1)
					if reason == "" {
						reason = "banned via maddy accounts ban"
					}
					return accountsRemoveFull(ctx, reason, "Banned")
				},
			},
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
		if isInternalSettingsKey(u) {
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
		if isInternalSettingsKey(u.Username) {
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
		if isInternalSettingsKey(u) {
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

func accountsStatus(ctx *cli.Context) error {
	be, err := openUserDBForBlock(ctx, ctx.String("cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(be)

	st, err := openStorageForBlock(ctx, ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(st)

	mbe, ok := st.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not implement ManageableStorage")
	}

	users, err := be.ListUsers()
	if err != nil {
		return err
	}
	nCreds := 0
	for _, u := range users {
		if isInternalSettingsKey(u) {
			continue
		}
		nCreds++
	}

	regOpen, _ := be.IsRegistrationOpen()
	jitOn, _ := be.IsJitRegistrationEnabled()

	total, nIMAP, err := mbe.GetStat()
	if err != nil {
		return err
	}
	blocked, err := mbe.ListBlockedUsers()
	if err != nil {
		return err
	}

	fmt.Printf("Auth (%s):\n", ctx.String("cfg-block"))
	fmt.Printf("  Login accounts:     %d\n", nCreds)
	fmt.Printf("  Registration open:  %v\n", regOpen)
	fmt.Printf("  JIT registration:   %v\n", jitOn)
	fmt.Printf("Storage (%s):\n", ctx.String("storage-cfg-block"))
	fmt.Printf("  IMAP accounts:      %d\n", nIMAP)
	fmt.Printf("  Total storage (B):  %d\n", total)
	fmt.Printf("  Blocklisted users:  %d\n", len(blocked))
	return nil
}

func accountsInfo(ctx *cli.Context) error {
	username := auth.NormalizeUsername(ctx.Args().First())
	if username == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	be, err := openUserDBForBlock(ctx, ctx.String("cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(be)

	st, err := openStorageForBlock(ctx, ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(st)

	mbe, ok := st.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not implement ManageableStorage")
	}

	_, hasCreds, err := be.GetUserPasswordHash(username)
	if err != nil {
		return err
	}

	blocked, err := mbe.IsBlocked(username)
	if err != nil {
		return err
	}

	used, max, isDef, err := mbe.GetQuota(username)
	if err != nil {
		return err
	}

	infoMap, err := mbe.GetAllAccountInfo()
	if err != nil {
		return err
	}
	accInfo := infoMap[username]

	hasIMAP := false
	list, err := mbe.ListIMAPAccts()
	if err != nil {
		return err
	}
	for _, u := range list {
		if u == username {
			hasIMAP = true
			break
		}
	}

	fmt.Printf("Username:           %s\n", username)
	fmt.Printf("Has credentials:    %v\n", hasCreds)
	fmt.Printf("IMAP mailbox:       %v\n", hasIMAP)
	fmt.Printf("Blocklisted:          %v\n", blocked)
	fmt.Printf("Quota used / max:     %d / %d bytes\n", used, max)
	fmt.Printf("Default quota flag: %v\n", isDef)
	fmt.Printf("Created at (unix):  %d\n", accInfo.CreatedAt)
	fmt.Printf("First login (unix): %d\n", accInfo.FirstLoginAt)
	fmt.Printf("Last login (unix):  %d\n", accInfo.LastLoginAt)
	return nil
}

func accountsCreateFull(ctx *cli.Context) error {
	be, err := openUserDBForBlock(ctx, ctx.String("cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(be)

	st, err := openStorageForBlock(ctx, ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(st)

	mbe, ok := st.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not implement ManageableStorage")
	}

	if err := usersCreate(be, ctx); err != nil {
		return err
	}

	username := auth.NormalizeUsername(ctx.Args().First())
	if err := mbe.CreateIMAPAcct(username); err != nil {
		_ = be.DeleteUser(username)
		return fmt.Errorf("IMAP account creation failed (credentials rolled back): %w", err)
	}
	fmt.Fprintf(os.Stderr, "Created login + IMAP: %s\n", username)
	return nil
}

func accountsCreateRandomFull(ctx *cli.Context) error {
	be, err := openUserDBForBlock(ctx, ctx.String("cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(be)

	st, err := openStorageForBlock(ctx, ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(st)

	mbe, ok := st.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not implement ManageableStorage")
	}

	hostname := getHostnameFromConfig(ctx.String("config"))
	if hostname == "" {
		return fmt.Errorf("could not read $(hostname) from config (required for email address)")
	}

	const maxAttempts = 5
	for attempt := 0; attempt < maxAttempts; attempt++ {
		local, err := generateRandomUsername(12)
		if err != nil {
			return err
		}
		password, err := generateCLIPassword(24)
		if err != nil {
			return err
		}
		email := local + "@" + hostname

		blocked, err := mbe.IsBlocked(email)
		if err != nil {
			return err
		}
		if blocked {
			continue
		}

		if authHash, ok := be.(*pass_table.Auth); ok {
			err = authHash.CreateUserHash(email, password, pass_table.DefaultHash, pass_table.HashOpts{})
		} else {
			err = be.CreateUser(email, password)
		}
		if err != nil {
			if strings.Contains(err.Error(), "already exist") {
				continue
			}
			return err
		}

		if err := mbe.CreateIMAPAcct(email); err != nil {
			_ = be.DeleteUser(email)
			return fmt.Errorf("IMAP account creation failed: %w", err)
		}

		uriHost := strings.Trim(hostname, "[]")
		dclogin := fmt.Sprintf("dclogin:%s:%s::%s", email, password, uriHost)
		type result struct {
			Email    string `json:"email"`
			Password string `json:"password"`
			DCLogin  string `json:"dclogin"`
		}
		r := result{Email: email, Password: password, DCLogin: dclogin}
		data, err := json.MarshalIndent(r, "", "  ")
		if err != nil {
			return err
		}
		if !ctx.Bool("json-only") {
			fmt.Fprintf(os.Stderr, "Created account:\n")
		}
		fmt.Println(string(data))
		return nil
	}

	return fmt.Errorf("failed to create account after %d attempts", maxAttempts)
}

func accountsRemoveFull(ctx *cli.Context, reason, pastVerb string) error {
	username := auth.NormalizeUsername(ctx.Args().First())
	if username == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	if !ctx.Bool("yes") {
		msg := fmt.Sprintf("%s %s — remove credentials, delete IMAP mail, and block re-registration?", pastVerb, username)
		if !clitools2.Confirmation(msg, false) {
			return fmt.Errorf("cancelled")
		}
	}

	be, err := openUserDBForBlock(ctx, ctx.String("cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(be)

	st, err := openStorageForBlock(ctx, ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer closeIfNeeded(st)

	mbe, ok := st.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not implement ManageableStorage")
	}

	_ = be.DeleteUser(username)
	if err := mbe.DeleteAccount(username, reason); err != nil {
		return err
	}
	fmt.Printf("%s %s (credentials cleared, mail removed, user blocklisted)\n", pastVerb, username)
	return nil
}
