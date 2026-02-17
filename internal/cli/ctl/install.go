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
	"bufio"
	"crypto"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/base64"
	"encoding/pem"
	"fmt"
	"io"
	"io/fs"
	"log"
	"math/big"
	"net"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"strconv"
	"strings"
	"text/template"
	"time"

	_ "embed" // for embedding templates

	"github.com/themadorg/madmail/internal/auth"
	maddycli "github.com/themadorg/madmail/internal/cli"
	clitools2 "github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/themadorg/madmail/internal/endpoint/iroh"
	"github.com/urfave/cli/v2"
)

func silentPrint(noLog bool, format string, a ...interface{}) {
	if !noLog {
		if len(a) > 0 {
			fmt.Printf(format, a...)
		} else {
			fmt.Print(format)
		}
	}
}

func silentPrintln(noLog bool, a ...interface{}) {
	if !noLog {
		fmt.Println(a...)
	}
}

//go:embed maddy.conf.j2
var maddyConfigTemplate string

// InstallConfig holds all configuration values for the installation
type InstallConfig struct {
	// Basic configuration
	Hostname      string
	PrimaryDomain string
	LocalDomains  string
	StateDir      string
	RuntimeDir    string
	Generated     string
	SimpleInstall bool

	// TLS configuration
	TLSCertPath   string
	TLSKeyPath    string
	GenerateCerts bool

	// Network configuration
	SMTPPort       string
	SubmissionPort string
	SubmissionTLS  string
	IMAPPort       string
	IMAPTLS        string

	// Chatmail configuration
	EnableChatmail       bool
	ChatmailHTTPPort     string
	ChatmailHTTPSPort    string
	ChatmailUsernameLen  int
	ChatmailPasswordLen  int
	EnableContactSharing bool

	// Shadowsocks configuration
	EnableSS   bool
	SSAddr     string
	SSPassword string
	SSCipher   string

	// TURN configuration
	EnableTURN bool
	TURNServer string
	TURNPort   string
	TURNSecret string
	TURNTTL    int

	// PGP Encryption configuration
	RequirePGPEncryption     bool
	AllowSecureJoin          bool
	PGPPassthroughSenders    []string
	PGPPassthroughRecipients []string

	// DNS configuration (for template)
	A             string
	AAAA          string
	DKIM_Entry    string
	STS_ID        string
	ACME_Account  string
	UseCloudflare bool // Add Cloudflare proxy disable tags
	PublicIP      string

	// Security configuration
	AllowInsecureAuth bool
	TurnOffTLS        bool

	// System configuration
	MaddyUser      string
	MaddyGroup     string
	ConfigDir      string
	SystemdPath    string
	BinaryPath     string
	LibexecDir     string
	NoLog          bool
	Debug          bool
	MaxMessageSize string
	SkipSync       bool
	SkipUser       bool
	SkipSystemd    bool
	// Iroh relay configuration
	EnableIroh bool
	IrohPort   string

	// Internal state
	SkipPrompts bool

	// Admin API
	AdminToken string
}

// Default configuration values
func defaultConfig() *InstallConfig {
	return &InstallConfig{
		Hostname:                 "example.org",
		PrimaryDomain:            "example.org",
		LocalDomains:             "$(primary_domain)",
		StateDir:                 "/var/lib/maddy",
		Generated:                time.Now().Format("2006-01-02 15:04:05"),
		SimpleInstall:            false,
		TLSCertPath:              "/etc/maddy/certs/fullchain.pem",
		TLSKeyPath:               "/etc/maddy/certs/privkey.pem",
		GenerateCerts:            false,
		SMTPPort:                 "25",
		SubmissionPort:           "587",
		SubmissionTLS:            "465",
		IMAPPort:                 "143",
		IMAPTLS:                  "993",
		AllowInsecureAuth:        false,
		EnableChatmail:           false,
		ChatmailHTTPPort:         "80",
		ChatmailHTTPSPort:        "443",
		ChatmailUsernameLen:      8,
		ChatmailPasswordLen:      16,
		EnableContactSharing:     true,
		RequirePGPEncryption:     false,
		AllowSecureJoin:          true,
		PGPPassthroughSenders:    []string{},
		PGPPassthroughRecipients: []string{},
		UseCloudflare:            true, // Default to adding Cloudflare proxy disable tags
		MaddyUser:                "maddy",
		MaddyGroup:               "maddy",
		ConfigDir:                "/etc/maddy",
		PublicIP:                 "127.0.0.1",
		SystemdPath:              "/etc/systemd/system",
		BinaryPath:               "/usr/local/bin/maddy",
		LibexecDir:               "/var/lib/maddy",
		RuntimeDir:               "/run/maddy",
		NoLog:                    true,
		EnableSS:                 true,
		SSAddr:                   "0.0.0.0:8388",
		SSCipher:                 "aes-128-gcm",
		EnableTURN:               true,
		TURNPort:                 "3478",
		TURNTTL:                  86400,
		MaxMessageSize:           "32M",
		SkipSync:                 false,
		EnableIroh:               false,
		IrohPort:                 "3340",
	}
}

var logger *log.Logger

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:  "install",
			Usage: "Install and configure maddy mail server",
			Description: `Install maddy mail server with interactive or non-interactive configuration.

This command will:
- Create maddy user and group
- Install systemd service files
- Generate configuration file
- Set up initial certificates (if needed)
- Configure DNS recommendations

Examples:
  maddy install                          # Interactive installation
  maddy install --non-interactive       # Non-interactive with defaults
  maddy install --domain example.org    # Non-interactive with domain
`,
			Action: installCommand,
			Flags: []cli.Flag{
				&cli.BoolFlag{
					Name:    "non-interactive",
					Aliases: []string{"n"},
					Usage:   "Run non-interactive installation with default values",
				},
				&cli.BoolFlag{
					Name:    "simple",
					Aliases: []string{"s"},
					Usage:   "Run simplified installation with minimal questions",
				},
				&cli.StringFlag{
					Name:  "domain",
					Usage: "Primary domain for the mail server",
				},
				&cli.StringFlag{
					Name:  "hostname",
					Usage: "Hostname for the mail server (MX record)",
				},
				&cli.StringFlag{
					Name:  "state-dir",
					Usage: "Directory for maddy state files",
					Value: "/var/lib/maddy",
				},
				&cli.StringFlag{
					Name:  "config-dir",
					Usage: "Directory for maddy configuration",
					Value: "/etc/maddy",
				},
				&cli.StringFlag{
					Name:  "libexec-dir",
					Usage: "Directory for maddy runtime files (same as state-dir by default)",
					Value: "/var/lib/maddy",
				},
				&cli.StringFlag{
					Name:  "cert-path",
					Usage: "Path to TLS certificate file",
				},
				&cli.StringFlag{
					Name:  "key-path",
					Usage: "Path to TLS private key file",
				},
				&cli.BoolFlag{
					Name:  "enable-chatmail",
					Usage: "Enable chatmail endpoint for user registration",
				},
				&cli.BoolFlag{
					Name:  "require-pgp-encryption",
					Usage: "Require PGP encryption for outgoing messages",
				},
				&cli.BoolFlag{
					Name:  "allow-secure-join",
					Usage: "Allow secure join requests even without encryption",
					Value: true,
				},
				&cli.BoolFlag{
					Name:  "enable-contact-sharing",
					Usage: "Enable DeltaChat contact sharing feature",
					Value: true,
				},
				&cli.StringSliceFlag{
					Name:  "pgp-passthrough-senders",
					Usage: "Sender addresses that bypass PGP encryption requirements",
				},
				&cli.StringSliceFlag{
					Name:  "pgp-passthrough-recipients",
					Usage: "Recipient addresses that bypass PGP encryption requirements",
				},
				&cli.BoolFlag{
					Name:  "dry-run",
					Usage: "Show what would be done without making changes",
				},
				&cli.BoolFlag{
					Name:  "skip-dns",
					Usage: "Skip interactive DNS configuration and verification",
				},
				&cli.BoolFlag{
					Name:  "turn-off-tls",
					Usage: "Disable TLS for user registration and client configuration (useful for localhost/self-signed)",
				},
				&cli.BoolFlag{
					Name:  "generate-certs",
					Usage: "Generate self-signed TLS certificates if they don't exist",
				},
				&cli.BoolFlag{
					Name:  "cloudflare",
					Usage: "Add Cloudflare proxy disable tags to DNS records (default: true)",
					Value: true,
				},
				&cli.StringFlag{
					Name:  "ip",
					Usage: "Public IP address (sets domain/hostname in --simple mode, only PublicIP in advanced mode)",
				},
				&cli.BoolFlag{
					Name:    "debug",
					Aliases: []string{"d"},
					Usage:   "Enable debug logging (overrides --simple silence)",
				},
				&cli.BoolFlag{
					Name:  "enable-ss",
					Usage: "Enable Shadowsocks proxy for faster messaging",
				},
				&cli.StringFlag{
					Name:  "ss-password",
					Usage: "Shadowsocks password",
				},
				&cli.BoolFlag{
					Name:  "enable-turn",
					Usage: "Enable TURN server for video calls",
					Value: true,
				},
				&cli.BoolFlag{
					Name:  "disable-turn",
					Usage: "Disable TURN server for video calls",
				},
				&cli.StringFlag{
					Name:  "turn-server",
					Usage: "TURN server hostname",
				},
				&cli.StringFlag{
					Name:  "turn-secret",
					Usage: "TURN server shared secret",
				},
				&cli.StringFlag{
					Name:  "max-message-size",
					Usage: "Maximum message size (e.g. 32M, 100M)",
					Value: "32M",
				},
				&cli.BoolFlag{
					Name:  "skip-sync",
					Usage: "Disable SQLite synchronous mode (unsafe, may corrupt data on crash)",
				},
				&cli.StringFlag{
					Name:  "binary-path",
					Usage: "Path to install maddy binary",
					Value: "/usr/local/bin/maddy",
				},
				&cli.StringFlag{
					Name:  "systemd-path",
					Usage: "Directory for systemd service files",
					Value: "/etc/systemd/system",
				},
				&cli.StringFlag{
					Name:  "maddy-user",
					Usage: "User to run maddy as",
					Value: "maddy",
				},
				&cli.StringFlag{
					Name:  "maddy-group",
					Usage: "Group to run maddy as",
					Value: "maddy",
				},
				&cli.BoolFlag{
					Name:  "skip-user",
					Usage: "Skip creation of maddy user and group",
				},
				&cli.BoolFlag{
					Name:  "skip-systemd",
					Usage: "Skip installation of systemd service files",
				},
				&cli.BoolFlag{
					Name:  "enable-iroh",
					Usage: "Enable Iroh relay deployment",
				},
				&cli.StringFlag{
					Name:  "iroh-port",
					Usage: "Port for the Iroh relay",
					Value: "3340",
				},
			},
		})
}

func installCommand(ctx *cli.Context) error {
	// Apply command line flags
	config := defaultConfig()
	if ctx.Bool("debug") {
		config.Debug = true
		config.NoLog = false
	} else if ctx.Bool("simple") {
		config.NoLog = true
	}

	// Initialize logger for installation-time output.
	// We only silence installation logs if --simple is used AND --debug is NOT used.
	installLogSilent := config.NoLog && !ctx.Bool("debug")
	if err := initLogger(installLogSilent); err != nil {
		return fmt.Errorf("failed to initialize logger: %v", err)
	}

	logger.Println("Starting maddy installation process")
	silentPrintln(config.NoLog, "🚀 Maddy Mail Server Installation")
	silentPrintln(config.NoLog, "==================================")

	// Apply early flags needed for root check
	if ctx.IsSet("binary-path") {
		config.BinaryPath = ctx.String("binary-path")
	}
	if ctx.IsSet("config-dir") {
		config.ConfigDir = ctx.String("config-dir")
	}
	if ctx.IsSet("state-dir") {
		config.StateDir = ctx.String("state-dir")
	}

	// Check if running as root
	if os.Geteuid() != 0 && !ctx.Bool("dry-run") {
		// If we're not root, we can only continue if we're not using system-wide paths
		isSystemInstall := config.ConfigDir == "/etc/maddy" ||
			config.StateDir == "/var/lib/maddy" ||
			config.BinaryPath == "/usr/local/bin/maddy"

		if isSystemInstall {
			if config.NoLog {
				os.Exit(1)
			}
			return fmt.Errorf("installation to system paths requires root (use sudo or specify local paths with --config-dir, --state-dir, and --binary-path)")
		}

		// Non-root, local install: default to current user/group and skip system-level steps
		u, err := user.Current()
		if err == nil {
			config.MaddyUser = u.Username
			config.SkipUser = true
			config.SkipSystemd = true
			if g, err := user.LookupGroupId(u.Gid); err == nil {
				config.MaddyGroup = g.Name
			}
		}
		// In local install, runtime_dir should be inside state_dir to avoid permission issues
		config.RuntimeDir = filepath.Join(config.StateDir, "run")
	}

	// Apply command line flags
	if ctx.IsSet("domain") {
		config.PrimaryDomain = ctx.String("domain")
		if !ctx.IsSet("hostname") {
			config.Hostname = ctx.String("domain")
		}
	}
	if ctx.Bool("skip-sync") {
		config.SkipSync = true
	}
	if ctx.IsSet("hostname") {
		config.Hostname = ctx.String("hostname")
	}
	if ctx.IsSet("state-dir") {
		config.StateDir = ctx.String("state-dir")
	}
	if ctx.IsSet("config-dir") {
		config.ConfigDir = ctx.String("config-dir")
	}
	if ctx.IsSet("libexec-dir") {
		config.LibexecDir = ctx.String("libexec-dir")
	} else {
		// If libexec-dir is not set, use the same as state-dir
		config.LibexecDir = config.StateDir
	}
	if ctx.IsSet("cert-path") {
		config.TLSCertPath = ctx.String("cert-path")
	} else if ctx.IsSet("config-dir") {
		config.TLSCertPath = filepath.Join(config.ConfigDir, "certs", "fullchain.pem")
	}

	if ctx.IsSet("key-path") {
		config.TLSKeyPath = ctx.String("key-path")
	} else if ctx.IsSet("config-dir") {
		config.TLSKeyPath = filepath.Join(config.ConfigDir, "certs", "privkey.pem")
	}

	if ctx.IsSet("generate-certs") {
		config.GenerateCerts = ctx.Bool("generate-certs")
	}
	if ctx.IsSet("enable-chatmail") {
		config.EnableChatmail = ctx.Bool("enable-chatmail")
	}
	if ctx.IsSet("enable-contact-sharing") {
		config.EnableContactSharing = ctx.Bool("enable-contact-sharing")
	}
	if ctx.IsSet("require-pgp-encryption") {
		config.RequirePGPEncryption = ctx.Bool("require-pgp-encryption")
	}
	if ctx.IsSet("allow-secure-join") {
		config.AllowSecureJoin = ctx.Bool("allow-secure-join")
	}
	if ctx.IsSet("pgp-passthrough-senders") {
		config.PGPPassthroughSenders = ctx.StringSlice("pgp-passthrough-senders")
	}
	if ctx.IsSet("pgp-passthrough-recipients") {
		config.PGPPassthroughRecipients = ctx.StringSlice("pgp-passthrough-recipients")
	}
	if ctx.Bool("simple") {
		config.SimpleInstall = true
		config.EnableChatmail = true
		config.EnableContactSharing = true
		config.GenerateCerts = true
		if ctx.Bool("turn-off-tls") {
			config.TurnOffTLS = true
		}
		if ctx.IsSet("ip") {
			config.PublicIP = ctx.String("ip")
			config.PrimaryDomain = auth.WrapIP(config.PublicIP)
			config.LocalDomains = fmt.Sprintf("$(primary_domain) %s", config.PublicIP)
			config.A = config.PublicIP
			config.Hostname = ctx.String("ip")
			config.SkipPrompts = true
		}
	} else {
		// Advanced mode
		if ctx.Bool("turn-off-tls") {
			config.TurnOffTLS = true
		}
		// Process --ip flag in advanced mode: only sets PublicIP and A record
		if ctx.IsSet("ip") {
			config.PublicIP = ctx.String("ip")
			config.A = config.PublicIP
			// Note: Domain and hostname come from their own flags or prompts
		}
	}

	if ctx.Bool("enable-ss") {
		config.EnableSS = true
	}
	if ctx.IsSet("ss-password") {
		config.SSPassword = ctx.String("ss-password")
	}

	if ctx.Bool("disable-turn") {
		config.EnableTURN = false
	} else if ctx.IsSet("enable-turn") {
		config.EnableTURN = ctx.Bool("enable-turn")
	}
	// If neither --disable-turn nor --enable-turn is set, keep the default (true)

	if ctx.IsSet("turn-server") {
		config.EnableTURN = true
		config.TURNServer = ctx.String("turn-server")
	}
	if ctx.IsSet("turn-secret") {
		config.TURNSecret = ctx.String("turn-secret")
	}

	if ctx.IsSet("max-message-size") {
		config.MaxMessageSize = ctx.String("max-message-size")
	}
	if ctx.IsSet("systemd-path") {
		config.SystemdPath = ctx.String("systemd-path")
	}
	if ctx.IsSet("maddy-user") {
		config.MaddyUser = ctx.String("maddy-user")
	}
	if ctx.IsSet("maddy-group") {
		config.MaddyGroup = ctx.String("maddy-group")
	}
	if ctx.Bool("skip-user") {
		config.SkipUser = true
	}
	if ctx.Bool("skip-systemd") {
		config.SkipSystemd = true
	}
	if ctx.Bool("enable-iroh") {
		config.EnableIroh = true
	}
	if ctx.IsSet("iroh-port") {
		config.IrohPort = ctx.String("iroh-port")
	}

	// Convert all paths to absolute paths to avoid issues with relative paths in config
	paths := []*string{
		&config.StateDir,
		&config.ConfigDir,
		&config.LibexecDir,
		&config.BinaryPath,
		&config.TLSCertPath,
		&config.TLSKeyPath,
		&config.RuntimeDir,
	}
	for _, p := range paths {
		if *p != "" {
			abs, err := filepath.Abs(*p)
			if err == nil {
				*p = abs
			}
		}
	}

	// Run interactive configuration if not in non-interactive mode
	if !ctx.Bool("non-interactive") {
		if err := runInteractiveConfig(config); err != nil {
			return fmt.Errorf("interactive configuration failed: %v", err)
		}
	}

	// Ensure all required secrets are generated (for both interactive and non-interactive modes)
	if err := ensureRequiredSecrets(config); err != nil {
		return err
	}

	logger.Printf("Configuration: %+v", config)

	// Run installation steps
	steps := []struct {
		name string
		fn   func(*InstallConfig, bool) error
		skip bool
	}{
		{"Checking system requirements", checkSystemRequirements, config.SkipSystemd},
		{"Creating maddy user and group", createMaddyUser, config.SkipUser},
		{"Creating directories", createDirectories, false},
		{"Setting up certificates", setupCertificates, false},
		{"Generating DKIM keys", prepareDKIMKeys, false},
		{"Installing systemd service files", installSystemdFiles, config.SkipSystemd},
		{"Generating configuration file", generateConfigFile, false},
		{"Setting up permissions", setupPermissions, false},
		{"Installing binary", installBinary, false},
		{"Installing Iroh Relay", installIrohRelay, !config.EnableIroh},
	}

	for i, step := range steps {
		if step.skip {
			logger.Printf("Skipping step %d: %s", i+1, step.name)
			continue
		}

		silentPrint(config.NoLog, "\n[%d/%d] %s...\n", i+1, len(steps), step.name)
		logger.Printf("Step %d: %s", i+1, step.name)

		if err := step.fn(config, ctx.Bool("dry-run")); err != nil {
			logger.Printf("Step %d failed: %v", i+1, err)
			if config.NoLog {
				os.Exit(1)
			}
			return fmt.Errorf("step '%s' failed: %v", step.name, err)
		}

		silentPrint(config.NoLog, "✅ %s completed\n", step.name)
		logger.Printf("Step %d completed successfully", i+1)
	}

	// DNS Configuration step - ELIMINATED as per request
	/*
		if !ctx.Bool("skip-dns") && !ctx.Bool("non-interactive") {
			fmt.Printf("\n🌐 DNS Configuration\n")
			fmt.Println("====================")
			if err := configureDNS(config, ctx.Bool("dry-run")); err != nil {
				logger.Printf("DNS configuration failed: %v", err)
				fmt.Printf("⚠️  DNS configuration failed: %v\n", err)
				fmt.Printf("You can continue and configure DNS manually later.\n")
			}
		} else if ctx.Bool("skip-dns") {
			fmt.Printf("\n⏭️  Skipping DNS configuration (--skip-dns flag provided)\n")
		} else {
			fmt.Printf("\n⏭️  Skipping DNS configuration (non-interactive mode)\n")
		}
	*/
	// Print next steps
	printNextSteps(config)

	silentPrintln(config.NoLog, "\n🎉 Installation completed successfully!")
	// Log final summary
	logger.Println("=== INSTALLATION SUMMARY ===")
	logger.Printf("User created: %s with home directory %s", config.MaddyUser, config.StateDir)
	logger.Printf("Directories created: %s, %s, %s/certs",
		config.StateDir, config.ConfigDir, config.ConfigDir)
	logger.Printf("Files created: %s/maddy.conf, %s/maddy.service, %s/maddy@.service, %s",
		config.ConfigDir, config.SystemdPath, config.SystemdPath, config.BinaryPath)
	logger.Printf("Permissions set: %s owned by %s:%s", config.StateDir, config.MaddyUser, config.MaddyGroup)
	logger.Printf("Network ports configured: SMTP:%s, Submission:%s/%s, IMAP:%s/%s",
		config.SMTPPort, config.SubmissionPort, config.SubmissionTLS, config.IMAPPort, config.IMAPTLS)
	if config.EnableChatmail {
		logger.Printf("Chatmail enabled on ports HTTP:%s, HTTPS:%s", config.ChatmailHTTPPort, config.ChatmailHTTPSPort)
	}
	logger.Println("Installation completed successfully")
	fmt.Println("\n🎉 Installation completed successfully!")

	return nil
}

func initLogger(noLog bool) error {
	if noLog {
		logger = log.New(io.Discard, "", 0)
		return nil
	}

	// Always use terminal only for installation logs as per request
	logger = log.New(os.Stderr, "", log.LstdFlags)
	return nil
}

func runInteractiveConfig(config *InstallConfig) error {
	fmt.Println("\n📋 Interactive Configuration")

	if !config.SimpleInstall {
		fmt.Printf("Maddy supports two installation modes:\n")
		fmt.Printf("1. Simple Install (Fast, uses recommended defaults)\n")
		fmt.Printf("2. Advanced Install (Customize every setting)\n")
		choice := promptString("Choose mode (1-2)", "1")
		if choice == "1" {
			config.SimpleInstall = true
			config.NoLog = true
		}
	}

	if config.SimpleInstall {
		fmt.Println("\n🚀 Simple Install: Minimal configuration needed.")

		if !config.SkipPrompts {
			config.PrimaryDomain = promptString("Primary domain (e.g., example.org)", config.PrimaryDomain)
			config.Hostname = config.PrimaryDomain

			// If it's an IP, ensure LocalDomains includes both bracketed and unbracketed versions
			if ip := net.ParseIP(config.PrimaryDomain); ip != nil {
				config.LocalDomains = fmt.Sprintf("[%s] %s", config.PrimaryDomain, config.PrimaryDomain)
			}

			config.PrimaryDomain = auth.WrapIP(config.PrimaryDomain)
			config.PublicIP = promptString("Public IP address (for DNS and registration)", config.PublicIP)
		} else {
			fmt.Printf("Using IP from flags: %s\n", config.PublicIP)
			// Ensure domains are set correctly from IP flag if not already done
			if ip := net.ParseIP(config.PublicIP); ip != nil {
				config.LocalDomains = fmt.Sprintf("[%s] %s", config.PublicIP, config.PublicIP)
			}
		}

		config.EnableChatmail = true
		config.GenerateCerts = true

		if config.PrimaryDomain == "localhost" || config.PrimaryDomain == "127.0.0.1" {
			config.AllowInsecureAuth = true
		}

		config.A = config.PublicIP

		return nil
	}

	fmt.Println("Please provide the following information (press Enter for defaults):")

	// Primary domain
	config.PrimaryDomain = promptString("Primary domain", config.PrimaryDomain)

	// Hostname (MX record)
	defaultHostname := config.PrimaryDomain
	config.Hostname = promptString("Hostname (MX record)", defaultHostname)

	// Wrap PrimaryDomain if it's an IP
	if ip := net.ParseIP(config.PrimaryDomain); ip != nil {
		config.LocalDomains = fmt.Sprintf("[%s] %s", config.PrimaryDomain, config.PrimaryDomain)
	}
	config.PrimaryDomain = auth.WrapIP(config.PrimaryDomain)

	config.PublicIP = promptString("Public IP address", config.PublicIP)
	config.A = config.PublicIP

	// Additional domains
	additionalDomains := promptString("Additional domains (comma-separated, optional)", "")
	if additionalDomains != "" {
		config.LocalDomains = fmt.Sprintf("$(primary_domain) %s", strings.ReplaceAll(additionalDomains, ",", " "))
	}

	// State directory
	config.StateDir = promptString("State directory", config.StateDir)

	// Configuration directory
	config.ConfigDir = promptString("Configuration directory", config.ConfigDir)

	// TLS certificates
	fmt.Println("\n🔒 TLS Certificate Configuration")
	config.TLSCertPath = promptString("TLS certificate path", config.TLSCertPath)
	config.TLSKeyPath = promptString("TLS private key path", config.TLSKeyPath)

	// Check if certificates exist
	if _, err := os.Stat(config.TLSCertPath); os.IsNotExist(err) {
		fmt.Printf("   ⚠️  TLS certificates not found at %s\n", config.TLSCertPath)
		config.GenerateCerts = clitools2.Confirmation("Generate self-signed certificates?", true)
	}

	// Network ports
	fmt.Println("\n🌐 Network Configuration")
	config.SMTPPort = promptString("SMTP port", config.SMTPPort)
	config.SubmissionPort = promptString("Submission port", config.SubmissionPort)
	config.SubmissionTLS = promptString("Submission TLS port", config.SubmissionTLS)
	config.IMAPPort = promptString("IMAP port", config.IMAPPort)
	config.IMAPTLS = promptString("IMAP TLS port", config.IMAPTLS)

	// Insecure auth toggle
	config.AllowInsecureAuth = clitools2.Confirmation("Allow insecure (plain text) authentication?", config.AllowInsecureAuth)

	// Chatmail configuration
	fmt.Println("\n💬 Chatmail Configuration")
	config.EnableChatmail = clitools2.Confirmation("Enable chatmail endpoint for user registration", config.EnableChatmail)

	if config.EnableChatmail {
		config.ChatmailHTTPPort = promptString("Chatmail HTTP port", config.ChatmailHTTPPort)
		config.ChatmailHTTPSPort = promptString("Chatmail HTTPS port", config.ChatmailHTTPSPort)
		config.ChatmailUsernameLen = promptInt("Chatmail username length", config.ChatmailUsernameLen)
		config.ChatmailPasswordLen = promptInt("Chatmail password length", config.ChatmailPasswordLen)
	}

	// Shadowsocks configuration
	fmt.Println("\n🚀 Shadowsocks Configuration")
	if config.SSPassword == "" {
		// Generate a random password if not set
		b := make([]byte, 16)
		if _, err := rand.Read(b); err != nil {
			return fmt.Errorf("failed to generate shadowsocks password: %v", err)
		}
		config.SSPassword = base64.RawURLEncoding.EncodeToString(b)
	}
	config.EnableSS = clitools2.Confirmation("Enable Shadowsocks proxy for faster messaging?", config.EnableSS)
	if config.EnableSS {
		config.SSAddr = promptString("Shadowsocks listen address", config.SSAddr)
		config.SSPassword = promptString("Shadowsocks password", config.SSPassword)
		config.SSCipher = promptString("Shadowsocks cipher", config.SSCipher)
	}

	// TURN configuration
	fmt.Println("\n📞 TURN Server Configuration")
	config.EnableTURN = clitools2.Confirmation("Enable TURN server for video calls?", config.EnableTURN)
	if config.EnableTURN {
		if config.TURNServer == "" {
			config.TURNServer = config.Hostname
		}
		config.TURNServer = promptString("TURN server hostname", config.TURNServer)
		config.TURNPort = promptString("TURN server port", config.TURNPort)
		if config.TURNSecret == "" {
			b := make([]byte, 16)
			if _, err := rand.Read(b); err != nil {
				return fmt.Errorf("failed to generate TURN secret: %v", err)
			}
			config.TURNSecret = base64.RawURLEncoding.EncodeToString(b)
		}
		config.TURNSecret = promptString("TURN server shared secret", config.TURNSecret)
		config.TURNTTL = promptInt("TURN credential TTL (seconds)", config.TURNTTL)
	}

	// PGP Encryption configuration
	fmt.Println("\n🔐 PGP Encryption Configuration")
	config.RequirePGPEncryption = clitools2.Confirmation("Require PGP encryption for outgoing messages", config.RequirePGPEncryption)

	if config.RequirePGPEncryption {
		config.AllowSecureJoin = clitools2.Confirmation("Allow secure join requests without encryption", config.AllowSecureJoin)

		passthroughSenders := promptString("Passthrough senders (comma-separated email addresses that bypass encryption)", "")
		if passthroughSenders != "" {
			config.PGPPassthroughSenders = strings.Split(strings.ReplaceAll(passthroughSenders, " ", ""), ",")
		}

		passthroughRecipients := promptString("Passthrough recipients (comma-separated email addresses that bypass encryption)", "")
		if passthroughRecipients != "" {
			config.PGPPassthroughRecipients = strings.Split(strings.ReplaceAll(passthroughRecipients, " ", ""), ",")
		}
	}

	// Message size limit
	fmt.Println("\n📦 Message Size Configuration")
	config.MaxMessageSize = promptString("Maximum message size (e.g., 32M, 100M)", config.MaxMessageSize)

	// DNS Provider Configuration
	fmt.Println("\n🌐 DNS Provider Configuration")
	config.UseCloudflare = clitools2.Confirmation("Add Cloudflare proxy disable tags to DNS records", config.UseCloudflare)

	// Logging Configuration
	fmt.Println("\n📝 Logging Configuration")
	config.NoLog = !clitools2.Confirmation("Enable logging for the server and installation?", !config.NoLog)
	if config.NoLog {
		// Re-initialize logger to be silent if user chose no log
		_ = initLogger(true)
	}

	return nil
}

func promptString(prompt, defaultValue string) string {
	fmt.Printf("%s [%s]: ", prompt, defaultValue)

	scanner := bufio.NewScanner(os.Stdin)
	if scanner.Scan() {
		value := strings.TrimSpace(scanner.Text())
		if value == "" {
			return defaultValue
		}
		return value
	}

	return defaultValue
}

func promptInt(prompt string, defaultValue int) int {
	for {
		result := promptString(prompt, strconv.Itoa(defaultValue))
		if value, err := strconv.Atoi(result); err == nil {
			return value
		}
		fmt.Printf("Invalid number, please try again.\n")
	}
}

// ensureRequiredSecrets generates any required but missing secrets
func ensureRequiredSecrets(config *InstallConfig) error {
	// Generate a random password for Shadowsocks if it's enabled and not set
	if config.EnableSS && config.SSPassword == "" {
		b := make([]byte, 16)
		if _, err := rand.Read(b); err != nil {
			return fmt.Errorf("failed to generate shadowsocks password: %v", err)
		}
		config.SSPassword = base64.RawURLEncoding.EncodeToString(b)
	}

	if config.EnableTURN {
		if config.TURNServer == "" {
			config.TURNServer = config.Hostname
		}
		if config.TURNSecret == "" {
			b := make([]byte, 16)
			if _, err := rand.Read(b); err != nil {
				return fmt.Errorf("failed to generate TURN secret: %v", err)
			}
			config.TURNSecret = base64.RawURLEncoding.EncodeToString(b)
		}
	}

	// Admin API token is auto-generated at runtime by the chatmail endpoint.
	// No need to generate it during install.

	return nil
}

func checkSystemRequirements(config *InstallConfig, dryRun bool) error {
	logger.Println("Checking system requirements")

	fmt.Printf("   Checking systemd availability...\n")
	// Check if systemd is available
	if _, err := os.Stat("/bin/systemctl"); err != nil {
		if _, err := os.Stat("/usr/bin/systemctl"); err != nil {
			return fmt.Errorf("systemd not found - this installer requires systemd")
		}
	}
	fmt.Printf("     ✓ systemd found\n")

	fmt.Printf("   Checking system utilities...\n")
	// Check if we're on a supported system
	if _, err := exec.LookPath("useradd"); err != nil {
		return fmt.Errorf("useradd command not found - unsupported system")
	}
	fmt.Printf("     ✓ useradd command available\n")

	fmt.Printf("   Checking network ports...\n")
	// Check available ports
	ports := []string{config.SMTPPort, config.SubmissionPort, config.SubmissionTLS, config.IMAPPort, config.IMAPTLS}
	if config.EnableChatmail {
		ports = append(ports, config.ChatmailHTTPPort, config.ChatmailHTTPSPort)
	}

	portWarnings := 0
	for _, port := range ports {
		if err := checkPortAvailable(port, dryRun); err != nil {
			logger.Printf("Port check warning: %v", err)
			fmt.Printf("     ⚠️  Warning: %v\n", err)
			portWarnings++
		} else if !dryRun {
			fmt.Printf("     ✓ Port %s appears available\n", port)
		} else {
			fmt.Printf("     • Port %s (would check)\n", port)
		}
	}

	if portWarnings > 0 {
		fmt.Printf("   ⚠️  %d port warnings (installation can continue)\n", portWarnings)
	} else {
		fmt.Printf("   ✓ All ports appear available\n")
	}

	return nil
}

func checkPortAvailable(port string, dryRun bool) error {
	if dryRun {
		return nil
	}

	// Simple check using netstat or ss
	cmd := exec.Command("ss", "-tln", fmt.Sprintf("sport = :%s", port))
	output, err := cmd.Output()
	if err != nil {
		// Try with netstat if ss is not available
		cmd = exec.Command("netstat", "-tln")
		output, err = cmd.Output()
		if err != nil {
			return nil // Skip check if neither command is available
		}
	}

	if strings.Contains(string(output), ":"+port) {
		return fmt.Errorf("port %s appears to be in use", port)
	}

	return nil
}

func createMaddyUser(config *InstallConfig, dryRun bool) error {
	logger.Printf("Creating maddy user: %s", config.MaddyUser)

	// Check if user already exists
	if _, err := user.Lookup(config.MaddyUser); err == nil {
		logger.Printf("User %s already exists", config.MaddyUser)
		fmt.Printf("   ℹ️  User %s already exists\n", config.MaddyUser)
		return nil
	}

	if dryRun {
		fmt.Printf("   Would create user: %s with group: %s\n", config.MaddyUser, config.MaddyGroup)
		fmt.Printf("   Home directory: %s\n", config.StateDir)
		fmt.Printf("   Shell: /sbin/nologin (no login access)\n")
		return nil
	}

	fmt.Printf("   Creating user: %s\n", config.MaddyUser)
	fmt.Printf("   Creating group: %s\n", config.MaddyGroup)
	fmt.Printf("   Home directory: %s\n", config.StateDir)
	fmt.Printf("   Shell: /sbin/nologin (no login access)\n")

	// Create user and group
	cmd := exec.Command("useradd",
		"-mrU",                // create home directory and user group
		"-s", "/sbin/nologin", // no shell access
		"-d", config.StateDir, // home directory
		"-c", "maddy mail server", // comment
		config.MaddyUser,
	)

	if err := cmd.Run(); err != nil {
		return fmt.Errorf("failed to create user %s: %v", config.MaddyUser, err)
	}

	// Get the created user info to show UID/GID
	createdUser, err := user.Lookup(config.MaddyUser)
	if err == nil {
		fmt.Printf("   ✓ User created: %s (UID: %s)\n", config.MaddyUser, createdUser.Uid)
		fmt.Printf("   ✓ Group created: %s (GID: %s)\n", config.MaddyGroup, createdUser.Gid)
	}

	logger.Printf("Successfully created user: %s with group: %s, home: %s, shell: /sbin/nologin",
		config.MaddyUser, config.MaddyGroup, config.StateDir)
	return nil
}

func createDirectories(config *InstallConfig, dryRun bool) error {
	dirs := []struct {
		path  string
		perm  os.FileMode
		owner string
	}{
		{config.StateDir, 0755, config.MaddyUser},
		{config.ConfigDir, 0755, "root"},
		{filepath.Join(config.ConfigDir, "certs"), 0755, "root"},
	}

	for _, dir := range dirs {
		logger.Printf("Creating directory: %s (owner: %s, permissions: %o)", dir.path, dir.owner, dir.perm)

		if dryRun {
			fmt.Printf("   Would create directory: %s\n", dir.path)
			fmt.Printf("     Owner: %s, Permissions: %o\n", dir.owner, dir.perm)
			continue
		}

		fmt.Printf("   Creating directory: %s\n", dir.path)
		fmt.Printf("     Owner: %s, Permissions: %o\n", dir.owner, dir.perm)

		if err := os.MkdirAll(dir.path, dir.perm); err != nil {
			return fmt.Errorf("failed to create directory %s: %v", dir.path, err)
		}

		// Set ownership
		if dir.owner != "root" {
			maddyUser, err := user.Lookup(dir.owner)
			if err != nil {
				return fmt.Errorf("failed to lookup user %s: %v", dir.owner, err)
			}

			uid, _ := strconv.Atoi(maddyUser.Uid)
			gid, _ := strconv.Atoi(maddyUser.Gid)

			if err := os.Chown(dir.path, uid, gid); err != nil {
				return fmt.Errorf("failed to set ownership for %s: %v", dir.path, err)
			}
			fmt.Printf("     ✓ Set ownership to %s:%s (UID:%d, GID:%d)\n", dir.owner, dir.owner, uid, gid)
			logger.Printf("Set ownership of %s to %s:%s (UID:%d, GID:%d)", dir.path, dir.owner, dir.owner, uid, gid)
		} else {
			fmt.Printf("     ✓ Owner: root (system default)\n")
		}
	}

	return nil
}

func setupCertificates(config *InstallConfig, dryRun bool) error {
	if !config.GenerateCerts {
		logger.Println("Skipping certificate generation (GenerateCerts=false)")
		return nil
	}

	logger.Printf("Generating self-signed certificates at %s and %s", config.TLSCertPath, config.TLSKeyPath)

	if dryRun {
		fmt.Printf("   Would generate self-signed certificates:\n")
		fmt.Printf("     Cert: %s\n", config.TLSCertPath)
		fmt.Printf("     Key: %s\n", config.TLSKeyPath)
		return nil
	}

	fmt.Printf("   Generating self-signed certificates...\n")
	fmt.Printf("     Primary domain: %s\n", config.PrimaryDomain)
	fmt.Printf("     Hostname: %s\n", config.Hostname)

	// Generate a new private key
	priv, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return fmt.Errorf("failed to generate private key: %v", err)
	}

	// Create certificate template
	notBefore := time.Now()
	notAfter := notBefore.Add(365 * 24 * time.Hour) // 1 year

	serialNumberLimit := new(big.Int).Lsh(big.NewInt(1), 128)
	serialNumber, err := rand.Int(rand.Reader, serialNumberLimit)
	if err != nil {
		return fmt.Errorf("failed to generate serial number: %v", err)
	}

	template := x509.Certificate{
		SerialNumber: serialNumber,
		Subject: pkix.Name{
			Organization: []string{"Maddy Mail Server"},
			CommonName:   config.PrimaryDomain,
		},
		NotBefore: notBefore,
		NotAfter:  notAfter,

		KeyUsage:              x509.KeyUsageKeyEncipherment | x509.KeyUsageDigitalSignature,
		ExtKeyUsage:           []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		BasicConstraintsValid: true,
	}

	// Add DNS names
	template.DNSNames = append(template.DNSNames, config.PrimaryDomain)
	if config.Hostname != config.PrimaryDomain {
		template.DNSNames = append(template.DNSNames, config.Hostname)
	}

	// Create self-signed certificate
	derBytes, err := x509.CreateCertificate(rand.Reader, &template, &template, &priv.PublicKey, priv)
	if err != nil {
		return fmt.Errorf("failed to create certificate: %v", err)
	}

	// Create target directory
	if err := os.MkdirAll(filepath.Dir(config.TLSCertPath), 0755); err != nil {
		return fmt.Errorf("failed to create certificate directory: %v", err)
	}
	if err := os.MkdirAll(filepath.Dir(config.TLSKeyPath), 0755); err != nil {
		return fmt.Errorf("failed to create key directory: %v", err)
	}

	// Write certificate
	certFile, err := os.Create(config.TLSCertPath)
	if err != nil {
		return fmt.Errorf("failed to create certificate file: %v", err)
	}
	defer certFile.Close()
	if err := pem.Encode(certFile, &pem.Block{Type: "CERTIFICATE", Bytes: derBytes}); err != nil {
		return fmt.Errorf("failed to encode certificate: %v", err)
	}

	// Write private key
	keyFile, err := os.OpenFile(config.TLSKeyPath, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0600)
	if err != nil {
		return fmt.Errorf("failed to create key file: %v", err)
	}
	defer keyFile.Close()
	privBytes, err := x509.MarshalPKCS8PrivateKey(priv)
	if err != nil {
		return fmt.Errorf("failed to marshal private key: %v", err)
	}
	if err := pem.Encode(keyFile, &pem.Block{Type: "PRIVATE KEY", Bytes: privBytes}); err != nil {
		return fmt.Errorf("failed to encode private key: %v", err)
	}

	// Set permissions (readable by maddy user)
	maddyUser, err := user.Lookup(config.MaddyUser)
	if err == nil {
		uid, _ := strconv.Atoi(maddyUser.Uid)
		gid, _ := strconv.Atoi(maddyUser.Gid)
		if err := os.Chown(config.TLSCertPath, uid, gid); err != nil {
			logger.Printf("Warning: failed to set ownership for %s: %v", config.TLSCertPath, err)
		}
		if err := os.Chown(config.TLSKeyPath, uid, gid); err != nil {
			logger.Printf("Warning: failed to set ownership for %s: %v", config.TLSKeyPath, err)
		}
	}

	fmt.Printf("     ✓ Certificates generated successfully\n")
	logger.Printf("Successfully generated certificates: cert=%s, key=%s", config.TLSCertPath, config.TLSKeyPath)

	return nil
}

func installSystemdFiles(config *InstallConfig, dryRun bool) error {
	logger.Println("Installing systemd service files")

	systemdFiles := map[string]string{
		"maddy.service":  systemdServiceTemplate,
		"maddy@.service": systemdInstanceTemplate,
	}

	for filename, content := range systemdFiles {
		destPath := filepath.Join(config.SystemdPath, filename)
		logger.Printf("Installing %s to %s (permissions: 644)", filename, destPath)

		if dryRun {
			fmt.Printf("   Would install systemd file: %s\n", destPath)
			fmt.Printf("     Source: embedded template\n")
			fmt.Printf("     Permissions: 644\n")
			continue
		}

		fmt.Printf("   Installing: %s\n", destPath)
		fmt.Printf("     Source: embedded template\n")
		fmt.Printf("     Permissions: 644\n")

		// Execute template
		tmpl, err := template.New(filename).Parse(content)
		if err != nil {
			return fmt.Errorf("failed to parse template %s: %v", filename, err)
		}

		file, err := os.Create(destPath)
		if err != nil {
			return fmt.Errorf("failed to create %s: %v", destPath, err)
		}
		defer file.Close()

		if err := tmpl.Execute(file, config); err != nil {
			return fmt.Errorf("failed to execute template %s: %v", filename, err)
		}

		// Set permissions
		if err := os.Chmod(destPath, 0644); err != nil {
			return fmt.Errorf("failed to set permissions for %s: %v", destPath, err)
		}

		fmt.Printf("     ✓ Created successfully\n")
		logger.Printf("Successfully created %s with permissions 644", destPath)
	}

	// Reload systemd
	if !dryRun {
		fmt.Printf("   Reloading systemd daemon...\n")
		cmd := exec.Command("systemctl", "daemon-reload")
		if err := cmd.Run(); err != nil {
			logger.Printf("Warning: failed to reload systemd: %v", err)
			fmt.Printf("   ⚠️  Warning: failed to reload systemd daemon\n")
		} else {
			fmt.Printf("   ✓ Systemd daemon reloaded\n")
			logger.Println("Successfully reloaded systemd daemon")
		}
	} else {
		fmt.Printf("   Would reload systemd daemon\n")
	}

	return nil
}

func generateConfigFile(config *InstallConfig, dryRun bool) error {
	logger.Println("Generating configuration file")

	configPath := filepath.Join(config.ConfigDir, "maddy.conf")
	logger.Printf("Generating config file: %s (permissions: 644)", configPath)

	if dryRun {
		fmt.Printf("   Would generate config file: %s\n", configPath)
		fmt.Printf("     Domain: %s\n", config.PrimaryDomain)
		fmt.Printf("     Hostname: %s\n", config.Hostname)
		fmt.Printf("     Public IP: %s\n", config.PublicIP)
		fmt.Printf("     State directory: %s\n", config.StateDir)
		if config.EnableChatmail {
			fmt.Printf("     Chatmail: enabled (HTTP:%s, HTTPS:%s)\n", config.ChatmailHTTPPort, config.ChatmailHTTPSPort)
		} else {
			fmt.Printf("     Chatmail: disabled\n")
		}
		fmt.Printf("     Permissions: 644\n")
		return nil
	}

	fmt.Printf("   Generating: %s\n", configPath)
	fmt.Printf("     Domain: %s\n", config.PrimaryDomain)
	fmt.Printf("     Hostname: %s\n", config.Hostname)
	fmt.Printf("     Public IP: %s\n", config.PublicIP)
	fmt.Printf("     State directory: %s\n", config.StateDir)
	if config.EnableChatmail {
		fmt.Printf("     Chatmail: enabled (HTTP:%s, HTTPS:%s)\n", config.ChatmailHTTPPort, config.ChatmailHTTPSPort)
	} else {
		fmt.Printf("     Chatmail: disabled\n")
	}
	fmt.Printf("     Permissions: 644\n")

	// Execute template
	tmpl, err := template.New("maddy.conf").Parse(maddyConfigTemplate)
	if err != nil {
		return fmt.Errorf("failed to parse config template: %v", err)
	}

	file, err := os.Create(configPath)
	if err != nil {
		return fmt.Errorf("failed to create config file: %v", err)
	}
	defer file.Close()

	if err := tmpl.Execute(file, config); err != nil {
		return fmt.Errorf("failed to execute config template: %v", err)
	}

	fmt.Printf("     ✓ Configuration file created successfully\n")

	// Set permissions
	if err := os.Chmod(configPath, 0644); err != nil {
		return fmt.Errorf("failed to set permissions for config file: %v", err)
	}

	fmt.Printf("     ✓ Configuration file created successfully\n")
	logger.Printf("Successfully created configuration file %s with permissions 644", configPath)
	logger.Printf("Configuration includes: hostname=%s, domain=%s, state_dir=%s",
		config.Hostname, config.PrimaryDomain, config.StateDir)
	if config.EnableChatmail {
		logger.Printf("Chatmail enabled on HTTP:%s, HTTPS:%s", config.ChatmailHTTPPort, config.ChatmailHTTPSPort)
	}

	return nil
}

func setupPermissions(config *InstallConfig, dryRun bool) error {
	logger.Println("Setting up permissions")

	if dryRun {
		fmt.Printf("   Would set up file permissions for: %s\n", config.StateDir)
		fmt.Printf("     Owner: %s:%s (recursive)\n", config.MaddyUser, config.MaddyGroup)
		return nil
	}

	fmt.Printf("   Setting up permissions for: %s\n", config.StateDir)
	fmt.Printf("     Owner: %s:%s (recursive)\n", config.MaddyUser, config.MaddyGroup)

	// Set state directory ownership
	maddyUser, err := user.Lookup(config.MaddyUser)
	if err != nil {
		return fmt.Errorf("failed to lookup maddy user: %v", err)
	}

	uid, _ := strconv.Atoi(maddyUser.Uid)
	gid, _ := strconv.Atoi(maddyUser.Gid)

	// Count files for progress indication
	fileCount := 0
	if err := filepath.WalkDir(config.StateDir, func(path string, d fs.DirEntry, err error) error {
		if err == nil {
			fileCount++
		}
		return nil
	}); err != nil {
		return fmt.Errorf("failed to count files in state directory: %v", err)
	}

	fmt.Printf("     Processing %d files and directories...\n", fileCount)

	// Set ownership for state directory and its contents
	processedCount := 0
	if err := filepath.WalkDir(config.StateDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		processedCount++
		if processedCount%10 == 0 || processedCount == fileCount {
			fmt.Printf("     ✓ Processed %d/%d items\n", processedCount, fileCount)
		}
		return os.Chown(path, uid, gid)
	}); err != nil {
		return fmt.Errorf("failed to set ownership for state directory: %v", err)
	}

	fmt.Printf("     ✓ Permissions set successfully\n")

	return nil
}

func installBinary(config *InstallConfig, dryRun bool) error {
	logger.Printf("Installing binary to %s (permissions: 755)", config.BinaryPath)

	if dryRun {
		fmt.Printf("   Would install binary to: %s\n", config.BinaryPath)
		fmt.Printf("     Permissions: 755 (executable)\n")
		return nil
	}

	// Get current executable path
	currentBinary, err := os.Executable()
	if err != nil {
		return fmt.Errorf("failed to get current binary path: %v", err)
	}

	fmt.Printf("   Installing binary: %s\n", config.BinaryPath)
	fmt.Printf("     Source: %s\n", currentBinary)
	fmt.Printf("     Permissions: 755 (executable)\n")

	// Copy binary to target location
	sourceFile, err := os.Open(currentBinary)
	if err != nil {
		return fmt.Errorf("failed to open source binary: %v", err)
	}
	defer sourceFile.Close()

	// Create destination directory if it doesn't exist
	destDir := filepath.Dir(config.BinaryPath)
	if err := os.MkdirAll(destDir, 0755); err != nil {
		return fmt.Errorf("failed to create binary directory: %v", err)
	}

	destFile, err := os.Create(config.BinaryPath)
	if err != nil {
		return fmt.Errorf("failed to create destination binary: %v", err)
	}
	defer destFile.Close()

	// Copy file contents
	if _, err := destFile.ReadFrom(sourceFile); err != nil {
		return fmt.Errorf("failed to copy binary: %v", err)
	}

	// Set executable permissions
	if err := os.Chmod(config.BinaryPath, 0755); err != nil {
		return fmt.Errorf("failed to set binary permissions: %v", err)
	}

	fmt.Printf("     ✓ Binary installed successfully\n")
	logger.Printf("Successfully installed binary from %s to %s with permissions 755", currentBinary, config.BinaryPath)

	return nil
}

func installIrohRelay(config *InstallConfig, dryRun bool) error {
	if !config.EnableIroh {
		return nil
	}

	irohRelayPath := filepath.Join("/usr/local/lib/maddy", "iroh-relay")
	if dryRun {
		silentPrint(config.NoLog, "Dry-run: Extracting iroh-relay binary to %s\n", irohRelayPath)
	} else {
		if err := iroh.ExtractBinary(irohRelayPath); err != nil {
			return fmt.Errorf("failed to extract iroh-relay: %v", err)
		}
	}

	// Create config file
	irohConfigPath := filepath.Join(config.ConfigDir, "iroh-relay.toml")
	irohConfig := fmt.Sprintf(`enable_relay = true
http_bind_addr = "[::]:%s"
enable_stun = false
enable_metrics = false
access = "everyone"
`, config.IrohPort)

	if dryRun {
		silentPrint(config.NoLog, "Dry-run: Writing iroh-relay config to %s\n", irohConfigPath)
	} else {
		if err := os.WriteFile(irohConfigPath, []byte(irohConfig), 0644); err != nil {
			return fmt.Errorf("failed to write iroh-relay config: %v", err)
		}
	}

	// Create systemd service
	servicePath := filepath.Join(config.SystemdPath, "iroh-relay.service")
	debugEnv := ""
	if config.Debug {
		debugEnv = "Environment=RUST_LOG=debug\n"
	}
	serviceContent := fmt.Sprintf(`[Unit]
Description=Iroh Relay
After=network.target

[Service]
%sExecStart=%s --config-path %s
Restart=on-failure
User=%s
Group=%s

[Install]
WantedBy=multi-user.target
`, debugEnv, irohRelayPath, irohConfigPath, config.MaddyUser, config.MaddyGroup)

	if dryRun {
		silentPrint(config.NoLog, "Dry-run: Writing systemd service for iroh-relay to %s\n", servicePath)
	} else {
		if err := os.WriteFile(servicePath, []byte(serviceContent), 0644); err != nil {
			return fmt.Errorf("failed to write iroh-relay service file: %v", err)
		}

		if !config.SkipSystemd {
			_ = exec.Command("systemctl", "daemon-reload").Run()
			_ = exec.Command("systemctl", "enable", "iroh-relay").Run()
			_ = exec.Command("systemctl", "restart", "iroh-relay").Run()
		}
	}

	return nil
}

func prepareDKIMKeys(config *InstallConfig, dryRun bool) error {
	// Generate STS ID if not set
	if config.STS_ID == "" {
		config.STS_ID = time.Now().Format("20060102150405")
	}

	// Check for existing DKIM keys generated by maddy
	dkimKeyPath := filepath.Join(config.StateDir, "dkim_keys", fmt.Sprintf("%s_default.key", config.PrimaryDomain))
	dkimDNSPath := filepath.Join(config.StateDir, "dkim_keys", fmt.Sprintf("%s_default.dns", config.PrimaryDomain))

	if dryRun {
		fmt.Printf("   Would check for DKIM key at: %s\n", dkimKeyPath)
		fmt.Printf("   Would check for DKIM DNS record at: %s\n", dkimDNSPath)
		config.DKIM_Entry = fmt.Sprintf("default._domainkey.%s.    300   TXT \"v=DKIM1; k=rsa; p=[will-be-generated-by-maddy]\"", config.PrimaryDomain)
		return nil
	}

	// Check if DKIM key and DNS record exist (generated by maddy)
	if _, err := os.Stat(dkimDNSPath); err == nil {
		// Read the DNS record directly from maddy's generated file
		dnsContent, err := os.ReadFile(dkimDNSPath)
		if err != nil {
			return fmt.Errorf("failed to read DKIM DNS file: %v", err)
		}
		config.DKIM_Entry = fmt.Sprintf("default._domainkey.%s.    300   TXT \"%s\"", config.PrimaryDomain, strings.TrimSpace(string(dnsContent)))
		fmt.Printf("   Using existing DKIM keys from: %s\n", dkimKeyPath)
	} else if _, err := os.Stat(dkimKeyPath); err == nil {
		// Key exists but no DNS file - generate DNS record from private key
		fmt.Printf("   DKIM key exists, generating DNS record...\n")
		if err := generateDKIMDNSRecord(dkimKeyPath, dkimDNSPath); err != nil {
			return fmt.Errorf("failed to generate DKIM DNS record: %v", err)
		}
		dnsContent, err := os.ReadFile(dkimDNSPath)
		if err != nil {
			return fmt.Errorf("failed to read generated DKIM DNS file: %v", err)
		}
		config.DKIM_Entry = fmt.Sprintf("default._domainkey.%s.    300   TXT \"%s\"", config.PrimaryDomain, strings.TrimSpace(string(dnsContent)))
	} else {
		// No DKIM key exists - generate new key and DNS record
		fmt.Printf("   Generating new RSA 2048 DKIM key pair...\n")
		if err := generateDKIMKeyPair(dkimKeyPath, dkimDNSPath, config.MaddyUser); err != nil {
			return fmt.Errorf("failed to generate DKIM key pair: %v", err)
		}
		dnsContent, err := os.ReadFile(dkimDNSPath)
		if err != nil {
			return fmt.Errorf("failed to read generated DKIM DNS file: %v", err)
		}
		config.DKIM_Entry = fmt.Sprintf("default._domainkey.%s.    300   TXT \"%s\"", config.PrimaryDomain, strings.TrimSpace(string(dnsContent)))
	}

	return nil
}

func generateDKIMKeyPair(keyPath, dnsPath, maddyUser string) error {
	fmt.Printf("     Generating new RSA 2048 DKIM key pair...\n")

	// Create directory if needed (matching maddy's internal logic)
	if err := os.MkdirAll(filepath.Dir(keyPath), 0o777); err != nil {
		return fmt.Errorf("failed to create DKIM keys directory: %v", err)
	}

	// Generate RSA 2048 private key
	pkey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return fmt.Errorf("failed to generate RSA key: %v", err)
	}

	// Marshal private key to PKCS#8 format
	keyBlob, err := x509.MarshalPKCS8PrivateKey(pkey)
	if err != nil {
		return fmt.Errorf("failed to marshal private key: %v", err)
	}

	// Create private key file with proper permissions
	f, err := os.OpenFile(keyPath, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return fmt.Errorf("failed to create private key file: %v", err)
	}
	defer f.Close()

	// Write PEM-encoded private key
	if err := pem.Encode(f, &pem.Block{
		Type:  "PRIVATE KEY",
		Bytes: keyBlob,
	}); err != nil {
		return fmt.Errorf("failed to write private key: %v", err)
	}

	// Generate DNS record
	if err := generateDKIMDNSRecord(keyPath, dnsPath); err != nil {
		return fmt.Errorf("failed to generate DNS record: %v", err)
	}

	// Set proper ownership for both files
	maddyUserInfo, err := user.Lookup(maddyUser)
	if err != nil {
		return fmt.Errorf("failed to lookup maddy user: %v", err)
	}

	uid, _ := strconv.Atoi(maddyUserInfo.Uid)
	gid, _ := strconv.Atoi(maddyUserInfo.Gid)

	if err := os.Chown(keyPath, uid, gid); err != nil {
		return fmt.Errorf("failed to set ownership for DKIM key: %v", err)
	}

	if err := os.Chown(dnsPath, uid, gid); err != nil {
		return fmt.Errorf("failed to set ownership for DKIM DNS file: %v", err)
	}

	fmt.Printf("     ✓ DKIM key generated: %s\n", keyPath)
	fmt.Printf("     ✓ DKIM DNS record generated: %s\n", dnsPath)
	return nil
}

func generateDKIMDNSRecord(keyPath, dnsPath string) error {
	// Load private key
	keyData, err := os.ReadFile(keyPath)
	if err != nil {
		return fmt.Errorf("failed to read private key: %v", err)
	}

	block, _ := pem.Decode(keyData)
	if block == nil {
		return fmt.Errorf("invalid PEM block in key file")
	}

	var pkey crypto.Signer
	switch block.Type {
	case "PRIVATE KEY": // PKCS#8
		key, err := x509.ParsePKCS8PrivateKey(block.Bytes)
		if err != nil {
			return fmt.Errorf("failed to parse PKCS8 private key: %v", err)
		}
		var ok bool
		pkey, ok = key.(crypto.Signer)
		if !ok {
			return fmt.Errorf("key is not a signer")
		}
	case "RSA PRIVATE KEY": // PKCS#1
		key, err := x509.ParsePKCS1PrivateKey(block.Bytes)
		if err != nil {
			return fmt.Errorf("failed to parse PKCS1 private key: %v", err)
		}
		pkey = key
	default:
		return fmt.Errorf("unsupported key type: %s", block.Type)
	}

	// Extract public key and determine algorithm
	var keyBlob []byte
	var dkimAlgoName string
	pubkey := pkey.Public()

	switch pubkey := pubkey.(type) {
	case *rsa.PublicKey:
		dkimAlgoName = "rsa"
		keyBlob, err = x509.MarshalPKIXPublicKey(pubkey)
		if err != nil {
			return fmt.Errorf("failed to marshal RSA public key: %v", err)
		}
	case ed25519.PublicKey:
		dkimAlgoName = "ed25519"
		keyBlob = pubkey
	default:
		return fmt.Errorf("unsupported public key type: %T", pubkey)
	}

	// Create DNS record content (matching maddy's internal format)
	keyRecord := fmt.Sprintf("v=DKIM1; k=%s; p=%s", dkimAlgoName, base64.StdEncoding.EncodeToString(keyBlob))

	// Write DNS record file
	dnsFile, err := os.Create(dnsPath)
	if err != nil {
		return fmt.Errorf("failed to create DNS record file: %v", err)
	}
	defer dnsFile.Close()

	if _, err := io.WriteString(dnsFile, keyRecord); err != nil {
		return fmt.Errorf("failed to write DNS record: %v", err)
	}

	return nil
}

func printNextSteps(config *InstallConfig) {
	if config.NoLog {
		return
	}
	// Print installation summary first
	printInstallationSummary(config)

	fmt.Println("\n📋 Next Steps")
	fmt.Println("=============")

	if config.SimpleInstall {
		fmt.Printf("1. Configure A record for %s:\n", config.PrimaryDomain)
		fmt.Printf("   - A record: %s → [your-server-ip]\n", config.PrimaryDomain)
		fmt.Printf("   (All other DNS records are optional for now but recommended later for better deliverability)\n")
	} else {
		fmt.Printf("1. Configure DNS records for %s:\n", config.PrimaryDomain)

		zoneFilePath := filepath.Join(config.ConfigDir, fmt.Sprintf("%s.zone", config.PrimaryDomain))
		if _, err := os.Stat(zoneFilePath); err == nil {
			fmt.Printf("   📄 DNS zone file available at: %s\n", zoneFilePath)
			fmt.Printf("   Import this file into your DNS provider (e.g., Cloudflare)\n")
			fmt.Printf("   ⚠️  For Cloudflare: Disable proxy (set to 'DNS only') for mail records!\n")
			fmt.Printf("   Or manually add the following records:\n")
		} else {
			fmt.Printf("   Add the following DNS records at your DNS provider:\n")
			fmt.Printf("   ⚠️  For Cloudflare: Disable proxy (set to 'DNS only') for mail records!\n")
		}

		fmt.Printf("   - A record: %s → [your-server-ip]\n", config.PrimaryDomain)
		fmt.Printf("   - MX record: %s → %s\n", config.PrimaryDomain, config.Hostname)
		fmt.Printf("   - TXT record (SPF): %s → \"v=spf1 mx ~all\"\n", config.PrimaryDomain)
		fmt.Printf("   - TXT record (DMARC): _dmarc.%s → \"v=DMARC1; p=reject; adkim=s; aspf=s\"\n", config.PrimaryDomain)
		if config.STS_ID != "" {
			fmt.Printf("   - TXT record (MTA-STS): _mta-sts.%s → \"v=STSv1; id=%s\"\n", config.PrimaryDomain, config.STS_ID)
			fmt.Printf("   - CNAME record: mta-sts.%s → %s\n", config.PrimaryDomain, config.PrimaryDomain)
		}
		if config.DKIM_Entry != "" {
			fmt.Printf("   - DKIM record: %s\n", strings.SplitN(config.DKIM_Entry, "TXT", 2)[0]+"→ [DKIM public key]")
		}
	}

	fmt.Printf("\n2. Set up TLS certificates:\n")
	if config.GenerateCerts {
		fmt.Printf("   ✓ Self-signed certificates have been generated:\n")
		fmt.Printf("     - Certificate: %s\n", config.TLSCertPath)
		fmt.Printf("     - Private key: %s\n", config.TLSKeyPath)
		fmt.Printf("   ⚠️  Note: Browsers will show a warning because these are self-signed.\n")
		fmt.Printf("       For production, consider using Let's Encrypt (Certbot).\n")
	} else {
		fmt.Printf("   - Place certificate at: %s\n", config.TLSCertPath)
		fmt.Printf("   - Place private key at: %s\n", config.TLSKeyPath)
	}
	fmt.Printf("   - Make certificates readable by maddy user:\n")
	fmt.Printf("     sudo chown root:%s %s %s\n", config.MaddyGroup, config.TLSCertPath, config.TLSKeyPath)
	fmt.Printf("     sudo chmod 640 %s %s\n", config.TLSCertPath, config.TLSKeyPath)

	fmt.Printf("\n3. Create first user account:\n")
	fmt.Printf("   sudo %s --config %s/maddy.conf creds create postmaster@%s\n",
		config.BinaryPath, config.ConfigDir, config.PrimaryDomain)
	fmt.Printf("   sudo %s --config %s/maddy.conf imap-acct create postmaster@%s\n",
		config.BinaryPath, config.ConfigDir, config.PrimaryDomain)

	fmt.Printf("\n4. Test configuration (optional):\n")
	prefix := "sudo "
	if os.Geteuid() != 0 {
		prefix = ""
	}
	fmt.Printf("   %s%s --config %s/maddy.conf run --libexec %s\n",
		prefix, config.BinaryPath, config.ConfigDir, config.LibexecDir)
	fmt.Printf("   (Press Ctrl+C to stop test run)\n")

	if !config.SkipSystemd {
		fmt.Printf("\n5. Start maddy service:\n")
		fmt.Printf("   sudo systemctl enable maddy\n")
		fmt.Printf("   sudo systemctl start maddy\n")

		fmt.Printf("\n6. Check service status:\n")
		fmt.Printf("   sudo systemctl status maddy\n")
		fmt.Printf("   sudo journalctl -u maddy -f\n")
	}

	if config.EnableChatmail {
		fmt.Printf("\n7. Chatmail is enabled:\n")
		fmt.Printf("   - HTTP endpoint: http://%s:%s\n", config.Hostname, config.ChatmailHTTPPort)
		fmt.Printf("   - HTTPS endpoint: https://%s:%s (if configured)\n", config.Hostname, config.ChatmailHTTPSPort)
		fmt.Printf("\n🔑 Admin API:\n")
		fmt.Printf("   The admin API is enabled by default with an auto-generated token.\n")
		fmt.Printf("   Retrieve the token after first startup:\n")
		fmt.Printf("     maddy admin-token\n")
		fmt.Printf("\n   To disable: add 'admin_token disabled' to your chatmail block\n")
		fmt.Printf("   To set custom: add 'admin_token your-custom-token' to your chatmail block\n")
	}

	fmt.Printf("\n📖 Documentation: https://maddy.email\n")
	fmt.Printf("📄 Configuration file: %s/maddy.conf\n", config.ConfigDir)

}

func printInstallationSummary(config *InstallConfig) {
	if config.NoLog {
		return
	}
	fmt.Println("\n📊 Installation Summary")
	fmt.Println("=======================")

	// Users and Groups Created
	fmt.Printf("👤 Users and Groups Created:\n")
	fmt.Printf("   - User: %s (UID: auto-assigned)\n", config.MaddyUser)
	fmt.Printf("   - Group: %s (GID: auto-assigned)\n", config.MaddyGroup)
	fmt.Printf("   - Home Directory: %s\n", config.StateDir)
	fmt.Printf("   - Shell: /sbin/nologin (no login access)\n")

	// Directories Created
	fmt.Printf("\n📁 Directories Created:\n")
	fmt.Printf("   - %s (owner: %s, permissions: 755)\n", config.StateDir, config.MaddyUser)
	fmt.Printf("   - %s (owner: root, permissions: 755)\n", config.ConfigDir)
	fmt.Printf("   - %s/certs (owner: root, permissions: 755)\n", config.ConfigDir)

	// Files Created
	fmt.Printf("\n📄 Files Created:\n")
	fmt.Printf("   - %s/maddy.conf (owner: root, permissions: 644)\n", config.ConfigDir)
	if config.GenerateCerts {
		fmt.Printf("   - %s (owner: %s, permissions: 644)\n", config.TLSCertPath, config.MaddyUser)
		fmt.Printf("   - %s (owner: %s, permissions: 600)\n", config.TLSKeyPath, config.MaddyUser)
	}
	fmt.Printf("   - %s/maddy.service (owner: root, permissions: 644)\n", config.SystemdPath)
	fmt.Printf("   - %s/maddy@.service (owner: root, permissions: 644)\n", config.SystemdPath)
	fmt.Printf("   - %s (owner: root, permissions: 755)\n", config.BinaryPath)

	// Permissions Applied
	fmt.Printf("\n🔐 Permissions Applied:\n")
	fmt.Printf("   - %s and all contents: owner %s:%s\n", config.StateDir, config.MaddyUser, config.MaddyGroup)
	fmt.Printf("   - Configuration files: readable by all, writable by root\n")
	fmt.Printf("   - Binary: executable by all\n")
	fmt.Printf("   - Log file: writable by root\n")
	fmt.Printf("   - Systemd services: standard systemd permissions\n")

	// Network Configuration
	fmt.Printf("\n🌐 Network Ports Configured:\n")
	fmt.Printf("   - SMTP: %s (incoming mail)\n", config.SMTPPort)
	fmt.Printf("   - Submission: %s (outgoing mail)\n", config.SubmissionPort)
	fmt.Printf("   - Submission TLS: %s (secure outgoing mail)\n", config.SubmissionTLS)
	fmt.Printf("   - IMAP: %s (mail access)\n", config.IMAPPort)
	fmt.Printf("   - IMAP TLS: %s (secure mail access)\n", config.IMAPTLS)

	if config.EnableChatmail {
		fmt.Printf("   - Chatmail HTTP: %s (user registration)\n", config.ChatmailHTTPPort)
		fmt.Printf("   - Chatmail HTTPS: %s (secure user registration)\n", config.ChatmailHTTPSPort)
	}

	// System Integration
	fmt.Printf("\n⚙️  System Integration:\n")
	fmt.Printf("   - Systemd daemon reloaded\n")
	fmt.Printf("   - Service available as: systemctl {start|stop|status} maddy\n")
	fmt.Printf("   - Instance service available as: systemctl {start|stop|status} maddy@<config>\n")
	fmt.Printf("   - Binary available system-wide at: %s\n", config.BinaryPath)
	fmt.Printf("   - Service command: %s --config %s/maddy.conf run --libexec %s\n",
		config.BinaryPath, config.ConfigDir, config.LibexecDir)

	// Database and Storage
	fmt.Printf("\n💾 Database and Storage:\n")
	fmt.Printf("   - SQLite databases will be created in: %s\n", config.StateDir)
	fmt.Printf("   - credentials.db (user authentication)\n")
	fmt.Printf("   - imapsql.db (IMAP mail storage)\n")
	fmt.Printf("   - Message storage: %s/messages/\n", config.StateDir)
	fmt.Printf("   - DKIM keys: %s/dkim_keys/\n", config.StateDir)
	fmt.Printf("   - MTA-STS cache: %s/mtasts_cache/\n", config.StateDir)

	// Security Features
	fmt.Printf("\n🔒 Security Features Enabled:\n")
	fmt.Printf("   - Systemd sandboxing (PrivateTmp, ProtectSystem, etc.)\n")
	fmt.Printf("   - Non-root execution (runs as %s user)\n", config.MaddyUser)
	fmt.Printf("   - Capability dropping (only CAP_NET_BIND_SERVICE)\n")
	fmt.Printf("   - File permissions restricted (umask 0007)\n")
	fmt.Printf("   - Resource limits applied (FD: 131072, Processes: 512)\n")

	if config.EnableChatmail {
		fmt.Printf("\n💬 Chatmail Features:\n")
		fmt.Printf("   - Automatic user registration enabled\n")
		fmt.Printf("   - Username length: %d characters\n", config.ChatmailUsernameLen)
		fmt.Printf("   - Password length: %d characters\n", config.ChatmailPasswordLen)
		fmt.Printf("   - Only auto-generated usernames allowed\n")
	}
}

// Embedded templates
const systemdServiceTemplate = `[Unit]
Description=maddy mail server
Documentation=man:maddy(1)
Documentation=man:maddy.conf(5)
Documentation=https://maddy.email
After=network-online.target

[Service]
Type=notify
NotifyAccess=main

User={{.MaddyUser}}
Group={{.MaddyGroup}}

# cd to state directory to make sure any relative paths
# in config will be relative to it unless handled specially.
WorkingDirectory={{.StateDir}}

ConfigurationDirectory=maddy
RuntimeDirectory=maddy
StateDirectory=maddy
LogsDirectory=maddy
ReadOnlyPaths=/usr/lib/maddy
ReadWritePaths={{.StateDir}}

# Strict sandboxing. You have no reason to trust code written by strangers from GitHub.
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ProtectKernelTunables=true
ProtectHostname=true
ProtectClock=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6

# Additional sandboxing. You need to disable all of these options
# for privileged helper binaries (for system auth) to work correctly.
NoNewPrivileges=true
PrivateDevices=true
DeviceAllow=/dev/syslog
RestrictSUIDSGID=true
ProtectKernelModules=true
MemoryDenyWriteExecute=true
RestrictNamespaces=true
RestrictRealtime=true
LockPersonality=true

# Graceful shutdown with a reasonable timeout.
TimeoutStopSec=7s
KillMode=mixed
KillSignal=SIGTERM

# Required to bind on ports lower than 1024.
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

# Force all files created by maddy to be only readable by it
# and maddy group.
UMask=0007

# Bump FD limitations. Even idle mail server can have a lot of FDs open (think
# of idle IMAP connections, especially ones abandoned on the other end and
# slowly timing out).
LimitNOFILE=131072

# Limit processes count to something reasonable to
# prevent resources exhausting due to big amounts of helper
# processes launched.
LimitNPROC=512

# Restart server on any problem.
Restart=on-failure
# ... Unless it is a configuration problem.
RestartPreventExitStatus=2

{{if .SkipSync}}
Environment=MADDY_SQLITE_UNSAFE_SYNC_OFF=1
{{end}}

ExecStart={{.BinaryPath}} --config {{.ConfigDir}}/maddy.conf {{if .Debug}}--debug {{end}}run --libexec {{.LibexecDir}}

ExecReload=/bin/kill -USR1 $MAINPID
ExecReload=/bin/kill -USR2 $MAINPID

[Install]
WantedBy=multi-user.target
`

const systemdInstanceTemplate = `[Unit]
Description=maddy mail server (using %i.conf)
Documentation=man:maddy(1)
Documentation=man:maddy.conf(5)
Documentation=https://maddy.email
After=network-online.target

[Service]
Type=notify
NotifyAccess=main

User={{.MaddyUser}}
Group={{.MaddyGroup}}

ConfigurationDirectory=maddy
RuntimeDirectory=maddy
StateDirectory=maddy
LogsDirectory=maddy
ReadOnlyPaths=/usr/lib/maddy
ReadWritePaths={{.StateDir}}

# Strict sandboxing. You have no reason to trust code written by strangers from GitHub.
PrivateTmp=true
PrivateHome=true
ProtectSystem=strict
ProtectKernelTunables=true
ProtectHostname=true
ProtectClock=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
DeviceAllow=/dev/syslog

# Additional sandboxing. You need to disable all of these options
# for privileged helper binaries (for system auth) to work correctly.
NoNewPrivileges=true
PrivateDevices=true
RestrictSUIDSGID=true
ProtectKernelModules=true
MemoryDenyWriteExecute=true
RestrictNamespaces=true
RestrictRealtime=true
LockPersonality=true

# Graceful shutdown with a reasonable timeout.
TimeoutStopSec=7s
KillMode=mixed
KillSignal=SIGTERM

# Required to bind on ports lower than 1024.
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

# Force all files created by maddy to be only readable by it and
# maddy group.
UMask=0007

# Bump FD limitations. Even idle mail server can have a lot of FDs open (think
# of idle IMAP connections, especially ones abandoned on the other end and
# slowly timing out).
LimitNOFILE=131072

# Limit processes count to something reasonable to
# prevent resources exhausting due to big amounts of helper
# processes launched.
LimitNPROC=512

# Restart server on any problem.
Restart=on-failure
# ... Unless it is a configuration problem.
RestartPreventExitStatus=2

{{if .SkipSync}}
Environment=MADDY_SQLITE_UNSAFE_SYNC_OFF=1
{{end}}

ExecStart={{.BinaryPath}} --config {{.ConfigDir}}/%i.conf run --libexec {{.LibexecDir}}
ExecReload=/bin/kill -USR1 $MAINPID
ExecReload=/bin/kill -USR2 $MAINPID

[Install]
WantedBy=multi-user.target
`
