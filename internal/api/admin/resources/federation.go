package resources

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/themadorg/madmail/internal/federationtracker"
)

// FederationRuleEntry is a single domain rule for JSON serialization.
type FederationRuleEntry struct {
	Domain    string `json:"domain"`
	CreatedAt int64  `json:"created_at"`
}

// FederationRulesResponse is the response for GET /admin/federation/rules.
type FederationRulesResponse struct {
	Rules []FederationRuleEntry `json:"rules"`
	Total int                   `json:"total"`
}

// FederationRuleRequest is the request body for POST/DELETE /admin/federation/rules.
type FederationRuleRequest struct {
	Domain string `json:"domain"`
}

// FederationRulesHandler creates a handler for /admin/federation/rules.
// Supports GET (list), POST (add), and DELETE (remove) operations.
// All reads come from RAM (no DB query); writes are synchronous dual-writes.
func FederationRulesHandler() func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		ps := federationtracker.GlobalPolicy()

		switch method {
		case "GET":
			rules := ps.ListRules()
			entries := make([]FederationRuleEntry, 0, len(rules))
			for domain, createdAt := range rules {
				entries = append(entries, FederationRuleEntry{
					Domain:    domain,
					CreatedAt: createdAt,
				})
			}
			return FederationRulesResponse{
				Rules: entries,
				Total: len(entries),
			}, 200, nil

		case "POST":
			var req FederationRuleRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			domain := strings.TrimSpace(req.Domain)
			if domain == "" {
				return nil, 400, fmt.Errorf("domain is required")
			}
			count, err := ps.AddRule(domain)
			if err != nil {
				return nil, 500, fmt.Errorf("failed to add rule: %v", err)
			}
			return map[string]interface{}{
				"domain": domain,
				"total":  count,
			}, 200, nil

		case "DELETE":
			var req FederationRuleRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			domain := strings.TrimSpace(req.Domain)
			if domain == "" {
				return nil, 400, fmt.Errorf("domain is required")
			}
			remaining, err := ps.RemoveRule(domain)
			if err != nil {
				return nil, 500, fmt.Errorf("failed to remove rule: %v", err)
			}
			return map[string]interface{}{
				"domain":    domain,
				"remaining": remaining,
			}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// FederationPolicyHandler creates a handler for /admin/settings/federation.
// Uses the genericDBToggleHandler pattern for __FEDERATION_ENABLED__.
func FederationPolicyHandler(deps SettingsToggleDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			val, ok, err := deps.GetSetting(KeyFederationEnabled)
			if err != nil {
				return nil, 500, fmt.Errorf("failed to get setting: %v", err)
			}
			enabled := false
			if ok && val == "true" {
				enabled = true
			}
			// Also get the policy
			policy := "ACCEPT"
			if pval, pok, perr := deps.GetSetting(KeyFederationPolicy); perr == nil && pok && pval != "" {
				policy = pval
			}
			return map[string]interface{}{
				"enabled": enabled,
				"policy":  policy,
			}, 200, nil

		case "POST":
			var req struct {
				Enabled *bool  `json:"enabled,omitempty"`
				Policy  string `json:"policy,omitempty"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}

			if req.Enabled != nil {
				val := "false"
				if *req.Enabled {
					val = "true"
				}
				if err := deps.SetSetting(KeyFederationEnabled, val); err != nil {
					return nil, 500, fmt.Errorf("failed to set federation enabled: %v", err)
				}
			}

			if req.Policy != "" {
				p := strings.ToUpper(req.Policy)
				if p != "ACCEPT" && p != "REJECT" {
					return nil, 400, fmt.Errorf("invalid policy: %s (expected ACCEPT or REJECT)", req.Policy)
				}
				if err := deps.SetSetting(KeyFederationPolicy, p); err != nil {
					return nil, 500, fmt.Errorf("failed to set federation policy: %v", err)
				}
			}

			// Return current state
			enabled := false
			if val, ok, err := deps.GetSetting(KeyFederationEnabled); err == nil && ok {
				enabled = val == "true"
			}
			policy := "ACCEPT"
			if pval, pok, perr := deps.GetSetting(KeyFederationPolicy); perr == nil && pok && pval != "" {
				policy = pval
			}
			return map[string]interface{}{
				"enabled": enabled,
				"policy":  policy,
			}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}

// FederationServersResponse is the response for GET /admin/federation/servers.
type FederationServersResponse struct {
	Servers []FederationServerEntry `json:"servers"`
	Total   int                     `json:"total"`
}

// FederationServerEntry is a single server stat for JSON serialization.
type FederationServerEntry struct {
	Domain               string  `json:"domain"`
	QueuedMessages       int64   `json:"queued_messages"`
	FailedHTTP           int64   `json:"failed_http"`
	FailedHTTPS          int64   `json:"failed_https"`
	FailedSMTP           int64   `json:"failed_smtp"`
	SuccessHTTP          int64   `json:"success_http"`
	SuccessHTTPS         int64   `json:"success_https"`
	SuccessSMTP          int64   `json:"success_smtp"`
	SuccessfulDeliveries int64   `json:"successful_deliveries"`
	MeanLatencyMs        float64 `json:"mean_latency_ms"`
	LastActive           int64   `json:"last_active"`
}

// FederationServersHandler creates a handler for /admin/federation/servers.
// Reads directly from RAM via the FederationTracker, no DB hit.
func FederationServersHandler() func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if method != "GET" {
			return nil, 405, fmt.Errorf("method %s not allowed, use GET", method)
		}

		stats := federationtracker.Global().GetAll()
		entries := make([]FederationServerEntry, 0, len(stats))
		for _, s := range stats {
			meanLatency := float64(0)
			if s.SuccessfulDeliveries > 0 {
				meanLatency = float64(s.TotalLatencyMs) / float64(s.SuccessfulDeliveries)
			}
			entries = append(entries, FederationServerEntry{
				Domain:               s.Domain,
				QueuedMessages:       s.QueuedMessages,
				FailedHTTP:           s.FailedHTTP,
				FailedHTTPS:          s.FailedHTTPS,
				FailedSMTP:           s.FailedSMTP,
				SuccessHTTP:          s.SuccessHTTP,
				SuccessHTTPS:         s.SuccessHTTPS,
				SuccessSMTP:          s.SuccessSMTP,
				SuccessfulDeliveries: s.SuccessfulDeliveries,
				MeanLatencyMs:        meanLatency,
				LastActive:           s.LastActive,
			})
		}

		return FederationServersResponse{
			Servers: entries,
			Total:   len(entries),
		}, 200, nil
	}
}
