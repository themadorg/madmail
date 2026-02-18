package resources

import (
	"encoding/json"
	"fmt"
)

// ReloadDeps provides the callback for reloading the service configuration.
type ReloadDeps struct {
	// ReloadConfig is called to regenerate the config file and restart the service.
	// It reads port/config overrides from the DB, regenerates maddy.conf, and restarts.
	// Returns nil on success; the caller should expect the process to be terminated shortly after.
	ReloadConfig func() error
}

type reloadResponse struct {
	Status  string `json:"status"`
	Message string `json:"message"`
}

// ReloadHandler creates a handler for POST /admin/reload.
// This endpoint regenerates the configuration file from DB-stored overrides
// and triggers a service restart via SIGUSR2 (graceful reload).
func ReloadHandler(deps ReloadDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if method != "POST" {
			return nil, 405, fmt.Errorf("method %s not allowed (use POST)", method)
		}

		if deps.ReloadConfig == nil {
			return nil, 501, fmt.Errorf("reload not supported in this configuration")
		}

		if err := deps.ReloadConfig(); err != nil {
			return nil, 500, fmt.Errorf("reload failed: %v", err)
		}

		return reloadResponse{
			Status:  "reloading",
			Message: "Configuration regenerated. Service is restarting.",
		}, 200, nil
	}
}
