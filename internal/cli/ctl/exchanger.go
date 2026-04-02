package ctl

import (
	"fmt"
	"os"
	"text/tabwriter"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/db"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:  "exchanger",
			Usage: "Pull-based email relay (exchanger) management",
			Description: `These commands manage remote exchangers that madmail 
periodically polls to download incoming messages.`,
			Subcommands: []*cli.Command{
				{
					Name:  "list",
					Usage: "List configured exchangers",
					Action: func(ctx *cli.Context) error {
						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						var exchangers []db.Exchanger
						if err := database.Find(&exchangers).Error; err != nil {
							return err
						}

						if len(exchangers) == 0 {
							fmt.Println("No exchangers configured.")
							return nil
						}

						w := tabwriter.NewWriter(os.Stdout, 0, 8, 2, ' ', 0)
						fmt.Fprintln(w, "NAME\tENDPOINT\tINTERVAL\tENABLED\tLAST POLL")
						for _, e := range exchangers {
							lastPoll := "Never"
							if !e.LastPollAt.IsZero() {
								lastPoll = e.LastPollAt.Format("2006-01-02 15:04:05")
							}
							fmt.Fprintf(w, "%s\t%s\t%ds\t%v\t%s\n", e.Name, e.URL, e.PollInterval, e.Enabled, lastPoll)
						}
						return w.Flush()
					},
				},
				{
					Name:      "add",
					Usage:     "Add a new exchanger",
					ArgsUsage: "NAME ENDPOINT",
					Flags: []cli.Flag{
						&cli.IntFlag{
							Name:  "interval",
							Usage: "Polling interval in seconds",
							Value: 60,
						},
					},
					Action: func(ctx *cli.Context) error {
						name := ctx.Args().Get(0)
						endpoint := ctx.Args().Get(1)
						if name == "" || endpoint == "" {
							return cli.Exit("Error: NAME and ENDPOINT are required", 2)
						}

						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						// Ensure table exists
						if err := database.AutoMigrate(&db.Exchanger{}); err != nil {
							return err
						}

						ex := db.Exchanger{
							Name:         name,
							URL:          endpoint,
							Enabled:      true,
							PollInterval: ctx.Int("interval"),
						}
						if err := database.Create(&ex).Error; err != nil {
							return err
						}

						fmt.Printf("Exchanger added: %s -> %s\n", name, endpoint)
						return nil
					},
				},
				{
					Name:      "remove",
					Usage:     "Remove an exchanger",
					ArgsUsage: "NAME",
					Action: func(ctx *cli.Context) error {
						name := ctx.Args().First()
						if name == "" {
							return cli.Exit("Error: NAME is required", 2)
						}

						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						if err := database.Delete(&db.Exchanger{Name: name}).Error; err != nil {
							return err
						}

						fmt.Printf("Exchanger removed: %s\n", name)
						return nil
					},
				},
				{
					Name:      "enable",
					Usage:     "Enable an exchanger",
					ArgsUsage: "NAME",
					Action: func(ctx *cli.Context) error {
						name := ctx.Args().First()
						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						if err := database.Model(&db.Exchanger{Name: name}).Update("enabled", true).Error; err != nil {
							return err
						}
						fmt.Printf("Exchanger enabled: %s\n", name)
						return nil
					},
				},
				{
					Name:      "disable",
					Usage:     "Disable an exchanger",
					ArgsUsage: "NAME",
					Action: func(ctx *cli.Context) error {
						name := ctx.Args().First()
						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						if err := database.Model(&db.Exchanger{Name: name}).Update("enabled", false).Error; err != nil {
							return err
						}
						fmt.Printf("Exchanger disabled: %s\n", name)
						return nil
					},
				},
			},
		})
}
