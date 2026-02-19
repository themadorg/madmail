package resources

import (
	"encoding/json"
	"fmt"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// EndpointCacheDeps are the dependencies needed by the endpoint cache handler.
type EndpointCacheDeps struct {
	DB *gorm.DB
}

// DNSCacheDeps is an alias for backward compatibility.
type DNSCacheDeps = EndpointCacheDeps

type endpointEntry struct {
	LookupKey  string `json:"lookup_key"`
	TargetHost string `json:"target_host"`
	Comment    string `json:"comment,omitempty"`
}

type endpointListResponse struct {
	Overrides []endpointEntry `json:"overrides"`
	Total     int             `json:"total"`
}

// EndpointCacheHandler creates a handler for /admin/endpoint-cache (and /admin/dns for backward compat).
func EndpointCacheHandler(deps EndpointCacheDeps) func(string, json.RawMessage) (interface{}, int, error) {
	// Ensure the dns_overrides table exists
	if deps.DB != nil {
		_ = deps.DB.AutoMigrate(&mdb.EndpointOverride{})
	}

	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if deps.DB == nil {
			return nil, 503, fmt.Errorf("endpoint cache database not available")
		}

		switch method {
		case "GET":
			var overrides []mdb.EndpointOverride
			if err := deps.DB.Find(&overrides).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to list endpoint overrides: %v", err)
			}
			entries := make([]endpointEntry, len(overrides))
			for i, o := range overrides {
				entries[i] = endpointEntry{
					LookupKey:  o.LookupKey,
					TargetHost: o.TargetHost,
					Comment:    o.Comment,
				}
			}
			return endpointListResponse{Overrides: entries, Total: len(entries)}, 200, nil

		case "POST":
			var req struct {
				LookupKey  string `json:"lookup_key"`
				TargetHost string `json:"target_host"`
				Comment    string `json:"comment,omitempty"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.LookupKey == "" || req.TargetHost == "" {
				return nil, 400, fmt.Errorf("lookup_key and target_host are required")
			}
			override := mdb.EndpointOverride{
				LookupKey:  req.LookupKey,
				TargetHost: req.TargetHost,
				Comment:    req.Comment,
			}
			if err := deps.DB.Save(&override).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to save endpoint override: %v", err)
			}
			return endpointEntry{
				LookupKey:  req.LookupKey,
				TargetHost: req.TargetHost,
				Comment:    req.Comment,
			}, 201, nil

		case "DELETE":
			var req struct {
				LookupKey string `json:"lookup_key"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.LookupKey == "" {
				return nil, 400, fmt.Errorf("lookup_key is required")
			}
			result := deps.DB.Where("lookup_key = ?", req.LookupKey).Delete(&mdb.EndpointOverride{})
			if result.Error != nil {
				return nil, 500, fmt.Errorf("failed to delete endpoint override: %v", result.Error)
			}
			if result.RowsAffected == 0 {
				return nil, 404, fmt.Errorf("endpoint override not found: %s", req.LookupKey)
			}
			return map[string]string{"deleted": req.LookupKey}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// DNSCacheHandler is an alias for backward compatibility.
var DNSCacheHandler = EndpointCacheHandler
