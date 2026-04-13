package ctl

import (
	"fmt"
	"os"
	"strings"
	"text/tabwriter"
	"time"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/federationtracker"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:  "federation",
			Usage: "Federation policy and rules management",
			Description: `These commands manage federation policies and domain exception
rules. When the policy is ACCEPT, the rules act as a blocklist.
When the policy is REJECT, the rules act as an allowlist.`,
			Subcommands: []*cli.Command{
				{
					Name:      "policy",
					Usage:     "Set the global federation posture (accept or reject)",
					ArgsUsage: "accept|reject",
					Action: func(ctx *cli.Context) error {
						policy := strings.ToUpper(ctx.Args().First())
						if policy != "ACCEPT" && policy != "REJECT" {
							return cli.Exit("Error: policy must be 'accept' or 'reject'", 2)
						}

						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						// Set the policy in the settings table (same pattern as other CLI settings)
						if err := database.Exec(
							"INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
							"__FEDERATION_POLICY__", policy,
						).Error; err != nil {
							// Try the generic table_entries approach
							if err := database.Exec(
								"INSERT OR REPLACE INTO table_entries (key, value) VALUES (?, ?)",
								"__FEDERATION_POLICY__", policy,
							).Error; err != nil {
								return fmt.Errorf("failed to set policy: %v", err)
							}
						}

						fmt.Printf("Success: Global federation policy switched to %s.\n", policy)
						return nil
					},
				},
				{
					Name:      "block",
					Usage:     "Add a domain to the rules table (blocklist in ACCEPT mode)",
					ArgsUsage: "DOMAIN",
					Action: func(ctx *cli.Context) error {
						domain := ctx.Args().First()
						if domain == "" {
							return cli.Exit("Error: DOMAIN is required", 2)
						}

						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						ps := federationtracker.GlobalPolicy()
						ps.Init(database)

						count, err := ps.AddRule(domain)
						if err != nil {
							return fmt.Errorf("failed to add rule: %v", err)
						}

						fmt.Printf("Success: '%s' added to rules. Currently blocking %d total domain(s).\n", domain, count)
						return nil
					},
				},
				{
					Name:      "allow",
					Usage:     "Add a domain to the rules table (allowlist in REJECT mode)",
					ArgsUsage: "DOMAIN",
					Action: func(ctx *cli.Context) error {
						domain := ctx.Args().First()
						if domain == "" {
							return cli.Exit("Error: DOMAIN is required", 2)
						}

						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						ps := federationtracker.GlobalPolicy()
						ps.Init(database)

						count, err := ps.AddRule(domain)
						if err != nil {
							return fmt.Errorf("failed to add rule: %v", err)
						}

						fmt.Printf("Success: '%s' added to rules. Currently trusting %d total domain(s).\n", domain, count)
						return nil
					},
				},
				{
					Name:      "remove",
					Usage:     "Remove a domain from the rules table",
					ArgsUsage: "DOMAIN",
					Action: func(ctx *cli.Context) error {
						domain := ctx.Args().First()
						if domain == "" {
							return cli.Exit("Error: DOMAIN is required", 2)
						}

						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						ps := federationtracker.GlobalPolicy()
						ps.Init(database)

						remaining, err := ps.RemoveRule(domain)
						if err != nil {
							return fmt.Errorf("failed to remove rule: %v", err)
						}

						fmt.Printf("Success: Removed '%s' from rules. %d remaining.\n", domain, remaining)
						return nil
					},
				},
				{
					Name:  "flush",
					Usage: "Remove ALL domain exceptions (emergency override)",
					Action: func(ctx *cli.Context) error {
						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						ps := federationtracker.GlobalPolicy()
						ps.Init(database)

						if err := ps.FlushRules(); err != nil {
							return fmt.Errorf("failed to flush rules: %v", err)
						}

						fmt.Println("WARNING: Configuration flushed. 0 custom domains remain in active list.")
						return nil
					},
				},
				{
					Name:  "list",
					Usage: "Show current policy and all active rules",
					Action: func(ctx *cli.Context) error {
						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						ps := federationtracker.GlobalPolicy()
						ps.Init(database)

						// Read policy from DB
						policy := "ACCEPT"
						var policyVal struct{ Value string }
						if err := database.Raw(
							"SELECT value FROM settings WHERE key = ?",
							"__FEDERATION_POLICY__",
						).Scan(&policyVal).Error; err == nil && policyVal.Value != "" {
							policy = policyVal.Value
						} else {
							// Try table_entries
							if err := database.Raw(
								"SELECT value FROM table_entries WHERE key = ?",
								"__FEDERATION_POLICY__",
							).Scan(&policyVal).Error; err == nil && policyVal.Value != "" {
								policy = policyVal.Value
							}
						}

						fmt.Println("[ FEDERATION STATE ]")
						fmt.Printf("Policy:   %s\n\n", policy)

						rules := ps.ListRules()
						if len(rules) == 0 {
							fmt.Println("[ ACTIVE RULES ]")
							fmt.Println("No exceptions configured.")
							fmt.Println("---")
							fmt.Println("Total: 0 exceptions.")
							return nil
						}

						fmt.Println("[ ACTIVE RULES ]")
						w := tabwriter.NewWriter(os.Stdout, 0, 8, 2, ' ', 0)
						i := 1
						for domain, createdAt := range rules {
							dateStr := time.Unix(createdAt, 0).Format("2006-01-02")
							fmt.Fprintf(w, "%d. %s\t(Added: %s)\n", i, domain, dateStr)
							i++
						}
						w.Flush()
						fmt.Println("---")
						fmt.Printf("Total: %d exceptions.\n", len(rules))
						return nil
					},
				},
				{
					Name:  "status",
					Usage: "Show live federation traffic diagnostics from RAM",
					Action: func(ctx *cli.Context) error {
						cfg := getDBConfig(ctx)
						database, err := openDB(cfg)
						if err != nil {
							return err
						}
						defer closeDB(database)

						// Hydrate tracker from DB (since CLI doesn't share memory with server)
						tracker := federationtracker.Global()
						tracker.Hydrate(database)

						stats := tracker.GetAll()
						if len(stats) == 0 {
							fmt.Println("[ TRAFFIC ANOMALIES ]")
							fmt.Println("No federation traffic recorded.")
							return nil
						}

						fmt.Println("[ TRAFFIC ANOMALIES ]")
						for _, s := range stats {
							totalFailed := s.FailedHTTP + s.FailedHTTPS + s.FailedSMTP

							// Build success transport info
							successInfo := fmt.Sprintf("%d Delivered", s.SuccessfulDeliveries)
							if s.SuccessHTTPS > 0 || s.SuccessHTTP > 0 || s.SuccessSMTP > 0 {
								var parts []string
								if s.SuccessHTTPS > 0 {
									parts = append(parts, fmt.Sprintf("%d HTTPS", s.SuccessHTTPS))
								}
								if s.SuccessHTTP > 0 {
									parts = append(parts, fmt.Sprintf("%d HTTP", s.SuccessHTTP))
								}
								if s.SuccessSMTP > 0 {
									parts = append(parts, fmt.Sprintf("%d SMTP", s.SuccessSMTP))
								}
								successInfo += " (" + strings.Join(parts, ", ") + ")"
							}

							// Build failure transport info
							failInfo := ""
							if s.FailedHTTPS > 0 {
								failInfo += fmt.Sprintf(" %d Failed (HTTPS)", s.FailedHTTPS)
							}
							if s.FailedHTTP > 0 {
								failInfo += fmt.Sprintf(" %d Failed (HTTP)", s.FailedHTTP)
							}
							if s.FailedSMTP > 0 {
								failInfo += fmt.Sprintf(" %d Failed (SMTP)", s.FailedSMTP)
							}
							if failInfo == "" && totalFailed == 0 {
								failInfo = " 0 Failed"
							}
							fmt.Printf("- %s : %s / %d pending /%s\n", s.Domain, successInfo, s.QueuedMessages, failInfo)
						}
						return nil
					},
				},
			},
		})
}
