package chatmail

import (
	"testing"
	"time"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
)

func setupTestDB(t *testing.T) *gorm.DB {
	t.Helper()
	db, err := gorm.Open(sqlite.Open(":memory:"), &gorm.Config{})
	if err != nil {
		t.Fatalf("Failed to open test database: %v", err)
	}
	// Auto-migrate all tables needed for token tests
	if err := db.AutoMigrate(&mdb.RegistrationToken{}, &mdb.Quota{}, &mdb.TableEntry{}); err != nil {
		t.Fatalf("Failed to migrate tables: %v", err)
	}
	return db
}

func TestValidateToken_NotFound(t *testing.T) {
	db := setupTestDB(t)
	err := validateToken(db, "nonexistent-token")
	if err == nil {
		t.Fatal("expected error for nonexistent token, got nil")
	}
	if err != ErrTokenNotFound {
		t.Errorf("expected ErrTokenNotFound, got: %v", err)
	}
}

func TestValidateToken_Expired(t *testing.T) {
	db := setupTestDB(t)
	past := time.Now().Add(-1 * time.Hour)
	db.Create(&mdb.RegistrationToken{
		Token:     "expired-token",
		MaxUses:   10,
		ExpiresAt: &past,
	})

	err := validateToken(db, "expired-token")
	if err == nil {
		t.Fatal("expected error for expired token, got nil")
	}
	if err != ErrTokenExpired {
		t.Errorf("expected ErrTokenExpired, got: %v", err)
	}
}

func TestValidateToken_Exhausted_HardLimit(t *testing.T) {
	db := setupTestDB(t)
	db.Create(&mdb.RegistrationToken{
		Token:     "used-up-token",
		MaxUses:   2,
		UsedCount: 2,
	})

	err := validateToken(db, "used-up-token")
	if err == nil {
		t.Fatal("expected error for exhausted token, got nil")
	}
	if err != ErrTokenExhausted {
		t.Errorf("expected ErrTokenExhausted, got: %v", err)
	}
}

func TestValidateToken_Exhausted_WithPendingReservations(t *testing.T) {
	db := setupTestDB(t)
	db.Create(&mdb.RegistrationToken{
		Token:     "pending-token",
		MaxUses:   2,
		UsedCount: 1,
	})

	// Create a pending reservation (registered but never logged in)
	db.Create(&mdb.Quota{
		Username:     "user1@example.com",
		CreatedAt:    time.Now().Unix(),
		FirstLoginAt: 1,
		UsedToken:    "pending-token",
	})

	err := validateToken(db, "pending-token")
	if err == nil {
		t.Fatal("expected error for token at capacity with pending reservations, got nil")
	}
	if err != ErrTokenExhausted {
		t.Errorf("expected ErrTokenExhausted, got: %v", err)
	}
}

func TestValidateToken_Success(t *testing.T) {
	db := setupTestDB(t)
	db.Create(&mdb.RegistrationToken{
		Token:     "valid-token",
		MaxUses:   5,
		UsedCount: 2,
	})

	// One pending reservation
	db.Create(&mdb.Quota{
		Username:     "user1@example.com",
		CreatedAt:    time.Now().Unix(),
		FirstLoginAt: 1,
		UsedToken:    "valid-token",
	})

	// UsedCount(2) + pending(1) = 3 < 5 = MaxUses
	err := validateToken(db, "valid-token")
	if err != nil {
		t.Fatalf("expected no error for valid token, got: %v", err)
	}
}

func TestValidateToken_NotExpired(t *testing.T) {
	db := setupTestDB(t)
	future := time.Now().Add(24 * time.Hour)
	db.Create(&mdb.RegistrationToken{
		Token:     "future-token",
		MaxUses:   5,
		ExpiresAt: &future,
	})

	err := validateToken(db, "future-token")
	if err != nil {
		t.Fatalf("expected no error for non-expired token, got: %v", err)
	}
}

func TestConsumeToken_Success(t *testing.T) {
	db := setupTestDB(t)
	db.Create(&mdb.RegistrationToken{
		Token:     "consume-me",
		MaxUses:   3,
		UsedCount: 1,
	})

	err := consumeToken(db, "consume-me")
	if err != nil {
		t.Fatalf("expected no error, got: %v", err)
	}

	// Verify increment
	var token mdb.RegistrationToken
	db.Where("token = ?", "consume-me").First(&token)
	if token.UsedCount != 2 {
		t.Errorf("expected UsedCount to be 2, got %d", token.UsedCount)
	}
}

func TestConsumeToken_AlreadyExhausted(t *testing.T) {
	db := setupTestDB(t)
	db.Create(&mdb.RegistrationToken{
		Token:     "full-token",
		MaxUses:   1,
		UsedCount: 1,
	})

	err := consumeToken(db, "full-token")
	if err == nil {
		t.Fatal("expected error for exhausted token, got nil")
	}
}

func TestConsumeToken_Nonexistent(t *testing.T) {
	db := setupTestDB(t)

	err := consumeToken(db, "no-such-token")
	if err == nil {
		t.Fatal("expected error for nonexistent token, got nil")
	}
}

func TestConsumeToken_AtomicIncrement(t *testing.T) {
	db := setupTestDB(t)
	db.Create(&mdb.RegistrationToken{
		Token:     "atomic-test",
		MaxUses:   3,
		UsedCount: 0,
	})

	// Consume 3 times — should succeed 3 times
	for i := 0; i < 3; i++ {
		if err := consumeToken(db, "atomic-test"); err != nil {
			t.Fatalf("consume #%d failed: %v", i+1, err)
		}
	}

	// 4th should fail
	if err := consumeToken(db, "atomic-test"); err == nil {
		t.Fatal("expected 4th consume to fail, got nil")
	}

	var token mdb.RegistrationToken
	db.Where("token = ?", "atomic-test").First(&token)
	if token.UsedCount != 3 {
		t.Errorf("expected UsedCount to be 3, got %d", token.UsedCount)
	}
}

func TestIsTokenRequired_Default(t *testing.T) {
	db := setupTestDB(t)

	// Create a "passwords" table to simulate the settings table
	db.Exec("CREATE TABLE IF NOT EXISTS passwords (key TEXT PRIMARY KEY, value TEXT NOT NULL)")

	if isTokenRequired(db) {
		t.Error("expected isTokenRequired to return false by default")
	}
}

func TestIsTokenRequired_Enabled(t *testing.T) {
	db := setupTestDB(t)

	db.Exec("CREATE TABLE IF NOT EXISTS passwords (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
	db.Exec("INSERT INTO passwords (key, value) VALUES (?, ?)", "__REGISTRATION_TOKEN_REQUIRED__", "true")

	if !isTokenRequired(db) {
		t.Error("expected isTokenRequired to return true when enabled")
	}
}

func TestIsTokenRequired_DisabledExplicitly(t *testing.T) {
	db := setupTestDB(t)

	db.Exec("CREATE TABLE IF NOT EXISTS passwords (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
	db.Exec("INSERT INTO passwords (key, value) VALUES (?, ?)", "__REGISTRATION_TOKEN_REQUIRED__", "false")

	if isTokenRequired(db) {
		t.Error("expected isTokenRequired to return false when explicitly disabled")
	}
}

func TestGetPendingReservations(t *testing.T) {
	db := setupTestDB(t)

	// No reservations
	count := getPendingReservations(db, "some-token")
	if count != 0 {
		t.Errorf("expected 0 pending reservations, got %d", count)
	}

	// Add 2 pending + 1 consumed
	db.Create(&mdb.Quota{Username: "a@x.com", FirstLoginAt: 1, UsedToken: "some-token", CreatedAt: time.Now().Unix()})
	db.Create(&mdb.Quota{Username: "b@x.com", FirstLoginAt: 1, UsedToken: "some-token", CreatedAt: time.Now().Unix()})
	db.Create(&mdb.Quota{Username: "c@x.com", FirstLoginAt: time.Now().Unix(), UsedToken: "", CreatedAt: time.Now().Unix()}) // already consumed

	count = getPendingReservations(db, "some-token")
	if count != 2 {
		t.Errorf("expected 2 pending reservations, got %d", count)
	}

	// Different token should not count
	count = getPendingReservations(db, "other-token")
	if count != 0 {
		t.Errorf("expected 0 pending reservations for other token, got %d", count)
	}
}

func TestValidateToken_PendingNotCounted_WhenAlreadyLoggedIn(t *testing.T) {
	db := setupTestDB(t)
	db.Create(&mdb.RegistrationToken{
		Token:     "mixed-token",
		MaxUses:   2,
		UsedCount: 1,
	})

	// One user with first_login_at > 1 (already logged in, consumed) — should not count as pending
	db.Create(&mdb.Quota{
		Username:     "logged-in@example.com",
		CreatedAt:    time.Now().Unix(),
		FirstLoginAt: time.Now().Unix(),
		UsedToken:    "", // cleared after consumption
	})

	// UsedCount(1) + pending(0) = 1 < 2 = MaxUses
	err := validateToken(db, "mixed-token")
	if err != nil {
		t.Fatalf("expected no error, got: %v", err)
	}
}
