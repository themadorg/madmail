package module

import "sync"

// SettingsProvider is a global registry where a module can register
// itself as the settings provider. Other modules can then look up
// settings by key during their initialization.
//
// This is used for port access control: the auth DB registers itself
// as the settings provider, and SMTP/IMAP/TURN/chatmail endpoints
// check if they should bind to localhost only.
//
// The two-phase registration solves init-order issues:
// 1. RegisterSettingsProviderInstance(name) is called in the module factory
//    (before Init), recording WHICH module will provide settings.
// 2. RegisterSettingsProvider(fn) is called in the module's Init(),
//    registering the actual lookup function.
// 3. IsLocalOnly() triggers lazy Init of the named instance if the
//    provider function hasn't been registered yet.

var (
	settingsProviderMu       sync.RWMutex
	settingsProvider         func(key string) (string, bool, error)
	settingsProviderInstance string
)

// RegisterSettingsProviderInstance records which module instance will
// provide settings. Call this from the module factory (New) so the
// instance name is known before any Init() runs.
func RegisterSettingsProviderInstance(instName string) {
	settingsProviderMu.Lock()
	defer settingsProviderMu.Unlock()
	settingsProviderInstance = instName
}

// RegisterSettingsProvider registers the actual lookup function.
// Call this from the module's Init().
func RegisterSettingsProvider(fn func(key string) (string, bool, error)) {
	settingsProviderMu.Lock()
	defer settingsProviderMu.Unlock()
	settingsProvider = fn
}

// ensureSettingsProvider triggers lazy initialization of the settings
// provider module if it hasn't been initialized yet.
func ensureSettingsProvider() {
	settingsProviderMu.RLock()
	provider := settingsProvider
	instName := settingsProviderInstance
	settingsProviderMu.RUnlock()

	if provider != nil || instName == "" {
		return
	}

	// Trigger lazy Init of the settings module via GetInstance.
	// GetInstance calls Init() which calls RegisterSettingsProvider().
	_, _ = GetInstance(instName)
}

// GetGlobalSetting reads a setting from the registered settings provider.
// Returns ("", false, nil) if no provider is registered or the key is not found.
func GetGlobalSetting(key string) (string, bool, error) {
	settingsProviderMu.RLock()
	provider := settingsProvider
	settingsProviderMu.RUnlock()

	if provider == nil {
		ensureSettingsProvider()
		settingsProviderMu.RLock()
		provider = settingsProvider
		settingsProviderMu.RUnlock()
	}

	if provider == nil {
		return "", false, nil
	}
	return provider(key)
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
