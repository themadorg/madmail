package resources

import (
	"encoding/json"
	"fmt"

	"github.com/themadorg/madmail/internal/endpoint_cache"
)

// EndpointCacheDeps are the dependencies needed by the endpoint cache handler.
type EndpointCacheDeps struct {
	Cache *endpoint_cache.Cache
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
// All reads and writes go through the in-memory Cache so changes are
// immediately visible to the running server without a DB round-trip.
func EndpointCacheHandler(deps EndpointCacheDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if deps.Cache == nil {
			return nil, 503, fmt.Errorf("endpoint cache not available")
		}

		switch method {
		case "GET":
			overrides, err := deps.Cache.List()
			if err != nil {
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
			if err := deps.Cache.Set(req.LookupKey, req.TargetHost, req.Comment); err != nil {
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
			// Check existence first
			if _, err := deps.Cache.Get(req.LookupKey); err != nil {
				return nil, 404, fmt.Errorf("endpoint override not found: %s", req.LookupKey)
			}
			if err := deps.Cache.Delete(req.LookupKey); err != nil {
				return nil, 500, fmt.Errorf("failed to delete endpoint override: %v", err)
			}
			return map[string]string{"deleted": req.LookupKey}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// DNSCacheHandler is an alias for backward compatibility.
var DNSCacheHandler = EndpointCacheHandler
