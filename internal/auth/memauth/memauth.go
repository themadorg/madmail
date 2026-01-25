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

// Package memauth implements an in-memory authentication module.
// Credentials are stored in RAM in plaintext and will be lost on restart.
// This is suitable for simple/ephemeral deployments where persistent
// credential storage is not required.
package memauth

import (
	"context"
	"fmt"
	"sync"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	"golang.org/x/text/secure/precis"
)

// Auth implements in-memory authentication storage.
type Auth struct {
	instName string
	Log      log.Logger

	// credentials maps username -> plaintext password
	credentials sync.Map

	// settings stores configuration flags
	settings sync.Map

	// autoCreate enables automatic account creation on first login
	autoCreate bool
}

// New creates a new in-memory auth module.
func New(_, instName string, _, _ []string) (module.Module, error) {
	return &Auth{
		instName: instName,
		Log:      log.Logger{Name: "memauth"},
	}, nil
}

func (a *Auth) Name() string {
	return "memauth"
}

func (a *Auth) InstanceName() string {
	return a.instName
}

func (a *Auth) Init(cfg *config.Map) error {
	cfg.Bool("debug", true, false, &a.Log.Debug)
	cfg.Bool("auto_create", false, false, &a.autoCreate)

	if _, err := cfg.Process(); err != nil {
		return err
	}

	a.Log.Debugln("in-memory auth initialized")
	return nil
}

// AuthPlain authenticates a user with username and password.
func (a *Auth) AuthPlain(username, password string) error {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return err
	}

	storedPassword, ok := a.credentials.Load(key)
	if !ok {
		// Check JIT registration
		jitEnabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			return err
		}
		if jitEnabled {
			if err := a.CreateUser(username, password); err != nil {
				return fmt.Errorf("memauth: auto-create failed for %s: %w", key, err)
			}
			return nil
		}
		return module.ErrUnknownCredentials
	}

	// Compare plaintext passwords
	if storedPassword.(string) != password {
		return module.ErrUnknownCredentials
	}

	return nil
}

// ListUsers returns all registered usernames.
func (a *Auth) ListUsers() ([]string, error) {
	var users []string
	a.credentials.Range(func(key, _ interface{}) bool {
		users = append(users, key.(string))
		return true
	})
	return users, nil
}

// CreateUser creates a new user with the given password.
func (a *Auth) CreateUser(username, password string) error {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("memauth: create user %s: %w", username, err)
	}

	// Check if user already exists
	if _, ok := a.credentials.Load(key); ok {
		return fmt.Errorf("memauth: user %s already exists", key)
	}

	// Store plaintext password
	a.credentials.Store(key, password)
	a.Log.Debugf("created user: %s", key)
	return nil
}

// SetUserPassword updates the password for an existing user.
func (a *Auth) SetUserPassword(username, password string) error {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("memauth: set password %s: %w", username, err)
	}

	// Store plaintext password
	a.credentials.Store(key, password)
	return nil
}

// DeleteUser removes a user from the store.
func (a *Auth) DeleteUser(username string) error {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("memauth: delete user %s: %w", username, err)
	}

	a.credentials.Delete(key)
	a.Log.Debugf("deleted user: %s", key)
	return nil
}

// IsRegistrationOpen returns whether new user registration is open.
func (a *Auth) IsRegistrationOpen() (bool, error) {
	val, ok := a.settings.Load("__REGISTRATION_OPEN__")
	if !ok {
		return a.autoCreate, nil
	}
	return val.(bool), nil
}

// SetRegistrationOpen sets whether new user registration is open.
func (a *Auth) SetRegistrationOpen(open bool) error {
	a.settings.Store("__REGISTRATION_OPEN__", open)
	return nil
}

// IsJitRegistrationEnabled returns whether JIT (Just-In-Time) registration is enabled.
func (a *Auth) IsJitRegistrationEnabled() (bool, error) {
	val, ok := a.settings.Load("__JIT_REGISTRATION_ENABLED__")
	if !ok {
		return a.IsRegistrationOpen()
	}
	return val.(bool), nil
}

// SetJitRegistrationEnabled sets whether JIT registration is enabled.
func (a *Auth) SetJitRegistrationEnabled(enabled bool) error {
	a.settings.Store("__JIT_REGISTRATION_ENABLED__", enabled)
	return nil
}

// IsTurnEnabled returns whether TURN is enabled.
func (a *Auth) IsTurnEnabled() (bool, error) {
	val, ok := a.settings.Load("__TURN_ENABLED__")
	if !ok {
		return true, nil // Default to enabled
	}
	return val.(bool), nil
}

// SetTurnEnabled sets whether TURN is enabled.
func (a *Auth) SetTurnEnabled(enabled bool) error {
	a.settings.Store("__TURN_ENABLED__", enabled)
	return nil
}

// IsLoggingDisabled returns whether logging is disabled.
func (a *Auth) IsLoggingDisabled() (bool, error) {
	val, ok := a.settings.Load("__LOG_DISABLED__")
	if !ok {
		return false, nil
	}
	return val.(bool), nil
}

// SetLoggingDisabled sets whether logging is disabled.
func (a *Auth) SetLoggingDisabled(disabled bool) error {
	a.settings.Store("__LOG_DISABLED__", disabled)
	if disabled {
		log.DefaultLogger.Out = log.NopOutput{}
	}
	return nil
}

// Lookup implements module.Table for compatibility.
func (a *Auth) Lookup(ctx context.Context, key string) (string, bool, error) {
	key = auth.NormalizeUsername(key)
	normalizedKey, err := precis.UsernameCaseMapped.CompareKey(key)
	if err != nil {
		return "", false, err
	}

	val, ok := a.credentials.Load(normalizedKey)
	if !ok {
		return "", false, nil
	}
	return val.(string), true, nil
}

func init() {
	module.Register("auth.memauth", New)
}

// Compile-time interface check
var _ module.PlainUserDB = (*Auth)(nil)
