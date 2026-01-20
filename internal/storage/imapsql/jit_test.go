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
	"math/rand"
	"os"
	"path/filepath"
	"testing"

	imapsql "github.com/foxcpp/go-imap-sql"
	"github.com/emersion/go-imap/backend"
	"github.com/themadorg/madmail/internal/testutils"
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
)

// setupTestStorageForJIT creates a Storage instance for testing JIT functionality
func setupTestStorageForJIT(t *testing.T) (*Storage, func()) {
	t.Helper()

	testDir := testutils.Dir(t)
	dbPath := filepath.Join(testDir, "test.db")

	// Create SQLite database file for both GORM and backend to share
	gormDB, err := gorm.Open(sqlite.Open(dbPath), &gorm.Config{})
	if err != nil {
		t.Fatalf("Failed to open GORM database: %v", err)
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

	// Initialize quota table
	if err := store.initQuotaTable(); err != nil {
		t.Fatalf("Failed to init quota table: %v", err)
	}

	// Set up authNormalize function (required for GetOrCreateIMAPAcct)
	store.authNormalize = func(ctx context.Context, username string) (string, error) {
		// Simple normalization: just return the username as-is for testing
		return username, nil
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

// mockSettingsTable is a simple mock implementation of module.Table for testing
type mockSettingsTable struct {
	data map[string]string
}

func (m *mockSettingsTable) Lookup(ctx context.Context, key string) (string, bool, error) {
	val, ok := m.data[key]
	return val, ok, nil
}

func (m *mockSettingsTable) SetKey(key, value string) error {
	if m.data == nil {
		m.data = make(map[string]string)
	}
	m.data[key] = value
	return nil
}

func TestIsJitRegistrationEnabled(t *testing.T) {
	store, cleanup := setupTestStorageForJIT(t)
	defer cleanup()

	t.Run("returns true when explicitly enabled", func(t *testing.T) {
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "true",
		}}

		enabled, err := store.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to be enabled")
		}
	})

	t.Run("returns false when explicitly disabled", func(t *testing.T) {
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "false",
		}}

		enabled, err := store.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if enabled {
			t.Error("Expected JIT registration to be disabled")
		}
	})

	t.Run("defaults to registration open status when not set", func(t *testing.T) {
		// Set registration open to true
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__REGISTRATION_OPEN__": "true",
		}}

		enabled, err := store.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to default to registration open (true)")
		}

		// Set registration open to false
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__REGISTRATION_OPEN__": "false",
		}}

		enabled, err = store.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if enabled {
			t.Error("Expected JIT registration to default to registration open (false)")
		}
	})

	t.Run("defaults to registration open when settings table is nil", func(t *testing.T) {
		store.settingsTable = nil
		store.autoCreate = true

		enabled, err := store.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to default to autoCreate (true)")
		}
	})
}

func TestGetOrCreateIMAPAcct_WithJIT(t *testing.T) {
	store, cleanup := setupTestStorageForJIT(t)
	defer cleanup()

	t.Run("creates account when JIT enabled and user doesn't exist", func(t *testing.T) {
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "true",
		}}

		username := "newuser@example.com"

		user, err := store.GetOrCreateIMAPAcct(username)
		if err != nil {
			t.Fatalf("GetOrCreateIMAPAcct returned error: %v", err)
		}
		if user == nil {
			t.Fatal("Expected user to be created")
		}

		// Verify user exists in backend
		_, err = store.Back.GetUser(username)
		if err != nil {
			t.Errorf("User was not created in backend: %v", err)
		}
	})

	t.Run("does not create account when JIT disabled and user doesn't exist", func(t *testing.T) {
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "false",
		}}

		username := "nonexistent@example.com"

		user, err := store.GetOrCreateIMAPAcct(username)
		if err == nil {
			t.Error("Expected error when JIT is disabled and user doesn't exist")
		}
		if user != nil {
			t.Error("Expected nil user when JIT is disabled and user doesn't exist")
		}
		if err != backend.ErrInvalidCredentials {
			t.Errorf("Expected ErrInvalidCredentials, got %v", err)
		}

		// Verify user was NOT created in backend
		_, err = store.Back.GetUser(username)
		if err == nil {
			t.Error("User should not be created when JIT is disabled")
		}
	})

	t.Run("returns existing user when JIT disabled but user exists", func(t *testing.T) {
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "true",
		}}

		username := "existinguser@example.com"

		// Create user first
		_, err := store.GetOrCreateIMAPAcct(username)
		if err != nil {
			t.Fatalf("Failed to create user: %v", err)
		}

		// Now disable JIT
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "false",
		}}

		// Should still be able to get existing user
		user, err := store.GetOrCreateIMAPAcct(username)
		if err != nil {
			t.Fatalf("GetOrCreateIMAPAcct returned error for existing user: %v", err)
		}
		if user == nil {
			t.Fatal("Expected existing user to be returned")
		}
	})

	t.Run("returns existing user regardless of JIT setting", func(t *testing.T) {
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "true",
		}}

		username := "testuser@example.com"

		// Create user with JIT enabled
		_, err := store.GetOrCreateIMAPAcct(username)
		if err != nil {
			t.Fatalf("Failed to create user: %v", err)
		}

		// Test with JIT enabled
		user1, err := store.GetOrCreateIMAPAcct(username)
		if err != nil {
			t.Fatalf("GetOrCreateIMAPAcct failed with JIT enabled: %v", err)
		}
		if user1 == nil {
			t.Fatal("Expected user to be returned")
		}

		// Test with JIT disabled
		store.settingsTable = &mockSettingsTable{data: map[string]string{
			"__JIT_REGISTRATION_ENABLED__": "false",
		}}

		user2, err := store.GetOrCreateIMAPAcct(username)
		if err != nil {
			t.Fatalf("GetOrCreateIMAPAcct failed with JIT disabled: %v", err)
		}
		if user2 == nil {
			t.Fatal("Expected existing user to be returned even with JIT disabled")
		}
	})
}
