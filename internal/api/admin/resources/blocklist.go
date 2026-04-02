package resources

import (
	"encoding/json"
	"fmt"
	"time"

	"github.com/themadorg/madmail/framework/module"
)

// BlocklistDeps are the dependencies needed by the blocklist resource handler.
type BlocklistDeps struct {
	Storage module.ManageableStorage
}

type blocklistEntry struct {
	Username  string `json:"username"`
	Reason    string `json:"reason"`
	BlockedAt string `json:"blocked_at"`
}

type blocklistResponse struct {
	Blocked []blocklistEntry `json:"blocked"`
	Total   int              `json:"total"`
}

type blockRequest struct {
	Username string `json:"username"`
	Reason   string `json:"reason"`
}

// BlocklistHandler creates a handler for /admin/blocklist.
func BlocklistHandler(deps BlocklistDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			entries, err := deps.Storage.ListBlockedUsers()
			if err != nil {
				return nil, 500, fmt.Errorf("failed to list blocked users: %v", err)
			}
			blocked := make([]blocklistEntry, 0, len(entries))
			for _, e := range entries {
				blocked = append(blocked, blocklistEntry{
					Username:  e.Username,
					Reason:    e.Reason,
					BlockedAt: e.BlockedAt.Format(time.RFC3339),
				})
			}
			return blocklistResponse{
				Blocked: blocked,
				Total:   len(blocked),
			}, 200, nil

		case "POST":
			var req blockRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Username == "" {
				return nil, 400, fmt.Errorf("username is required")
			}
			if req.Reason == "" {
				req.Reason = "manually blocked"
			}
			if err := deps.Storage.BlockUser(req.Username, req.Reason); err != nil {
				return nil, 500, fmt.Errorf("failed to block user: %v", err)
			}
			return map[string]string{"blocked": req.Username}, 200, nil

		case "DELETE":
			var req blockRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Username == "" {
				return nil, 400, fmt.Errorf("username is required")
			}
			if err := deps.Storage.UnblockUser(req.Username); err != nil {
				return nil, 500, fmt.Errorf("failed to unblock user: %v", err)
			}
			return map[string]string{"unblocked": req.Username}, 200, nil

		case "PATCH":
			// Bulk operations
			var bulkReq struct {
				Action string `json:"action"`
			}
			if err := json.Unmarshal(body, &bulkReq); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}

			switch bulkReq.Action {
			case "delete_all":
				entries, err := deps.Storage.ListBlockedUsers()
				if err != nil {
					return nil, 500, fmt.Errorf("failed to list blocked users: %v", err)
				}
				unblocked := 0
				var errors []string
				for _, e := range entries {
					if err := deps.Storage.UnblockUser(e.Username); err != nil {
						errors = append(errors, fmt.Sprintf("%s: %v", e.Username, err))
						continue
					}
					unblocked++
				}
				resp := map[string]interface{}{"unblocked": unblocked}
				if len(errors) > 0 {
					resp["errors"] = errors
				}
				return resp, 200, nil
			default:
				return nil, 400, fmt.Errorf("unknown action: %s", bulkReq.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}
