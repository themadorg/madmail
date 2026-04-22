package ctl

import (
	"fmt"
	"strconv"
	"strings"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/urfave/cli/v2"
)

type portAccessSpec struct {
	name        string
	displayName string
	portKey     string
	defaultPort string
	localKeys   []string
}

var portAccessSpecs = []portAccessSpec{
	{name: "smtp", displayName: "SMTP (25)", portKey: "__SMTP_PORT__", defaultPort: "25", localKeys: []string{"__SMTP_LOCAL_ONLY__"}},
	{name: "submission", displayName: "Submission (587)", portKey: "__SUBMISSION_PORT__", defaultPort: "587", localKeys: []string{"__SUBMISSION_LOCAL_ONLY__"}},
	{name: "submission-tls", displayName: "Submission TLS (465)", portKey: "__SUBMISSION_TLS_PORT__", defaultPort: "465", localKeys: []string{"__SUBMISSION_TLS_LOCAL_ONLY__"}},
	{name: "imap", displayName: "IMAP (143)", portKey: "__IMAP_PORT__", defaultPort: "143", localKeys: []string{"__IMAP_LOCAL_ONLY__"}},
	{name: "imap-tls", displayName: "IMAP TLS (993)", portKey: "__IMAP_TLS_PORT__", defaultPort: "993", localKeys: []string{"__IMAP_TLS_LOCAL_ONLY__"}},
	{name: "turn", displayName: "TURN (3478)", portKey: "__TURN_PORT__", defaultPort: "3478", localKeys: []string{"__TURN_LOCAL_ONLY__"}},
	{name: "sasl", displayName: "SASL (24)", portKey: "__SASL_PORT__", defaultPort: "24", localKeys: []string{"__SASL_LOCAL_ONLY__"}},
	{name: "iroh", displayName: "Iroh (3340)", portKey: "__IROH_PORT__", defaultPort: "3340", localKeys: []string{"__IROH_LOCAL_ONLY__"}},
	{name: "shadowsocks", displayName: "Shadowsocks (8388)", portKey: "__SS_PORT__", defaultPort: "8388"},
	{name: "http", displayName: "HTTP (80)", portKey: "__HTTP_PORT__", defaultPort: "80", localKeys: []string{"__HTTP_LOCAL_ONLY__"}},
	{name: "https", displayName: "HTTPS (443)", portKey: "__HTTPS_PORT__", defaultPort: "443", localKeys: []string{"__HTTPS_LOCAL_ONLY__"}},
}

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "port",
		Usage: "Manage admin-panel ports (status/mode/port number)",
		Description: `Inspect and control admin-panel ports.

Examples:
  maddy port status
  maddy port submission status
  maddy port submission local
  maddy port submission public
  maddy port submission set 2525
  maddy port submission reset`,
		Subcommands: buildPortSubcommands(),
	})
}

func buildPortSubcommands() []*cli.Command {
	commonFlags := []cli.Flag{
		&cli.StringFlag{
			Name:    "state-dir",
			Usage:   "Path to the state directory",
			EnvVars: []string{"MADDY_STATE_DIR"},
		},
	}

	subcommands := []*cli.Command{
		{
			Name:   "status",
			Usage:  "Show mode and value for all admin-panel ports",
			Flags:  commonFlags,
			Action: portStatusAll,
		},
	}

	for _, spec := range portAccessSpecs {
		s := spec
		subcommands = append(subcommands, &cli.Command{
			Name:        s.name,
			Aliases:     serviceAliases(s.name),
			Usage:       "Manage " + s.displayName,
			Subcommands: buildServiceCommands(s, commonFlags),
		})
	}

	return subcommands
}

func buildServiceCommands(spec portAccessSpec, commonFlags []cli.Flag) []*cli.Command {
	commands := []*cli.Command{
		{
			Name:   "status",
			Usage:  "Show current mode and value",
			Flags:  commonFlags,
			Action: func(c *cli.Context) error { return portServiceStatus(c, spec) },
		},
		{
			Name:      "set",
			Usage:     "Set port value (restart required)",
			ArgsUsage: "<1-65535>",
			Flags:     commonFlags,
			Action: func(c *cli.Context) error {
				return portServiceSetPort(c, spec, c.Args().First())
			},
		},
		{
			Name:   "reset",
			Usage:  "Reset port to config/default (restart required)",
			Flags:  commonFlags,
			Action: func(c *cli.Context) error { return portServiceResetPort(c, spec) },
		},
	}

	if len(spec.localKeys) > 0 {
		commands = append(commands,
			&cli.Command{
				Name:   "local",
				Usage:  "Set to localhost-only (restart required)",
				Flags:  commonFlags,
				Action: func(c *cli.Context) error { return portServiceSetMode(c, spec, "local") },
			},
			&cli.Command{
				Name:   "public",
				Usage:  "Set to public (restart required)",
				Flags:  commonFlags,
				Action: func(c *cli.Context) error { return portServiceSetMode(c, spec, "public") },
			},
		)
	}

	return commands
}

func serviceAliases(name string) []string {
	switch name {
	case "submission-tls":
		return []string{"submission_tls"}
	case "imap-tls":
		return []string{"imap_tls"}
	case "shadowsocks":
		return []string{"ss"}
	default:
		return nil
	}
}

func portStatusAll(c *cli.Context) error {
	cfg := getDBConfig(c)
	settings := readSettingsFromDB(cfg)

	fmt.Println()
	for _, spec := range portAccessSpecs {
		mode := serviceMode(settings, spec)
		port := servicePort(settings, spec)
		fmt.Printf("  %-24s port=%s mode=%s\n", spec.displayName+":", port, mode)
	}
	fmt.Println()
	fmt.Println("  Note: restart service after changes.")
	fmt.Println()
	return nil
}

func portServiceStatus(c *cli.Context, spec portAccessSpec) error {
	cfg := getDBConfig(c)
	settings := readSettingsFromDB(cfg)
	mode := serviceMode(settings, spec)
	port := servicePort(settings, spec)

	fmt.Println()
	fmt.Printf("  %s:\n", spec.displayName)
	fmt.Printf("    port: %s\n", port)
	fmt.Printf("    mode: %s\n", mode)
	fmt.Println()
	return nil
}

func portServiceSetMode(c *cli.Context, spec portAccessSpec, mode string) error {
	if len(spec.localKeys) == 0 {
		return fmt.Errorf("%s does not support local/public mode", spec.displayName)
	}

	cfg := getDBConfig(c)
	switch mode {
	case "local":
		for _, key := range spec.localKeys {
			if err := setSetting(cfg, key, "true"); err != nil {
				return fmt.Errorf("failed to set %s: %v", key, err)
			}
		}
	case "public":
		for _, key := range spec.localKeys {
			if err := deleteSetting(cfg, key); err != nil {
				return fmt.Errorf("failed to clear %s: %v", key, err)
			}
		}
	default:
		return fmt.Errorf("unsupported mode: %s", mode)
	}

	fmt.Printf("✅ %s set to %s (restart required)\n", spec.displayName, mode)
	return nil
}

func portServiceSetPort(c *cli.Context, spec portAccessSpec, val string) error {
	if val == "" {
		return fmt.Errorf("port value is required")
	}
	p, err := strconv.Atoi(val)
	if err != nil || p < 1 || p > 65535 {
		return fmt.Errorf("invalid port %q (must be 1-65535)", val)
	}

	cfg := getDBConfig(c)
	if err := setSetting(cfg, spec.portKey, strconv.Itoa(p)); err != nil {
		return fmt.Errorf("failed to set %s: %v", spec.portKey, err)
	}
	fmt.Printf("✅ %s port set to %d (restart required)\n", spec.displayName, p)
	return nil
}

func portServiceResetPort(c *cli.Context, spec portAccessSpec) error {
	cfg := getDBConfig(c)
	if err := deleteSetting(cfg, spec.portKey); err != nil {
		return fmt.Errorf("failed to reset %s: %v", spec.portKey, err)
	}
	fmt.Printf("✅ %s port reset to config/default (restart required)\n", spec.displayName)
	return nil
}

func servicePort(settings map[string]string, spec portAccessSpec) string {
	if v := strings.TrimSpace(settings[spec.portKey]); v != "" {
		return v + " (override)"
	}
	return spec.defaultPort + " (default)"
}

func serviceMode(settings map[string]string, spec portAccessSpec) string {
	if len(spec.localKeys) == 0 {
		return "n/a"
	}
	for _, key := range spec.localKeys {
		if strings.EqualFold(settings[key], "true") {
			return "local"
		}
	}
	return "public"
}

