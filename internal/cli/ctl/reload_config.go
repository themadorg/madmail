package ctl

import (
	"bytes"
	"crypto/tls"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"

	frameworkconfig "github.com/themadorg/madmail/framework/config"
	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "reload",
		Usage: "Apply DB config overrides and restart the service",
		Description: `Runs the same action as "Apply & restart" in the admin web UI: merge
database port and other overrides into the pending config file, then exit so
systemd (or your supervisor) restarts the process with the new listeners.

Use this after CLI changes such as "madmail port https set …" instead of
running "systemctl restart" yourself.

Signal note: SIGUSR2 only reloads in-memory credential and quota caches from
the database; it does NOT rebind HTTP/HTTPS ports. Port changes require this
command (or POST /admin/reload), which performs a clean process restart.`,
		Flags: []cli.Flag{
			&cli.StringFlag{
				Name:    "state-dir",
				Usage:   "Path to the state directory",
				EnvVars: []string{"MADDY_STATE_DIR", "MADMAIL_STATE_DIR"},
			},
			&cli.StringFlag{
				Name:  "url",
				Usage: "Override admin API base URL (default: from config + settings DB, like madmail admin-token)",
			},
			&cli.BoolFlag{
				Name:  "insecure",
				Usage: "Skip TLS certificate verification (for self-signed dev servers)",
			},
		},
		Action: runServiceReload,
	})
}

type adminAPIResponse struct {
	Status  int             `json:"status"`
	Error   *string         `json:"error"`
	Version string          `json:"version"`
	Body    json.RawMessage `json:"body"`
}

func runServiceReload(c *cli.Context) error {
	token, err := loadAdminTokenForCLI(c)
	if err != nil {
		return err
	}

	apiURL := strings.TrimSuffix(strings.TrimSpace(c.String("url")), "/")
	if apiURL == "" {
		apiURL = strings.TrimSuffix(buildAdminURL(getDBConfig(c)), "/")
	}

	envelope := map[string]any{
		"method":   "POST",
		"resource": "/admin/reload",
		"headers":  map[string]string{"Authorization": "Bearer " + token},
		"body":     map[string]any{},
	}
	payload, err := json.Marshal(envelope)
	if err != nil {
		return err
	}

	transport := &http.Transport{
		TLSClientConfig: &tls.Config{MinVersion: tls.VersionTLS12},
	}
	if c.Bool("insecure") {
		transport.TLSClientConfig.InsecureSkipVerify = true
	}
	client := &http.Client{
		Timeout:   120 * time.Second,
		Transport: transport,
	}

	req, err := http.NewRequest(http.MethodPost, apiURL, bytes.NewReader(payload))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", frameworkconfig.BinaryName()+"/reload")

	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("admin API request to %s: %w", apiURL, err)
	}
	defer resp.Body.Close()
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}

	var parsed adminAPIResponse
	if err := json.Unmarshal(respBody, &parsed); err != nil {
		s := string(respBody)
		if len(s) > 200 {
			s = s[:200] + "…"
		}
		return fmt.Errorf("invalid JSON from admin API (HTTP %d): %s", resp.StatusCode, s)
	}
	if parsed.Error != nil && *parsed.Error != "" {
		return fmt.Errorf("admin API: %s", *parsed.Error)
	}
	if parsed.Status >= 400 {
		return fmt.Errorf("admin API failed (status %d)", parsed.Status)
	}
	// Admin API always uses HTTP 200; real status is in JSON.
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("HTTP %d from admin API", resp.StatusCode)
	}

	fmt.Printf("✅ Reload requested at %s — the process will exit and %s should restart it.\n",
		apiURL, describeSupervisor())
	return nil
}

func describeSupervisor() string {
	if fi, err := os.Stat("/run/systemd/system"); err == nil && fi.IsDir() {
		return "systemd"
	}
	return "your service manager"
}
