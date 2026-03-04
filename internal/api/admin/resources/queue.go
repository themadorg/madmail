package resources

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
)

// QueueDeps are the dependencies needed by the queue resource handler.
type QueueDeps struct {
	Storage module.ManageableStorage
}

type purgeResponse struct {
	Action  string `json:"action"`
	Message string `json:"message"`
	Deleted int    `json:"deleted,omitempty"`
}

// messagesDir returns the path to the message blobs directory (state_dir/messages).
func messagesDir() string {
	return filepath.Join(config.StateDirectory, "messages")
}

// purgeAllBlobs deletes all files and subdirectories inside the messages directory.
func purgeAllBlobs() (int, error) {
	dir := messagesDir()
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil
		}
		return 0, err
	}
	deleted := 0
	for _, e := range entries {
		path := filepath.Join(dir, e.Name())
		if err := os.RemoveAll(path); err != nil {
			return deleted, fmt.Errorf("failed to remove %s: %v", e.Name(), err)
		}
		deleted++
	}
	return deleted, nil
}

// purgeBlobsOlderThan deletes files in the messages directory that are older than the given duration.
func purgeBlobsOlderThan(retention time.Duration) (int, error) {
	dir := messagesDir()
	cutoff := time.Now().Add(-retention)
	deleted := 0

	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // skip errors (e.g. permission)
		}
		if path == dir {
			return nil // skip root
		}
		if info.ModTime().Before(cutoff) {
			if info.IsDir() {
				os.RemoveAll(path)
			} else {
				os.Remove(path)
			}
			deleted++
			if info.IsDir() {
				return filepath.SkipDir
			}
		}
		return nil
	})
	return deleted, err
}

// QueueHandler creates a handler for /admin/queue.
func QueueHandler(deps QueueDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "POST":
			var req struct {
				Action    string `json:"action"`
				Username  string `json:"username,omitempty"`
				Retention string `json:"retention,omitempty"` // e.g. "72h", "168h", "24h"
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
					Message: "purged all messages from database",
				}, 200, nil

			case "purge_read":
				if err := deps.Storage.PurgeReadIMAPMsgs(); err != nil {
					return nil, 500, fmt.Errorf("failed to purge read messages: %v", err)
				}
				return purgeResponse{
					Action:  "purge_read",
					Message: "purged read messages from database",
				}, 200, nil

			case "purge_older":
				if req.Retention == "" {
					return nil, 400, fmt.Errorf("retention is required for purge_older (e.g. \"72h\", \"168h\")")
				}
				retention, err := time.ParseDuration(req.Retention)
				if err != nil {
					return nil, 400, fmt.Errorf("invalid retention format: %v (use Go duration like \"72h\", \"168h\")", err)
				}
				if retention <= 0 {
					return nil, 400, fmt.Errorf("retention must be positive")
				}
				if err := deps.Storage.PruneUnreadIMAPMsgs(retention); err != nil {
					return nil, 500, fmt.Errorf("failed to prune messages older than %v: %v", retention, err)
				}
				return purgeResponse{
					Action:  "purge_older",
					Message: fmt.Sprintf("pruned database messages older than %v", retention),
				}, 200, nil

			case "purge_blobs":
				// Delete all files from state_dir/messages/
				deleted, err := purgeAllBlobs()
				if err != nil {
					return nil, 500, fmt.Errorf("failed to purge message blobs: %v", err)
				}
				return purgeResponse{
					Action:  "purge_blobs",
					Message: fmt.Sprintf("deleted %d entries from %s", deleted, messagesDir()),
					Deleted: deleted,
				}, 200, nil

			case "purge_blobs_older":
				// Delete files older than retention from state_dir/messages/
				if req.Retention == "" {
					return nil, 400, fmt.Errorf("retention is required for purge_blobs_older (e.g. \"72h\", \"168h\")")
				}
				retention, err := time.ParseDuration(req.Retention)
				if err != nil {
					return nil, 400, fmt.Errorf("invalid retention format: %v", err)
				}
				if retention <= 0 {
					return nil, 400, fmt.Errorf("retention must be positive")
				}
				deleted, err := purgeBlobsOlderThan(retention)
				if err != nil {
					return nil, 500, fmt.Errorf("failed to purge old message blobs: %v", err)
				}
				return purgeResponse{
					Action:  "purge_blobs_older",
					Message: fmt.Sprintf("deleted %d entries older than %v from %s", deleted, retention, messagesDir()),
					Deleted: deleted,
				}, 200, nil

			default:
				return nil, 400, fmt.Errorf("unknown action: %s", req.Action)
			}

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}
