//go:build cgo && !nosqlite3
// +build cgo,!nosqlite3

/*
Maddy Mail Server - Composable all-in-one email server.
Copyright © 2019-2020 Max Mazurov <fox.cpp@disroot.org>, Maddy Mail Server contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

package imapsql

import (
	"context"
	"errors"
	"testing"
	"time"

	mdb "github.com/themadorg/madmail/internal/db"
	"github.com/themadorg/madmail/internal/testutils"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
)

// setupTestStorage creates a minimal Storage instance for testing
// with an in-memory SQLite database
func setupTestStorage(t *testing.T) (*Storage, func()) {
	t.Helper()

	// Create in-memory SQLite database
	db, err := gorm.Open(sqlite.Open(":memory:"), &gorm.Config{})
	if err != nil {
		t.Fatalf("Failed to open test database: %v", err)
	}

	// Auto-migrate quota table
	if err := db.AutoMigrate(&mdb.Quota{}); err != nil {
		t.Fatalf("Failed to migrate quota table: %v", err)
	}

	store := &Storage{
		GORMDB: db,
		Log:    testutils.Logger(t, "imapsql"),
	}

	cleanup := func() {
		sqlDB, _ := db.DB()
		if sqlDB != nil {
			sqlDB.Close()
		}
	}

	return store, cleanup
}

func TestUpdateFirstLogin(t *testing.T) {
	store, cleanup := setupTestStorage(t)
	defer cleanup()

	t.Run("does not create quota entry if it doesn't exist", func(t *testing.T) {
		username := "newuser@example.com"

		err := store.UpdateFirstLogin(username)
		if err != nil {
			t.Fatalf("UpdateFirstLogin returned error: %v", err)
		}

		// Verify quota entry was NOT created (lazy approach - follow same pattern as before)
		var quota mdb.Quota
		err = store.GORMDB.Where("username = ?", username).First(&quota).Error
		if !errors.Is(err, gorm.ErrRecordNotFound) {
			t.Errorf("Expected quota entry to not exist (lazy approach), but got: %v", err)
		}
	})

	t.Run("updates quota entry with value 1", func(t *testing.T) {
		username := "existinguser@example.com"
		originalCreatedAt := time.Now().Add(-24 * time.Hour).Unix()

		// Create quota entry with value 1 (never logged in, new user)
		quota := mdb.Quota{
			Username:     username,
			CreatedAt:    originalCreatedAt,
			FirstLoginAt: 1,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Wait a bit to ensure timestamp difference
		time.Sleep(100 * time.Millisecond)

		err := store.UpdateFirstLogin(username)
		if err != nil {
			t.Fatalf("UpdateFirstLogin returned error: %v", err)
		}

		// Verify first login was updated from 1 to actual timestamp (> 1)
		var updatedQuota mdb.Quota
		if err := store.GORMDB.Where("username = ?", username).First(&updatedQuota).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}

		if updatedQuota.CreatedAt != originalCreatedAt {
			t.Errorf("CreatedAt should not change, expected %d, got %d", originalCreatedAt, updatedQuota.CreatedAt)
		}
		if updatedQuota.FirstLoginAt == 1 {
			t.Error("FirstLoginAt should be updated from 1 to actual timestamp (> 1)")
		}
		if updatedQuota.FirstLoginAt <= 1 {
			t.Error("FirstLoginAt should be > 1 (actual timestamp)")
		}
		if updatedQuota.FirstLoginAt <= time.Now().Unix()-10 {
			t.Error("FirstLoginAt should be recent")
		}
	})

	t.Run("does not update entry that already has first login", func(t *testing.T) {
		username := "loggedinuser@example.com"
		originalCreatedAt := time.Now().Add(-24 * time.Hour).Unix()
		originalFirstLogin := time.Now().Add(-12 * time.Hour).Unix()

		// Create quota entry with existing first login
		quota := mdb.Quota{
			Username:     username,
			CreatedAt:    originalCreatedAt,
			FirstLoginAt: originalFirstLogin,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		err := store.UpdateFirstLogin(username)
		if err != nil {
			t.Fatalf("UpdateFirstLogin returned error: %v", err)
		}

		// Verify first login was NOT updated (should remain the same)
		var updatedQuota mdb.Quota
		if err := store.GORMDB.Where("username = ?", username).First(&updatedQuota).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}

		if updatedQuota.CreatedAt != originalCreatedAt {
			t.Errorf("CreatedAt should not change, expected %d, got %d", originalCreatedAt, updatedQuota.CreatedAt)
		}
		if updatedQuota.FirstLoginAt != originalFirstLogin {
			t.Errorf("FirstLoginAt should NOT be updated if already logged in (> 0), expected %d, got %d", originalFirstLogin, updatedQuota.FirstLoginAt)
		}
	})
}

func TestMigrateFirstLoginFromCreatedAt(t *testing.T) {
	store, cleanup := setupTestStorage(t)
	defer cleanup()

	t.Run("migrates users with created_at > 0", func(t *testing.T) {
		username1 := "user1@example.com"
		username2 := "user2@example.com"
		now := time.Now().Unix()
		createdAt1 := time.Now().Add(-48 * time.Hour).Unix() // 48 hours ago
		createdAt2 := time.Now().Add(-24 * time.Hour).Unix() // 24 hours ago

		// Create quota entries with created_at > 0
		quota1 := mdb.Quota{
			Username:     username1,
			CreatedAt:    createdAt1,
			FirstLoginAt: 0,
		}
		quota2 := mdb.Quota{
			Username:     username2,
			CreatedAt:    createdAt2,
			FirstLoginAt: 0,
		}
		if err := store.GORMDB.Create(&quota1).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}
		if err := store.GORMDB.Create(&quota2).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Run migration
		err := store.MigrateFirstLoginFromCreatedAt()
		if err != nil {
			t.Fatalf("MigrateFirstLoginFromCreatedAt returned error: %v", err)
		}

		// Verify first login was set to now (approximately)
		var migratedQuota1 mdb.Quota
		if err := store.GORMDB.Where("username = ?", username1).First(&migratedQuota1).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}
		// Should be set to now (within 1 second tolerance)
		if migratedQuota1.FirstLoginAt < now-1 || migratedQuota1.FirstLoginAt > now+1 {
			t.Errorf("Expected FirstLoginAt to be approximately %d, got %d", now, migratedQuota1.FirstLoginAt)
		}

		var migratedQuota2 mdb.Quota
		if err := store.GORMDB.Where("username = ?", username2).First(&migratedQuota2).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}
		// Should be set to now (within 1 second tolerance)
		if migratedQuota2.FirstLoginAt < now-1 || migratedQuota2.FirstLoginAt > now+1 {
			t.Errorf("Expected FirstLoginAt to be approximately %d, got %d", now, migratedQuota2.FirstLoginAt)
		}
	})

	t.Run("fixes and migrates users with created_at = 0", func(t *testing.T) {
		username := "zerouser@example.com"
		now := time.Now().Unix()

		// Create quota entry with created_at = 0 (legacy data)
		quota := mdb.Quota{
			Username:     username,
			CreatedAt:    0,
			FirstLoginAt: 0,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Run migration
		err := store.MigrateFirstLoginFromCreatedAt()
		if err != nil {
			t.Fatalf("MigrateFirstLoginFromCreatedAt returned error: %v", err)
		}

		// Verify created_at was fixed and first_login_at was set to now
		var updatedQuota mdb.Quota
		if err := store.GORMDB.Where("username = ?", username).First(&updatedQuota).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}
		// created_at should be set to now (within 1 second tolerance)
		if updatedQuota.CreatedAt < now-1 || updatedQuota.CreatedAt > now+1 {
			t.Errorf("Expected CreatedAt to be approximately %d, got %d", now, updatedQuota.CreatedAt)
		}
		// first_login_at should be set to now (within 1 second tolerance)
		if updatedQuota.FirstLoginAt < now-1 || updatedQuota.FirstLoginAt > now+1 {
			t.Errorf("Expected FirstLoginAt to be approximately %d, got %d", now, updatedQuota.FirstLoginAt)
		}
	})

	t.Run("does not migrate users with existing first login", func(t *testing.T) {
		username := "user3@example.com"
		createdAt := time.Now().Add(-48 * time.Hour).Unix()
		existingFirstLogin := time.Now().Add(-12 * time.Hour).Unix()

		// Create quota entry with existing first login (> 1, already logged in)
		quota := mdb.Quota{
			Username:     username,
			CreatedAt:    createdAt,
			FirstLoginAt: existingFirstLogin,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Run migration - should NOT touch users with existing first login
		err := store.MigrateFirstLoginFromCreatedAt()
		if err != nil {
			t.Fatalf("MigrateFirstLoginFromCreatedAt returned error: %v", err)
		}

		// Verify first login was NOT changed (migration only affects 0/NULL)
		var updatedQuota mdb.Quota
		if err := store.GORMDB.Where("username = ?", username).First(&updatedQuota).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}
		// Should remain unchanged
		if updatedQuota.FirstLoginAt != existingFirstLogin {
			t.Errorf("Expected FirstLoginAt to remain %d (users with existing login should not be migrated), got %d", existingFirstLogin, updatedQuota.FirstLoginAt)
		}
	})

	t.Run("skips migration if no records need it", func(t *testing.T) {
		// Create users that don't need migration (all have first_login_at != 0 and != NULL)
		username1 := "user4@example.com"
		username2 := "user5@example.com"
		createdAt := time.Now().Add(-48 * time.Hour).Unix()

		// User with first_login_at = 1 (new user, never logged in)
		quota1 := mdb.Quota{
			Username:     username1,
			CreatedAt:    createdAt,
			FirstLoginAt: 1,
		}
		// User with first_login_at > 1 (has logged in)
		quota2 := mdb.Quota{
			Username:     username2,
			CreatedAt:    createdAt,
			FirstLoginAt: time.Now().Add(-12 * time.Hour).Unix(),
		}
		if err := store.GORMDB.Create(&quota1).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}
		if err := store.GORMDB.Create(&quota2).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Run migration - should be skipped (no records with first_login_at = 0 or NULL)
		err := store.MigrateFirstLoginFromCreatedAt()
		if err != nil {
			t.Fatalf("MigrateFirstLoginFromCreatedAt returned error: %v", err)
		}

		// Verify users were NOT migrated
		var updatedQuota1 mdb.Quota
		if err := store.GORMDB.Where("username = ?", username1).First(&updatedQuota1).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}
		if updatedQuota1.FirstLoginAt != 1 {
			t.Errorf("Expected FirstLoginAt to remain 1 (migration skipped), got %d", updatedQuota1.FirstLoginAt)
		}

		var updatedQuota2 mdb.Quota
		if err := store.GORMDB.Where("username = ?", username2).First(&updatedQuota2).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}
		if updatedQuota2.FirstLoginAt != quota2.FirstLoginAt {
			t.Errorf("Expected FirstLoginAt to remain %d (migration skipped), got %d", quota2.FirstLoginAt, updatedQuota2.FirstLoginAt)
		}
	})

	t.Run("does not migrate users with first_login_at = 1", func(t *testing.T) {
		username := "newuser@example.com"
		createdAt := time.Now().Add(-48 * time.Hour).Unix()

		// Create quota entry with first_login_at = 1 (new user, never logged in)
		quota := mdb.Quota{
			Username:     username,
			CreatedAt:    createdAt,
			FirstLoginAt: 1,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Run migration - should NOT touch users with first_login_at = 1
		err := store.MigrateFirstLoginFromCreatedAt()
		if err != nil {
			t.Fatalf("MigrateFirstLoginFromCreatedAt returned error: %v", err)
		}

		// Verify first login was NOT changed (migration only affects 0/NULL)
		var updatedQuota mdb.Quota
		if err := store.GORMDB.Where("username = ?", username).First(&updatedQuota).Error; err != nil {
			t.Fatalf("Failed to find quota entry: %v", err)
		}
		// Should remain 1 (new users are not migrated, only legacy 0/NULL users)
		if updatedQuota.FirstLoginAt != 1 {
			t.Errorf("Expected FirstLoginAt to remain 1 (new users should not be migrated), got %d", updatedQuota.FirstLoginAt)
		}
	})
}

// mockSettingsTable is a simple mock implementation of module.Table for testing
type mockSettingsTable struct {
	db *gorm.DB
}

type Setting struct {
	Key   string `gorm:"primaryKey"`
	Value string
}

func (m *mockSettingsTable) Lookup(ctx context.Context, key string) (string, bool, error) {
	var setting Setting
	err := m.db.Where("key = ?", key).First(&setting).Error
	if err != nil {
		if err == gorm.ErrRecordNotFound {
			return "", false, nil
		}
		return "", false, err
	}
	return setting.Value, true, nil
}

func (m *mockSettingsTable) LookupMulti(ctx context.Context, key string) ([]string, error) {
	// Not needed for this test
	return nil, nil
}

func (m *mockSettingsTable) SetKey(key, value string) error {
	setting := Setting{Key: key, Value: value}
	return m.db.Save(&setting).Error
}
