# Implementation Plan: Registration Tokens (Deep Dive, Pruning & Late Validation)

This document provides a comprehensive roadmap for implementing registration tokens in Madmail, covering deferred consumption, auto-pruning, and the "Late Validation" corner case.

## 1. Database Schema (`internal/db/models.go`)

### Updated Models
```go
type RegistrationToken struct {
	Token      string    `gorm:"primaryKey"`
	MaxUses    int       `gorm:"column:max_uses;default:1"`
	UsedCount  int       `gorm:"column:used_count;default:0"` // Persisted successes
	Comment    string    `gorm:"column:comment"`
	CreatedAt  time.Time `gorm:"autoCreateTime"`
	ExpiresAt  *time.Time `gorm:"column:expires_at"`
}

type Quota struct {
	Username     string `gorm:"primaryKey"`
	MaxStorage   int64
	CreatedAt    int64
	FirstLoginAt int64  // 1 = Registered but never logged in; >1 = Logged in
	LastLoginAt  int64
	UsedToken    string `gorm:"column:used_token"` // The token used during /new
}
```

## 2. Admin API & Resources (`internal/api/admin/`)

### Resource Definitions (`internal/api/admin/resources/tokens.go`)
Create a new file for API data structures:
```go
type RegistrationToken struct {
	Token               string    `json:"token"`
	MaxUses             int       `json:"max_uses"`
	UsedCount           int       `json:"used_count"`
	PendingReservations int       `json:"pending_reservations"`
	Comment             string    `json:"comment"`
	CreatedAt           time.Time `json:"created_at"`
	ExpiresAt           *time.Time `json:"expires_at,omitempty"`
}
```

### Handler Registration (`internal/api/admin/admin.go`)
- In `setupAdminAPI`, register the `/registration-token` resource handler.
- Dispatch logic:
    - `GET`: Return all tokens (including calculated `PendingReservations`).
    - `POST`: Create or Update a token.
    - `DELETE`: Remove a token.

## 3. Advanced Token Logic (`internal/endpoint/chatmail/tokens.go`)

### `validateToken(token string) error`
- **Initial Verification**: Check existence, expiration, and `UsedCount < MaxUses`.
- **Reservation Check**: Count records in `Quota` where `UsedToken == token` AND `FirstLoginAt == 1`.
- **Total Load**: `UsedCount + pendingReservations`.
- **Strict Limit**: If `Total Load >= MaxUses`, return error. This prevents "over-reservation" during the 3-hour pending window.

### `consumeToken(token string) error`
- **Atomic Update**: `UPDATE registration_tokens SET used_count = used_count + 1 WHERE token = ? AND used_count < max_uses`.
- Returns error if the update affected 0 rows (e.g., token was deleted or reached its limit).

## 4. Account Lifecycle & Corner Cases

### Account Creation (`/new`)
- Perform strict `validateToken` check.
- informative Response: "Your account is reserved. Please login within 3 hours to activate. Usage of your registration token will be finalized upon first login."

### The "Late Validation" Corner Case (Delete on First Login)
Update `UpdateFirstLogin(username string)` in `internal/storage/imapsql/imapsql.go`:
1. If `quota.FirstLoginAt == 1` and `quota.UsedToken != ""`:
    1. Attempt `consumeToken(quota.UsedToken)`.
    2. **Case A: Success**: Token exists and has a slot. Increment `UsedCount`, clear `UsedToken` from quota, and set `FirstLoginAt`. Proceed with login.
    3. **Case B: Failure**: Token was deleted, expired, or all slots were taken by faster users after this account was reserved.
        - **Action**: **Delete the account immediately**. 
        - **Reason**: The "reservation" is no longer valid. The user "lost their slot".

## 5. CLI Subcommand (`internal/cli/ctl/registration_token.go`)

- **Commands**: `create`, `list`, `delete`, `status`.
- **Status details**: Show specific breakdown of `Consumed` vs `Pending (Reserved)` for every token.

## 6. Admin Web Interface Expansion

The Admin Web SPA (SvelteKit) will be updated with a dedicated "Registration Tokens" management suite.

### A. Token Overview Dashboard
- **Global Stats**: Summary cards for `Total Tokens`, `Active Reservations`, and `Exhausted Tokens`.
- **Live Health Monitor**: Real-time indication of how many users are currently in the "3-hour pending" window.

### B. Management Page Features
- **Token Table**:
    - **Usage Meter**: A visual progress bar showing `(Confirmed Usage + Pending Reservations) / Max Uses`.
    - **Status Badges**: `Active`, `Exhausted`, `Expired`, or `Deleted/Missing`.
    - **Expiration countdown**: Show "Expires in 2 days" or "Expired yesterday".
- **Action Menu**:
    - **Create Token**: Dialog with options for `Max Uses`, `Comment`, and custom or auto-generated `Token String`.
    - **Revoke/Delete**: Instant deletion of a token.
    - **Extend**: Easily update `ExpiresAt` or increase `MaxUses`.

### C. Account Details Integration
- **Account View**: When viewing a specific user account, show the "Registered via Token" field.
- **Pruning Status**: For accounts with `FirstLoginAt = 1`, show a countdown timer: "Auto-deletion in 2h 15m".

### D. System Settings Page
- **Toggle switch**: `Registration Token Required` (Global enable/disable).
- **Pruning Timer**: Field to adjust the `unused_account_retention`.

## 7. Testing Strategy

### Pruning & Late Validation Test
1. Create token with `max_uses: 1`.
2. Post `/new` with the token.
3. Delete the token via Admin API.
4. Perform first login for the account. Verify account is deleted and login fails.

## 8. Migration Plan
- Use GORM's `AutoMigrate` for the `RegistrationToken` table and the new `Quota.UsedToken` column.
- Integrate the setting `registration_token_required` into the existing settings system.
