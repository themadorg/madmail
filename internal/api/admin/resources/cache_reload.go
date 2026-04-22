package resources

import (
	"encoding/json"
	"fmt"

	"github.com/themadorg/madmail/framework/module"
)

// CacheReloadDeps wires optional cache reload implementations for the running process.
type CacheReloadDeps struct {
	AuthDB module.PlainUserDB
	// Storage may implement module.QuotaCacheReloader (e.g. storage.imapsql).
	Storage interface{}
}

type cacheReloadResponse struct {
	CredentialsCacheReloaded bool   `json:"credentials_cache_reloaded"`
	QuotaCacheReloaded       bool   `json:"quota_cache_reloaded"`
	Message                  string `json:"message"`
}

// CacheReloadHandler handles POST /admin/cache/reload.
// It refreshes in-memory caches that mirror on-disk state so tools like
// `maddy creds` / `maddy accounts` take effect without restarting the server.
func CacheReloadHandler(deps CacheReloadDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		_ = body
		if method != "POST" {
			return nil, 405, fmt.Errorf("method %s not allowed (use POST)", method)
		}

		var resp cacheReloadResponse
		if deps.AuthDB != nil {
			if r, ok := deps.AuthDB.(module.CredentialsCacheReloader); ok {
				if err := r.ReloadCredentialsCache(); err != nil {
					return nil, 500, fmt.Errorf("reload credentials cache: %w", err)
				}
				resp.CredentialsCacheReloaded = true
			}
		}
		if deps.Storage != nil {
			if r, ok := deps.Storage.(module.QuotaCacheReloader); ok {
				if err := r.ReloadQuotaCache(); err != nil {
					return nil, 500, fmt.Errorf("reload quota cache: %w", err)
				}
				resp.QuotaCacheReloaded = true
			}
		}

		switch {
		case resp.CredentialsCacheReloaded && resp.QuotaCacheReloaded:
			resp.Message = "Credentials and quota caches reloaded from database."
		case resp.CredentialsCacheReloaded:
			resp.Message = "Credentials cache reloaded from database."
		case resp.QuotaCacheReloaded:
			resp.Message = "Quota cache reloaded from database."
		default:
			resp.Message = "No reloadable caches in this configuration (auth or storage does not implement reload hooks)."
		}

		return resp, 200, nil
	}
}
