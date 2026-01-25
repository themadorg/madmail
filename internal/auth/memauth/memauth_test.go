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

package memauth

import (
	"testing"

	"github.com/themadorg/madmail/framework/module"
)

func TestCreateAndAuth(t *testing.T) {
	a := &Auth{
		instName:   "test",
		autoCreate: false,
	}

	// Create a user
	if err := a.CreateUser("testuser@example.com", "password123"); err != nil {
		t.Fatalf("CreateUser failed: %v", err)
	}

	// Auth with correct password should succeed
	if err := a.AuthPlain("testuser@example.com", "password123"); err != nil {
		t.Errorf("AuthPlain with correct password failed: %v", err)
	}

	// Auth with wrong password should fail
	if err := a.AuthPlain("testuser@example.com", "wrongpassword"); err != module.ErrUnknownCredentials {
		t.Errorf("AuthPlain with wrong password should return ErrUnknownCredentials, got: %v", err)
	}

	// Auth with non-existent user should fail
	if err := a.AuthPlain("nonexistent@example.com", "password"); err != module.ErrUnknownCredentials {
		t.Errorf("AuthPlain with non-existent user should return ErrUnknownCredentials, got: %v", err)
	}
}

func TestAutoCreate(t *testing.T) {
	a := &Auth{
		instName:       "test",
		autoCreate:     true,
		minPasswordLen: 12,
	}

	// Auth with short password should fail (trust-on-first-login requires 12+ chars)
	if err := a.AuthPlain("shortpass@example.com", "short"); err != module.ErrUnknownCredentials {
		t.Errorf("AuthPlain with short password should return ErrUnknownCredentials, got: %v", err)
	}

	// Auth with 12+ char password should auto-create
	if err := a.AuthPlain("newuser@example.com", "password12chars"); err != nil {
		t.Errorf("AuthPlain with auto_create and 12+ char password should succeed: %v", err)
	}

	// Should be able to auth again with same password
	if err := a.AuthPlain("newuser@example.com", "password12chars"); err != nil {
		t.Errorf("AuthPlain after auto-create failed: %v", err)
	}

	// Wrong password should still fail
	if err := a.AuthPlain("newuser@example.com", "wrongpassword1"); err != module.ErrUnknownCredentials {
		t.Errorf("AuthPlain with wrong password should return ErrUnknownCredentials, got: %v", err)
	}
}

func TestListUsers(t *testing.T) {
	a := &Auth{
		instName: "test",
	}

	// Create some users
	users := []string{"user1@example.com", "user2@example.com", "user3@example.com"}
	for _, user := range users {
		if err := a.CreateUser(user, "password"); err != nil {
			t.Fatalf("CreateUser failed: %v", err)
		}
	}

	// List users
	listed, err := a.ListUsers()
	if err != nil {
		t.Fatalf("ListUsers failed: %v", err)
	}

	if len(listed) != len(users) {
		t.Errorf("ListUsers returned %d users, expected %d", len(listed), len(users))
	}
}

func TestDeleteUser(t *testing.T) {
	a := &Auth{
		instName: "test",
	}

	// Create a user
	if err := a.CreateUser("delete_me@example.com", "password"); err != nil {
		t.Fatalf("CreateUser failed: %v", err)
	}

	// Auth should work
	if err := a.AuthPlain("delete_me@example.com", "password"); err != nil {
		t.Errorf("AuthPlain before delete failed: %v", err)
	}

	// Delete user
	if err := a.DeleteUser("delete_me@example.com"); err != nil {
		t.Fatalf("DeleteUser failed: %v", err)
	}

	// Auth should fail
	if err := a.AuthPlain("delete_me@example.com", "password"); err != module.ErrUnknownCredentials {
		t.Errorf("AuthPlain after delete should return ErrUnknownCredentials, got: %v", err)
	}
}

func TestSetUserPassword(t *testing.T) {
	a := &Auth{
		instName: "test",
	}

	// Create a user
	if err := a.CreateUser("changepass@example.com", "oldpassword"); err != nil {
		t.Fatalf("CreateUser failed: %v", err)
	}

	// Auth with old password should work
	if err := a.AuthPlain("changepass@example.com", "oldpassword"); err != nil {
		t.Errorf("AuthPlain with old password failed: %v", err)
	}

	// Change password
	if err := a.SetUserPassword("changepass@example.com", "newpassword"); err != nil {
		t.Fatalf("SetUserPassword failed: %v", err)
	}

	// Auth with old password should fail
	if err := a.AuthPlain("changepass@example.com", "oldpassword"); err != module.ErrUnknownCredentials {
		t.Errorf("AuthPlain with old password after change should return ErrUnknownCredentials, got: %v", err)
	}

	// Auth with new password should work
	if err := a.AuthPlain("changepass@example.com", "newpassword"); err != nil {
		t.Errorf("AuthPlain with new password failed: %v", err)
	}
}

func TestSettings(t *testing.T) {
	a := &Auth{
		instName:   "test",
		autoCreate: false,
	}

	// Test registration open
	open, err := a.IsRegistrationOpen()
	if err != nil {
		t.Errorf("IsRegistrationOpen failed: %v", err)
	}
	if open {
		t.Error("Registration should be closed by default when auto_create is false")
	}

	if err := a.SetRegistrationOpen(true); err != nil {
		t.Errorf("SetRegistrationOpen failed: %v", err)
	}

	open, err = a.IsRegistrationOpen()
	if err != nil {
		t.Errorf("IsRegistrationOpen failed: %v", err)
	}
	if !open {
		t.Error("Registration should be open after SetRegistrationOpen(true)")
	}

	// Test TURN enabled
	turnEnabled, err := a.IsTurnEnabled()
	if err != nil {
		t.Errorf("IsTurnEnabled failed: %v", err)
	}
	if !turnEnabled {
		t.Error("TURN should be enabled by default")
	}

	if err := a.SetTurnEnabled(false); err != nil {
		t.Errorf("SetTurnEnabled failed: %v", err)
	}

	turnEnabled, err = a.IsTurnEnabled()
	if err != nil {
		t.Errorf("IsTurnEnabled failed: %v", err)
	}
	if turnEnabled {
		t.Error("TURN should be disabled after SetTurnEnabled(false)")
	}
}

func TestDuplicateUser(t *testing.T) {
	a := &Auth{
		instName: "test",
	}

	// Create a user
	if err := a.CreateUser("duplicate@example.com", "password"); err != nil {
		t.Fatalf("CreateUser failed: %v", err)
	}

	// Creating the same user again should fail
	if err := a.CreateUser("duplicate@example.com", "password"); err == nil {
		t.Error("CreateUser with duplicate username should fail")
	}
}
