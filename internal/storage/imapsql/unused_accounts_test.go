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
	"math/rand"
	"os"
	"path/filepath"
	"testing"
	"time"

	imapsql "github.com/foxcpp/go-imap-sql"
	mdb "github.com/themadorg/madmail/internal/db"
	"github.com/themadorg/madmail/internal/testutils"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
)

// setupTestStorageForPruning creates a Storage instance with a real backend for testing PruneUnusedAccounts
func setupTestStorageForPruning(t *testing.T) (*Storage, func()) {
	t.Helper()

	testDir := testutils.Dir(t)
	dbPath := filepath.Join(testDir, "test.db")

	// Create SQLite database file for both GORM and backend to share
	gormDB, err := gorm.Open(sqlite.Open(dbPath), &gorm.Config{})
	if err != nil {
		t.Fatalf("Failed to open GORM database: %v", err)
	}

	// Auto-migrate quota table
	if err := gormDB.AutoMigrate(&mdb.Quota{}); err != nil {
		t.Fatalf("Failed to migrate quota table: %v", err)
	}

	// Create real imapsql backend with the same database
	randSrc := rand.NewSource(0)
	prng := rand.New(randSrc)
	backend, err := imapsql.New("sqlite3", dbPath,
		&imapsql.FSStore{Root: filepath.Join(testDir, "messages")}, imapsql.Opts{
			PRNG: prng,
			Log:  testutils.Logger(t, "imapsql-backend"),
		})
	if err != nil {
		t.Fatalf("Failed to create backend: %v", err)
	}

	store := &Storage{
		GORMDB: gormDB,
		Log:    testutils.Logger(t, "imapsql"),
		Back:   backend,
	}

	cleanup := func() {
		backend.Close()
		sqlDB, _ := gormDB.DB()
		if sqlDB != nil {
			sqlDB.Close()
		}
		os.RemoveAll(testDir)
	}

	return store, cleanup
}

func TestPruneUnusedAccounts(t *testing.T) {
	store, cleanup := setupTestStorageForPruning(t)
	defer cleanup()

	t.Run("deletes accounts that never logged in and are older than retention", func(t *testing.T) {
		now := time.Now()
		retention := 24 * time.Hour

		// Create accounts that should be deleted:
		// - first_login_at = 1 (never logged in, prunable)
		// - created_at < cutoff (older than retention)
		oldUnusedUser1 := "oldunused1@example.com"
		oldUnusedUser2 := "oldunused2@example.com"
		oldCreatedAt := now.Add(-48 * time.Hour).Unix() // 48 hours ago

		quota1 := mdb.Quota{
			Username:     oldUnusedUser1,
			CreatedAt:    oldCreatedAt,
			FirstLoginAt: 1,
		}
		quota2 := mdb.Quota{
			Username:     oldUnusedUser2,
			CreatedAt:    oldCreatedAt,
			FirstLoginAt: 1,
		}

		if err := store.GORMDB.Create(&quota1).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}
		if err := store.GORMDB.Create(&quota2).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Create users in backend (required for DeleteIMAPAcct to work)
		if err := store.Back.CreateUser(oldUnusedUser1); err != nil {
			t.Fatalf("Failed to create user in backend: %v", err)
		}
		if err := store.Back.CreateUser(oldUnusedUser2); err != nil {
			t.Fatalf("Failed to create user in backend: %v", err)
		}

		// Run pruning
		err := store.PruneUnusedAccounts(retention)
		if err != nil {
			t.Fatalf("PruneUnusedAccounts returned error: %v", err)
		}

		// Verify accounts were deleted from quota table
		var count int64
		store.GORMDB.Model(&mdb.Quota{}).Where("username IN ?", []string{oldUnusedUser1, oldUnusedUser2}).Count(&count)
		if count != 0 {
			t.Errorf("Expected 0 quota entries, got %d", count)
		}

		// Verify users were deleted from backend
		_, err1 := store.Back.GetUser(oldUnusedUser1)
		_, err2 := store.Back.GetUser(oldUnusedUser2)
		if err1 == nil {
			t.Error("User1 should be deleted from backend")
		}
		if err2 == nil {
			t.Error("User2 should be deleted from backend")
		}
	})

	t.Run("does not delete accounts that logged in", func(t *testing.T) {
		now := time.Now()
		retention := 24 * time.Hour

		// Create account that logged in (even if old)
		oldUsedUser := "oldused@example.com"
		oldCreatedAt := now.Add(-48 * time.Hour).Unix()
		firstLoginAt := now.Add(-12 * time.Hour).Unix() // First logged in 12 hours ago

		quota := mdb.Quota{
			Username:     oldUsedUser,
			CreatedAt:    oldCreatedAt,
			FirstLoginAt: firstLoginAt,
		}

		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		if err := store.Back.CreateUser(oldUsedUser); err != nil {
			t.Fatalf("Failed to create user in backend: %v", err)
		}

		// Run pruning
		err := store.PruneUnusedAccounts(retention)
		if err != nil {
			t.Fatalf("PruneUnusedAccounts returned error: %v", err)
		}

		// Verify account was NOT deleted
		var count int64
		store.GORMDB.Model(&mdb.Quota{}).Where("username = ?", oldUsedUser).Count(&count)
		if count != 1 {
			t.Errorf("Expected 1 quota entry, got %d", count)
		}

		// Verify user was NOT deleted from backend
		_, err = store.Back.GetUser(oldUsedUser)
		if err != nil {
			t.Error("User should not be deleted from backend")
		}
	})

	t.Run("does not delete recent accounts even if unused", func(t *testing.T) {
		now := time.Now()
		retention := 24 * time.Hour

		// Create account that is recent (even if unused)
		recentUnusedUser := "recentunused@example.com"
		recentCreatedAt := now.Add(-12 * time.Hour).Unix() // Created 12 hours ago

		quota := mdb.Quota{
			Username:     recentUnusedUser,
			CreatedAt:    recentCreatedAt,
			FirstLoginAt: 1,
		}

		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		if err := store.Back.CreateUser(recentUnusedUser); err != nil {
			t.Fatalf("Failed to create user in backend: %v", err)
		}

		// Run pruning
		err := store.PruneUnusedAccounts(retention)
		if err != nil {
			t.Fatalf("PruneUnusedAccounts returned error: %v", err)
		}

		// Verify account was NOT deleted
		var count int64
		store.GORMDB.Model(&mdb.Quota{}).Where("username = ?", recentUnusedUser).Count(&count)
		if count != 1 {
			t.Errorf("Expected 1 quota entry, got %d", count)
		}

		// Verify user was NOT deleted from backend
		_, err = store.Back.GetUser(recentUnusedUser)
		if err != nil {
			t.Error("User should not be deleted from backend")
		}
	})

	t.Run("returns nil when no accounts to delete", func(t *testing.T) {
		retention := 24 * time.Hour

		// No accounts in database
		err := store.PruneUnusedAccounts(retention)
		if err != nil {
			t.Fatalf("PruneUnusedAccounts returned error: %v", err)
		}
	})

	t.Run("deletes from auth DB if configured", func(t *testing.T) {
		now := time.Now()
		retention := 24 * time.Hour
		oldCreatedAt := now.Add(-48 * time.Hour).Unix()

		username := "authtest@example.com"

		// Create quota entry
		quota := mdb.Quota{
			Username:     username,
			CreatedAt:    oldCreatedAt,
			FirstLoginAt: 1,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		if err := store.Back.CreateUser(username); err != nil {
			t.Fatalf("Failed to create user in backend: %v", err)
		}

		// Note: Testing auth DB deletion would require mocking module.GetInstance,
		// which is complex. The auth DB deletion logic is tested in integration tests.
		// Here we verify that the core pruning logic works correctly.

		err := store.PruneUnusedAccounts(retention)
		if err != nil {
			t.Fatalf("PruneUnusedAccounts returned error: %v", err)
		}

		// Verify account was deleted from quota table
		var count int64
		store.GORMDB.Model(&mdb.Quota{}).Where("username = ?", username).Count(&count)
		if count != 0 {
			t.Errorf("Expected 0 quota entries, got %d", count)
		}

		// Verify user was deleted from backend
		_, err = store.Back.GetUser(username)
		if err == nil {
			t.Error("User should be deleted from backend")
		}
	})

	t.Run("handles deletion errors gracefully", func(t *testing.T) {
		now := time.Now()
		retention := 24 * time.Hour
		oldCreatedAt := now.Add(-48 * time.Hour).Unix()

		// Create account
		username := "errorexample@example.com"
		quota := mdb.Quota{
			Username:     username,
			CreatedAt:    oldCreatedAt,
			FirstLoginAt: 1,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			t.Fatalf("Failed to create quota entry: %v", err)
		}

		// Don't create user in backend, so DeleteUser will fail
		// The function should continue and log the error

		// Run pruning - should not fail even if backend deletion fails
		err := store.PruneUnusedAccounts(retention)
		if err != nil {
			t.Fatalf("PruneUnusedAccounts should handle errors gracefully, got error: %v", err)
		}

		// Verify quota entry was still deleted (backend error doesn't prevent quota deletion)
		var count int64
		store.GORMDB.Model(&mdb.Quota{}).Where("username = ?", username).Count(&count)
		if count != 0 {
			t.Errorf("Expected 0 quota entries, got %d", count)
		}
	})
}
