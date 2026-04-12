package resources

import (
	"encoding/json"
	"errors"
	"fmt"
	"time"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// TokensDeps provides the DB connection for token management.
type TokensDeps struct {
	DB *gorm.DB
}

// Token API request/response types

type tokenCreateRequest struct {
	Token   string `json:"token"`    // Optional: auto-generated if empty
	MaxUses int    `json:"max_uses"` // Default: 1
	Comment string `json:"comment"`
	// ExpiresIn is a duration string like "72h" or "168h".
	// If set, the token expires after this duration from creation.
	ExpiresIn string `json:"expires_in,omitempty"`
	// ExpiresAt is an absolute expiration time.
	ExpiresAt *time.Time `json:"expires_at,omitempty"`
}

type tokenResponse struct {
	Token               string     `json:"token"`
	MaxUses             int        `json:"max_uses"`
	UsedCount           int        `json:"used_count"`
	PendingReservations int        `json:"pending_reservations"`
	Comment             string     `json:"comment"`
	CreatedAt           time.Time  `json:"created_at"`
	ExpiresAt           *time.Time `json:"expires_at,omitempty"`
	Status              string     `json:"status"` // "active", "exhausted", "expired"
}

type tokenListResponse struct {
	Tokens []tokenResponse `json:"tokens"`
	Total  int             `json:"total"`
}

type tokenDeleteRequest struct {
	Token string `json:"token"`
}

// TokensHandler creates a handler for /admin/registration-token.
// GET: List all tokens with pending reservation counts.
// POST: Create or update a token.
// DELETE: Remove a token.
func TokensHandler(deps TokensDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		switch method {
		case "GET":
			return handleTokensList(deps)
		case "POST":
			return handleTokenCreate(deps, body)
		case "DELETE":
			return handleTokenDelete(deps, body)
		default:
			return nil, 405, fmt.Errorf("method %s not allowed for /admin/registration-token", method)
		}
	}
}

func handleTokensList(deps TokensDeps) (interface{}, int, error) {
	var tokens []mdb.RegistrationToken
	if err := deps.DB.Order("created_at DESC").Find(&tokens).Error; err != nil {
		return nil, 500, fmt.Errorf("failed to list tokens: %v", err)
	}

	// Batch-fetch pending reservation counts in a single query instead of N+1
	type pendingCount struct {
		UsedToken string
		Count     int64
	}
	var pendingCounts []pendingCount
	deps.DB.Model(&mdb.Quota{}).
		Select("used_token, COUNT(*) as count").
		Where("used_token != '' AND first_login_at = 1").
		Group("used_token").
		Scan(&pendingCounts)

	pendingMap := make(map[string]int64, len(pendingCounts))
	for _, pc := range pendingCounts {
		pendingMap[pc.UsedToken] = pc.Count
	}

	now := time.Now()
	result := make([]tokenResponse, 0, len(tokens))
	for _, t := range tokens {
		pending := pendingMap[t.Token]

		status := "active"
		if t.ExpiresAt != nil && t.ExpiresAt.Before(now) {
			status = "expired"
		} else if t.UsedCount >= t.MaxUses {
			status = "exhausted"
		} else if int64(t.UsedCount)+pending >= int64(t.MaxUses) {
			status = "exhausted"
		}

		result = append(result, tokenResponse{
			Token:               t.Token,
			MaxUses:             t.MaxUses,
			UsedCount:           t.UsedCount,
			PendingReservations: int(pending),
			Comment:             t.Comment,
			CreatedAt:           t.CreatedAt,
			ExpiresAt:           t.ExpiresAt,
			Status:              status,
		})
	}

	return tokenListResponse{
		Tokens: result,
		Total:  len(result),
	}, 200, nil
}

func handleTokenCreate(deps TokensDeps, body json.RawMessage) (interface{}, int, error) {
	var req tokenCreateRequest
	if err := json.Unmarshal(body, &req); err != nil {
		return nil, 400, fmt.Errorf("invalid request body: %v", err)
	}

	// Auto-generate token if not provided
	if req.Token == "" {
		generated, err := generateRandomString(24)
		if err != nil {
			return nil, 500, fmt.Errorf("failed to generate token: %v", err)
		}
		req.Token = generated
	}

	if req.MaxUses <= 0 {
		req.MaxUses = 1
	}

	// Calculate expiration time
	var expiresAt *time.Time
	if req.ExpiresAt != nil {
		expiresAt = req.ExpiresAt
	} else if req.ExpiresIn != "" {
		dur, err := time.ParseDuration(req.ExpiresIn)
		if err != nil {
			return nil, 400, fmt.Errorf("invalid expires_in duration: %v", err)
		}
		t := time.Now().Add(dur)
		expiresAt = &t
	}

	// Check if token already exists — if so, update it
	var existing mdb.RegistrationToken
	err := deps.DB.Where("token = ?", req.Token).First(&existing).Error
	if err == nil {
		// Update existing token
		existing.MaxUses = req.MaxUses
		existing.Comment = req.Comment
		existing.ExpiresAt = expiresAt
		if err := deps.DB.Save(&existing).Error; err != nil {
			return nil, 500, fmt.Errorf("failed to update token: %v", err)
		}

		var pending int64
		deps.DB.Model(&mdb.Quota{}).
			Where("used_token = ? AND first_login_at = 1", req.Token).
			Count(&pending)

		return tokenResponse{
			Token:               existing.Token,
			MaxUses:             existing.MaxUses,
			UsedCount:           existing.UsedCount,
			PendingReservations: int(pending),
			Comment:             existing.Comment,
			CreatedAt:           existing.CreatedAt,
			ExpiresAt:           existing.ExpiresAt,
			Status:              "active",
		}, 200, nil
	}

	if !errors.Is(err, gorm.ErrRecordNotFound) {
		return nil, 500, fmt.Errorf("failed to check token: %v", err)
	}

	// Create new token
	token := mdb.RegistrationToken{
		Token:     req.Token,
		MaxUses:   req.MaxUses,
		Comment:   req.Comment,
		ExpiresAt: expiresAt,
	}

	if err := deps.DB.Create(&token).Error; err != nil {
		return nil, 500, fmt.Errorf("failed to create token: %v", err)
	}

	return tokenResponse{
		Token:     token.Token,
		MaxUses:   token.MaxUses,
		UsedCount: 0,
		Comment:   token.Comment,
		CreatedAt: token.CreatedAt,
		ExpiresAt: token.ExpiresAt,
		Status:    "active",
	}, 201, nil
}

func handleTokenDelete(deps TokensDeps, body json.RawMessage) (interface{}, int, error) {
	var req tokenDeleteRequest
	if err := json.Unmarshal(body, &req); err != nil {
		return nil, 400, fmt.Errorf("invalid request body: %v", err)
	}
	if req.Token == "" {
		return nil, 400, fmt.Errorf("token is required")
	}

	result := deps.DB.Where("token = ?", req.Token).Delete(&mdb.RegistrationToken{})
	if result.Error != nil {
		return nil, 500, fmt.Errorf("failed to delete token: %v", result.Error)
	}
	if result.RowsAffected == 0 {
		return nil, 404, fmt.Errorf("token not found")
	}

	return map[string]string{"deleted": req.Token}, 200, nil
}
