package resources

import (
	"encoding/json"
	"fmt"
	"testing"
)

// mockSettingStore is an in-memory key-value store for testing.
type mockSettingStore struct {
	data map[string]string
}

func newMockSettingStore() *mockSettingStore {
	return &mockSettingStore{data: make(map[string]string)}
}

func (m *mockSettingStore) Get(key string) (string, bool, error) {
	v, ok := m.data[key]
	return v, ok, nil
}

func (m *mockSettingStore) Set(key, value string) error {
	m.data[key] = value
	return nil
}

func (m *mockSettingStore) Delete(key string) error {
	delete(m.data, key)
	return nil
}

// buildTestDeps creates a SettingsToggleDeps backed by a mock store.
func buildTestDeps() (SettingsToggleDeps, *mockSettingStore) {
	store := newMockSettingStore()

	deps := SettingsToggleDeps{
		IsRegistrationOpen: func() (bool, error) {
			v, ok, _ := store.Get(KeyRegistrationOpen)
			if !ok {
				return false, nil
			}
			return v == "true", nil
		},
		SetRegistrationOpen: func(open bool) error {
			val := "false"
			if open {
				val = "true"
			}
			return store.Set(KeyRegistrationOpen, val)
		},
		IsJitRegistrationEnabled: func() (bool, error) {
			v, ok, _ := store.Get(KeyJitRegistrationEnabled)
			if !ok {
				return false, nil
			}
			return v == "true", nil
		},
		SetJitRegistrationEnabled: func(enabled bool) error {
			val := "false"
			if enabled {
				val = "true"
			}
			return store.Set(KeyJitRegistrationEnabled, val)
		},
		IsTurnEnabled: func() (bool, error) {
			v, ok, _ := store.Get(KeyTurnEnabled)
			if !ok {
				return true, nil
			}
			return v == "true", nil
		},
		SetTurnEnabled: func(enabled bool) error {
			val := "false"
			if enabled {
				val = "true"
			}
			return store.Set(KeyTurnEnabled, val)
		},
		GetSetting:    store.Get,
		SetSetting:    store.Set,
		DeleteSetting: store.Delete,
	}

	return deps, store
}

func TestRegistrationHandler_GetDefault(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := RegistrationHandler(deps)

	resp, status, err := handler("GET", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r := resp.(toggleStatusResponse)
	if r.Status != "closed" {
		t.Errorf("expected 'closed', got %q", r.Status)
	}
}

func TestRegistrationHandler_Open(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := RegistrationHandler(deps)

	body, _ := json.Marshal(actionRequest{Action: "open"})
	resp, status, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r := resp.(toggleStatusResponse)
	if r.Status != "open" {
		t.Errorf("expected 'open', got %q", r.Status)
	}

	// Verify via GET
	resp, _, _ = handler("GET", nil)
	r = resp.(toggleStatusResponse)
	if r.Status != "open" {
		t.Errorf("expected 'open' after set, got %q", r.Status)
	}
}

func TestRegistrationHandler_InvalidAction(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := RegistrationHandler(deps)

	body, _ := json.Marshal(actionRequest{Action: "invalid"})
	_, status, err := handler("POST", body)
	if status != 400 {
		t.Errorf("expected 400, got %d", status)
	}
	if err == nil {
		t.Error("expected error for invalid action")
	}
}

func TestGenericDBToggleHandler_DefaultEnabled(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := genericDBToggleHandler(KeyIrohEnabled, deps)

	resp, status, err := handler("GET", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r := resp.(toggleStatusResponse)
	// Default is "enabled" when not set
	if r.Status != "enabled" {
		t.Errorf("expected 'enabled' default, got %q", r.Status)
	}
}

func TestGenericDBToggleHandler_Disable(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := genericDBToggleHandler(KeyIrohEnabled, deps)

	body, _ := json.Marshal(actionRequest{Action: "disable"})
	resp, status, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r := resp.(toggleStatusResponse)
	if r.Status != "disabled" {
		t.Errorf("expected 'disabled', got %q", r.Status)
	}

	// Verify via GET
	resp, _, _ = handler("GET", nil)
	r = resp.(toggleStatusResponse)
	if r.Status != "disabled" {
		t.Errorf("expected 'disabled' after set, got %q", r.Status)
	}
}

func TestGenericSettingHandler_SetAndGet(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := GenericSettingHandler(KeySMTPPort, deps)

	// Initially not set
	resp, status, err := handler("GET", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r := resp.(settingValueResponse)
	if r.IsSet {
		t.Error("expected IsSet=false initially")
	}

	// Set the value
	body, _ := json.Marshal(settingValueRequest{Action: "set", Value: "2525"})
	resp, status, err = handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r = resp.(settingValueResponse)
	if !r.IsSet || r.Value != "2525" {
		t.Errorf("expected IsSet=true Value=2525, got IsSet=%v Value=%q", r.IsSet, r.Value)
	}

	// Read it back
	resp, _, _ = handler("GET", nil)
	r = resp.(settingValueResponse)
	if !r.IsSet || r.Value != "2525" {
		t.Errorf("expected IsSet=true Value=2525, got IsSet=%v Value=%q", r.IsSet, r.Value)
	}
}

func TestGenericSettingHandler_Reset(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := GenericSettingHandler(KeySMTPPort, deps)

	// Set then reset
	body, _ := json.Marshal(settingValueRequest{Action: "set", Value: "2525"})
	handler("POST", body)

	body, _ = json.Marshal(settingValueRequest{Action: "reset"})
	resp, status, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r := resp.(settingValueResponse)
	if r.IsSet {
		t.Error("expected IsSet=false after reset")
	}

	// Verify via GET
	resp, _, _ = handler("GET", nil)
	r = resp.(settingValueResponse)
	if r.IsSet {
		t.Error("expected IsSet=false after reset, confirmed by GET")
	}
}

func TestGenericSettingHandler_SetEmptyValueFails(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := GenericSettingHandler(KeySMTPPort, deps)

	body, _ := json.Marshal(settingValueRequest{Action: "set", Value: ""})
	_, status, err := handler("POST", body)
	if status != 400 {
		t.Errorf("expected 400, got %d", status)
	}
	if err == nil {
		t.Error("expected error for empty value")
	}
}

func TestGenericSettingHandler_InvalidAction(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := GenericSettingHandler(KeySMTPPort, deps)

	body, _ := json.Marshal(settingValueRequest{Action: "invalid"})
	_, status, err := handler("POST", body)
	if status != 400 {
		t.Errorf("expected 400, got %d", status)
	}
	if err == nil {
		t.Error("expected error for invalid action")
	}
}

func TestGenericSettingHandler_MethodNotAllowed(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := GenericSettingHandler(KeySMTPPort, deps)

	_, status, err := handler("DELETE", nil)
	if status != 405 {
		t.Errorf("expected 405, got %d", status)
	}
	if err == nil {
		t.Error("expected error for unsupported method")
	}
}

func TestAllSettingsHandler(t *testing.T) {
	deps, store := buildTestDeps()
	handler := AllSettingsHandler(deps)

	// Set some values
	store.Set(KeySMTPPort, "2525")
	store.Set(KeyIrohEnabled, "false")
	store.Set(KeyTurnRealm, "example.com")

	resp, status, err := handler("GET", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}

	r := resp.(AllSettingsResponse)

	// Check port
	if !r.SMTPPort.IsSet || r.SMTPPort.Value != "2525" {
		t.Errorf("unexpected SMTPPort: %+v", r.SMTPPort)
	}

	// Check toggle
	if r.IrohEnabled != "disabled" {
		t.Errorf("expected iroh_enabled=disabled, got %q", r.IrohEnabled)
	}

	// Check config
	if !r.TurnRealm.IsSet || r.TurnRealm.Value != "example.com" {
		t.Errorf("unexpected TurnRealm: %+v", r.TurnRealm)
	}

	// Check unset value
	if r.IMAPPort.IsSet {
		t.Errorf("expected IMAPPort to be unset, got %+v", r.IMAPPort)
	}
}

func TestAllSettingsHandler_MethodNotAllowed(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := AllSettingsHandler(deps)

	_, status, err := handler("POST", nil)
	if status != 405 {
		t.Errorf("expected 405, got %d", status)
	}
	if err == nil {
		t.Error("expected error for POST on bulk settings")
	}
}

// TestAllSettingKeys verifies all exported key constants have the __KEY__ format.
func TestAllSettingKeys(t *testing.T) {
	keys := []string{
		KeyRegistrationOpen,
		KeyJitRegistrationEnabled,
		KeyTurnEnabled,
		KeyLogDisabled,
		KeyIrohEnabled,
		KeySsEnabled,
		KeySMTPPort,
		KeySubmissionPort,
		KeyIMAPPort,
		KeyTurnPort,
		KeySaslPort,
		KeyIrohPort,
		KeySsPort,
		KeySMTPHostname,
		KeyTurnRealm,
		KeyTurnSecret,
		KeyTurnRelayIP,
		KeyTurnTTL,
		KeyIrohRelayURL,
		KeySsCipher,
		KeySsPassword,
	}

	for _, k := range keys {
		if len(k) < 5 || k[:2] != "__" || k[len(k)-2:] != "__" {
			t.Errorf("key %q does not follow __KEY__ convention", k)
		}
	}

	// Check for duplicates
	seen := map[string]bool{}
	for _, k := range keys {
		if seen[k] {
			t.Errorf("duplicate key: %s", k)
		}
		seen[k] = true
	}
}

// TestTurnHandler tests the TURN toggle handler.
func TestTurnHandler(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := TurnHandler(deps)

	// Default should be enabled
	resp, status, _ := handler("GET", nil)
	if status != 200 {
		t.Errorf("expected 200, got %d", status)
	}
	r := resp.(toggleStatusResponse)
	if r.Status != "enabled" {
		t.Errorf("expected 'enabled', got %q", r.Status)
	}

	// Disable
	body, _ := json.Marshal(actionRequest{Action: "disable"})
	resp, _, _ = handler("POST", body)
	r = resp.(toggleStatusResponse)
	if r.Status != "disabled" {
		t.Errorf("expected 'disabled', got %q", r.Status)
	}
}

// TestPortSettingsRoundTrip tests set/get/reset for all port setting keys.
func TestPortSettingsRoundTrip(t *testing.T) {
	portKeys := []string{
		KeySMTPPort,
		KeySubmissionPort,
		KeyIMAPPort,
		KeyTurnPort,
		KeySaslPort,
		KeyIrohPort,
		KeySsPort,
	}

	for _, key := range portKeys {
		t.Run(key, func(t *testing.T) {
			deps, _ := buildTestDeps()
			handler := GenericSettingHandler(key, deps)

			// Set
			body, _ := json.Marshal(settingValueRequest{Action: "set", Value: "8080"})
			resp, status, err := handler("POST", body)
			if err != nil {
				t.Fatalf("set failed: %v", err)
			}
			if status != 200 {
				t.Errorf("expected 200, got %d", status)
			}
			r := resp.(settingValueResponse)
			if r.Value != "8080" || !r.IsSet {
				t.Errorf("unexpected: %+v", r)
			}

			// Get
			resp, _, _ = handler("GET", nil)
			r = resp.(settingValueResponse)
			if r.Value != "8080" || !r.IsSet || r.Key != key {
				t.Errorf("GET failed: %+v", r)
			}

			// Reset
			body, _ = json.Marshal(settingValueRequest{Action: "reset"})
			resp, _, _ = handler("POST", body)
			r = resp.(settingValueResponse)
			if r.IsSet {
				t.Errorf("expected unset after reset: %+v", r)
			}

			// Verify reset
			resp, _, _ = handler("GET", nil)
			r = resp.(settingValueResponse)
			if r.IsSet {
				t.Errorf("expected unset after reset GET: %+v", r)
			}
		})
	}
}

// TestConfigSettingsRoundTrip tests set/get/reset for all config setting keys.
func TestConfigSettingsRoundTrip(t *testing.T) {
	configKeys := map[string]string{
		KeySMTPHostname: "mail.example.com",
		KeyTurnRealm:    "example.com",
		KeyTurnSecret:   "super-secret-key",
		KeyTurnRelayIP:  "203.0.113.1",
		KeyTurnTTL:      "3600",
		KeyIrohRelayURL: "https://iroh.example.com",
		KeySsCipher:     "AEAD_CHACHA20_POLY1305",
		KeySsPassword:   "my-password-123",
	}

	for key, testValue := range configKeys {
		t.Run(key, func(t *testing.T) {
			deps, _ := buildTestDeps()
			handler := GenericSettingHandler(key, deps)

			// Set
			body, _ := json.Marshal(settingValueRequest{Action: "set", Value: testValue})
			resp, status, err := handler("POST", body)
			if err != nil {
				t.Fatalf("set failed: %v", err)
			}
			if status != 200 {
				t.Errorf("expected 200, got %d", status)
			}
			r := resp.(settingValueResponse)
			if r.Value != testValue || !r.IsSet || r.Key != key {
				t.Errorf("unexpected response: %+v", r)
			}

			// Get
			resp, _, _ = handler("GET", nil)
			r = resp.(settingValueResponse)
			if r.Value != testValue || !r.IsSet {
				t.Errorf("GET failed: %+v", r)
			}

			// Reset
			body, _ = json.Marshal(settingValueRequest{Action: "reset"})
			handler("POST", body)

			resp, _, _ = handler("GET", nil)
			r = resp.(settingValueResponse)
			if r.IsSet {
				t.Errorf("expected unset after reset: %+v", r)
			}
		})
	}
}

// Ensure unused import doesn't cause issues
var _ = fmt.Sprintf

// TestValidateSettingValue_Injection tests that config injection attacks are blocked.
func TestValidateSettingValue_Injection(t *testing.T) {
	tests := []struct {
		name    string
		key     string
		value   string
		wantErr bool
	}{
		// Port injection attempts
		{"newline in port", KeySMTPPort, "25\n  malicious true", true},
		{"non-numeric port", KeySMTPPort, "abc", true},
		{"port too high", KeySMTPPort, "99999", true},
		{"port zero", KeySMTPPort, "0", true},
		{"port negative", KeySMTPPort, "-1", true},
		{"valid port", KeySMTPPort, "2525", false},
		{"valid port max", KeySMTPPort, "65535", false},
		{"valid port min", KeySMTPPort, "1", false},

		// Config value injection attempts
		{"newline injection in hostname", KeySMTPHostname, "host.com\n  evil true", true},
		{"null byte in hostname", KeySMTPHostname, "host.com\x00evil", true},
		{"carriage return in hostname", KeySMTPHostname, "host.com\revil", true},
		{"quote in ss_password", KeySsPassword, "pass\"injected", true},
		{"backslash in value", KeyTurnSecret, "secret\\path", true},
		{"spaces in value", KeyTurnRealm, "has space", true},

		// Valid values
		{"valid hostname", KeySMTPHostname, "mail.example.com", false},
		{"valid URL", KeyIrohRelayURL, "https://iroh.example.com", false},
		{"valid IP", KeyTurnRelayIP, "192.168.1.1", false},
		{"valid secret", KeyTurnSecret, "my-secret-key-42", false},
		{"valid cipher", KeySsCipher, "aes-128-gcm", false},
		{"valid password", KeySsPassword, "strong-pass-123", false},
		{"valid TTL", KeyTurnTTL, "3600", false},
		{"invalid TTL zero", KeyTurnTTL, "0", true},
		{"invalid TTL negative", KeyTurnTTL, "-10", true},
		{"invalid TTL non-numeric", KeyTurnTTL, "abc", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := validateSettingValue(tt.key, tt.value)
			if (err != nil) != tt.wantErr {
				t.Errorf("validateSettingValue(%q, %q) error = %v, wantErr %v",
					tt.key, tt.value, err, tt.wantErr)
			}
		})
	}
}

// TestGenericSettingHandler_RejectsInjection tests that the handler rejects injection via API.
func TestGenericSettingHandler_RejectsInjection(t *testing.T) {
	deps, _ := buildTestDeps()
	handler := GenericSettingHandler(KeySMTPPort, deps)

	// Try to inject via newline in port value
	body, _ := json.Marshal(settingValueRequest{Action: "set", Value: "25\n  evil true"})
	_, status, err := handler("POST", body)
	if status != 400 {
		t.Errorf("expected 400 for injection attempt, got %d", status)
	}
	if err == nil {
		t.Error("expected error for injection attempt")
	}

	// Ensure the value was NOT stored
	resp, _, _ := handler("GET", nil)
	r := resp.(settingValueResponse)
	if r.IsSet {
		t.Error("injection value should NOT have been stored")
	}
}
