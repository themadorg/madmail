package resources

import (
	"crypto/rand"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/themadorg/madmail/framework/module"
)

// AccountsDeps are the dependencies needed by the accounts resource handler.
type AccountsDeps struct {
	AuthDB     module.PlainUserDB
	Storage    module.ManageableStorage
	MailDomain string // Domain for email addresses (e.g., example.com)
}

type accountEntry struct {
	Username       string `json:"username"`
	UsedBytes      int64  `json:"used_bytes"`
	MaxBytes       int64  `json:"max_bytes"`
	IsDefaultQuota bool   `json:"is_default_quota"`
	CreatedAt      int64  `json:"created_at"`
	FirstLoginAt   int64  `json:"first_login_at"`
	LastLoginAt    int64  `json:"last_login_at"`
}

type accountListResponse struct {
	Accounts []accountEntry `json:"accounts"`
	Total    int            `json:"total"`
}

type deleteAccountRequest struct {
	Username string `json:"username"`
}

type createAccountResponse struct {
	Email    string `json:"email"`
	Password string `json:"password"`
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
			// Fetch per-user storage usage in a single query
			usageMap, _ := deps.Storage.GetAllUsedStorage() // non-fatal if it fails
			if usageMap == nil {
				usageMap = make(map[string]int64)
			}
			// Fetch per-user account dates in a single query
			infoMap, _ := deps.Storage.GetAllAccountInfo()
			if infoMap == nil {
				infoMap = make(map[string]module.AccountInfo)
			}
			accounts := make([]accountEntry, 0, len(users))
			for _, u := range users {
				// Skip internal settings keys stored in the same DB
				if strings.HasPrefix(u, "__") && strings.HasSuffix(u, "__") {
					continue
				}
				info := infoMap[u]
				used, max, isDefault, _ := deps.Storage.GetQuota(u)
				accounts = append(accounts, accountEntry{
					Username:       u,
					UsedBytes:      used,
					MaxBytes:       max,
					IsDefaultQuota: isDefault,
					CreatedAt:      info.CreatedAt,
					FirstLoginAt:   info.FirstLoginAt,
					LastLoginAt:    info.LastLoginAt,
				})
			}
			return accountListResponse{
				Accounts: accounts,
				Total:    len(accounts),
			}, 200, nil

		case "POST":
			// Admin-only account creation. Bypasses registration status check.
			// This allows admins to create accounts even when registration is closed.
			if deps.MailDomain == "" {
				return nil, 503, fmt.Errorf("account creation not configured (no mail domain)")
			}

			const maxAttempts = 5
			for attempt := 0; attempt < maxAttempts; attempt++ {
				username, err := generateRandomString(12)
				if err != nil {
					return nil, 500, fmt.Errorf("failed to generate username: %v", err)
				}

				password, err := generateRandomPassword(24)
				if err != nil {
					return nil, 500, fmt.Errorf("failed to generate password: %v", err)
				}

				email := username + "@" + deps.MailDomain

				// Check blocklist
				if blocked, _ := deps.Storage.IsBlocked(email); blocked {
					continue // retry
				}

				// Create user in auth DB (uses bcrypt by default via CreateUser)
				if err := deps.AuthDB.CreateUser(email, password); err != nil {
					if strings.Contains(err.Error(), "already exist") {
						continue // retry
					}
					return nil, 500, fmt.Errorf("failed to create user: %v", err)
				}

				// Create IMAP account
				if err := deps.Storage.CreateIMAPAcct(email); err != nil {
					// Clean up auth entry
					_ = deps.AuthDB.DeleteUser(email)
					return nil, 500, fmt.Errorf("failed to create IMAP account: %v", err)
				}

				return createAccountResponse{
					Email:    email,
					Password: password,
				}, 201, nil
			}

			return nil, 500, fmt.Errorf("failed to create account after max retries")

		case "DELETE":
			var req deleteAccountRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Username == "" {
				return nil, 400, fmt.Errorf("username is required")
			}

			// Step 1: Delete credentials from AuthDB
			if err := deps.AuthDB.DeleteUser(req.Username); err != nil {
				// Log but continue — storage/blocklist cleanup is more important
				_ = err
			}

			// Step 2: Full storage cleanup + block via DeleteAccount
			// This deletes IMAP account, quota record, and adds to blocklist
			if err := deps.Storage.DeleteAccount(req.Username, "deleted via admin panel"); err != nil {
				return nil, 500, fmt.Errorf("failed to fully delete account: %v", err)
			}

			return map[string]string{"deleted": req.Username}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed for /admin/accounts", method)
		}
	}
}

// generateRandomString generates a random alphanumeric string for usernames.
func generateRandomString(length int) (string, error) {
	const charset = "abcdefghijklmnopqrstuvwxyz0123456789"
	b := make([]byte, length)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	for i := range b {
		b[i] = charset[b[i]%byte(len(charset))]
	}
	return string(b), nil
}

// generateRandomPassword generates a random password with mixed characters.
func generateRandomPassword(length int) (string, error) {
	const charset = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=[]{}|;:,.<>?"
	b := make([]byte, length)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	for i := range b {
		b[i] = charset[b[i]%byte(len(charset))]
	}
	return string(b), nil
}
