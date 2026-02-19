package endpoint_cache

import (
	"context"
	"testing"

	"github.com/themadorg/madmail/framework/log"
	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
	"gorm.io/gorm/logger"
)

func testDB(t *testing.T) *gorm.DB {
	t.Helper()
	db, err := gorm.Open(sqlite.Open(":memory:"), &gorm.Config{
		Logger: logger.Default.LogMode(logger.Silent),
	})
	if err != nil {
		t.Fatal("failed to open test DB:", err)
	}
	if err := db.AutoMigrate(&mdb.EndpointOverride{}); err != nil {
		t.Fatal("failed to migrate:", err)
	}
	return db
}

func testCache(t *testing.T) *Cache {
	t.Helper()
	db := testDB(t)
	c, err := New(db, log.Logger{Name: "endpoint_cache/test"})
	if err != nil {
		t.Fatal("failed to create cache:", err)
	}
	return c
}

// ---------------------------------------------------------------------------
// Resolve — IP behaviour: "an IP is an IP"
// ---------------------------------------------------------------------------

func TestResolve_NoOverride_BareIP(t *testing.T) {
	c := testCache(t)
	// A bare IP without any override should return itself.
	result, err := c.Resolve(context.Background(), "1.2.3.4")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "1.2.3.4" {
		t.Errorf("expected 1.2.3.4, got %q", result)
	}
}

func TestResolve_NoOverride_BracketedIP(t *testing.T) {
	c := testCache(t)
	// Bracketed IP without override → return the IP itself (no brackets).
	result, err := c.Resolve(context.Background(), "[10.0.3.162]")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "10.0.3.162" {
		t.Errorf("expected 10.0.3.162, got %q", result)
	}
}

func TestResolve_NoOverride_Domain(t *testing.T) {
	c := testCache(t)
	// Domain without override → return empty (let OS DNS handle it).
	result, err := c.Resolve(context.Background(), "nonexistent.example.com")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "" {
		t.Errorf("expected empty string (no override), got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Resolve — overrides
// ---------------------------------------------------------------------------

func TestResolve_WithOverride_IP(t *testing.T) {
	c := testCache(t)
	if err := c.Set("1.1.1.1", "2.2.2.2", "test override"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	result, err := c.Resolve(context.Background(), "1.1.1.1")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "2.2.2.2" {
		t.Errorf("expected 2.2.2.2, got %s", result)
	}
}

func TestResolve_WithOverride_BracketedIP(t *testing.T) {
	c := testCache(t)
	if err := c.Set("1.1.1.1", "2.2.2.2", "bracketed test"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	// Brackets should be stripped before lookup → match "1.1.1.1"
	result, err := c.Resolve(context.Background(), "[1.1.1.1]")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "2.2.2.2" {
		t.Errorf("expected 2.2.2.2, got %s", result)
	}
}

func TestResolve_BareIP_MatchesBracketedOverride(t *testing.T) {
	c := testCache(t)
	// Override stored without brackets
	if err := c.Set("10.0.3.162", "10.0.3.100", "bare→bracket"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	// Both bare and bracketed lookups should match
	for _, input := range []string{"10.0.3.162", "[10.0.3.162]"} {
		result, err := c.Resolve(context.Background(), input)
		if err != nil {
			t.Fatalf("%s: unexpected error: %v", input, err)
		}
		if result != "10.0.3.100" {
			t.Errorf("%s: expected 10.0.3.100, got %s", input, result)
		}
	}
}

func TestResolve_WithOverride_Domain(t *testing.T) {
	c := testCache(t)
	if err := c.Set("nine.testrun.org", "10.0.0.1", "domain override"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	result, err := c.Resolve(context.Background(), "nine.testrun.org")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "10.0.0.1" {
		t.Errorf("expected 10.0.0.1, got %s", result)
	}
}

func TestResolve_CaseInsensitive(t *testing.T) {
	c := testCache(t)
	if err := c.Set("nine.testrun.org", "10.0.0.1", "case test"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	result, err := c.Resolve(context.Background(), "NINE.TESTRUN.ORG")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "10.0.0.1" {
		t.Errorf("expected 10.0.0.1, got %s", result)
	}
}

func TestResolve_TrailingDot(t *testing.T) {
	c := testCache(t)
	if err := c.Set("nine.testrun.org", "10.0.0.1", "dot test"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	result, err := c.Resolve(context.Background(), "nine.testrun.org.")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "10.0.0.1" {
		t.Errorf("expected 10.0.0.1, got %s", result)
	}
}

func TestResolve_IPv6(t *testing.T) {
	c := testCache(t)
	if err := c.Set("::1", "::2", "ipv6 test"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	result, err := c.Resolve(context.Background(), "[ipv6:::1]")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "::2" {
		t.Errorf("expected ::2, got %s", result)
	}
}

// ---------------------------------------------------------------------------
// ResolveMX
// ---------------------------------------------------------------------------

func TestResolveMX_WithOverride(t *testing.T) {
	c := testCache(t)
	if err := c.Set("example.com", "mx.override.test", "MX override"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	records, hit, err := c.ResolveMX(context.Background(), "example.com")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if !hit {
		t.Error("expected cache hit")
	}
	if len(records) != 1 || records[0].Host != "mx.override.test" {
		t.Errorf("expected mx.override.test, got %v", records)
	}
}

func TestResolveMX_BracketedIP(t *testing.T) {
	c := testCache(t)
	if err := c.Set("10.0.3.162", "10.0.3.200", "MX bracketed IP"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	records, hit, err := c.ResolveMX(context.Background(), "[10.0.3.162]")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if !hit {
		t.Error("expected cache hit for bracketed IP")
	}
	if len(records) != 1 || records[0].Host != "10.0.3.200" {
		t.Errorf("expected 10.0.3.200, got %v", records)
	}
}

func TestResolveMX_BareIP(t *testing.T) {
	c := testCache(t)
	if err := c.Set("10.0.3.162", "10.0.3.200", "MX bare IP"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	records, hit, err := c.ResolveMX(context.Background(), "10.0.3.162")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if !hit {
		t.Error("expected cache hit for bare IP")
	}
	if len(records) != 1 || records[0].Host != "10.0.3.200" {
		t.Errorf("expected 10.0.3.200, got %v", records)
	}
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

func TestCRUD_Set_Get_Delete(t *testing.T) {
	c := testCache(t)

	if err := c.Set("test.example.com", "1.2.3.4", "crud test"); err != nil {
		t.Fatal("Set failed:", err)
	}

	override, err := c.Get("test.example.com")
	if err != nil {
		t.Fatal("Get failed:", err)
	}
	if override.LookupKey != "test.example.com" || override.TargetHost != "1.2.3.4" {
		t.Errorf("unexpected values: %+v", override)
	}

	if err := c.Set("test.example.com", "5.6.7.8", "updated"); err != nil {
		t.Fatal("Set (update) failed:", err)
	}
	override, _ = c.Get("test.example.com")
	if override.TargetHost != "5.6.7.8" {
		t.Errorf("expected 5.6.7.8, got %s", override.TargetHost)
	}

	list, _ := c.List()
	if len(list) != 1 {
		t.Errorf("expected 1 entry, got %d", len(list))
	}

	if err := c.Delete("test.example.com"); err != nil {
		t.Fatal("Delete failed:", err)
	}
	_, err = c.Get("test.example.com")
	if err == nil {
		t.Error("expected error after delete, got nil")
	}
}
