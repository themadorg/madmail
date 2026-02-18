package chatmail

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
	"time"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/internal/api/admin/resources"
)

// portMapping maps DB setting keys to the config file patterns they override.
// Each entry defines: the DB key, the regex pattern to match in maddy.conf, and
// a template to generate the replacement line.
type portMapping struct {
	dbKey string
	// regex must capture the prefix and port separately so we can replace just the port
	pattern *regexp.Regexp
	// replacement template: %s is replaced with the new port value
	replaceFmt string
}

// configOverrides defines all port/address settings that can be updated at runtime
// through the Admin API. Each entry maps a DB key to a config file pattern.
var configOverrides = []portMapping{
	// SMTP port: matches "smtp tcp://0.0.0.0:<port>"
	{
		dbKey:      resources.KeySMTPPort,
		pattern:    regexp.MustCompile(`(smtp\s+tcp://0\.0\.0\.0:)\d+`),
		replaceFmt: "${1}%s",
	},
	// Submission TLS port: matches first "tls://0.0.0.0:<port>" in submission line
	// Note: submission has both tls:// and tcp:// — we handle the tcp:// separately
	{
		dbKey:      resources.KeySubmissionPort,
		pattern:    regexp.MustCompile(`(submission\s+tls://0\.0\.0\.0:\d+\s+tcp://0\.0\.0\.0:)\d+`),
		replaceFmt: "${1}%s",
	},
	// IMAP TLS port + plain port: matches "imap tls://0.0.0.0:<port> tcp://0.0.0.0:<port>"
	{
		dbKey:      resources.KeyIMAPPort,
		pattern:    regexp.MustCompile(`(imap\s+tls://0\.0\.0\.0:\d+\s+tcp://0\.0\.0\.0:)\d+`),
		replaceFmt: "${1}%s",
	},
	// TURN port: matches "turn udp://0.0.0.0:<port> tcp://0.0.0.0:<port>" plus "turn_port <port>"
	{
		dbKey:      resources.KeyTurnPort,
		pattern:    regexp.MustCompile(`(turn\s+udp://0\.0\.0\.0:)\d+(\s+tcp://0\.0\.0\.0:)\d+`),
		replaceFmt: "${1}%s${2}%s",
	},
	// Iroh port: matches "iroh_relay_url http://<ip>:<port>"
	{
		dbKey:      resources.KeyIrohPort,
		pattern:    regexp.MustCompile(`(iroh_relay_url\s+http://[^:]+:)\d+`),
		replaceFmt: "${1}%s",
	},
	// TURN secret: matches "turn_secret <value>" and "secret <value>" in turn block
	{
		dbKey:      resources.KeyTurnSecret,
		pattern:    regexp.MustCompile(`(turn_secret\s+)\S+`),
		replaceFmt: "${1}%s",
	},
	// TURN realm: matches "realm <value>" in turn block
	{
		dbKey:      resources.KeyTurnRealm,
		pattern:    regexp.MustCompile(`(^\s+realm\s+)\S+`),
		replaceFmt: "${1}%s",
	},
	// TURN relay_ip: matches "relay_ip <value>" in turn block
	{
		dbKey:      resources.KeyTurnRelayIP,
		pattern:    regexp.MustCompile(`(^\s+relay_ip\s+)\S+`),
		replaceFmt: "${1}%s",
	},
	// TURN TTL: matches "turn_ttl <value>" in IMAP block
	{
		dbKey:      resources.KeyTurnTTL,
		pattern:    regexp.MustCompile(`(turn_ttl\s+)\S+`),
		replaceFmt: "${1}%s",
	},
	// Shadowsocks address (port): matches 'ss_addr "0.0.0.0:<port>"'
	{
		dbKey:      resources.KeySsPort,
		pattern:    regexp.MustCompile(`(ss_addr\s+"0\.0\.0\.0:)\d+(")`),
		replaceFmt: "${1}%s${2}",
	},
	// Shadowsocks password: matches 'ss_password "<value>"'
	{
		dbKey:      resources.KeySsPassword,
		pattern:    regexp.MustCompile(`(ss_password\s+")[^"]+(")`),
		replaceFmt: "${1}%s${2}",
	},
	// Shadowsocks cipher: matches 'ss_cipher "<value>"'
	{
		dbKey:      resources.KeySsCipher,
		pattern:    regexp.MustCompile(`(ss_cipher\s+")[^"]+(")`),
		replaceFmt: "${1}%s${2}",
	},
	// HTTP port: matches 'chatmail tcp://0.0.0.0:<port>'
	{
		dbKey:      resources.KeyHTTPPort,
		pattern:    regexp.MustCompile(`(chatmail\s+tcp://0\.0\.0\.0:)\d+`),
		replaceFmt: "${1}%s",
	},
	// HTTPS port: matches 'chatmail tls://0.0.0.0:<port>'
	{
		dbKey:      resources.KeyHTTPSPort,
		pattern:    regexp.MustCompile(`(chatmail\s+tls://0\.0\.0\.0:)\d+`),
		replaceFmt: "${1}%s",
	},
}

// reloadConfig reads port/config overrides from the database, applies them
// to the maddy.conf configuration file, and restarts the service.
//
// The flow is:
// 1. Read current maddy.conf
// 2. For each setting key with a DB override, patch the config file
// 3. Write the updated config
// 4. Restart the maddy service (via systemctl or self-signal)
func (e *Endpoint) reloadConfig() error {
	// Find the config file path
	configPath := findConfigPath()
	if configPath == "" {
		return fmt.Errorf("cannot find maddy.conf: checked /etc/maddy/maddy.conf and state_dir")
	}

	// Read the current config
	data, err := os.ReadFile(configPath)
	if err != nil {
		return fmt.Errorf("failed to read config file %s: %v", configPath, err)
	}

	content := string(data)
	modified := false

	// Apply each override from the database
	for _, mapping := range configOverrides {
		val, isSet, err := e.authDB.GetSetting(mapping.dbKey)
		if err != nil {
			e.logger.Error("reload: failed to read setting", err, "key", mapping.dbKey)
			continue
		}
		if !isSet || val == "" {
			continue // No override set, keep config as-is
		}

		// Defense-in-depth: reject values that could inject config directives.
		// The API layer validates too, but this protects against direct DB tampering.
		if strings.ContainsAny(val, "\n\r\x00\"\\") {
			e.logger.Error("reload: REJECTED unsafe value for "+mapping.dbKey, nil,
				"reason", "contains newline/null/quote/backslash")
			continue
		}

		// Build the replacement string
		var replacement string
		if mapping.dbKey == resources.KeyTurnPort {
			// TURN port appears twice (udp + tcp) in the same line
			replacement = fmt.Sprintf(mapping.replaceFmt, val, val)
		} else {
			replacement = fmt.Sprintf(mapping.replaceFmt, val)
		}

		newContent := mapping.pattern.ReplaceAllString(content, replacement)
		if newContent != content {
			e.logger.Printf("reload: applied %s = %s", mapping.dbKey, val)
			content = newContent
			modified = true
		}
	}

	// Also handle turn_port references inside the IMAP block
	if val, isSet, err := e.authDB.GetSetting(resources.KeyTurnPort); err == nil && isSet && val != "" {
		turnPortInIMAP := regexp.MustCompile(`(turn_port\s+)\d+`)
		newContent := turnPortInIMAP.ReplaceAllString(content, "${1}"+val)
		if newContent != content {
			content = newContent
			modified = true
		}
	}

	// Also handle "secret" inside the turn {} block (distinct from turn_secret in IMAP)
	if val, isSet, err := e.authDB.GetSetting(resources.KeyTurnSecret); err == nil && isSet && val != "" {
		secretInTurn := regexp.MustCompile(`(\s+secret\s+)\S+`)
		newContent := secretInTurn.ReplaceAllString(content, "${1}"+val)
		if newContent != content {
			content = newContent
			modified = true
		}
	}

	if modified {
		// Write the modified config to a pending file in the state dir.
		// The state dir (/var/lib/maddy/) is writable by the maddy user,
		// while the config dir (/etc/maddy/) is owned by root.
		// An ExecStartPre script in the systemd unit copies the pending file
		// to the actual config location on next startup.
		pendingPath := filepath.Join(config.StateDirectory, "maddy.conf.pending")
		if err := os.WriteFile(pendingPath, []byte(content), 0640); err != nil {
			return fmt.Errorf("failed to write pending config to %s: %v", pendingPath, err)
		}
		e.logger.Printf("reload: pending config written to %s", pendingPath)
	}

	// Always restart. Some settings (like port access local-only) don't modify
	// the config file but still require a restart to re-bind listeners.
	e.logger.Printf("reload: restarting service")
	return restartService()
}

// findConfigPath locates the maddy.conf file.
func findConfigPath() string {
	// Check standard locations
	candidates := []string{
		"/etc/maddy/maddy.conf",
	}

	// Also check relative to state directory
	if config.StateDirectory != "" {
		candidates = append(candidates,
			config.StateDirectory+"/../maddy.conf",
			config.StateDirectory+"/maddy.conf",
		)
	}

	for _, path := range candidates {
		if info, err := os.Stat(path); err == nil && !info.IsDir() {
			return path
		}
	}
	return ""
}

// restartService schedules a SIGTERM to the current process after a short delay.
// The delay allows the HTTP response to be sent before the process terminates.
// The systemd unit uses Restart=always, so systemd will restart the service
// with the newly written config file.
func restartService() error {
	time.AfterFunc(500*time.Millisecond, func() {
		_ = syscall.Kill(os.Getpid(), syscall.SIGTERM)
	})
	return nil
}

// configOverrideKeys returns the list of DB setting keys that affect the config file.
// This is used by the admin API to determine which settings require a restart.
func configOverrideKeys() map[string]bool {
	keys := make(map[string]bool, len(configOverrides))
	for _, m := range configOverrides {
		keys[m.dbKey] = true
	}
	return keys
}

// For testing: allow the replacer to work on arbitrary text
func applyPortOverride(content string, mapping portMapping, value string) string {
	var replacement string
	if strings.Count(mapping.replaceFmt, "%s") == 2 {
		replacement = fmt.Sprintf(mapping.replaceFmt, value, value)
	} else {
		replacement = fmt.Sprintf(mapping.replaceFmt, value)
	}
	return mapping.pattern.ReplaceAllString(content, replacement)
}

// logDBOverrides logs all settings that have been overridden in the database.
// Called at startup so users know which config values are being superseded by DB values.
func (e *Endpoint) logDBOverrides() {
	// Check config override keys (ports, hostnames, etc.)
	for _, mapping := range configOverrides {
		val, isSet, err := e.authDB.GetSetting(mapping.dbKey)
		if err != nil || !isSet {
			continue
		}
		e.logger.Printf("DB override active: %s = %s (config file value ignored)", mapping.dbKey, val)
	}

	// Check toggle settings
	toggleKeys := []string{
		resources.KeySsEnabled,
		resources.KeyIrohEnabled,
		resources.KeyLogDisabled,
		resources.KeyAdminPath,
	}
	for _, key := range toggleKeys {
		val, isSet, err := e.authDB.GetSetting(key)
		if err != nil || !isSet {
			continue
		}
		e.logger.Printf("DB override active: %s = %s", key, val)
	}
}
