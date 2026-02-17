package resources

import (
	"encoding/json"
	"fmt"

	"github.com/themadorg/madmail/framework/module"
)

// QuotaDeps are the dependencies needed by the quota resource handler.
type QuotaDeps struct {
	Storage module.ManageableStorage
}

type quotaGetRequest struct {
	Username string `json:"username,omitempty"`
}

type quotaSetRequest struct {
	Username string `json:"username,omitempty"`
	MaxBytes int64  `json:"max_bytes"`
}

type quotaUserResponse struct {
	Username  string `json:"username"`
	UsedBytes int64  `json:"used_bytes"`
	MaxBytes  int64  `json:"max_bytes"`
	IsDefault bool   `json:"is_default"`
}

type quotaDefaultResponse struct {
	DefaultQuota int64 `json:"default_quota_bytes"`
}

type quotaStatsResponse struct {
	TotalStorageBytes int64 `json:"total_storage_bytes"`
	AccountsCount     int   `json:"accounts_count"`
	DefaultQuotaBytes int64 `json:"default_quota_bytes"`
}

// QuotaHandler creates a handler for /admin/quota.
func QuotaHandler(deps QuotaDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			// If body has username, get specific user quota; otherwise return stats
			var req quotaGetRequest
			if len(body) > 0 {
				_ = json.Unmarshal(body, &req)
			}

			if req.Username != "" {
				used, max, isDefault, err := deps.Storage.GetQuota(req.Username)
				if err != nil {
					return nil, 500, fmt.Errorf("failed to get quota: %v", err)
				}
				return quotaUserResponse{
					Username:  req.Username,
					UsedBytes: used,
					MaxBytes:  max,
					IsDefault: isDefault,
				}, 200, nil
			}

			// Return overall stats
			totalStorage, accountsCount, err := deps.Storage.GetStat()
			if err != nil {
				return nil, 500, fmt.Errorf("failed to get storage stats: %v", err)
			}
			return quotaStatsResponse{
				TotalStorageBytes: totalStorage,
				AccountsCount:     accountsCount,
				DefaultQuotaBytes: deps.Storage.GetDefaultQuota(),
			}, 200, nil

		case "PUT":
			var req quotaSetRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}

			if req.Username != "" {
				// Set user-specific quota
				if err := deps.Storage.SetQuota(req.Username, req.MaxBytes); err != nil {
					return nil, 500, fmt.Errorf("failed to set quota: %v", err)
				}
				return map[string]interface{}{
					"username":  req.Username,
					"max_bytes": req.MaxBytes,
				}, 200, nil
			}

			// Set default quota
			if err := deps.Storage.SetDefaultQuota(req.MaxBytes); err != nil {
				return nil, 500, fmt.Errorf("failed to set default quota: %v", err)
			}
			return quotaDefaultResponse{DefaultQuota: req.MaxBytes}, 200, nil

		case "DELETE":
			// Reset a user's quota to default
			var req struct {
				Username string `json:"username"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Username == "" {
				return nil, 400, fmt.Errorf("username is required")
			}
			if err := deps.Storage.ResetQuota(req.Username); err != nil {
				return nil, 500, fmt.Errorf("failed to reset quota: %v", err)
			}
			return map[string]string{"reset": req.Username}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}
