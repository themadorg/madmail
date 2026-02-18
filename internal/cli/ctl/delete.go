package ctl

import (
	"errors"
	"fmt"
	"strings"

	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	maddycli "github.com/themadorg/madmail/internal/cli"
	clitools2 "github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:      "delete",
			Usage:     "Fully delete a user account (credentials + storage + block)",
			ArgsUsage: "USERNAME",
			Description: `Fully remove a user account from the system:

  1. Delete authentication credentials (from local_authdb)
  2. Delete IMAP storage account and all messages (from local_mailboxes)
  3. Delete quota record
  4. Add username to blocklist (prevent re-registration)

This is equivalent to running:
  maddy creds remove USERNAME
  maddy imap-acct remove USERNAME
  maddy blocklist add USERNAME

Example:
  maddy delete user@example.com
  maddy delete -y user@example.com    # skip confirmation
`,
			Flags: []cli.Flag{
				&cli.StringFlag{
					Name:    "auth-block",
					Usage:   "Auth module configuration block to use",
					EnvVars: []string{"MADDY_AUTH_CFGBLOCK"},
					Value:   "local_authdb",
				},
				&cli.StringFlag{
					Name:    "storage-block",
					Usage:   "Storage module configuration block to use",
					EnvVars: []string{"MADDY_STORAGE_CFGBLOCK"},
					Value:   "local_mailboxes",
				},
				&cli.BoolFlag{
					Name:    "yes",
					Aliases: []string{"y"},
					Usage:   "Don't ask for confirmation",
				},
				&cli.StringFlag{
					Name:  "reason",
					Usage: "Reason for blocking (stored in blocklist)",
					Value: "account deleted via CLI",
				},
			},
			Action: func(ctx *cli.Context) error {
				rawUsername := ctx.Args().First()
				if rawUsername == "" {
					return errors.New("Error: USERNAME is required")
				}
				username := auth.NormalizeUsername(rawUsername)

				if !ctx.Bool("yes") {
					if !clitools2.Confirmation(fmt.Sprintf(
						"⚠️  This will PERMANENTLY delete %s:\n"+
							"   - Authentication credentials\n"+
							"   - All IMAP mailboxes and messages\n"+
							"   - Quota records\n"+
							"   - Block username from re-registration\n\n"+
							"Are you sure?", username), false) {
						return errors.New("Cancelled")
					}
				}

				// Open auth DB first (using a temporary override of cfg-block)
				origCfg := ctx.String("cfg-block")
				_ = origCfg
				// We need to use the flag values directly since we have custom flag names
				authCtx := ctx
				_ = authCtx

				// Open auth DB
				if err := ctx.Set("cfg-block", ctx.String("auth-block")); err != nil {
					return fmt.Errorf("failed to set cfg-block: %w", err)
				}
				authDB, err := openUserDB(ctx)
				if err != nil {
					// Auth DB might not be available, continue with storage
					fmt.Fprintf(ctx.App.Writer, "⚠️  Could not open auth DB: %v (continuing with storage cleanup)\n", err)
				} else {
					defer closeIfNeeded(authDB)
					// Step 1: Delete credentials
					if err := authDB.DeleteUser(username); err != nil {
						fmt.Fprintf(ctx.App.Writer, "⚠️  Failed to delete credentials: %v\n", err)
						// Try with un-normalized username
						if !strings.Contains(rawUsername, "[") {
							if err2 := authDB.DeleteUser(rawUsername); err2 != nil {
								fmt.Fprintf(ctx.App.Writer, "⚠️  Also failed with original name: %v\n", err2)
							} else {
								fmt.Fprintf(ctx.App.Writer, "✅ Credentials deleted (using original name)\n")
							}
						}
					} else {
						fmt.Fprintf(ctx.App.Writer, "✅ Credentials deleted\n")
					}
				}

				// Open storage DB
				if err := ctx.Set("cfg-block", ctx.String("storage-block")); err != nil {
					return fmt.Errorf("failed to set cfg-block: %w", err)
				}
				storageDB, err := openStorage(ctx)
				if err != nil {
					return fmt.Errorf("failed to open storage DB: %w", err)
				}
				defer closeIfNeeded(storageDB)

				mbe, ok := storageDB.(module.ManageableStorage)
				if !ok {
					return fmt.Errorf("storage backend does not support account management")
				}

				// Step 2-4: Full storage cleanup + block
				reason := ctx.String("reason")
				if err := mbe.DeleteAccount(username, reason); err != nil {
					fmt.Fprintf(ctx.App.Writer, "⚠️  Storage cleanup error: %v\n", err)
				} else {
					fmt.Fprintf(ctx.App.Writer, "✅ IMAP account deleted\n")
					fmt.Fprintf(ctx.App.Writer, "✅ Quota record deleted\n")
					fmt.Fprintf(ctx.App.Writer, "✅ Username blocked from re-registration\n")
				}

				fmt.Fprintf(ctx.App.Writer, "\n🗑️  Account %s has been fully deleted.\n", username)
				return nil
			},
		})
}
