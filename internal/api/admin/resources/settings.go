package resources

import (
	"encoding/json"
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

// SettingsToggleDeps provides methods to read/write settings flags.
type SettingsToggleDeps struct {
	IsRegistrationOpen           func() (bool, error)
	SetRegistrationOpen          func(bool) error
	IsJitRegistrationEnabled     func() (bool, error)
	SetJitRegistrationEnabled    func(bool) error
	IsTurnEnabled                func() (bool, error)
	SetTurnEnabled               func(bool) error
	DeleteSetting                func(key string) error
	GetShadowsocksActiveSettings func() (password, cipher, port string)
	GetSetting                   func(key string) (string, bool, error)
	SetSetting                   func(key, value string) error
}

type actionRequest struct {
	Action string `json:"action"`
}

type toggleStatusResponse struct {
	Status string `json:"status"`
}

// settingValueRequest is used for setting a string value (port, hostname, etc.)
// Value accepts both JSON strings ("8083") and numbers (8083).
type settingValueRequest struct {
	Action string `json:"action"` // "set" or "reset"
	Value  string `json:"value"`
}

func (r *settingValueRequest) UnmarshalJSON(data []byte) error {
	// Use a raw struct to avoid infinite recursion
	type raw struct {
		Action string          `json:"action"`
		Value  json.RawMessage `json:"value"`
	}
	var v raw
	if err := json.Unmarshal(data, &v); err != nil {
		return err
	}
	r.Action = v.Action
	if len(v.Value) > 0 {
		// Try string first
		var s string
		if err := json.Unmarshal(v.Value, &s); err == nil {
			r.Value = s
		} else {
			// Fall back to number → string
			r.Value = strings.TrimSpace(string(v.Value))
		}
	}
	return nil
}

// portKeys is the set of setting keys that must be valid port numbers.
var portKeys = map[string]bool{
	KeySMTPPort: true, KeySubmissionPort: true, KeyIMAPPort: true,
	KeyTurnPort: true, KeySaslPort: true, KeyIrohPort: true, KeySsPort: true,
	KeyHTTPPort: true, KeyHTTPSPort: true,
}

// safeValuePattern matches values that are safe to insert into maddy.conf.
// Rejects newlines, null bytes, quotes, backslashes, and other control characters
// that could be used for config injection.
var safeValuePattern = regexp.MustCompile(`^[a-zA-Z0-9._:/@\-]+$`)

// validateSettingValue validates a setting value based on its key type.
// Returns an error if the value could cause config injection or is otherwise invalid.
func validateSettingValue(key, value string) error {
	// Reject null bytes and newlines universally — these can never be valid
	if strings.ContainsAny(value, "\x00\n\r") {
		return fmt.Errorf("value contains invalid characters")
	}

	// Port keys must be valid port numbers (1-65535)
	if portKeys[key] {
		port, err := strconv.Atoi(value)
		if err != nil || port < 1 || port > 65535 {
			return fmt.Errorf("invalid port number: must be 1-65535")
		}
		return nil
	}

	// TTL must be a positive integer
	if key == KeyTurnTTL {
		ttl, err := strconv.Atoi(value)
		if err != nil || ttl < 1 {
			return fmt.Errorf("invalid TTL: must be a positive integer")
		}
		return nil
	}

	// All other config values: must match safe character set
	if !safeValuePattern.MatchString(value) {
		return fmt.Errorf("value contains disallowed characters")
	}

	// Additional length limit to prevent DoS via very large values
	if len(value) > 253 { // RFC 1035 max domain name length
		return fmt.Errorf("value too long (max 253 characters)")
	}

	return nil
}

// settingValueResponse is used for returning a string value.
type settingValueResponse struct {
	Key             string `json:"key"`
	Value           string `json:"value"`
	IsSet           bool   `json:"is_set"`           // true if explicitly set in DB, false if using default
	RestartRequired bool   `json:"restart_required"` // true after a change that requires service reload
}

// AllSettingsResponse is the response for GET /admin/settings.
type AllSettingsResponse struct {
	// Toggle settings
	Registration    string `json:"registration"`     // "open" or "closed"
	JitRegistration string `json:"jit_registration"` // "enabled" or "disabled"
	TurnEnabled     string `json:"turn_enabled"`     // "enabled" or "disabled"
	IrohEnabled     string `json:"iroh_enabled"`     // "enabled" or "disabled"
	SsEnabled       string `json:"ss_enabled"`       // "enabled" or "disabled"
	LogDisabled     string `json:"log_disabled"`     // "enabled" or "disabled"

	// Port settings
	SMTPPort       settingValueResponse `json:"smtp_port"`
	SubmissionPort settingValueResponse `json:"submission_port"`
	IMAPPort       settingValueResponse `json:"imap_port"`
	TurnPort       settingValueResponse `json:"turn_port"`
	SaslPort       settingValueResponse `json:"sasl_port"`
	IrohPort       settingValueResponse `json:"iroh_port"`
	SsPort         settingValueResponse `json:"ss_port"`
	HTTPPort       settingValueResponse `json:"http_port"`
	HTTPSPort      settingValueResponse `json:"https_port"`

	// Per-port access control: "public" (default) or "local" (Shadowsocks only)
	SMTPAccess       string `json:"smtp_access"`
	SubmissionAccess string `json:"submission_access"`
	IMAPAccess       string `json:"imap_access"`
	TurnAccess       string `json:"turn_access"`
	SaslAccess       string `json:"sasl_access"`
	IrohAccess       string `json:"iroh_access"`
	HTTPAccess       string `json:"http_access"`
	HTTPSAccess      string `json:"https_access"`

	// Hostname / address settings
	SMTPHostname   settingValueResponse `json:"smtp_hostname"`
	TurnRealm      settingValueResponse `json:"turn_realm"`
	TurnSecret     settingValueResponse `json:"turn_secret"`
	TurnRelayIP    settingValueResponse `json:"turn_relay_ip"`
	TurnTTL        settingValueResponse `json:"turn_ttl"`
	IrohRelayURL   settingValueResponse `json:"iroh_relay_url"`
	SsCipher       settingValueResponse `json:"ss_cipher"`
	SsPassword     settingValueResponse `json:"ss_password"`
	ShadowsocksURL string               `json:"shadowsocks_url"`
	AdminPath      settingValueResponse `json:"admin_path"`
}

// Setting key constants for all configurable values.
const (
	KeyRegistrationOpen       = "__REGISTRATION_OPEN__"
	KeyJitRegistrationEnabled = "__JIT_REGISTRATION_ENABLED__"
	KeyTurnEnabled            = "__TURN_ENABLED__"
	KeyLogDisabled            = "__LOG_DISABLED__"
	KeyIrohEnabled            = "__IROH_ENABLED__"
	KeySsEnabled              = "__SS_ENABLED__"

	// Port settings
	KeySMTPPort       = "__SMTP_PORT__"
	KeySubmissionPort = "__SUBMISSION_PORT__"
	KeyIMAPPort       = "__IMAP_PORT__"
	KeyTurnPort       = "__TURN_PORT__"
	KeySaslPort       = "__SASL_PORT__"
	KeyIrohPort       = "__IROH_PORT__"
	KeySsPort         = "__SS_PORT__"
	KeyHTTPPort       = "__HTTP_PORT__"
	KeyHTTPSPort      = "__HTTPS_PORT__"

	// Per-port access control ("true" = local only, default unset = public)
	KeySMTPLocalOnly       = "__SMTP_LOCAL_ONLY__"
	KeySubmissionLocalOnly = "__SUBMISSION_LOCAL_ONLY__"
	KeyIMAPLocalOnly       = "__IMAP_LOCAL_ONLY__"
	KeyTurnLocalOnly       = "__TURN_LOCAL_ONLY__"
	KeySaslLocalOnly       = "__SASL_LOCAL_ONLY__"
	KeyIrohLocalOnly       = "__IROH_LOCAL_ONLY__"
	KeyHTTPLocalOnly       = "__HTTP_LOCAL_ONLY__"
	KeyHTTPSLocalOnly      = "__HTTPS_LOCAL_ONLY__"

	// Configuration settings
	KeySMTPHostname = "__SMTP_HOSTNAME__"
	KeyTurnRealm    = "__TURN_REALM__"
	KeyTurnSecret   = "__TURN_SECRET__"
	KeyTurnRelayIP  = "__TURN_RELAY_IP__"
	KeyTurnTTL      = "__TURN_TTL__"
	KeyIrohRelayURL = "__IROH_RELAY_URL__"
	KeySsCipher     = "__SS_CIPHER__"
	KeySsPassword   = "__SS_PASSWORD__"
	KeyAdminPath    = "__ADMIN_PATH__"
)

// RegistrationHandler creates a handler for /admin/registration.
func RegistrationHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			open, err := deps.IsRegistrationOpen()
			if err != nil {
				return nil, 500, fmt.Errorf("failed to check registration status: %v", err)
			}
			status := "closed"
			if open {
				status = "open"
			}
			return toggleStatusResponse{Status: status}, 200, nil

		case "POST":
			var req actionRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			switch req.Action {
			case "open":
				if err := deps.SetRegistrationOpen(true); err != nil {
					return nil, 500, fmt.Errorf("failed to open registration: %v", err)
				}
				return toggleStatusResponse{Status: "open"}, 200, nil
			case "close":
				if err := deps.SetRegistrationOpen(false); err != nil {
					return nil, 500, fmt.Errorf("failed to close registration: %v", err)
				}
				return toggleStatusResponse{Status: "closed"}, 200, nil
			default:
				return nil, 400, fmt.Errorf("invalid action: %s (expected open|close)", req.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// JitRegistrationHandler creates a handler for /admin/registration/jit.
func JitRegistrationHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			enabled, err := deps.IsJitRegistrationEnabled()
			if err != nil {
				return nil, 500, fmt.Errorf("failed to check JIT status: %v", err)
			}
			status := "disabled"
			if enabled {
				status = "enabled"
			}
			return toggleStatusResponse{Status: status}, 200, nil

		case "POST":
			var req actionRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			switch req.Action {
			case "enable":
				if err := deps.SetJitRegistrationEnabled(true); err != nil {
					return nil, 500, fmt.Errorf("failed to enable JIT: %v", err)
				}
				return toggleStatusResponse{Status: "enabled"}, 200, nil
			case "disable":
				if err := deps.SetJitRegistrationEnabled(false); err != nil {
					return nil, 500, fmt.Errorf("failed to disable JIT: %v", err)
				}
				return toggleStatusResponse{Status: "disabled"}, 200, nil
			default:
				return nil, 400, fmt.Errorf("invalid action: %s (expected enable|disable)", req.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// TurnHandler creates a handler for /admin/services/turn.
func TurnHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			enabled, err := deps.IsTurnEnabled()
			if err != nil {
				return nil, 500, fmt.Errorf("failed to check TURN status: %v", err)
			}
			status := "disabled"
			if enabled {
				status = "enabled"
			}
			return toggleStatusResponse{Status: status}, 200, nil

		case "POST":
			var req actionRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			switch req.Action {
			case "enable":
				if err := deps.SetTurnEnabled(true); err != nil {
					return nil, 500, fmt.Errorf("failed to enable TURN: %v", err)
				}
				return toggleStatusResponse{Status: "enabled"}, 200, nil
			case "disable":
				if err := deps.SetTurnEnabled(false); err != nil {
					return nil, 500, fmt.Errorf("failed to disable TURN: %v", err)
				}
				return toggleStatusResponse{Status: "disabled"}, 200, nil
			default:
				return nil, 400, fmt.Errorf("invalid action: %s (expected enable|disable)", req.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// genericDBToggleHandler creates a handler for DB-backed boolean settings like Iroh and Shadowsocks.
func genericDBToggleHandler(settingKey string, deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			val, ok, err := deps.GetSetting(settingKey)
			if err != nil {
				return nil, 500, fmt.Errorf("failed to get setting: %v", err)
			}
			status := "enabled" // Default to enabled if not set
			if ok && val == "false" {
				status = "disabled"
			}
			return toggleStatusResponse{Status: status}, 200, nil

		case "POST":
			var req actionRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			switch req.Action {
			case "enable":
				if err := deps.SetSetting(settingKey, "true"); err != nil {
					return nil, 500, fmt.Errorf("failed to enable: %v", err)
				}
				return toggleStatusResponse{Status: "enabled"}, 200, nil
			case "disable":
				if err := deps.SetSetting(settingKey, "false"); err != nil {
					return nil, 500, fmt.Errorf("failed to disable: %v", err)
				}
				return toggleStatusResponse{Status: "disabled"}, 200, nil
			default:
				return nil, 400, fmt.Errorf("invalid action: %s (expected enable|disable)", req.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// IrohHandler creates a handler for /admin/services/iroh.
func IrohHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return genericDBToggleHandler(KeyIrohEnabled, deps)
}

// ShadowsocksHandler creates a handler for /admin/services/shadowsocks.
func ShadowsocksHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return genericDBToggleHandler(KeySsEnabled, deps)
}

// LogHandler creates a handler for /admin/services/log.
func LogHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return genericDBToggleHandler(KeyLogDisabled, deps)
}

// GenericSettingHandler creates a handler for a single string-valued DB setting.
// Supports GET (read), POST with action "set" + "value", and POST with action "reset".
func GenericSettingHandler(settingKey string, deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			val, ok, err := deps.GetSetting(settingKey)
			if err != nil {
				return nil, 500, fmt.Errorf("failed to get setting %s: %v", settingKey, err)
			}
			return settingValueResponse{
				Key:   settingKey,
				Value: val,
				IsSet: ok,
			}, 200, nil

		case "POST":
			var req settingValueRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			switch req.Action {
			case "set":
				if req.Value == "" {
					return nil, 400, fmt.Errorf("value is required for action 'set'")
				}
				// Validate value to prevent config injection
				if err := validateSettingValue(settingKey, req.Value); err != nil {
					return nil, 400, fmt.Errorf("invalid value for %s: %v", settingKey, err)
				}
				if err := deps.SetSetting(settingKey, req.Value); err != nil {
					return nil, 500, fmt.Errorf("failed to set %s: %v", settingKey, err)
				}
				return settingValueResponse{
					Key:             settingKey,
					Value:           req.Value,
					IsSet:           true,
					RestartRequired: true,
				}, 200, nil
			case "reset":
				if deps.DeleteSetting != nil {
					if err := deps.DeleteSetting(settingKey); err != nil {
						return nil, 500, fmt.Errorf("failed to reset %s: %v", settingKey, err)
					}
				}
				return settingValueResponse{
					Key:             settingKey,
					Value:           "",
					IsSet:           false,
					RestartRequired: true,
				}, 200, nil
			default:
				return nil, 400, fmt.Errorf("invalid action: %s (expected set|reset)", req.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// AllSettingsHandler creates a handler for GET /admin/settings that returns all settings at once.
func AllSettingsHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if method != "GET" {
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}

		resp := AllSettingsResponse{}

		// Toggle settings
		if open, err := deps.IsRegistrationOpen(); err == nil {
			if open {
				resp.Registration = "open"
			} else {
				resp.Registration = "closed"
			}
		}
		if jit, err := deps.IsJitRegistrationEnabled(); err == nil {
			if jit {
				resp.JitRegistration = "enabled"
			} else {
				resp.JitRegistration = "disabled"
			}
		}
		if turn, err := deps.IsTurnEnabled(); err == nil {
			if turn {
				resp.TurnEnabled = "enabled"
			} else {
				resp.TurnEnabled = "disabled"
			}
		}

		// DB-backed toggle settings
		getToggle := func(key string) string {
			val, ok, err := deps.GetSetting(key)
			if err != nil || !ok {
				return "enabled" // default
			}
			if val == "false" {
				return "disabled"
			}
			return "enabled"
		}
		resp.IrohEnabled = getToggle(KeyIrohEnabled)
		resp.SsEnabled = getToggle(KeySsEnabled)
		resp.LogDisabled = getToggle(KeyLogDisabled)

		// String settings helper
		getSetting := func(key string, activeVal string) settingValueResponse {
			val, ok, err := deps.GetSetting(key)
			if err != nil {
				return settingValueResponse{Key: key, Value: activeVal}
			}
			if !ok {
				return settingValueResponse{Key: key, Value: activeVal, IsSet: false}
			}
			return settingValueResponse{Key: key, Value: val, IsSet: ok}
		}

		ssPass, ssCiph, ssPort := "", "", ""
		if deps.GetShadowsocksActiveSettings != nil {
			ssPass, ssCiph, ssPort = deps.GetShadowsocksActiveSettings()
		}

		// Port settings
		resp.SMTPPort = getSetting(KeySMTPPort, "")
		resp.SubmissionPort = getSetting(KeySubmissionPort, "")
		resp.IMAPPort = getSetting(KeyIMAPPort, "")
		resp.TurnPort = getSetting(KeyTurnPort, "")
		resp.SaslPort = getSetting(KeySaslPort, "")
		resp.IrohPort = getSetting(KeyIrohPort, "")
		resp.SsPort = getSetting(KeySsPort, ssPort)
		resp.HTTPPort = getSetting(KeyHTTPPort, "")
		resp.HTTPSPort = getSetting(KeyHTTPSPort, "")

		// Per-port access control
		getAccess := func(key string) string {
			val, ok, err := deps.GetSetting(key)
			if err != nil || !ok || val != "true" {
				return "public" // default: open to internet
			}
			return "local" // local only (Shadowsocks)
		}
		resp.SMTPAccess = getAccess(KeySMTPLocalOnly)
		resp.SubmissionAccess = getAccess(KeySubmissionLocalOnly)
		resp.IMAPAccess = getAccess(KeyIMAPLocalOnly)
		resp.TurnAccess = getAccess(KeyTurnLocalOnly)
		resp.SaslAccess = getAccess(KeySaslLocalOnly)
		resp.IrohAccess = getAccess(KeyIrohLocalOnly)
		resp.HTTPAccess = getAccess(KeyHTTPLocalOnly)
		resp.HTTPSAccess = getAccess(KeyHTTPSLocalOnly)

		// Configuration settings
		resp.SMTPHostname = getSetting(KeySMTPHostname, "")
		resp.TurnRealm = getSetting(KeyTurnRealm, "")
		resp.TurnSecret = getSetting(KeyTurnSecret, "")
		resp.TurnRelayIP = getSetting(KeyTurnRelayIP, "")
		resp.TurnTTL = getSetting(KeyTurnTTL, "")
		resp.IrohRelayURL = getSetting(KeyIrohRelayURL, "")
		resp.SsCipher = getSetting(KeySsCipher, ssCiph)
		resp.SsPassword = getSetting(KeySsPassword, ssPass)
		resp.AdminPath = getSetting(KeyAdminPath, "")

		return resp, 200, nil
	}
}
