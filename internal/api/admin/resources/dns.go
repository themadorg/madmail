package resources

import (
	"encoding/json"
	"fmt"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// DNSCacheDeps are the dependencies needed by the DNS cache handler.
type DNSCacheDeps struct {
	DB *gorm.DB
}

type dnsEntry struct {
	LookupKey  string `json:"lookup_key"`
	TargetHost string `json:"target_host"`
	Comment    string `json:"comment,omitempty"`
}

type dnsListResponse struct {
	Overrides []dnsEntry `json:"overrides"`
	Total     int        `json:"total"`
}

// DNSCacheHandler creates a handler for /admin/dns.
func DNSCacheHandler(deps DNSCacheDeps) func(string, json.RawMessage) (interface{}, int, error) {
	// Ensure the dns_overrides table exists
	if deps.DB != nil {
		_ = deps.DB.AutoMigrate(&mdb.DNSOverride{})
	}

	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if deps.DB == nil {
			return nil, 503, fmt.Errorf("DNS cache database not available")
		}

		switch method {
		case "GET":
			var overrides []mdb.DNSOverride
			if err := deps.DB.Find(&overrides).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to list DNS overrides: %v", err)
			}
			entries := make([]dnsEntry, len(overrides))
			for i, o := range overrides {
				entries[i] = dnsEntry{
					LookupKey:  o.LookupKey,
					TargetHost: o.TargetHost,
					Comment:    o.Comment,
				}
			}
			return dnsListResponse{Overrides: entries, Total: len(entries)}, 200, nil

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
			override := mdb.DNSOverride{
				LookupKey:  req.LookupKey,
				TargetHost: req.TargetHost,
				Comment:    req.Comment,
			}
			if err := deps.DB.Save(&override).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to save DNS override: %v", err)
			}
			return dnsEntry{
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
			result := deps.DB.Where("lookup_key = ?", req.LookupKey).Delete(&mdb.DNSOverride{})
			if result.Error != nil {
				return nil, 500, fmt.Errorf("failed to delete DNS override: %v", result.Error)
			}
			if result.RowsAffected == 0 {
				return nil, 404, fmt.Errorf("DNS override not found: %s", req.LookupKey)
			}
			return map[string]string{"deleted": req.LookupKey}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}
