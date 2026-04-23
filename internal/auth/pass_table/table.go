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
	"sync"

	"github.com/themadorg/madmail/framework/config"
	modconfig "github.com/themadorg/madmail/framework/config/module"
	"github.com/themadorg/madmail/framework/hooks"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	"golang.org/x/text/secure/precis"
)

type Auth struct {
	modName    string
	instName   string
	inlineArgs []string

	table         module.Table
	settingsTable module.Table
	autoCreate    bool
	jitDomain     string // If set, JIT registration is restricted to this domain

	// credCache mirrors a.table when it implements MutableTable: compare-key → stored value.
	// Populated at Init; reads use RAM first. Mutations update DB and cache together.
	// A cache miss falls back to the backing table once so CLI tools (maddy creds) can
	// change the DB on disk without restarting the server.
	credMu    sync.RWMutex
	credCache map[string]string
}

func New(modName, instName string, _, inlineArgs []string) (module.Module, error) {
	// Register this instance as the settings provider early, so
	// IsLocalOnly() can trigger our Init() if called before us.
	module.RegisterSettingsProviderInstance(instName)

	return &Auth{
		modName:    modName,
		instName:   instName,
		inlineArgs: inlineArgs,
	}, nil
}

func (a *Auth) Init(cfg *config.Map) error {
	cfg.Bool("auto_create", false, false, &a.autoCreate)
	cfg.String("jit_domain", false, false, "", &a.jitDomain)
	if len(a.inlineArgs) != 0 {
		return modconfig.ModuleFromNode("table", a.inlineArgs, cfg.Block, cfg.Globals, &a.table)
	}

	cfg.Custom("table", false, true, nil, modconfig.TableDirective, &a.table)
	cfg.Custom("settings_table", false, false, nil, modconfig.TableDirective, &a.settingsTable)
	if _, err := cfg.Process(); err != nil {
		return err
	}

	if err := a.hydrateCredCache(); err != nil {
		return err
	}

	// Check if logging is disabled dynamically.
	// However, if debug mode is enabled, keep the log output alive
	// so the administrator can see debug messages.
	disabled, _ := a.IsLoggingDisabled()
	if disabled && !log.DefaultLogger.Debug {
		log.DefaultLogger.Out = log.NopOutput{}
	}

	// Register this auth DB as the global settings provider so other
	// modules (smtp, imap, turn) can read port access control settings.
	module.RegisterSettingsProvider(a.GetSetting)

	// Rehydrate credCache on SIGUSR2 (EventReload). This is what makes
	// `maddy accounts ban / delete` take effect on the running daemon
	// without a restart: the CLI mutates SQL on disk and signals the
	// daemon, which drops stale password rows from the in-memory cache.
	hooks.AddHook(hooks.EventReload, func() {
		if err := a.ReloadCredentialsCache(); err != nil {
			log.DefaultLogger.Error("pass_table: reload credentials cache on SIGUSR2 failed", err)
		}
	})

	return nil
}

func (a *Auth) Name() string {
	return a.modName
}

func (a *Auth) InstanceName() string {
	return a.instName
}

func (a *Auth) primaryTableMutable() (module.MutableTable, bool) {
	t, ok := a.table.(module.MutableTable)
	return t, ok
}

// ReloadCredentialsCache implements module.CredentialsCacheReloader.
func (a *Auth) ReloadCredentialsCache() error {
	return a.hydrateCredCache()
}

func (a *Auth) hydrateCredCache() error {
	mtbl, ok := a.primaryTableMutable()
	if !ok {
		a.credMu.Lock()
		a.credCache = nil
		a.credMu.Unlock()
		return nil
	}
	keys, err := mtbl.Keys()
	if err != nil {
		return fmt.Errorf("%s: credentials cache hydrate (keys): %w", a.modName, err)
	}
	ctx := context.TODO()
	newMap := make(map[string]string, len(keys))
	for _, k := range keys {
		v, has, err := mtbl.Lookup(ctx, k)
		if err != nil {
			return fmt.Errorf("%s: credentials cache hydrate (lookup %q): %w", a.modName, k, err)
		}
		if has {
			newMap[k] = v
		}
	}
	a.credMu.Lock()
	a.credCache = newMap
	a.credMu.Unlock()
	return nil
}

func (a *Auth) lookupCred(ctx context.Context, key string) (string, bool, error) {
	if _, ok := a.primaryTableMutable(); !ok {
		return a.table.Lookup(ctx, key)
	}
	a.credMu.RLock()
	cc := a.credCache
	if cc != nil {
		v, ok := cc[key]
		a.credMu.RUnlock()
		if ok {
			return v, true, nil
		}
	} else {
		a.credMu.RUnlock()
		return a.table.Lookup(ctx, key)
	}
	val, found, err := a.table.Lookup(ctx, key)
	if err != nil || !found {
		return val, found, err
	}
	a.credMu.Lock()
	if a.credCache != nil {
		a.credCache[key] = val
	}
	a.credMu.Unlock()
	return val, true, nil
}

func (a *Auth) readFromPrimaryOrSettings(ctx context.Context, key string) (string, bool, error) {
	if a.settingsTable != nil {
		return a.settingsTable.Lookup(ctx, key)
	}
	return a.lookupCred(ctx, key)
}

func (a *Auth) credCachePut(k, v string) {
	if _, ok := a.primaryTableMutable(); !ok {
		return
	}
	a.credMu.Lock()
	defer a.credMu.Unlock()
	if a.credCache == nil {
		return
	}
	a.credCache[k] = v
}

func (a *Auth) credCacheDelete(k string) {
	if _, ok := a.primaryTableMutable(); !ok {
		return
	}
	a.credMu.Lock()
	defer a.credMu.Unlock()
	if a.credCache == nil {
		return
	}
	delete(a.credCache, k)
}

func (a *Auth) Lookup(ctx context.Context, username string) (string, bool, error) {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return "", false, err
	}

	return a.lookupCred(ctx, key)
}

func (a *Auth) GetUserPasswordHash(username string) (string, bool, error) {
	return a.Lookup(context.TODO(), username)
}

func (a *Auth) AuthPlain(username, password string) error {
	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return err
	}

	hash, ok, err := a.lookupCred(context.TODO(), key)
	if !ok {
		// Check JIT registration flag instead of general registration flag
		// This allows /new API to work while disabling automatic account creation on login
		jitEnabled, err := a.IsJitRegistrationEnabled()
		if err != nil {
			return err
		}
		if jitEnabled {
			// Validate the username domain before creating the account.
			// This prevents JIT account creation for arbitrary usernames
			// (e.g., user@wrongdomain, x@y@z, user@%5b1.2.3.4%5d).
			if err := auth.ValidateLoginDomain(username, a.jitDomain); err != nil {
				return module.ErrUnknownCredentials
			}
			// Refuse to resurrect banned usernames through SMTP/IMAP AUTH.
			// Without this check, `maddy accounts ban alice@host` would
			// remove credentials + mail but the next AUTH attempt would
			// JIT-create the account again with the supplied password.
			// The storage.imapsql GetUser path already does the same
			// check; this mirrors it at the auth layer so SMTP submission
			// (which doesn't go through GetUser) is equally protected.
			if blocked, _ := module.IsUsernameBlocked(username); blocked {
				return module.ErrUnknownCredentials
			}
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
	if err := hashVerify(password, parts[1]); err != nil {
		return err
	}

	// Opportunistic migration: re-hash with the current default
	// algorithm so legacy bcrypt entries are upgraded over time.
	if parts[0] != DefaultHash {
		go func() {
			_ = a.SetUserPassword(username, password)
		}()
	}
	return nil
}

func (a *Auth) ListUsers() ([]string, error) {
	tbl, ok := a.table.(module.MutableTable)
	if !ok {
		return nil, fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	a.credMu.RLock()
	cc := a.credCache
	a.credMu.RUnlock()
	if cc != nil {
		l := make([]string, 0, len(cc))
		for k := range cc {
			l = append(l, k)
		}
		return l, nil
	}

	l, err := tbl.Keys()
	if err != nil {
		return nil, fmt.Errorf("%s: list users: %w", a.modName, err)
	}
	return l, nil
}

func (a *Auth) CreateUser(username, password string) error {
	return a.CreateUserHash(username, password, DefaultHash, HashOpts{})
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

	// Defence in depth: AuthPlain already gates JIT on the blocklist, but
	// other call sites (future admin tooling, replay from queues) could
	// reach CreateUserIfNotExists directly. Refuse banned usernames here
	// so "create on first login" is always blocklist-aware.
	if blocked, _ := module.IsUsernameBlocked(username); blocked {
		return fmt.Errorf("%s: create user %s: username is blocklisted", a.modName, key)
	}

	// Compute the password hash first (this is CPU-intensive but doesn't hold any locks)
	hash, err := HashCompute[DefaultHash](HashOpts{}, password)
	if err != nil {
		return fmt.Errorf("%s: create user %s: hash generation: %w", a.modName, key, err)
	}

	// Use SetKey which now uses upsert (INSERT OR REPLACE) to atomically
	// create or update the user. This avoids the race condition where
	// multiple concurrent requests try to create the same user.
	if err := tbl.SetKey(key, DefaultHash+":"+hash); err != nil {
		return fmt.Errorf("%s: create user %s: %w", a.modName, key, err)
	}
	a.credCachePut(key, DefaultHash+":"+hash)
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

	_, ok, err = a.lookupCred(context.TODO(), key)
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
	a.credCachePut(key, hashAlgo+":"+hash)
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
	hash, err := HashCompute[DefaultHash](HashOpts{}, password)
	if err != nil {
		return fmt.Errorf("%s: set password %s: hash generation: %w", a.modName, key, err)
	}

	if err := tbl.SetKey(key, DefaultHash+":"+hash); err != nil {
		return fmt.Errorf("%s: set password %s: %w", a.modName, key, err)
	}
	a.credCachePut(key, DefaultHash+":"+hash)
	return nil
}

func (a *Auth) SetUserPasswordHash(username, hash string) error {
	tbl, ok := a.table.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}

	username = auth.NormalizeUsername(username)
	key, err := precis.UsernameCaseMapped.CompareKey(username)
	if err != nil {
		return fmt.Errorf("%s: set password hash %s (raw): %w", a.modName, username, err)
	}

	if err := tbl.SetKey(key, hash); err != nil {
		return fmt.Errorf("%s: set password hash %s: %w", a.modName, key, err)
	}
	a.credCachePut(key, hash)
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
	a.credCacheDelete(key)
	return nil
}

func (a *Auth) IsRegistrationOpen() (bool, error) {
	val, ok, err := a.readFromPrimaryOrSettings(context.TODO(), "__REGISTRATION_OPEN__")
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
	if err := mtbl.SetKey("__REGISTRATION_OPEN__", val); err != nil {
		return err
	}
	if a.settingsTable == nil {
		a.credCachePut("__REGISTRATION_OPEN__", val)
	}
	return nil
}

func (a *Auth) IsTurnEnabled() (bool, error) {
	val, ok, err := a.readFromPrimaryOrSettings(context.TODO(), "__TURN_ENABLED__")
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
	if err := mtbl.SetKey("__TURN_ENABLED__", val); err != nil {
		return err
	}
	if a.settingsTable == nil {
		a.credCachePut("__TURN_ENABLED__", val)
	}
	return nil
}

func (a *Auth) IsLoggingDisabled() (bool, error) {
	val, ok, err := a.readFromPrimaryOrSettings(context.TODO(), "__LOG_DISABLED__")
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
	if err := mtbl.SetKey("__LOG_DISABLED__", val); err != nil {
		return err
	}
	if a.settingsTable == nil {
		a.credCachePut("__LOG_DISABLED__", val)
	}
	return nil
}

func (a *Auth) IsJitRegistrationEnabled() (bool, error) {
	val, ok, err := a.readFromPrimaryOrSettings(context.TODO(), "__JIT_REGISTRATION_ENABLED__")
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
	if err := mtbl.SetKey("__JIT_REGISTRATION_ENABLED__", val); err != nil {
		return err
	}
	if a.settingsTable == nil {
		a.credCachePut("__JIT_REGISTRATION_ENABLED__", val)
	}
	return nil
}

// GetSetting retrieves a raw string value from the settings table.
// Returns (value, true, nil) if found, ("", false, nil) if not set.
func (a *Auth) GetSetting(key string) (string, bool, error) {
	val, ok, err := a.readFromPrimaryOrSettings(context.TODO(), key)
	if err != nil {
		return "", false, err
	}
	if !ok {
		return "", false, nil
	}
	return val, true, nil
}

// SetSetting stores a raw string value in the settings table.
func (a *Auth) SetSetting(key, value string) error {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	mtbl, ok := tbl.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}
	if err := mtbl.SetKey(key, value); err != nil {
		return err
	}
	if a.settingsTable == nil {
		a.credCachePut(key, value)
	}
	return nil
}

// DeleteSetting removes a key from the settings table.
func (a *Auth) DeleteSetting(key string) error {
	tbl := a.table
	if a.settingsTable != nil {
		tbl = a.settingsTable
	}

	mtbl, ok := tbl.(module.MutableTable)
	if !ok {
		return fmt.Errorf("%s: table is not mutable, no management functionality available", a.modName)
	}
	if err := mtbl.RemoveKey(key); err != nil {
		return err
	}
	if a.settingsTable == nil {
		a.credCacheDelete(key)
	}
	return nil
}

func init() {
	module.Register("auth.pass_table", New)
}
