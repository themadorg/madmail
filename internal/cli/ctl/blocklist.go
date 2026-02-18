package ctl

import (
	"fmt"
	"os"

	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	maddycli "github.com/themadorg/madmail/internal/cli"
	clitools2 "github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:  "blocklist",
			Usage: "Manage blocked users (prevent re-registration)",
			Description: `These commands manage the blocklist of usernames that are
prevented from being re-created. When an account is deleted via the admin
panel or CLI, the username is automatically added to the blocklist.

Blocked users cannot register again via /new or JIT account creation.
`,
			Subcommands: []*cli.Command{
				{
					Name:  "list",
					Usage: "List all blocked users",
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
						return blocklistList(be)
					},
				},
				{
					Name:      "add",
					Usage:     "Block a username from re-registration",
					ArgsUsage: "USERNAME [REASON]",
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
						return blocklistAdd(be, ctx)
					},
				},
				{
					Name:      "remove",
					Usage:     "Unblock a username (allow re-registration)",
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
						return blocklistRemove(be, ctx)
					},
				},
			},
		})
}

func blocklistList(be module.Storage) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not support blocklist management")
	}

	entries, err := mbe.ListBlockedUsers()
	if err != nil {
		return err
	}

	if len(entries) == 0 {
		fmt.Fprintln(os.Stderr, "No blocked users.")
		return nil
	}

	fmt.Printf("%-45s %-25s %s\n", "Username", "Blocked At", "Reason")
	for _, e := range entries {
		fmt.Printf("%-45s %-25s %s\n", e.Username, e.BlockedAt.Format("2006-01-02 15:04:05"), e.Reason)
	}
	return nil
}

func blocklistAdd(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not support blocklist management")
	}

	username := auth.NormalizeUsername(ctx.Args().First())
	if username == "" {
		return fmt.Errorf("USERNAME is required")
	}

	reason := ctx.Args().Get(1)
	if reason == "" {
		reason = "manually blocked via CLI"
	}

	if err := mbe.BlockUser(username, reason); err != nil {
		return err
	}
	fmt.Printf("Blocked: %s\n", username)
	return nil
}

func blocklistRemove(be module.Storage, ctx *cli.Context) error {
	mbe, ok := be.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("storage backend does not support blocklist management")
	}

	username := auth.NormalizeUsername(ctx.Args().First())
	if username == "" {
		return fmt.Errorf("USERNAME is required")
	}

	if !ctx.Bool("yes") {
		if !clitools2.Confirmation(fmt.Sprintf("Are you sure you want to unblock %s?", username), false) {
			return fmt.Errorf("Cancelled")
		}
	}

	if err := mbe.UnblockUser(username); err != nil {
		return err
	}
	fmt.Printf("Unblocked: %s\n", username)
	return nil
}
