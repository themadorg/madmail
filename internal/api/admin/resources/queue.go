package resources

import (
	"encoding/json"
	"fmt"

	"github.com/themadorg/madmail/framework/module"
)

// QueueDeps are the dependencies needed by the queue resource handler.
type QueueDeps struct {
	Storage module.ManageableStorage
}

type purgeResponse struct {
	Action  string `json:"action"`
	Message string `json:"message"`
}

// QueueHandler creates a handler for /admin/queue.
func QueueHandler(deps QueueDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "POST":
			var req struct {
				Action   string `json:"action"`
				Username string `json:"username,omitempty"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}

			switch req.Action {
			case "purge_user":
				if req.Username == "" {
					return nil, 400, fmt.Errorf("username is required for purge_user")
				}
				if err := deps.Storage.PurgeIMAPMsgs(req.Username); err != nil {
					return nil, 500, fmt.Errorf("failed to purge messages for %s: %v", req.Username, err)
				}
				return purgeResponse{
					Action:  "purge_user",
					Message: fmt.Sprintf("purged messages for user %s", req.Username),
				}, 200, nil

			case "purge_all":
				if err := deps.Storage.PurgeAllIMAPMsgs(); err != nil {
					return nil, 500, fmt.Errorf("failed to purge all messages: %v", err)
				}
				return purgeResponse{
					Action:  "purge_all",
					Message: "purged all messages",
				}, 200, nil

			case "purge_read":
				if err := deps.Storage.PurgeReadIMAPMsgs(); err != nil {
					return nil, 500, fmt.Errorf("failed to purge read messages: %v", err)
				}
				return purgeResponse{
					Action:  "purge_read",
					Message: "purged read messages",
				}, 200, nil

			default:
				return nil, 400, fmt.Errorf("unknown action: %s (expected purge_user|purge_all|purge_read)", req.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}
