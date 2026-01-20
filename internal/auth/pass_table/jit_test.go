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

package pass_table

import (
	"context"
	"errors"
	"testing"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	"github.com/themadorg/madmail/internal/testutils"
	"golang.org/x/text/secure/precis"
)

// mockMutableTable is a mock implementation of module.MutableTable for testing
type mockMutableTable struct {
	data map[string]string
}

func (m *mockMutableTable) Lookup(_ context.Context, key string) (string, bool, error) {
	val, ok := m.data[key]
	return val, ok, nil
}

func (m *mockMutableTable) LookupMulti(_ context.Context, key string) ([]string, error) {
	val, ok := m.data[key]
	if !ok {
		return nil, nil
	}
	return []string{val}, nil
}

func (m *mockMutableTable) Keys() ([]string, error) {
	keys := make([]string, 0, len(m.data))
	for k := range m.data {
		keys = append(keys, k)
	}
	return keys, nil
}

func (m *mockMutableTable) RemoveKey(key string) error {
	delete(m.data, key)
	return nil
}

func (m *mockMutableTable) SetKey(key, value string) error {
	if m.data == nil {
		m.data = make(map[string]string)
	}
	m.data[key] = value
	return nil
}

func setupAuthForJIT(t *testing.T) *Auth {
	t.Helper()

	mod, err := New("pass_table", "", nil, []string{"dummy"})
	if err != nil {
		t.Fatalf("Failed to create auth module: %v", err)
	}

	err = mod.Init(config.NewMap(nil, config.Node{
		Children: []config.Node{},
	}))
	if err != nil {
		t.Fatalf("Failed to init auth module: %v", err)
	}

	a := mod.(*Auth)
	a.table = &mockMutableTable{data: make(map[string]string)}
	a.settingsTable = &mockMutableTable{data: make(map[string]string)}
	a.autoCreate = true // Default to true for testing

	return a
}

func TestIsJitRegistrationEnabled(t *testing.T) {
	t.Run("returns true when explicitly enabled", func(t *testing.T) {
		a := setupAuthForJIT(t)
		a.settingsTable.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "true")

		enabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to be enabled")
		}
	})

	t.Run("returns false when explicitly disabled", func(t *testing.T) {
		a := setupAuthForJIT(t)
		a.settingsTable.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "false")

		enabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if enabled {
			t.Error("Expected JIT registration to be disabled")
		}
	})

	t.Run("defaults to registration open status when not set", func(t *testing.T) {
		a := setupAuthForJIT(t)
		// Don't set JIT flag, should default to registration open

		// Set registration open to true
		a.settingsTable.(*mockMutableTable).SetKey("__REGISTRATION_OPEN__", "true")
		enabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to default to registration open (true)")
		}

		// Set registration open to false
		a.settingsTable.(*mockMutableTable).SetKey("__REGISTRATION_OPEN__", "false")
		enabled, err = a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if enabled {
			t.Error("Expected JIT registration to default to registration open (false)")
		}
	})

	t.Run("uses settings table when available", func(t *testing.T) {
		a := setupAuthForJIT(t)
		// Set JIT flag in settings table
		a.settingsTable.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "true")
		// Set different value in main table (should be ignored)
		a.table.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "false")

		enabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to use settings table value (true)")
		}
	})

	t.Run("uses main table when settings table not available", func(t *testing.T) {
		a := setupAuthForJIT(t)
		a.settingsTable = nil
		a.table.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "true")

		enabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to use main table value (true)")
		}
	})
}

func TestSetJitRegistrationEnabled(t *testing.T) {
	t.Run("enables JIT registration", func(t *testing.T) {
		a := setupAuthForJIT(t)

		err := a.SetJitRegistrationEnabled(true)
		if err != nil {
			t.Fatalf("SetJitRegistrationEnabled returned error: %v", err)
		}

		// Verify it was set
		val, ok := a.settingsTable.(*mockMutableTable).data["__JIT_REGISTRATION_ENABLED__"]
		if !ok {
			t.Error("JIT registration flag was not set")
		}
		if val != "true" {
			t.Errorf("Expected 'true', got %q", val)
		}

		// Verify it can be read back
		enabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if !enabled {
			t.Error("Expected JIT registration to be enabled")
		}
	})

	t.Run("disables JIT registration", func(t *testing.T) {
		a := setupAuthForJIT(t)

		err := a.SetJitRegistrationEnabled(false)
		if err != nil {
			t.Fatalf("SetJitRegistrationEnabled returned error: %v", err)
		}

		// Verify it was set
		val, ok := a.settingsTable.(*mockMutableTable).data["__JIT_REGISTRATION_ENABLED__"]
		if !ok {
			t.Error("JIT registration flag was not set")
		}
		if val != "false" {
			t.Errorf("Expected 'false', got %q", val)
		}

		// Verify it can be read back
		enabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			t.Fatalf("IsJitRegistrationEnabled returned error: %v", err)
		}
		if enabled {
			t.Error("Expected JIT registration to be disabled")
		}
	})

	t.Run("returns error when table is not mutable", func(t *testing.T) {
		a := setupAuthForJIT(t)
		// Use non-mutable table
		a.table = testutils.Table{M: make(map[string]string)}
		a.settingsTable = nil

		err := a.SetJitRegistrationEnabled(true)
		if err == nil {
			t.Error("Expected error when table is not mutable")
		}
	})

	t.Run("uses settings table when available", func(t *testing.T) {
		a := setupAuthForJIT(t)

		err := a.SetJitRegistrationEnabled(true)
		if err != nil {
			t.Fatalf("SetJitRegistrationEnabled returned error: %v", err)
		}

		// Verify it was set in settings table, not main table
		_, ok := a.settingsTable.(*mockMutableTable).data["__JIT_REGISTRATION_ENABLED__"]
		if !ok {
			t.Error("JIT registration flag was not set in settings table")
		}

		_, ok = a.table.(*mockMutableTable).data["__JIT_REGISTRATION_ENABLED__"]
		if ok {
			t.Error("JIT registration flag should not be set in main table when settings table is available")
		}
	})
}

func TestAuthPlain_WithJIT(t *testing.T) {
	t.Run("creates user when JIT enabled and user doesn't exist", func(t *testing.T) {
		a := setupAuthForJIT(t)
		a.settingsTable.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "true")

		// User doesn't exist in table
		username := "newuser@example.com"
		password := "testpassword"

		err := a.AuthPlain(username, password)
		if err != nil {
			t.Fatalf("AuthPlain returned error: %v", err)
		}

		// Verify user was created
		key, _ := precis.UsernameCaseMapped.CompareKey(auth.NormalizeUsername(username))
		hash, ok := a.table.(*mockMutableTable).data[key]
		if !ok {
			t.Error("User was not created")
		}
		if hash == "" {
			t.Error("User password hash was not set")
		}
	})

	t.Run("does not create user when JIT disabled and user doesn't exist", func(t *testing.T) {
		a := setupAuthForJIT(t)
		a.settingsTable.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "false")

		// User doesn't exist in table
		username := "newuser2@example.com"
		password := "testpassword"

		err := a.AuthPlain(username, password)
		if err == nil {
			t.Error("Expected error when JIT is disabled and user doesn't exist")
		}
		if !errors.Is(err, module.ErrUnknownCredentials) {
			t.Errorf("Expected ErrUnknownCredentials, got %v", err)
		}

		// Verify user was NOT created
		key, _ := precis.UsernameCaseMapped.CompareKey(auth.NormalizeUsername(username))
		_, ok := a.table.(*mockMutableTable).data[key]
		if ok {
			t.Error("User should not be created when JIT is disabled")
		}
	})

	t.Run("authenticates existing user regardless of JIT setting", func(t *testing.T) {
		a := setupAuthForJIT(t)
		addSHA256()

		username := "existinguser@example.com"
		password := "password"
		key, _ := precis.UsernameCaseMapped.CompareKey(auth.NormalizeUsername(username))

		// Create user manually
		a.table.(*mockMutableTable).SetKey(key, "sha256:U0FMVA==:8PDRAgaUqaLSk34WpYniXjaBgGM93Lc6iF4pw2slthw=")

		// Test with JIT enabled
		a.settingsTable.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "true")
		err := a.AuthPlain(username, password)
		if err != nil {
			t.Errorf("AuthPlain failed with JIT enabled: %v", err)
		}

		// Test with JIT disabled
		a.settingsTable.(*mockMutableTable).SetKey("__JIT_REGISTRATION_ENABLED__", "false")
		err = a.AuthPlain(username, password)
		if err != nil {
			t.Errorf("AuthPlain failed with JIT disabled: %v", err)
		}
	})
}
