/*
Maddy Mail Server - Composable all-in-one email server.
Copyright © 2019-2020 Max Mazurov <fox.cpp@disroot.org>, Maddy Mail Server contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

package ctl

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/emersion/go-imap"
	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	maddycli "github.com/themadorg/madmail/internal/cli"
	clitools2 "github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:  "imap-acct",
			Usage: "IMAP storage accounts management",
			Description: `These subcommands can be used to list/create/delete IMAP storage
accounts for any storage backend supported by maddy.

The corresponding storage backend should be configured in maddy.conf and be
defined in a top-level configuration block. By default, the name of that
block should be local_mailboxes but this can be changed using --cfg-block
flag for subcommands.

Note that in default configuration it is not enough to create an IMAP storage
account to grant server access. Additionally, user credentials should
be created using 'creds' subcommand.
`,
			Subcommands: []*cli.Command{
				{
					Name:  "list",
					Usage: "List storage accounts",
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "local_mailboxes",
						},
					},
					Action: func(ctx *cli.Context) error {
						be, err := openStorage(ctx)
						if err != nil {
							return err
						}
						defer closeIfNeeded(be)
						return imapAcctList(be, ctx)
					},
				},
				{
					Name:  "create",
					Usage: "Create IMAP storage account",
					Description: `In addition to account creation, this command
creates a set of default folder (mailboxes) with special-use attribute set.`,
					ArgsUsage: "USERNAME",
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "local_mailboxes",
						},
						&cli.BoolFlag{
							Name:  "no-specialuse",
							Usage: "Do not create special-use folders",
							Value: false,
						},
						&cli.StringFlag{
							Name:  "sent-name",
							Usage: "Name of special mailbox for sent messages, use empty string to not create any",
							Value: "Sent",
						},
						&cli.StringFlag{
							Name:  "trash-name",
							Usage: "Name of special mailbox for trash, use empty string to not create any",
							Value: "Trash",
						},
						&cli.StringFlag{
							Name:  "junk-name",
							Usage: "Name of special mailbox for 'junk' (spam), use empty string to not create any",
							Value: "Junk",
						},
						&cli.StringFlag{
							Name:  "drafts-name",
							Usage: "Name of special mailbox for drafts, use empty string to not create any",
							Value: "Drafts",
						},
						&cli.StringFlag{
							Name:  "archive-name",
							Usage: "Name of special mailbox for archive, use empty string to not create any",
							Value: "Archive",
						},
					},
					Action: func(ctx *cli.Context) error {
						be, err := openStorage(ctx)
						if err != nil {
							return err
						}
						defer closeIfNeeded(be)
						return imapAcctCreate(be, ctx)
					},
				},
				{
					Name:  "remove",
					Usage: "Delete IMAP storage account",
					Description: `If IMAP connections are open and using the specified account,
messages access will be killed off immediately though connection will remain open. No cache
or other buffering takes effect.`,
					ArgsUsage: "USERNAME",
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "local_mailboxes",
						},
						&cli.BoolFlag{
							Name:    "yes",
							Aliases: []string{"y"},
							Usage:   "Don't ask for confirmation",
						},
					},
					Action: func(ctx *cli.Context) error {
						be, err := openStorage(ctx)
						if err != nil {
							return err
						}
						defer closeIfNeeded(be)
						return imapAcctRemove(be, ctx)
					},
				},
				{
					Name:  "quota",
					Usage: "Manage accounts's quota",
					Subcommands: []*cli.Command{
						{
							Name:      "get",
							Usage:     "Get current usage and limit",
							ArgsUsage: "USERNAME",
							Flags: []cli.Flag{
								&cli.StringFlag{
									Name:    "cfg-block",
									Usage:   "Module configuration block to use",
									EnvVars: []string{"MADDY_CFGBLOCK"},
									Value:   "local_mailboxes",
								},
							},
							Action: func(ctx *cli.Context) error {
								be, err := openStorage(ctx)
								if err != nil {
									return err
								}
								defer closeIfNeeded(be)
								return imapAcctQuotaGet(be, ctx)
							},
						},
						{
							Name:      "set",
							Usage:     "Set a new limit",
							ArgsUsage: "USERNAME LIMIT",
							Flags: []cli.Flag{
								&cli.StringFlag{
									Name:    "cfg-block",
									Usage:   "Module configuration block to use",
									EnvVars: []string{"MADDY_CFGBLOCK"},
									Value:   "local_mailboxes",
								},
							},
							Action: func(ctx *cli.Context) error {
								be, err := openStorage(ctx)
								if err != nil {
									return err
								}
								defer closeIfNeeded(be)
								return imapAcctQuotaSet(be, ctx)
							},
						},
						{
							Name:      "reset",
							Usage:     "Reset quota to default",
							ArgsUsage: "USERNAME",
							Flags: []cli.Flag{
								&cli.StringFlag{
									Name:    "cfg-block",
									Usage:   "Module configuration block to use",
									EnvVars: []string{"MADDY_CFGBLOCK"},
									Value:   "local_mailboxes",
								},
							},
							Action: func(ctx *cli.Context) error {
								be, err := openStorage(ctx)
								if err != nil {
									return err
								}
								defer closeIfNeeded(be)
								return imapAcctQuotaReset(be, ctx)
							},
						},
						{
							Name:  "list",
							Usage: "List all accounts with quota info",
							Flags: []cli.Flag{
								&cli.StringFlag{
									Name:    "cfg-block",
									Usage:   "Module configuration block to use",
									EnvVars: []string{"MADDY_CFGBLOCK"},
									Value:   "local_mailboxes",
								},
							},
							Action: func(ctx *cli.Context) error {
								be, err := openStorage(ctx)
								if err != nil {
									return err
								}
								defer closeIfNeeded(be)
								return imapAcctQuotaList(be, ctx)
							},
						},
						{
							Name:      "set-default",
							Usage:     "Set the global default limit",
							ArgsUsage: "LIMIT",
							Flags: []cli.Flag{
								&cli.StringFlag{
									Name:    "cfg-block",
									Usage:   "Module configuration block to use",
									EnvVars: []string{"MADDY_CFGBLOCK"},
									Value:   "local_mailboxes",
								},
							},
							Action: func(ctx *cli.Context) error {
								be, err := openStorage(ctx)
								if err != nil {
									return err
								}
								defer closeIfNeeded(be)
								return imapAcctQuotaSetDefault(be, ctx)
							},
						},
					},
				},
				{
					Name:      "purge-msgs",
					Usage:     "Delete all messages for a storage account",
					ArgsUsage: "USERNAME",
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "local_mailboxes",
						},
					},
					Action: func(ctx *cli.Context) error {
						be, err := openStorage(ctx)
						if err != nil {
							return err
						}
						defer closeIfNeeded(be)
						return imapAcctPurge(be, ctx)
					},
				},
				{
					Name:  "stat",
					Usage: "Show storage statistics",
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "local_mailboxes",
						},
					},
					Action: func(ctx *cli.Context) error {
						be, err := openStorage(ctx)
						if err != nil {
							return err
						}
						defer closeIfNeeded(be)
						return imapAcctStat(be, ctx)
					},
				},
				{
					Name:  "appendlimit",
					Usage: "Query or set accounts's APPENDLIMIT value",
					Description: `APPENDLIMIT value determines the size of a message that
can be saved into a mailbox using IMAP APPEND command. This does not affect the size
of messages that can be delivered to the mailbox from non-IMAP sources (e.g. SMTP).

Global APPENDLIMIT value set via server configuration takes precedence over
per-account values configured using this command.

APPENDLIMIT value (either global or per-account) cannot be larger than
4 GiB due to IMAP protocol limitations.
`,
					ArgsUsage: "USERNAME",
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "local_mailboxes",
						},
						&cli.IntFlag{
							Name:    "value",
							Aliases: []string{"v"},
							Usage:   "Set APPENDLIMIT to specified value (in bytes)",
						},
					},
					Action: func(ctx *cli.Context) error {
						be, err := openStorage(ctx)
						if err != nil {
							return err
						}
						defer closeIfNeeded(be)
						return imapAcctAppendlimit(be, ctx)
					},
				},
				{
					Name:      "prune-unused",
					Usage:     "Delete accounts that never logged in",
					ArgsUsage: "RETENTION",
					Description: `Delete accounts that have never logged in (FirstLoginAt = 1)
and were created more than RETENTION ago.

Example: maddyctl imap-acct prune-unused 720h`,
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "local_mailboxes",
						},
					},
					Action: func(ctx *cli.Context) error {
						be, err := openStorage(ctx)
						if err != nil {
							return err
						}
						defer closeIfNeeded(be)
						return imapAcctPruneUnused(be, ctx)
					},
				},
			},
		})
}

type SpecialUseUser interface {
	CreateMailboxSpecial(name, specialUseAttr string) error
}

func formatBytes(b int64) string {
	const unit = 1024
	if b < unit {
		return fmt.Sprintf("%d B", b)
	}
	div, exp := int64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
		if exp >= 5 { // PB is max in the string "KMGTPE"
			break
		}
	}
	return fmt.Sprintf("%.1f %cB", float64(b)/float64(div), "KMGTPE"[exp])
}

func imapAcctList(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	list, err := mbe.ListIMAPAccts()
	if err != nil {
		return err
	}

	if len(list) == 0 && !ctx.Bool("quiet") {
		fmt.Fprintln(os.Stderr, "No users.")
		return nil
	}

	fmt.Printf("%-40s %-20s %-15s\n", "User", "Created At", "Usage")
	for _, user := range list {
		created, _ := mbe.GetAccountDate(user)
		used, _, _, _ := mbe.GetQuota(user)

		createdStr := "None"
		if created > 0 {
			createdStr = time.Unix(created, 0).Format("2006-01-02 15:04:05")
		}

		fmt.Printf("%-40s %-20s %-15s\n", user, createdStr, formatBytes(used))
	}
	return nil
}

func imapAcctCreate(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	username := auth.NormalizeUsername(ctx.Args().First())
	if username == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	if err := mbe.CreateIMAPAcct(username); err != nil {
		return err
	}

	act, err := mbe.GetIMAPAcct(username)
	if err != nil {
		return fmt.Errorf("failed to get user: %w", err)
	}

	suu, ok := act.(SpecialUseUser)
	if !ok {
		fmt.Fprintf(os.Stderr, "Note: Storage backend does not support SPECIAL-USE IMAP extension")
	}

	if ctx.Bool("no-specialuse") {
		return nil
	}

	createMbox := func(name, specialUseAttr string) error {
		if suu == nil {
			return act.CreateMailbox(name)
		}
		return suu.CreateMailboxSpecial(name, specialUseAttr)
	}

	if name := ctx.String("sent-name"); name != "" {
		if err := createMbox(name, imap.SentAttr); err != nil {
			fmt.Fprintf(os.Stderr, "Failed to create sent folder: %v", err)
		}
	}
	if name := ctx.String("trash-name"); name != "" {
		if err := createMbox(name, imap.TrashAttr); err != nil {
			fmt.Fprintf(os.Stderr, "Failed to create trash folder: %v", err)
		}
	}
	if name := ctx.String("junk-name"); name != "" {
		if err := createMbox(name, imap.JunkAttr); err != nil {
			fmt.Fprintf(os.Stderr, "Failed to create junk folder: %v", err)
		}
	}
	if name := ctx.String("drafts-name"); name != "" {
		if err := createMbox(name, imap.DraftsAttr); err != nil {
			fmt.Fprintf(os.Stderr, "Failed to create drafts folder: %v", err)
		}
	}
	if name := ctx.String("archive-name"); name != "" {
		if err := createMbox(name, imap.ArchiveAttr); err != nil {
			fmt.Fprintf(os.Stderr, "Failed to create archive folder: %v", err)
		}
	}

	return nil
}

func imapAcctRemove(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	rawUsername := ctx.Args().First()
	if rawUsername == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	if !ctx.Bool("yes") {
		if !clitools2.Confirmation("Are you sure you want to delete this user account?", false) {
			return errors.New("Cancelled")
		}
	}

	err := mbe.DeleteIMAPAcct(rawUsername)
	if err != nil && strings.Contains(err.Error(), "doesn't exists") {
		// try normalized
		err = mbe.DeleteIMAPAcct(auth.NormalizeUsername(rawUsername))
	}
	return err
}

func imapAcctQuotaGet(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	rawUsername := ctx.Args().First()
	if rawUsername == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	used, max, isDef, err := mbe.GetQuota(rawUsername)
	if (err != nil || max == 0 && used == 0) && !strings.Contains(rawUsername, "[") {
		// try normalized
		used, max, isDef, err = mbe.GetQuota(auth.NormalizeUsername(rawUsername))
	}

	if err != nil {
		return err
	}

	fmt.Printf("User: %s\n", rawUsername)
	fmt.Printf("Storage used: %s\n", formatBytes(used))
	if max > 0 {
		limitStr := formatBytes(max)
		if isDef {
			limitStr += " (default)"
		}
		fmt.Printf("Quota limit:  %s\n", limitStr)
	} else {
		fmt.Println("Quota limit:  None")
	}

	return nil
}

func imapAcctQuotaSet(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	rawUsername := ctx.Args().First()
	if rawUsername == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	limitStr := ctx.Args().Get(1)
	if limitStr == "" {
		return cli.Exit("Error: LIMIT is required", 2)
	}

	limit, err := config.ParseDataSize(limitStr)
	if err != nil {
		return fmt.Errorf("invalid limit value: %w", err)
	}

	err = mbe.SetQuota(rawUsername, int64(limit))
	if err != nil && !strings.Contains(rawUsername, "[") {
		// try normalized
		err = mbe.SetQuota(auth.NormalizeUsername(rawUsername), int64(limit))
	}
	return err
}

func imapAcctQuotaReset(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	rawUsername := ctx.Args().First()
	if rawUsername == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	err := mbe.ResetQuota(rawUsername)
	if err != nil && !strings.Contains(rawUsername, "[") {
		// try normalized
		err = mbe.ResetQuota(auth.NormalizeUsername(rawUsername))
	}
	return err
}

func imapAcctQuotaSetDefault(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	limitStr := ctx.Args().First()
	if limitStr == "" {
		return cli.Exit("Error: LIMIT is required", 2)
	}

	limit, err := config.ParseDataSize(limitStr)
	if err != nil {
		return fmt.Errorf("invalid limit value: %w", err)
	}

	return mbe.SetDefaultQuota(int64(limit))
}

func imapAcctQuotaList(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	list, err := mbe.ListIMAPAccts()
	if err != nil {
		return err
	}

	defQuota := mbe.GetDefaultQuota()
	defQuotaStr := "None"
	if defQuota > 0 {
		defQuotaStr = formatBytes(defQuota)
	}
	fmt.Printf("Global default quota: %s\n\n", defQuotaStr)

	fmt.Printf("%-40s %-20s %-15s %-15s\n", "User", "Created At", "Used", "Limit")
	for _, user := range list {
		created, _ := mbe.GetAccountDate(user)
		used, max, isDef, err := mbe.GetQuota(user)
		if err != nil {
			fmt.Printf("%-40s %-20s %-15s %-15s (error: %v)\n", user, "-", "-", "-", err)
			continue
		}

		createdStr := "None"
		if created > 0 {
			createdStr = time.Unix(created, 0).Format("2006-01-02 15:04:05")
		}

		maxStr := "None"
		if max > 0 {
			maxStr = formatBytes(max)
			if isDef {
				maxStr = "Default (" + maxStr + ")"
			}
		}
		fmt.Printf("%-40s %-20s %-15s %-15s\n", user, createdStr, formatBytes(used), maxStr)
	}
	return nil
}

func imapAcctStat(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	totalStorage, accountsCount, err := mbe.GetStat()
	if err != nil {
		return err
	}

	fmt.Printf("Total storage used: %s\n", formatBytes(totalStorage))
	fmt.Printf("Number of users:    %d\n", accountsCount)
	fmt.Println("Active connections: N/A (not supported by storage CLI)")

	return nil
}

func imapAcctPurge(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	rawUsername := ctx.Args().First()
	if rawUsername == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	if !clitools2.Confirmation(fmt.Sprintf("Are you sure you want to delete ALL messages for %s?", rawUsername), false) {
		return errors.New("Cancelled")
	}

	err := mbe.PurgeIMAPMsgs(rawUsername)
	if err != nil && !strings.Contains(rawUsername, "[") {
		// try normalized
		err = mbe.PurgeIMAPMsgs(auth.NormalizeUsername(rawUsername))
	}
	return err
}
func imapAcctPruneUnused(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return cli.Exit("Error: storage backend does not support accounts management using maddy command", 2)
	}

	retentionStr := ctx.Args().First()
	if retentionStr == "" {
		return cli.Exit("Error: RETENTION is required (e.g. 720h)", 2)
	}

	retention, err := time.ParseDuration(retentionStr)
	if err != nil {
		return fmt.Errorf("invalid retention value: %w", err)
	}

	return mbe.PruneUnusedAccounts(retention)
}
