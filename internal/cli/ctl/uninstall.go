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
	"fmt"
	"log"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"strings"

	frameworkconfig "github.com/themadorg/madmail/framework/config"
	maddycli "github.com/themadorg/madmail/internal/cli"
	clitools2 "github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/urfave/cli/v2"
)

// UninstallConfig holds information about the current maddy installation
type UninstallConfig struct {
	// Detected configuration
	ServiceFiles  []string
	ConfigFiles   []string
	StateDir      string
	ConfigDir     string
	RuntimeDir    string // e.g. /run/<binary> (systemd RuntimeDirectory=…)
	LogDir        string // e.g. /var/log/<binary> (systemd LogsDirectory=…)
	CacheDir      string // e.g. /var/cache/<binary> if used
	BinaryPath    string
	MaddyUser     string
	MaddyGroup    string
	LogFiles      []string
	DatabaseFiles []string
	CertFiles     []string

	// Service status
	ServiceRunning bool
	ServiceEnabled bool
	IrohRunning    bool
	IrohEnabled    bool

	// Installation detection
	InstallationFound bool
	SystemdUnit       string
	IrohUnit          string
	ConfigPath        string
	IrohConfigPath    string
	IrohBinaryPath    string
}

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:  "uninstall",
			Usage: "Uninstall " + frameworkconfig.BinaryName() + " mail server",
			Description: strings.Join([]string{
				"Uninstall the server and remove the systemd service, config, state/DB,",
				"runtime & log & cache dirs, and binary unless --keep-* flags are set.",
				"Detection uses systemd units, standard paths (/etc, /var/lib, /run, /var/log), and the running executable.",
				"",
				"Examples:",
				"  " + frameworkconfig.BinaryName() + " uninstall",
				"  " + frameworkconfig.BinaryName() + " uninstall --force",
				"  " + frameworkconfig.BinaryName() + " uninstall --keep-data",
				"  " + frameworkconfig.BinaryName() + " uninstall --keep-user",
			}, "\n"),
			Action: uninstallCommand,
			Flags: []cli.Flag{
				&cli.BoolFlag{
					Name:  "force",
					Usage: "Skip confirmation prompts and remove all components",
				},
				&cli.BoolFlag{
					Name:  "keep-data",
					Usage: "Keep mail data, databases, and state directory",
				},
				&cli.BoolFlag{
					Name:  "keep-user",
					Usage: "Keep maddy user and group accounts",
				},
				&cli.BoolFlag{
					Name:  "keep-config",
					Usage: "Keep configuration files",
				},
				&cli.BoolFlag{
					Name:  "keep-binary",
					Usage: "Keep maddy binary",
				},
				&cli.BoolFlag{
					Name:  "dry-run",
					Usage: "Show what would be removed without actually removing anything",
				},
				&cli.StringFlag{
					Name:  "log-file",
					Usage: "Uninstallation log file",
					Value: "/var/log/" + frameworkconfig.BinaryName() + "-uninstall.log",
				},
			},
		})
}

func uninstallCommand(ctx *cli.Context) error {
	// Initialize logger
	if err := initUninstallLogger(ctx.String("log-file")); err != nil {
		return fmt.Errorf("failed to initialize logger: %v", err)
	}

	logger.Println("Starting maddy uninstallation process")
	fmt.Println("🗑️  " + frameworkconfig.BinaryName() + " — uninstall")
	fmt.Println("====================================")

	// Check if running as root (unless dry-run)
	if os.Geteuid() != 0 && !ctx.Bool("dry-run") {
		return fmt.Errorf("uninstallation must be run as root (use sudo)")
	}

	// Detect current installation
	config, err := detectInstallation()
	if err != nil {
		return fmt.Errorf("failed to detect installation: %v", err)
	}

	if !config.InstallationFound {
		fmt.Println("❌ No maddy installation detected")
		fmt.Println("Nothing to uninstall.")
		return nil
	}

	logger.Printf("Detected installation: %+v", config)

	// Show what will be removed
	showUninstallPlan(config, ctx)

	// Get user confirmation (unless --force is used)
	if !ctx.Bool("force") && !ctx.Bool("dry-run") {
		if !confirmUninstall(config) {
			fmt.Println("Uninstallation cancelled.")
			return nil
		}
	}

	// Perform uninstallation steps
	steps := []struct {
		name string
		fn   func(*UninstallConfig, *cli.Context) error
	}{
		{"Stopping maddy service", stopService},
		{"Disabling maddy service", disableService},
		{"Removing systemd service files", removeSystemdFiles},
	}

	// Conditional steps based on flags
	if !ctx.Bool("keep-config") {
		steps = append(steps, struct {
			name string
			fn   func(*UninstallConfig, *cli.Context) error
		}{"Removing configuration files", removeConfigFiles})
	}

	if !ctx.Bool("keep-data") {
		steps = append(steps, struct {
			name string
			fn   func(*UninstallConfig, *cli.Context) error
		}{"Removing state, databases, runtime, logs, and cache", removeDataAndVolatile})
	}

	if !ctx.Bool("keep-binary") {
		steps = append(steps, struct {
			name string
			fn   func(*UninstallConfig, *cli.Context) error
		}{"Removing binary", removeBinary})
	}

	if !ctx.Bool("keep-user") {
		steps = append(steps, struct {
			name string
			fn   func(*UninstallConfig, *cli.Context) error
		}{"Removing maddy user and group", removeUser})
	}

	for i, step := range steps {
		fmt.Printf("\n[%d/%d] %s...\n", i+1, len(steps), step.name)
		logger.Printf("Step %d: %s", i+1, step.name)

		if err := step.fn(config, ctx); err != nil {
			logger.Printf("Step %d failed: %v", i+1, err)
			return fmt.Errorf("step '%s' failed: %v", step.name, err)
		}

		fmt.Printf("✅ %s completed\n", step.name)
		logger.Printf("Step %d completed successfully", i+1)
	}

	// Final cleanup
	if err := reloadSystemd(ctx.Bool("dry-run")); err != nil {
		logger.Printf("Warning: failed to reload systemd: %v", err)
		fmt.Printf("⚠️  Warning: failed to reload systemd daemon\n")
	}

	// Show uninstallation summary
	showUninstallSummary(config, ctx)

	logger.Println("Uninstallation completed successfully")
	fmt.Println("\n🎉 Uninstallation completed successfully!")

	return nil
}

func initUninstallLogger(logFile string) error {
	// Create log directory if it doesn't exist
	logDir := filepath.Dir(logFile)
	if err := os.MkdirAll(logDir, 0755); err != nil {
		return err
	}

	file, err := os.OpenFile(logFile, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err != nil {
		return err
	}

	logger = log.New(file, "", log.LstdFlags)
	return nil
}

func detectInstallation() (*UninstallConfig, error) {
	config := &UninstallConfig{
		ServiceFiles:  []string{},
		ConfigFiles:   []string{},
		LogFiles:      []string{},
		DatabaseFiles: []string{},
		CertFiles:     []string{},
	}

	// Check for systemd services
	systemdPaths := []string{"/etc/systemd/system", "/usr/lib/systemd/system", "/lib/systemd/system"}
	binName := frameworkconfig.BinaryName()
	serviceNames := []string{binName + ".service", binName + "@.service", "iroh-relay.service"}

	for _, path := range systemdPaths {
		for _, service := range serviceNames {
			servicePath := filepath.Join(path, service)
			if _, err := os.Stat(servicePath); err == nil {
				config.ServiceFiles = append(config.ServiceFiles, servicePath)
				config.InstallationFound = true
				if service == binName+".service" {
					config.SystemdUnit = binName
				}
				if service == "iroh-relay.service" {
					config.IrohUnit = "iroh-relay"
				}
			}
		}
	}

	// Check service status if found
	if config.SystemdUnit != "" {
		config.ServiceRunning = isServiceRunning(config.SystemdUnit)
		config.ServiceEnabled = isServiceEnabled(config.SystemdUnit)
	}
	if config.IrohUnit != "" {
		config.IrohRunning = isServiceRunning(config.IrohUnit)
		config.IrohEnabled = isServiceEnabled(config.IrohUnit)
	}

	// Try to detect configuration from systemd service
	if len(config.ServiceFiles) > 0 {
		if err := parseServiceFile(config, config.ServiceFiles[0]); err != nil {
			logger.Printf("Warning: failed to parse service file: %v", err)
		}
	}

	// Check common locations for config files
	configPaths := []string{"/etc/" + binName, "/usr/local/etc/" + binName}
	for _, path := range configPaths {
		if _, err := os.Stat(path); err == nil {
			config.ConfigDir = path
			config.InstallationFound = true

			// Find config files
			if err := filepath.Walk(path, func(filePath string, info os.FileInfo, err error) error {
				if err != nil {
					return nil
				}
				if !info.IsDir() {
					config.ConfigFiles = append(config.ConfigFiles, filePath)
				}
				return nil
			}); err != nil {
				logger.Printf("Warning: failed to walk config directory: %v", err)
			}
			break
		}
	}

	// Try to parse main config file to get more information
	if config.ConfigDir != "" {
		mainConfig := filepath.Join(config.ConfigDir, binName+".conf")
		if _, err := os.Stat(mainConfig); err == nil {
			config.ConfigPath = mainConfig
			if err := parseConfigFile(config, mainConfig); err != nil {
				logger.Printf("Warning: failed to parse config file: %v", err)
			}
		}
	}

	// Check for common binary locations
	binaryPaths := []string{
		"/usr/local/bin/" + binName,
		"/usr/bin/" + binName,
		"/opt/" + binName + "/bin/" + binName,
	}
	for _, path := range binaryPaths {
		if _, err := os.Stat(path); err == nil {
			config.BinaryPath = path
			config.InstallationFound = true
			break
		}
	}

	// Check for iroh-relay binary
	irohPaths := []string{
		"/usr/local/lib/" + binName + "/iroh-relay",
		"/usr/lib/" + binName + "/iroh-relay",
	}
	for _, path := range irohPaths {
		if _, err := os.Stat(path); err == nil {
			config.IrohBinaryPath = path
			config.InstallationFound = true
			break
		}
	}

	// Check for service user
	if _, err := user.Lookup(binName); err == nil {
		config.MaddyUser = binName
		config.MaddyGroup = binName
		config.InstallationFound = true
	}

	// Check for state directory and databases
	statePaths := []string{"/var/lib/" + binName, "/usr/local/var/lib/" + binName}
	if config.StateDir != "" {
		statePaths = []string{config.StateDir}
	}

	for _, path := range statePaths {
		if _, err := os.Stat(path); err == nil {
			config.StateDir = path
			config.InstallationFound = true

			// Find database files
			dbFiles := []string{"credentials.db", "imapsql.db"}
			for _, dbFile := range dbFiles {
				dbPath := filepath.Join(path, dbFile)
				if _, err := os.Stat(dbPath); err == nil {
					config.DatabaseFiles = append(config.DatabaseFiles, dbPath)
				}
			}
			break
		}
	}

	// Check for log files
	logPaths := []string{"/var/log/" + binName, "/usr/local/var/log/" + binName}
	for _, path := range logPaths {
		if _, err := os.Stat(path); err == nil {
			if config.LogDir == "" {
				config.LogDir = path
			}
			if err := filepath.Walk(path, func(filePath string, info os.FileInfo, err error) error {
				if err != nil {
					return nil
				}
				if !info.IsDir() {
					config.LogFiles = append(config.LogFiles, filePath)
				}
				return nil
			}); err != nil {
				logger.Printf("Warning: failed to walk log directory: %v", err)
			}
		}
	}

	config.RuntimeDir = "/run/" + binName
	if config.LogDir == "" {
		config.LogDir = "/var/log/" + binName
	}
	config.CacheDir = "/var/cache/" + binName

	applyUninstallPathDefaults(config)

	return config, nil
}

// applyUninstallPathDefaults fills standard FHS locations and the service name
// so uninstall can run even when systemd unit files are already gone.
func applyUninstallPathDefaults(config *UninstallConfig) {
	bin := frameworkconfig.BinaryName()
	if config.RuntimeDir == "" {
		config.RuntimeDir = "/run/" + bin
	}
	if config.LogDir == "" {
		config.LogDir = "/var/log/" + bin
	}
	if config.CacheDir == "" {
		config.CacheDir = "/var/cache/" + bin
	}
	if config.ConfigDir == "" {
		p := "/etc/" + bin
		if st, err := os.Stat(p); err == nil && st.IsDir() {
			config.ConfigDir = p
		}
	}
	if config.ConfigPath == "" && config.ConfigDir != "" {
		cf := filepath.Join(config.ConfigDir, bin+".conf")
		if _, err := os.Stat(cf); err == nil {
			config.ConfigPath = cf
		}
	}
	if config.StateDir == "" {
		p := "/var/lib/" + bin
		if st, err := os.Stat(p); err == nil && st.IsDir() {
			config.StateDir = p
		}
	}
	if ex, err := os.Executable(); err == nil {
		base := filepath.Base(ex)
		if (base == bin || strings.HasPrefix(base, bin)) && config.BinaryPath == "" {
			if st, err := os.Stat(ex); err == nil && !st.IsDir() {
				config.BinaryPath = ex
			}
		}
	}
	if config.SystemdUnit == "" {
		config.SystemdUnit = bin
	}
	// We have something to remove if any standard path or the binary exists
	if !config.InstallationFound {
		if config.ConfigDir != "" || config.StateDir != "" || len(config.ServiceFiles) > 0 || config.BinaryPath != "" {
			config.InstallationFound = true
		}
	}
}

func parseServiceFile(config *UninstallConfig, servicePath string) error {
	file, err := os.Open(servicePath)
	if err != nil {
		return err
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())

		// Look for ExecStart line
		if strings.HasPrefix(line, "ExecStart=") {
			parts := strings.Fields(line)
			if len(parts) > 0 {
				// Extract binary path
				execStart := strings.TrimPrefix(parts[0], "ExecStart=")
				if config.BinaryPath == "" {
					config.BinaryPath = execStart
				}

				// Look for --config flag
				for i, part := range parts {
					if part == "--config" && i+1 < len(parts) {
						config.ConfigPath = parts[i+1]
						config.ConfigDir = filepath.Dir(parts[i+1])
					}
				}
			}
		}

		// Look for User line
		if strings.HasPrefix(line, "User=") {
			config.MaddyUser = strings.TrimPrefix(line, "User=")
		}

		// Look for Group line
		if strings.HasPrefix(line, "Group=") {
			config.MaddyGroup = strings.TrimPrefix(line, "Group=")
		}

		// Look for WorkingDirectory or StateDirectory
		if strings.HasPrefix(line, "WorkingDirectory=") {
			config.StateDir = strings.TrimPrefix(line, "WorkingDirectory=")
		}
	}

	return scanner.Err()
}

func parseConfigFile(config *UninstallConfig, configPath string) error {
	file, err := os.Open(configPath)
	if err != nil {
		return err
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())

		// Look for state_dir directive
		if strings.HasPrefix(line, "state_dir ") {
			config.StateDir = strings.TrimSpace(strings.TrimPrefix(line, "state_dir"))
		}

		// Look for tls file directive to find cert files
		if strings.HasPrefix(line, "tls file ") {
			parts := strings.Fields(strings.TrimPrefix(line, "tls file"))
			for _, part := range parts {
				if strings.Contains(part, ".pem") || strings.Contains(part, ".crt") || strings.Contains(part, ".key") {
					config.CertFiles = append(config.CertFiles, part)
				}
			}
		}
	}

	return scanner.Err()
}

func isServiceRunning(serviceName string) bool {
	cmd := exec.Command("systemctl", "is-active", serviceName)
	output, err := cmd.Output()
	if err != nil {
		return false
	}
	return strings.TrimSpace(string(output)) == "active"
}

func isServiceEnabled(serviceName string) bool {
	cmd := exec.Command("systemctl", "is-enabled", serviceName)
	output, err := cmd.Output()
	if err != nil {
		return false
	}
	status := strings.TrimSpace(string(output))
	return status == "enabled" || status == "enabled-runtime"
}

func showUninstallPlan(config *UninstallConfig, ctx *cli.Context) {
	fmt.Println("\n📋 Uninstallation Plan")
	fmt.Println("======================")

	if config.ServiceRunning {
		fmt.Printf("🔴 Service Status: Running (will be stopped)\n")
	} else {
		fmt.Printf("⚪ Service Status: Not running\n")
	}

	if config.ServiceEnabled || config.IrohEnabled {
		fmt.Printf("🔴 Service Enabled: maddy=%v, iroh-relay=%v\n", config.ServiceEnabled, config.IrohEnabled)
	} else {
		fmt.Printf("⚪ Service Enabled: No\n")
	}

	fmt.Printf("\n📄 System Files to Remove:\n")
	for _, file := range config.ServiceFiles {
		fmt.Printf("   - %s\n", file)
	}

	if !ctx.Bool("keep-config") {
		fmt.Printf("\n⚙️  Configuration Files to Remove:\n")
		if config.ConfigDir != "" {
			fmt.Printf("   - %s (entire directory)\n", config.ConfigDir)
		}
		for _, file := range config.ConfigFiles {
			fmt.Printf("   - %s\n", file)
		}

		if len(config.CertFiles) > 0 {
			fmt.Printf("\n🔒 Certificate Files Referenced:\n")
			for _, file := range config.CertFiles {
				fmt.Printf("   - %s (may need manual removal)\n", file)
			}
		}
	}

	if !ctx.Bool("keep-data") {
		fmt.Printf("\n💾 Data and Databases to Remove:\n")
		if config.StateDir != "" {
			fmt.Printf("   - %s (entire directory)\n", config.StateDir)
		}
		for _, file := range config.DatabaseFiles {
			fmt.Printf("   - %s\n", file)
		}
		if config.LogDir != "" {
			fmt.Printf("   - %s (logs)\n", config.LogDir)
		}
		if config.CacheDir != "" {
			fmt.Printf("   - %s (cache, if present)\n", config.CacheDir)
		}
	}
	if config.RuntimeDir != "" {
		fmt.Printf("\n📁 Runtime directory to clear: %s\n", config.RuntimeDir)
	}

	if !ctx.Bool("keep-binary") {
		if config.BinaryPath != "" {
			fmt.Printf("\n🔧 Binary to Remove:\n")
			fmt.Printf("   - %s\n", config.BinaryPath)
		}
		if config.IrohBinaryPath != "" {
			fmt.Printf("   - %s (iroh-relay)\n", config.IrohBinaryPath)
		}
	}

	if !ctx.Bool("keep-user") && config.MaddyUser != "" {
		fmt.Printf("\n👤 User Account to Remove:\n")
		fmt.Printf("   - User: %s\n", config.MaddyUser)
		if config.MaddyGroup != "" {
			fmt.Printf("   - Group: %s\n", config.MaddyGroup)
		}
	}

	if len(config.LogFiles) > 0 {
		fmt.Printf("\n📝 Log Files Found:\n")
		for _, file := range config.LogFiles {
			fmt.Printf("   - %s\n", file)
		}
	}
}

func confirmUninstall(config *UninstallConfig) bool {
	fmt.Printf("\n⚠️  WARNING: This will permanently remove maddy and all associated data!\n")
	fmt.Printf("This includes:\n")
	fmt.Printf("- All email messages and mailboxes\n")
	fmt.Printf("- User account credentials\n")
	fmt.Printf("- Configuration files\n")
	fmt.Printf("- DKIM keys\n")
	fmt.Printf("- System service configuration\n")

	if config.StateDir != "" {
		fmt.Printf("\nMail data location: %s\n", config.StateDir)
	}

	fmt.Printf("\nIf you want to keep mail data, use --keep-data flag.\n")
	fmt.Printf("If you want to keep user account, use --keep-user flag.\n")

	return clitools2.Confirmation("Are you sure you want to proceed with uninstallation", false)
}

func stopService(config *UninstallConfig, ctx *cli.Context) error {
	// Always try to stop the main unit (covers active, failed, reloading, etc.)
	if config.SystemdUnit != "" {
		if ctx.Bool("dry-run") {
			fmt.Printf("Would stop service: %s\n", config.SystemdUnit)
		} else {
			cmd := exec.Command("systemctl", "stop", config.SystemdUnit)
			if err := cmd.Run(); err != nil {
				// Not necessarily loaded or already stopped; continue uninstall
				fmt.Printf("ℹ️  systemctl stop %s: %v (continuing)\n", config.SystemdUnit, err)
			} else {
				fmt.Printf("✅ Stopped service: %s\n", config.SystemdUnit)
			}
		}
	} else {
		fmt.Printf("ℹ️  No main systemd unit name; skip systemctl stop\n")
	}

	if config.IrohRunning {
		logger.Printf("Stopping service: %s", config.IrohUnit)
		if ctx.Bool("dry-run") {
			fmt.Printf("Would stop service: %s\n", config.IrohUnit)
		} else {
			cmd := exec.Command("systemctl", "stop", config.IrohUnit)
			if err := cmd.Run(); err != nil {
				return fmt.Errorf("failed to stop service %s: %v", config.IrohUnit, err)
			}
			fmt.Printf("✅ Stopped service: %s\n", config.IrohUnit)
		}
	}
	logger.Printf("Successfully stopped service: %s", config.SystemdUnit)
	return nil
}

func disableService(config *UninstallConfig, ctx *cli.Context) error {
	if config.SystemdUnit != "" {
		logger.Printf("Disabling service: %s", config.SystemdUnit)
		if ctx.Bool("dry-run") {
			fmt.Printf("Would disable service: %s\n", config.SystemdUnit)
		} else {
			cmd := exec.Command("systemctl", "disable", config.SystemdUnit)
			if err := cmd.Run(); err != nil {
				fmt.Printf("ℹ️  systemctl disable %s: %v (continuing)\n", config.SystemdUnit, err)
			} else {
				fmt.Printf("✅ Disabled service: %s\n", config.SystemdUnit)
			}
		}
	}

	if config.IrohEnabled {
		logger.Printf("Disabling service: %s", config.IrohUnit)
		if ctx.Bool("dry-run") {
			fmt.Printf("Would disable service: %s\n", config.IrohUnit)
		} else {
			cmd := exec.Command("systemctl", "disable", config.IrohUnit)
			if err := cmd.Run(); err != nil {
				return fmt.Errorf("failed to disable service %s: %v", config.IrohUnit, err)
			}
			fmt.Printf("✅ Disabled service: %s\n", config.IrohUnit)
		}
	}
	logger.Printf("Successfully disabled service: %s", config.SystemdUnit)
	return nil
}

func removeSystemdFiles(config *UninstallConfig, ctx *cli.Context) error {
	if len(config.ServiceFiles) == 0 {
		fmt.Printf("ℹ️  No systemd service files found\n")
		return nil
	}

	for _, file := range config.ServiceFiles {
		logger.Printf("Removing systemd service file: %s", file)

		if ctx.Bool("dry-run") {
			fmt.Printf("Would remove: %s\n", file)
			continue
		}

		if err := os.Remove(file); err != nil {
			logger.Printf("Warning: failed to remove %s: %v", file, err)
			fmt.Printf("⚠️  Warning: failed to remove %s: %v\n", file, err)
		} else {
			fmt.Printf("✅ Removed: %s\n", file)
			logger.Printf("Successfully removed: %s", file)
		}
	}

	return nil
}

func removeConfigFiles(config *UninstallConfig, ctx *cli.Context) error {
	if config.ConfigDir == "" && len(config.ConfigFiles) == 0 {
		fmt.Printf("ℹ️  No configuration files found\n")
		return nil
	}

	if ctx.Bool("dry-run") {
		if config.ConfigDir != "" {
			fmt.Printf("Would remove config directory: %s\n", config.ConfigDir)
		}
		for _, file := range config.ConfigFiles {
			fmt.Printf("Would remove config file: %s\n", file)
		}
		return nil
	}

	// Remove entire config directory if it exists
	if config.ConfigDir != "" {
		logger.Printf("Removing configuration directory: %s", config.ConfigDir)

		if err := os.RemoveAll(config.ConfigDir); err != nil {
			return fmt.Errorf("failed to remove config directory %s: %v", config.ConfigDir, err)
		}

		fmt.Printf("✅ Removed config directory: %s\n", config.ConfigDir)
		logger.Printf("Successfully removed config directory: %s", config.ConfigDir)
	} else {
		// Remove individual config files
		for _, file := range config.ConfigFiles {
			logger.Printf("Removing config file: %s", file)

			if err := os.Remove(file); err != nil {
				logger.Printf("Warning: failed to remove %s: %v", file, err)
				fmt.Printf("⚠️  Warning: failed to remove %s: %v\n", file, err)
			} else {
				fmt.Printf("✅ Removed: %s\n", file)
				logger.Printf("Successfully removed: %s", file)
			}
		}
	}

	return nil
}

func removeDataAndVolatile(config *UninstallConfig, ctx *cli.Context) error {
	// 1) State (mail, SQLite DBs, DKIM, etc.) — optional with --keep-data
	if !ctx.Bool("keep-data") {
		if config.StateDir == "" {
			fmt.Printf("ℹ️  No state directory to remove\n")
		} else {
			logger.Printf("Removing state directory: %s", config.StateDir)
			if ctx.Bool("dry-run") {
				fmt.Printf("Would remove state directory: %s\n", config.StateDir)
			} else {
				if err := os.RemoveAll(config.StateDir); err != nil {
					return fmt.Errorf("failed to remove state directory %s: %v", config.StateDir, err)
				}
				fmt.Printf("✅ Removed state directory: %s\n", config.StateDir)
				logger.Printf("Successfully removed state directory: %s", config.StateDir)
			}
		}
	} else {
		fmt.Printf("ℹ️  Keeping state directory and databases (--keep-data)\n")
	}

	// 2) Logs and /var/cache — part of a full uninstall unless --keep-data
	if !ctx.Bool("keep-data") {
		for _, p := range []struct {
			path  string
			label string
		}{
			{config.LogDir, "log directory"},
			{config.CacheDir, "cache directory"},
		} {
			if p.path == "" {
				continue
			}
			if ctx.Bool("dry-run") {
				fmt.Printf("Would remove %s: %s\n", p.label, p.path)
				continue
			}
			if err := os.RemoveAll(p.path); err != nil {
				if !os.IsNotExist(err) {
					logger.Printf("Warning: failed to remove %s: %v", p.path, err)
					fmt.Printf("⚠️  Warning: failed to remove %s: %s — %v\n", p.label, p.path, err)
				}
			} else {
				fmt.Printf("✅ Removed %s: %s\n", p.label, p.path)
			}
		}
	}

	// 3) Runtime (e.g. /run/madmail) — best-effort after service stop; even with --keep-data
	if config.RuntimeDir != "" {
		if ctx.Bool("dry-run") {
			fmt.Printf("Would remove runtime directory: %s\n", config.RuntimeDir)
		} else if err := os.RemoveAll(config.RuntimeDir); err != nil {
			fmt.Printf("⚠️  Warning: could not remove runtime directory %s: %v\n", config.RuntimeDir, err)
		} else {
			fmt.Printf("✅ Removed runtime directory: %s\n", config.RuntimeDir)
		}
	}

	return nil
}

func removeBinary(config *UninstallConfig, ctx *cli.Context) error {
	if config.BinaryPath != "" {
		logger.Printf("Removing binary: %s", config.BinaryPath)
		if ctx.Bool("dry-run") {
			fmt.Printf("Would remove binary: %s\n", config.BinaryPath)
		} else {
			if err := os.Remove(config.BinaryPath); err != nil {
				logger.Printf("Warning: failed to remove binary %s: %v", config.BinaryPath, err)
				fmt.Printf("⚠️  Warning: failed to remove %s: %v\n", config.BinaryPath, err)
			} else {
				fmt.Printf("✅ Removed binary: %s\n", config.BinaryPath)
			}
		}
	}

	if config.IrohBinaryPath != "" {
		logger.Printf("Removing binary: %s", config.IrohBinaryPath)
		if ctx.Bool("dry-run") {
			fmt.Printf("Would remove binary: %s\n", config.IrohBinaryPath)
		} else {
			if err := os.Remove(config.IrohBinaryPath); err != nil {
				logger.Printf("Warning: failed to remove binary %s: %v", config.IrohBinaryPath, err)
				fmt.Printf("⚠️  Warning: failed to remove %s: %v\n", config.IrohBinaryPath, err)
			} else {
				fmt.Printf("✅ Removed binary: %s\n", config.IrohBinaryPath)
			}
		}
	}

	return nil
}

func removeUser(config *UninstallConfig, ctx *cli.Context) error {
	if config.MaddyUser == "" {
		fmt.Printf("ℹ️  No maddy user found\n")
		return nil
	}

	// Check if user exists
	if _, err := user.Lookup(config.MaddyUser); err != nil {
		fmt.Printf("ℹ️  User %s does not exist\n", config.MaddyUser)
		return nil
	}

	logger.Printf("Removing user: %s", config.MaddyUser)

	if ctx.Bool("dry-run") {
		fmt.Printf("Would remove user: %s\n", config.MaddyUser)
		return nil
	}

	// Remove user and group
	cmd := exec.Command("userdel", "-r", config.MaddyUser)
	if err := cmd.Run(); err != nil {
		// Try without -r flag if it fails (home directory might not exist)
		cmd = exec.Command("userdel", config.MaddyUser)
		if err2 := cmd.Run(); err2 != nil {
			logger.Printf("Warning: failed to remove user %s: %v", config.MaddyUser, err)
			fmt.Printf("⚠️  Warning: failed to remove user %s: %v\n", config.MaddyUser, err)
		} else {
			fmt.Printf("✅ Removed user: %s\n", config.MaddyUser)
			logger.Printf("Successfully removed user: %s", config.MaddyUser)
		}
	} else {
		fmt.Printf("✅ Removed user and home directory: %s\n", config.MaddyUser)
		logger.Printf("Successfully removed user and home directory: %s", config.MaddyUser)
	}

	return nil
}

func reloadSystemd(dryRun bool) error {
	if dryRun {
		fmt.Printf("Would reload systemd daemon\n")
		return nil
	}

	cmd := exec.Command("systemctl", "daemon-reload")
	return cmd.Run()
}

func showUninstallSummary(config *UninstallConfig, ctx *cli.Context) {
	fmt.Println("\n📊 Uninstallation Summary")
	fmt.Println("=========================")

	fmt.Printf("🛑 Service Management:\n")
	if config.ServiceRunning {
		fmt.Printf("   - Stopped service: %s\n", config.SystemdUnit)
	}
	if config.IrohRunning {
		fmt.Printf("   - Stopped service: %s\n", config.IrohUnit)
	}
	if config.ServiceEnabled {
		fmt.Printf("   - Disabled service: %s\n", config.SystemdUnit)
	}
	if config.IrohEnabled {
		fmt.Printf("   - Disabled service: %s\n", config.IrohUnit)
	}
	for _, file := range config.ServiceFiles {
		fmt.Printf("   - Removed systemd file: %s\n", file)
	}

	if !ctx.Bool("keep-config") {
		fmt.Printf("\n📄 Configuration Removed:\n")
		if config.ConfigDir != "" {
			fmt.Printf("   - Configuration directory: %s\n", config.ConfigDir)
		}
	}

	if !ctx.Bool("keep-data") {
		fmt.Printf("\n💾 Data Removed:\n")
		if config.StateDir != "" {
			fmt.Printf("   - State directory: %s\n", config.StateDir)
		}
		for _, file := range config.DatabaseFiles {
			fmt.Printf("   - Database: %s\n", file)
		}
	}

	if !ctx.Bool("keep-binary") {
		if config.BinaryPath != "" {
			fmt.Printf("\n🔧 Binary Removed:\n")
			fmt.Printf("   - %s\n", config.BinaryPath)
		}
		if config.IrohBinaryPath != "" {
			fmt.Printf("   - %s (iroh-relay)\n", config.IrohBinaryPath)
		}
	}

	if !ctx.Bool("keep-user") && config.MaddyUser != "" {
		fmt.Printf("\n👤 User Account Removed:\n")
		fmt.Printf("   - User: %s\n", config.MaddyUser)
		if config.MaddyGroup != "" {
			fmt.Printf("   - Group: %s\n", config.MaddyGroup)
		}
	}

	// Show what was kept
	fmt.Printf("\n💾 Items Preserved:\n")
	if ctx.Bool("keep-config") {
		fmt.Printf("   - Configuration files (--keep-config)\n")
	}
	if ctx.Bool("keep-data") {
		fmt.Printf("   - Mail data and databases (--keep-data)\n")
	}
	if ctx.Bool("keep-user") {
		fmt.Printf("   - User account (--keep-user)\n")
	}
	if ctx.Bool("keep-binary") {
		fmt.Printf("   - Binary file (--keep-binary)\n")
	}

	if len(config.CertFiles) > 0 {
		fmt.Printf("\n🔒 Manual Cleanup Required:\n")
		fmt.Printf("   The following certificate files were referenced in the configuration\n")
		fmt.Printf("   and may need to be manually removed:\n")
		for _, file := range config.CertFiles {
			fmt.Printf("   - %s\n", file)
		}
	}

	if len(config.LogFiles) > 0 {
		fmt.Printf("\n📝 Log Files:\n")
		fmt.Printf("   The following log files were found and may contain useful information:\n")
		for _, file := range config.LogFiles {
			fmt.Printf("   - %s\n", file)
		}
		fmt.Printf("   These can be manually removed if no longer needed.\n")
	}

	fmt.Printf("\n✨ %s has been uninstalled.\n", frameworkconfig.BinaryName())
	if !ctx.Bool("keep-data") {
		fmt.Printf("⚠️  All mail data has been permanently deleted.\n")
	}
}
