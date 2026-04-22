package ctl

import (
	"fmt"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/urfave/cli/v2"
)

const (
	dbKeySubmissionLocalOnly    = "__SUBMISSION_LOCAL_ONLY__"
	dbKeySubmissionTLSLocalOnly = "__SUBMISSION_TLS_LOCAL_ONLY__"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "submission-access",
		Usage: "Manage SMTP submission access scope (public/local)",
		Description: `Control whether SMTP submission ports bind publicly (0.0.0.0)
or localhost-only (127.0.0.1).

This command updates these settings keys:
  - __SUBMISSION_LOCAL_ONLY__ (port 587)
  - __SUBMISSION_TLS_LOCAL_ONLY__ (port 465)

A service restart is required for listener rebind.

Examples:
  maddy submission-access status
  maddy submission-access local
  maddy submission-access public`,
		Subcommands: []*cli.Command{
			{
				Name:  "status",
				Usage: "Show current submission access mode",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: submissionAccessStatus,
			},
			{
				Name:  "local",
				Usage: "Set submission ports (465/587) to localhost-only",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: submissionAccessLocal,
			},
			{
				Name:  "public",
				Usage: "Set submission ports (465/587) to public access",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: submissionAccessPublic,
			},
		},
	})
}

func submissionAccessStatus(c *cli.Context) error {
	cfg := getDBConfig(c)
	settings := readSettingsFromDB(cfg)

	local587 := settings[dbKeySubmissionLocalOnly] == "true"
	local465 := settings[dbKeySubmissionTLSLocalOnly] == "true"

	fmt.Println()
	fmt.Printf("  Submission 587: %s\n", accessLabel(local587))
	fmt.Printf("  Submission 465: %s\n", accessLabel(local465))
	if local587 || local465 {
		fmt.Println("  Effective mode: local-only")
	} else {
		fmt.Println("  Effective mode: public")
	}
	fmt.Println()
	return nil
}

func submissionAccessLocal(c *cli.Context) error {
	cfg := getDBConfig(c)
	if err := setSetting(cfg, dbKeySubmissionLocalOnly, "true"); err != nil {
		return fmt.Errorf("failed to set %s: %v", dbKeySubmissionLocalOnly, err)
	}
	if err := setSetting(cfg, dbKeySubmissionTLSLocalOnly, "true"); err != nil {
		return fmt.Errorf("failed to set %s: %v", dbKeySubmissionTLSLocalOnly, err)
	}
	fmt.Println("✅ Submission access set to local-only (restart required)")
	return nil
}

func submissionAccessPublic(c *cli.Context) error {
	cfg := getDBConfig(c)
	if err := deleteSetting(cfg, dbKeySubmissionLocalOnly); err != nil {
		return fmt.Errorf("failed to clear %s: %v", dbKeySubmissionLocalOnly, err)
	}
	if err := deleteSetting(cfg, dbKeySubmissionTLSLocalOnly); err != nil {
		return fmt.Errorf("failed to clear %s: %v", dbKeySubmissionTLSLocalOnly, err)
	}
	fmt.Println("✅ Submission access set to public (restart required)")
	return nil
}

func accessLabel(local bool) string {
	if local {
		return "local"
	}
	return "public"
}
