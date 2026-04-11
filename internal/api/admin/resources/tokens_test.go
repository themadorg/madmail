package resources

import (
	"encoding/json"
	"testing"
	"time"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
)

func setupTokensTestDB(t *testing.T) *gorm.DB {
	t.Helper()
	db, err := gorm.Open(sqlite.Open(":memory:"), &gorm.Config{})
	if err != nil {
		t.Fatalf("Failed to open test database: %v", err)
	}
	if err := db.AutoMigrate(&mdb.RegistrationToken{}, &mdb.Quota{}); err != nil {
		t.Fatalf("Failed to migrate tables: %v", err)
	}
	return db
}

func TestTokensHandler_ListEmpty(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	result, status, err := handler("GET", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected status 200, got %d", status)
	}

	resp, ok := result.(tokenListResponse)
	if !ok {
		t.Fatalf("expected tokenListResponse, got %T", result)
	}
	if resp.Total != 0 {
		t.Errorf("expected 0 tokens, got %d", resp.Total)
	}
}

func TestTokensHandler_CreateWithAutoGenerate(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	body, _ := json.Marshal(tokenCreateRequest{
		MaxUses: 5,
		Comment: "test token",
	})

	result, status, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 201 {
		t.Errorf("expected status 201, got %d", status)
	}

	resp, ok := result.(tokenResponse)
	if !ok {
		t.Fatalf("expected tokenResponse, got %T", result)
	}
	if resp.Token == "" {
		t.Error("expected auto-generated token, got empty")
	}
	if resp.MaxUses != 5 {
		t.Errorf("expected MaxUses=5, got %d", resp.MaxUses)
	}
	if resp.Comment != "test token" {
		t.Errorf("expected Comment='test token', got '%s'", resp.Comment)
	}
	if resp.Status != "active" {
		t.Errorf("expected Status='active', got '%s'", resp.Status)
	}
}

func TestTokensHandler_CreateWithCustomToken(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	body, _ := json.Marshal(tokenCreateRequest{
		Token:   "my-custom-token",
		MaxUses: 10,
	})

	result, status, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 201 {
		t.Errorf("expected status 201, got %d", status)
	}

	resp := result.(tokenResponse)
	if resp.Token != "my-custom-token" {
		t.Errorf("expected token 'my-custom-token', got '%s'", resp.Token)
	}
}

func TestTokensHandler_CreateWithExpiration(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	body, _ := json.Marshal(tokenCreateRequest{
		Token:     "expiring-token",
		MaxUses:   1,
		ExpiresIn: "72h",
	})

	result, status, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 201 {
		t.Errorf("expected status 201, got %d", status)
	}

	resp := result.(tokenResponse)
	if resp.ExpiresAt == nil {
		t.Fatal("expected ExpiresAt to be set, got nil")
	}
	expected := time.Now().Add(72 * time.Hour)
	diff := resp.ExpiresAt.Sub(expected)
	if diff < -time.Minute || diff > time.Minute {
		t.Errorf("expected ExpiresAt to be ~72h from now, got %v (diff: %v)", resp.ExpiresAt, diff)
	}
}

func TestTokensHandler_UpdateExisting(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	// Create initial token
	db.Create(&mdb.RegistrationToken{
		Token:     "update-me",
		MaxUses:   1,
		UsedCount: 0,
		Comment:   "original",
	})

	// Update it via POST with same token
	body, _ := json.Marshal(tokenCreateRequest{
		Token:   "update-me",
		MaxUses: 10,
		Comment: "updated",
	})

	result, status, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected status 200 for update, got %d", status)
	}

	resp := result.(tokenResponse)
	if resp.MaxUses != 10 {
		t.Errorf("expected MaxUses=10, got %d", resp.MaxUses)
	}
	if resp.Comment != "updated" {
		t.Errorf("expected Comment='updated', got '%s'", resp.Comment)
	}
}

func TestTokensHandler_Delete(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	db.Create(&mdb.RegistrationToken{
		Token:   "delete-me",
		MaxUses: 1,
	})

	body, _ := json.Marshal(tokenDeleteRequest{Token: "delete-me"})
	_, status, err := handler("DELETE", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected status 200, got %d", status)
	}

	// Verify deletion
	var count int64
	db.Model(&mdb.RegistrationToken{}).Where("token = ?", "delete-me").Count(&count)
	if count != 0 {
		t.Error("token should have been deleted")
	}
}

func TestTokensHandler_DeleteNotFound(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	body, _ := json.Marshal(tokenDeleteRequest{Token: "nonexistent"})
	_, status, err := handler("DELETE", body)
	if err == nil {
		t.Fatal("expected error for nonexistent token")
	}
	if status != 404 {
		t.Errorf("expected status 404, got %d", status)
	}
}

func TestTokensHandler_ListWithPending(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	db.Create(&mdb.RegistrationToken{
		Token:     "token-with-pending",
		MaxUses:   5,
		UsedCount: 2,
	})

	// Create pending reservations
	db.Create(&mdb.Quota{Username: "a@x.com", FirstLoginAt: 1, UsedToken: "token-with-pending", CreatedAt: time.Now().Unix()})
	db.Create(&mdb.Quota{Username: "b@x.com", FirstLoginAt: 1, UsedToken: "token-with-pending", CreatedAt: time.Now().Unix()})

	result, status, err := handler("GET", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Errorf("expected status 200, got %d", status)
	}

	resp := result.(tokenListResponse)
	if resp.Total != 1 {
		t.Fatalf("expected 1 token, got %d", resp.Total)
	}

	token := resp.Tokens[0]
	if token.PendingReservations != 2 {
		t.Errorf("expected 2 pending reservations, got %d", token.PendingReservations)
	}
	if token.UsedCount != 2 {
		t.Errorf("expected UsedCount=2, got %d", token.UsedCount)
	}
}

func TestTokensHandler_StatusComputation(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	now := time.Now()
	past := now.Add(-1 * time.Hour)

	// Active token
	db.Create(&mdb.RegistrationToken{Token: "t-active", MaxUses: 5, UsedCount: 0})
	// Exhausted token
	db.Create(&mdb.RegistrationToken{Token: "t-exhausted", MaxUses: 1, UsedCount: 1})
	// Expired token
	db.Create(&mdb.RegistrationToken{Token: "t-expired", MaxUses: 10, ExpiresAt: &past})

	result, _, _ := handler("GET", nil)
	resp := result.(tokenListResponse)

	statusMap := make(map[string]string)
	for _, tok := range resp.Tokens {
		statusMap[tok.Token] = tok.Status
	}

	if statusMap["t-active"] != "active" {
		t.Errorf("expected 'active', got '%s'", statusMap["t-active"])
	}
	if statusMap["t-exhausted"] != "exhausted" {
		t.Errorf("expected 'exhausted', got '%s'", statusMap["t-exhausted"])
	}
	if statusMap["t-expired"] != "expired" {
		t.Errorf("expected 'expired', got '%s'", statusMap["t-expired"])
	}
}

func TestTokensHandler_MethodNotAllowed(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	_, status, err := handler("PUT", nil)
	if err == nil {
		t.Fatal("expected error for unsupported method")
	}
	if status != 405 {
		t.Errorf("expected status 405, got %d", status)
	}
}

func TestTokensHandler_CreateDefaultMaxUses(t *testing.T) {
	db := setupTokensTestDB(t)
	handler := TokensHandler(TokensDeps{DB: db})

	// Send with max_uses = 0 (should default to 1)
	body, _ := json.Marshal(tokenCreateRequest{
		Token: "default-max",
	})

	result, _, err := handler("POST", body)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	resp := result.(tokenResponse)
	if resp.MaxUses != 1 {
		t.Errorf("expected MaxUses=1 (default), got %d", resp.MaxUses)
	}
}
