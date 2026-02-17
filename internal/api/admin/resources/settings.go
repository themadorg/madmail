package resources

import (
	"encoding/json"
	"fmt"
)

// SettingsToggleDeps provides methods to read/write settings flags.
type SettingsToggleDeps struct {
	IsRegistrationOpen        func() (bool, error)
	SetRegistrationOpen       func(bool) error
	IsJitRegistrationEnabled  func() (bool, error)
	SetJitRegistrationEnabled func(bool) error
	IsTurnEnabled             func() (bool, error)
	SetTurnEnabled            func(bool) error
	// Iroh and Shadowsocks are new DB-backed settings
	GetSetting func(key string) (string, bool, error)
	SetSetting func(key, value string) error
}

type actionRequest struct {
	Action string `json:"action"`
}

type toggleStatusResponse struct {
	Status string `json:"status"`
}

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

// genericDBToggleHandler creates a handler for DB-backed settings like Iroh and Shadowsocks.
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
	return genericDBToggleHandler("__IROH_ENABLED__", deps)
}

// ShadowsocksHandler creates a handler for /admin/services/shadowsocks.
func ShadowsocksHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return genericDBToggleHandler("__SS_ENABLED__", deps)
}
