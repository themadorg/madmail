package chatmail

import (
	"errors"
	"fmt"
	"time"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// ErrTokenNotFound means the token does not exist.
var ErrTokenNotFound = errors.New("registration token not found")

// ErrTokenExpired means the token has passed its expiration date.
var ErrTokenExpired = errors.New("registration token has expired")

// ErrTokenExhausted means the token has reached its max_uses (including pending reservations).
var ErrTokenExhausted = errors.New("registration token has been fully used")

// validateToken checks whether a token is valid for a new registration:
// 1. Token must exist.
// 2. Token must not be expired.
// 3. UsedCount + pendingReservations < MaxUses (strict limit to prevent over-reservation).
func validateToken(db *gorm.DB, token string) error {
	var t mdb.RegistrationToken
	if err := db.Where("token = ?", token).First(&t).Error; err != nil {
		if errors.Is(err, gorm.ErrRecordNotFound) {
			return ErrTokenNotFound
		}
		return fmt.Errorf("failed to look up token: %w", err)
	}

	// Check expiration
	if t.ExpiresAt != nil && t.ExpiresAt.Before(time.Now()) {
		return ErrTokenExpired
	}

	// Check hard limit
	if t.UsedCount >= t.MaxUses {
		return ErrTokenExhausted
	}

	// Count pending reservations: Quota records where used_token matches
	// and first_login_at == 1 (registered but never logged in).
	var pendingReservations int64
	if err := db.Model(&mdb.Quota{}).
		Where("used_token = ? AND first_login_at = 1", token).
		Count(&pendingReservations).Error; err != nil {
		return fmt.Errorf("failed to count pending reservations: %w", err)
	}

	totalLoad := int64(t.UsedCount) + pendingReservations
	if totalLoad >= int64(t.MaxUses) {
		return ErrTokenExhausted
	}

	return nil
}

// consumeToken atomically increments used_count for a token, but only if
// used_count < max_uses. Returns an error if the update affected 0 rows
// (token deleted, expired, or reached its limit).
func consumeToken(db *gorm.DB, token string) error {
	result := db.Model(&mdb.RegistrationToken{}).
		Where("token = ? AND used_count < max_uses", token).
		Update("used_count", gorm.Expr("used_count + 1"))

	if result.Error != nil {
		return fmt.Errorf("failed to consume token: %w", result.Error)
	}
	if result.RowsAffected == 0 {
		return fmt.Errorf("token consumption failed: token may have been deleted or exhausted")
	}
	return nil
}

// isTokenRequired checks the settings table for the registration_token_required flag.
// Returns true if tokens are required for /new registrations.
func isTokenRequired(db *gorm.DB) bool {
	var entry mdb.TableEntry
	err := db.Table("passwords").
		Where("\"key\" = ?", "__REGISTRATION_TOKEN_REQUIRED__").
		First(&entry).Error
	if err != nil {
		return false // default: not required
	}
	return entry.Value == "true"
}

// getPendingReservations counts how many accounts are pending (registered
// but never logged in) for a given token.
func getPendingReservations(db *gorm.DB, token string) int64 {
	var count int64
	db.Model(&mdb.Quota{}).
		Where("used_token = ? AND first_login_at = 1", token).
		Count(&count)
	return count
}
