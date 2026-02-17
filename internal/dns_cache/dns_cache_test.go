package dns_cache

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
	if err := db.AutoMigrate(&mdb.DNSOverride{}); err != nil {
		t.Fatal("failed to migrate:", err)
	}
	return db
}

func testCache(t *testing.T) *Cache {
	t.Helper()
	db := testDB(t)
	c, err := New(db, log.Logger{Name: "dns_cache/test"})
	if err != nil {
		t.Fatal("failed to create cache:", err)
	}
	return c
}

func TestResolve_NoOverride_IP(t *testing.T) {
	c := testCache(t)
	// An IP without any override should return itself
	result, err := c.Resolve(context.Background(), "1.2.3.4")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "1.2.3.4" {
		t.Errorf("expected 1.2.3.4, got %s", result)
	}
}

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

	// Brackets should be stripped
	result, err := c.Resolve(context.Background(), "[1.1.1.1]")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if result != "2.2.2.2" {
		t.Errorf("expected 2.2.2.2, got %s", result)
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

func TestResolveMX_WithOverride(t *testing.T) {
	c := testCache(t)
	if err := c.Set("example.com", "mx.override.test", "MX override"); err != nil {
		t.Fatal("failed to set override:", err)
	}

	records, _, err := c.ResolveMX(context.Background(), "example.com")
	if err != nil {
		t.Fatal("unexpected error:", err)
	}
	if len(records) != 1 {
		t.Fatalf("expected 1 record, got %d", len(records))
	}
	if records[0].Host != "mx.override.test" {
		t.Errorf("expected mx.override.test, got %s", records[0].Host)
	}
	if records[0].Pref != 0 {
		t.Errorf("expected pref 0, got %d", records[0].Pref)
	}
}

func TestCRUD_Set_Get_Delete(t *testing.T) {
	c := testCache(t)

	// Set
	if err := c.Set("test.example.com", "1.2.3.4", "crud test"); err != nil {
		t.Fatal("Set failed:", err)
	}

	// Get
	override, err := c.Get("test.example.com")
	if err != nil {
		t.Fatal("Get failed:", err)
	}
	if override.LookupKey != "test.example.com" {
		t.Errorf("expected test.example.com, got %s", override.LookupKey)
	}
	if override.TargetHost != "1.2.3.4" {
		t.Errorf("expected 1.2.3.4, got %s", override.TargetHost)
	}
	if override.Comment != "crud test" {
		t.Errorf("expected 'crud test', got %s", override.Comment)
	}

	// Update
	if err := c.Set("test.example.com", "5.6.7.8", "updated"); err != nil {
		t.Fatal("Set (update) failed:", err)
	}
	override, err = c.Get("test.example.com")
	if err != nil {
		t.Fatal("Get (after update) failed:", err)
	}
	if override.TargetHost != "5.6.7.8" {
		t.Errorf("expected 5.6.7.8, got %s", override.TargetHost)
	}

	// List
	list, err := c.List()
	if err != nil {
		t.Fatal("List failed:", err)
	}
	if len(list) != 1 {
		t.Errorf("expected 1 entry, got %d", len(list))
	}

	// Delete
	if err := c.Delete("test.example.com"); err != nil {
		t.Fatal("Delete failed:", err)
	}
	_, err = c.Get("test.example.com")
	if err == nil {
		t.Error("expected error after delete, got nil")
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
