package ctl

import (
	"crypto/tls"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	frameworkconfig "github.com/themadorg/madmail/framework/config"
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
It automatically handles stopping and starting the maddy.service via systemd.
The existing configuration (maddy.conf) is never modified by this command.`,
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

const maxDownloadSize = 100 * 1024 * 1024 // 100 MB

func handleUpdateURL(url string) error {
	// Create temporary file
	tmpFile, err := os.CreateTemp("", "maddy-update-*")
	if err != nil {
		return fmt.Errorf("failed to create temp file: %w", err)
	}
	defer os.Remove(tmpFile.Name())
	defer tmpFile.Close()

	fmt.Printf("📥 Downloading %s...\n", url)

	// Create a client that accepts both HTTP and HTTPS (skip TLS verify for
	// self-signed certs commonly used by other Madmail servers).
	client := &http.Client{
		Timeout: 5 * time.Minute,
		Transport: &http.Transport{
			TLSClientConfig: &tls.Config{InsecureSkipVerify: true},
		},
	}

	resp, err := client.Get(url)
	if err != nil {
		return fmt.Errorf("failed to download: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download failed with status: %s", resp.Status)
	}

	// Pre-check Content-Length if available
	if resp.ContentLength > maxDownloadSize {
		return fmt.Errorf("file too large: %d bytes (max %d MB)", resp.ContentLength, maxDownloadSize/(1024*1024))
	}

	// Limit download to 100 MB
	limitedReader := io.LimitReader(resp.Body, maxDownloadSize+1)
	n, err := io.Copy(tmpFile, limitedReader)
	if err != nil {
		return fmt.Errorf("failed to save download: %w", err)
	}
	if n > maxDownloadSize {
		return fmt.Errorf("download exceeded maximum size of %d MB, aborting", maxDownloadSize/(1024*1024))
	}
	fmt.Printf("✅ Downloaded %d bytes\n", n)
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

	// Stop systemd services
	fmt.Println("⏹️ Stopping services...")
	binSvc := frameworkconfig.ServiceName()
	_ = exec.Command("systemctl", "stop", binSvc).Run()
	_ = exec.Command("systemctl", "stop", "iroh-relay.service").Run()

	// Wait for the service to fully stop to avoid "text file busy"
	time.Sleep(1 * time.Second)

	// Perform binary replacement using a temporary file to avoid "text file busy"
	fmt.Println("🔄 Replacing binary...")

	// Create temp file in the same directory as the target binary
	tmpDir := filepath.Dir(realBinPath)
	tmpBin, err := os.CreateTemp(tmpDir, "maddy-upgrade-*")
	if err != nil {
		return fmt.Errorf("failed to create temporary binary: %w", err)
	}
	tmpPath := tmpBin.Name()
	defer os.Remove(tmpPath) // Cleanup if we fail

	src, err := os.Open(newBinPath)
	if err != nil {
		tmpBin.Close()
		return err
	}
	defer src.Close()

	if _, err := io.Copy(tmpBin, src); err != nil {
		tmpBin.Close()
		return fmt.Errorf("failed to copy new binary: %w", err)
	}

	if err := tmpBin.Sync(); err != nil {
		tmpBin.Close()
		return fmt.Errorf("failed to sync temporary binary: %w", err)
	}
	tmpBin.Close()

	// Set permissions
	if err := os.Chmod(tmpPath, 0755); err != nil {
		return fmt.Errorf("failed to set permissions on new binary: %w", err)
	}

	// Atomic rename (or as atomic as it gets)
	if err := os.Rename(tmpPath, realBinPath); err != nil {
		// If rename fails (might be cross-device, though we tried to avoid it with tmpDir),
		// we fallback to removing the target and then renaming/copying.
		// But usually in /usr/local/bin, it should work.
		return fmt.Errorf("failed to replace binary: %w", err)
	}

	fmt.Println("▶️ Starting services...")
	if err := exec.Command("systemctl", "start", binSvc).Run(); err != nil {
		fmt.Printf("⚠️ Warning: Failed to start %s: %v\n", binSvc, err)
		fmt.Printf("Manual start might be required: systemctl start %s\n", binSvc)
	}

	if _, err := os.Stat("/etc/systemd/system/iroh-relay.service"); err == nil {
		if err := exec.Command("systemctl", "start", "iroh-relay.service").Run(); err != nil {
			fmt.Printf("⚠️ Warning: Failed to start iroh-relay.service: %v\n", err)
			fmt.Println("Manual start might be required: systemctl start iroh-relay.service")
		}
	}

	fmt.Println("🎉 Maddy has been successfully upgraded!")
	return nil
}
