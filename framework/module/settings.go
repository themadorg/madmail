package module

import "sync"

// SettingsProvider is a global registry where a module can register
// itself as the settings provider. Other modules can then look up
// settings by key during their initialization.
//
// This is used for port access control: the chatmail endpoint registers
// the auth DB as the settings provider, and SMTP/IMAP/TURN endpoints
// check if they should bind to localhost only.

var (
	settingsProviderMu sync.RWMutex
	settingsProvider   func(key string) (string, bool, error)
)

// RegisterSettingsProvider registers a function that other modules can use
// to look up settings. This should be called early in initialization
// (e.g. from the auth DB module's Init).
func RegisterSettingsProvider(fn func(key string) (string, bool, error)) {
	settingsProviderMu.Lock()
	defer settingsProviderMu.Unlock()
	settingsProvider = fn
}

// GetGlobalSetting reads a setting from the registered settings provider.
// Returns ("", false, nil) if no provider is registered or the key is not found.
func GetGlobalSetting(key string) (string, bool, error) {
	settingsProviderMu.RLock()
	defer settingsProviderMu.RUnlock()
	if settingsProvider == nil {
		return "", false, nil
	}
	return settingsProvider(key)
}

// IsLocalOnly checks if a setting key indicates local-only mode.
// Returns true if the setting value is "true".
func IsLocalOnly(key string) bool {
	val, ok, err := GetGlobalSetting(key)
	if err != nil || !ok {
		return false
	}
	return val == "true"
}
