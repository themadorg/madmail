package resources

import (
	"encoding/json"
	"fmt"

	"github.com/themadorg/madmail/framework/module"
)

// AccountsDeps are the dependencies needed by the accounts resource handler.
type AccountsDeps struct {
	AuthDB  module.PlainUserDB
	Storage module.ManageableStorage
}

type accountEntry struct {
	Username string `json:"username"`
}

type accountListResponse struct {
	Accounts []accountEntry `json:"accounts"`
	Total    int            `json:"total"`
}

type deleteAccountRequest struct {
	Username string `json:"username"`
}

// AccountsHandler creates a handler for /admin/accounts.
func AccountsHandler(deps AccountsDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			users, err := deps.AuthDB.ListUsers()
			if err != nil {
				return nil, 500, fmt.Errorf("failed to list users: %v", err)
			}
			accounts := make([]accountEntry, len(users))
			for i, u := range users {
				accounts[i] = accountEntry{Username: u}
			}
			return accountListResponse{
				Accounts: accounts,
				Total:    len(accounts),
			}, 200, nil

		case "DELETE":
			var req deleteAccountRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Username == "" {
				return nil, 400, fmt.Errorf("username is required")
			}

			// Delete credentials
			if err := deps.AuthDB.DeleteUser(req.Username); err != nil {
				return nil, 500, fmt.Errorf("failed to delete user credentials: %v", err)
			}
			// Delete IMAP account and mailboxes
			if err := deps.Storage.DeleteIMAPAcct(req.Username); err != nil {
				// Log but don't fail — credentials are already deleted
				return map[string]interface{}{
					"deleted":    req.Username,
					"imap_error": err.Error(),
				}, 200, nil
			}

			return map[string]string{"deleted": req.Username}, 200, nil

		default:
			// Note: Account creation is deliberately excluded from the API.
			// See issue #34: passwords are never exposed or accepted through the API.
			return nil, 405, fmt.Errorf("method %s not allowed (account creation is via CLI or registration endpoint only)", method)
		}
	}
}
