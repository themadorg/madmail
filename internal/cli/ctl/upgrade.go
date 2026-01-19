package ctl

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/themadorg/madmail/internal/auth"
	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/cli/clitools"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "upgrade",
		Usage: "Upgrade maddy binary from a local file or URL",
		Description: `Upgrade the maddy binary. This command verifies the Ed25519 signature
appended to the file before replacing the current executable.
It automatically handles stopping and starting the maddy.service via systemd.`,
		ArgsUsage: "PATH_OR_URL",
		Action:    upgradeCommand,
	})

	maddycli.AddSubcommand(&cli.Command{
		Name:        "update",
		Usage:       "Download and install maddy update from a URL",
		Description: `An alias for 'maddy upgrade' specifically for URLs.`,
		ArgsUsage:   "URL",
		Action:      upgradeCommand,
	})
}

func upgradeCommand(ctx *cli.Context) error {
	input := ctx.Args().First()
	if input == "" {
		return cli.Exit("Error: PATH or URL is required", 2)
	}

	if strings.HasPrefix(input, "http://") || strings.HasPrefix(input, "https://") {
		return handleUpdateURL(input)
	}

	return performUpgrade(input)
}

func handleUpdateURL(url string) error {
	// Create temporary file
	tmpFile, err := os.CreateTemp("", "maddy-update-*")
	if err != nil {
		return fmt.Errorf("failed to create temp file: %w", err)
	}
	defer os.Remove(tmpFile.Name())
	defer tmpFile.Close()

	fmt.Printf("📥 Downloading %s...\n", url)
	resp, err := http.Get(url)
	if err != nil {
		return fmt.Errorf("failed to download: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download failed with status: %s", resp.Status)
	}

	if _, err := io.Copy(tmpFile, resp.Body); err != nil {
		return fmt.Errorf("failed to save download: %w", err)
	}
	tmpFile.Close()

	return performUpgrade(tmpFile.Name())
}

func performUpgrade(newBinPath string) error {
	fmt.Println("🔍 Verifying digital signature...")
	ok, err := clitools.VerifySignature(newBinPath, auth.GetPublicKey())
	if err != nil {
		return fmt.Errorf("verification error: %w", err)
	}
	if !ok {
		return cli.Exit("❌ Error: INVALID SIGNATURE! This binary cannot be trusted. Upgrade aborted.", 1)
	}
	fmt.Println("✅ Signature verification successful.")

	// Determine current binary path
	currentBin, err := os.Executable()
	if err != nil {
		return fmt.Errorf("failed to get current executable path: %w", err)
	}
	// Follow symlinks to find the real path (e.g. if installed in /usr/local/bin/maddy)
	realBinPath, err := filepath.EvalSymlinks(currentBin)
	if err != nil {
		realBinPath = currentBin // Fallback
	}

	fmt.Printf("🚀 Target binary: %s\n", realBinPath)

	// Check for root privileges (required for systemctl and writing to /usr/local/bin)
	if os.Geteuid() != 0 {
		return cli.Exit("❌ Error: This command must be run as root (use sudo) to manage services and replace the binary.", 1)
	}

	// Stop systemd service
	fmt.Println("⏹️ Stopping maddy.service...")
	// We ignore error here because the service might not be running or named differently
	_ = exec.Command("systemctl", "stop", "maddy.service").Run()

	// Perform binary replacement
	fmt.Println("🔄 Replacing binary...")
	src, err := os.Open(newBinPath)
	if err != nil {
		return err
	}
	defer src.Close()

	dst, err := os.OpenFile(realBinPath, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0755)
	if err != nil {
		return fmt.Errorf("failed to open destination binary for writing: %w", err)
	}
	defer dst.Close()

	if _, err := io.Copy(dst, src); err != nil {
		return fmt.Errorf("failed to copy new binary: %w", err)
	}
	if err := dst.Sync(); err != nil {
		return fmt.Errorf("failed to sync destination binary: %w", err)
	}
	dst.Close()

	fmt.Println("▶️ Starting maddy.service...")
	if err := exec.Command("systemctl", "start", "maddy.service").Run(); err != nil {
		fmt.Printf("⚠️ Warning: Failed to start maddy.service: %v\n", err)
		fmt.Println("Manual start might be required: systemctl start maddy.service")
	}

	fmt.Println("🎉 Maddy has been successfully upgraded!")
	return nil
}
