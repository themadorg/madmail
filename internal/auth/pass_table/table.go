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
	"fmt"
	"strings"

	"github.com/themadorg/madmail/framework/config"
	modconfig "github.com/themadorg/madmail/framework/config/module"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	"golang.org/x/crypto/bcrypt"
	"golang.org/x/text/secure/precis"
)

type Auth struct {
	modName    string
	instName   string
	inlineArgs []string

	table         module.Table
	settingsTable module.Table
	autoCreate    bool
}

func New(modName, instName string, _, inlineArgs []string) (module.Module, error) {
	return &Auth{
		modName:    modName,
		instName:   instName,
		inlineArgs: inlineArgs,
	}, nil
}

func (a *Auth) Init(cfg *config.Map) error {
	cfg.Bool("auto_create", false, false, &a.autoCreate)
	if len(a.inlineArgs) != 0 {
		return modconfig.ModuleFromNode("table", a.inlineArgs, cfg.Block, cfg.Globals, &a.table)
	}

	cfg.Custom("table", false, true, nil, modconfig.TableDirective, &a.table)
	cfg.Custom("settings_table", false, false, nil, modconfig.TableDirective, &a.settingsTable)
	if _, err := cfg.Process(); err != nil {
		return err
	}

	// Check if logging is disabled dynamically.
	// However, if debug mode is enabled, keep the log output alive
	// so the administrator can see debug messages.
	disabled, _ := a.IsLoggingDisabled()
	if disabled && !log.DefaultLogger.Debug {
		log.DefaultLogger.Out = log.NopOutput{}
	}

	return nil
}

func (a *Auth) Name() string {
	return a.modName
}

func (a *Auth) InstanceName() string {
	return a.instName
}

func (a *Auth) Lookup(ctx context.Context, username string) (string, bool, error) {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return "", false, err
	}

	return a.table.Lookup(ctx, key)
}

func (a *Auth) AuthPlain(username, password string) error {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return err
	}

	hash, ok, err := a.table.Lookup(context.TODO(), key)
	if !ok {
		// Check JIT registration flag instead of general registration flag
		// This allows /new API to work while disabling automatic account creation on login
		jitEnabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			return err
		}
		if jitEnabled {
			// Use CreateUserIfNotExists which uses upsert to avoid race conditions
			// when multiple concurrent logins try to create the same user
			if err := a.CreateUserIfNotExists(username, password); err != nil {
				return fmt.Errorf("%s: auto-create failed for %s: %w", a.modName, key, err)
			}
			return nil
		}
		return module.ErrUnknownCredentials
	}
	if err != nil {
		return err
	}

	parts := strings.SplitN(hash, ":", 2)
	if len(parts) != 2 {
		return fmt.Errorf("%s: auth plain %s: no hash tag", a.modName, key)
	}
	hashVerify := HashVerify[parts[0]]
	if hashVerify == nil {
		return fmt.Errorf("%s: auth plain %s: unknown hash: %s", a.modName, key, parts[0])
	}
	return hashVerify(password, parts[1])
}

func (a *Auth) ListUsers() ([]string, error) {
	tbl, ok := a.table.(module.MutableTable)
	if !ok {
		return nil, fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	l, err := tbl.Keys()
	if err != nil {
		return nil, fmt.Errorf("%s: list users: %w", a.modName, err)
	}
	return l, nil
}

func (a *Auth) CreateUser(username, password string) error {
	return a.CreateUserHash(username, password, HashBcrypt, HashOpts{
		BcryptCost: bcrypt.DefaultCost,
	})
}

// CreateUserIfNotExists creates a user if they don't already exist.
// This is optimized for concurrent auto-create scenarios during login.
// Unlike CreateUser, it doesn't fail if the user already exists - it just
// returns nil (since the goal is to ensure the user exists).
// It also skips the initial Lookup to avoid the race condition where
// multiple concurrent requests all see "user not found" and then all try to create.
func (a *Auth) CreateUserIfNotExists(username, password string) error {
	tbl, ok := a.table.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("%s: create user %s (raw): %w", a.modName, username, err)
	}

	// Compute the password hash first (this is CPU-intensive but doesn't hold any locks)
	hash, err := HashCompute[HashBcrypt](HashOpts{BcryptCost: bcrypt.DefaultCost}, password)
	if err != nil {
		return fmt.Errorf("%s: create user %s: hash generation: %w", a.modName, key, err)
	}

	// Use SetKey which now uses upsert (INSERT OR REPLACE) to atomically
	// create or update the user. This avoids the race condition where
	// multiple concurrent requests try to create the same user.
	if err := tbl.SetKey(key, HashBcrypt+":"+hash); err != nil {
		return fmt.Errorf("%s: create user %s: %w", a.modName, key, err)
	}
	return nil
}

func (a *Auth) CreateUserHash(username, password string, hashAlgo string, opts HashOpts) error {
	tbl, ok := a.table.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	if _, ok := HashCompute[hashAlgo]; !ok {
		return fmt.Errorf("%s: unknown hash function: %v", a.modName, hashAlgo)
	}

	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("%s: create user %s (raw): %w", a.modName, username, err)
	}

	_, ok, err = tbl.Lookup(context.TODO(), key)
	if err != nil {
		return fmt.Errorf("%s: create user %s: %w", a.modName, key, err)
	}
	if ok {
		return fmt.Errorf("%s: credentials for %s already exist", a.modName, key)
	}

	hash, err := HashCompute[hashAlgo](opts, password)
	if err != nil {
		return fmt.Errorf("%s: create user %s: hash generation: %w", a.modName, key, err)
	}

	if err := tbl.SetKey(key, hashAlgo+":"+hash); err != nil {
		return fmt.Errorf("%s: create user %s: %w", a.modName, key, err)
	}
	return nil
}

func (a *Auth) SetUserPassword(username, password string) error {
	tbl, ok := a.table.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("%s: set password %s (raw): %w", a.modName, username, err)
	}

	// TODO: Allow to customize hash function.
	hash, err := HashCompute[HashBcrypt](HashOpts{
		BcryptCost: bcrypt.DefaultCost,
	}, password)
	if err != nil {
		return fmt.Errorf("%s: set password %s: hash generation: %w", a.modName, key, err)
	}

	if err := tbl.SetKey(key, "bcrypt:"+hash); err != nil {
		return fmt.Errorf("%s: set password %s: %w", a.modName, key, err)
	}
	return nil
}

func (a *Auth) DeleteUser(username string) error {
	tbl, ok := a.table.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("%s: del user %s (raw): %w", a.modName, username, err)
	}

	if err := tbl.RemoveKey(key); err != nil {
		return fmt.Errorf("%s: del user %s: %w", a.modName, key, err)
	}
	return nil
}

func (a *Auth) IsRegistrationOpen() (bool, error) {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	val, ok, err := tbl.Lookup(context.TODO(), "__REGISTRATION_OPEN__")
	if err != nil {
		return false, err
	}
	if !ok {
		// Fallback to static config
		return a.autoCreate, nil
	}
	return val == "true", nil
}

func (a *Auth) SetRegistrationOpen(open bool) error {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	mtbl, ok := tbl.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}
	val := "false"
	if open {
		val = "true"
	}
	return mtbl.SetKey("__REGISTRATION_OPEN__", val)
}

func (a *Auth) IsTurnEnabled() (bool, error) {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	val, ok, err := tbl.Lookup(context.TODO(), "__TURN_ENABLED__")
	if err != nil {
		return false, err
	}
	if !ok {
		// Default to true if not set
		return true, nil
	}
	return val == "true", nil
}

func (a *Auth) SetTurnEnabled(enabled bool) error {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	mtbl, ok := tbl.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}
	val := "false"
	if enabled {
		val = "true"
	}
	return mtbl.SetKey("__TURN_ENABLED__", val)
}

func (a *Auth) IsLoggingDisabled() (bool, error) {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	val, ok, err := tbl.Lookup(context.TODO(), "__LOG_DISABLED__")
	if err != nil {
		return false, err
	}
	if !ok {
		return false, nil
	}
	return val == "true", nil
}

func (a *Auth) SetLoggingDisabled(disabled bool) error {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	mtbl, ok := tbl.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	val := "false"
	if disabled {
		val = "true"
		// Only suppress output when debug mode is not active.
		// Debug flag takes priority over the No Log policy.
		if !log.DefaultLogger.Debug {
			log.DefaultLogger.Out = log.NopOutput{}
		}
	}
	return mtbl.SetKey("__LOG_DISABLED__", val)
}

func (a *Auth) IsJitRegistrationEnabled() (bool, error) {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	val, ok, err := tbl.Lookup(context.TODO(), "__JIT_REGISTRATION_ENABLED__")
	if err != nil {
		return false, err
	}
	if !ok {
		// Default to same as registration open if not explicitly set
		return a.IsRegistrationOpen()
	}
	return val == "true", nil
}

func (a *Auth) SetJitRegistrationEnabled(enabled bool) error {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	mtbl, ok := tbl.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}
	val := "false"
	if enabled {
		val = "true"
	}
	return mtbl.SetKey("__JIT_REGISTRATION_ENABLED__", val)
}

func init() {
	module.Register("auth.pass_table", New)
}
