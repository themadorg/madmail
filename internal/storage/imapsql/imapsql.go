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

// Package imapsql implements SQL-based storage module
// using go-imap-sql library (github.com/foxcpp/go-imap-sql).
//
// Interfaces implemented:
// - module.StorageBackend
// - module.PlainAuth
// - module.DeliveryTarget
package imapsql

import (
	"context"
	"crypto/sha1"
	"database/sql"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime/debug"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/emersion/go-imap"
	sortthread "github.com/emersion/go-imap-sortthread"
	"github.com/emersion/go-imap/backend"
	mess "github.com/foxcpp/go-imap-mess"
	imapsql "github.com/foxcpp/go-imap-sql"
	"github.com/themadorg/madmail/framework/config"
	modconfig "github.com/themadorg/madmail/framework/config/module"
	"github.com/themadorg/madmail/framework/dns"
	"github.com/themadorg/madmail/framework/hooks"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/authz"
	"github.com/themadorg/madmail/internal/quota"
	"github.com/themadorg/madmail/internal/updatepipe"
	"github.com/themadorg/madmail/internal/updatepipe/pubsub"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"

	_ "github.com/lib/pq"
)

type Storage struct {
	Back     *imapsql.Backend
	GORMDB   *gorm.DB
	instName string
	Log      log.Logger

	junkMbox string

	driver string
	dsn    []string

	resolver dns.Resolver

	updPipe      updatepipe.P
	updPushStop  chan struct{}
	outboundUpds chan mess.Update

	filters module.IMAPFilter

	deliveryMap       module.Table
	deliveryNormalize func(context.Context, string) (string, error)
	authMap           module.Table
	authNormalize     func(context.Context, string) (string, error)

	retention time.Duration

	unusedAccountRetention time.Duration
	authDBName             string

	defaultQuota int64
	autoCreate   bool

	settingsTable module.Table

	// QuotaCache is the in-memory quota cache for fast per-user lookups.
	// Exported so the IMAP endpoint, admin API, and CLI can access it.
	QuotaCache *quota.Cache

	// blockedSet mirrors blocked_users (username only) for O(1) IsBlocked
	// without a DB query on the hot path. Refreshed at startup, on SIGUSR2
	// (EventReload), and write-through on BlockUser/UnblockUser. If reload
	// never succeeded (nil), IsBlocked falls back to the database.
	blockedMu  sync.RWMutex
	blockedSet map[string]struct{}

	blobStore module.BlobStore
}

func (store *Storage) Name() string {
	return "imapsql"
}

func (store *Storage) InstanceName() string {
	return store.instName
}

// GetGORMDB implements module.GORMProvider.
// It exposes the shared GORM database connection so other modules
// (e.g. target.remote for DNS cache) can add their tables to the
// same database.
func (store *Storage) GetGORMDB() *gorm.DB {
	return store.GORMDB
}

func New(_, instName string, _, inlineArgs []string) (module.Module, error) {
	store := &Storage{
		instName: instName,
		Log:      log.Logger{Name: "imapsql"},
		resolver: dns.DefaultResolver(),
	}
	if len(inlineArgs) != 0 {
		if len(inlineArgs) == 1 {
			return nil, errors.New("imapsql: expected at least 2 arguments")
		}

		store.driver = inlineArgs[0]
		store.dsn = inlineArgs[1:]
	}
	return store, nil
}

func (store *Storage) Init(cfg *config.Map) error {
	var (
		driver            string
		dsn               []string
		appendlimitVal    int64 = -1
		compression       []string
		authNormalize     string
		deliveryNormalize string

		blobStore module.BlobStore
	)

	opts := imapsql.Opts{}
	cfg.String("driver", false, false, store.driver, &driver)
	cfg.StringList("dsn", false, false, store.dsn, &dsn)
	cfg.Callback("fsstore", func(m *config.Map, node config.Node) error {
		store.Log.Msg("'fsstore' directive is deprecated, use 'msg_store fs' instead")
		return modconfig.ModuleFromNode("storage.blob", append([]string{"fs"}, node.Args...),
			node, m.Globals, &blobStore)
	})
	cfg.Custom("msg_store", false, false, func() (interface{}, error) {
		var store module.BlobStore
		err := modconfig.ModuleFromNode("storage.blob", []string{"fs", "messages"},
			config.Node{}, nil, &store)
		return store, err
	}, func(m *config.Map, node config.Node) (interface{}, error) {
		var store module.BlobStore
		err := modconfig.ModuleFromNode("storage.blob", node.Args,
			node, m.Globals, &store)
		return store, err
	}, &blobStore)
	cfg.StringList("compression", false, false, []string{"off"}, &compression)
	cfg.DataSize("appendlimit", false, false, 32*1024*1024, &appendlimitVal)
	cfg.Bool("debug", true, false, &store.Log.Debug)
	cfg.Int("sqlite3_cache_size", false, false, 0, &opts.CacheSize)
	cfg.Int("sqlite3_busy_timeout", false, false, 5000, &opts.BusyTimeout)
	cfg.Bool("disable_recent", false, true, &opts.DisableRecent)
	cfg.String("junk_mailbox", false, false, "Junk", &store.junkMbox)
	cfg.Custom("imap_filter", false, false, func() (interface{}, error) {
		return nil, nil
	}, func(m *config.Map, node config.Node) (interface{}, error) {
		var filter module.IMAPFilter
		err := modconfig.GroupFromNode("imap_filters", node.Args, node, m.Globals, &filter)
		return filter, err
	}, &store.filters)
	cfg.Custom("auth_map", false, false, func() (interface{}, error) {
		return nil, nil
	}, modconfig.TableDirective, &store.authMap)
	cfg.String("auth_normalize", false, false, "auto", &authNormalize)
	cfg.Custom("delivery_map", false, false, func() (interface{}, error) {
		return nil, nil
	}, modconfig.TableDirective, &store.deliveryMap)
	cfg.String("delivery_normalize", false, false, "precis_casefold_email", &deliveryNormalize)
	cfg.Duration("retention", false, false, 0, &store.retention)
	cfg.Duration("unused_account_retention", false, false, 0, &store.unusedAccountRetention)
	cfg.String("auth_db", false, false, "", &store.authDBName)
	cfg.DataSize("default_quota", false, false, 1073741824, &store.defaultQuota)
	cfg.Bool("auto_create", false, false, &store.autoCreate)
	cfg.Custom("settings_table", false, false, func() (interface{}, error) {
		return nil, nil
	}, modconfig.TableDirective, &store.settingsTable)

	if _, err := cfg.Process(); err != nil {
		return err
	}

	if dsn == nil {
		return errors.New("imapsql: dsn is required")
	}
	if driver == "" {
		return errors.New("imapsql: driver is required")
	}

	// madmail is pure-Go: the mattn/go-sqlite3 backend is no longer compiled
	// in, so any legacy "sqlite3" driver name gets rewritten to "sqlite"
	// (the database/sql driver name registered by modernc.org/sqlite).
	if driver == "sqlite3" {
		store.Log.Debugln("driver=sqlite3 requested, using pure-Go modernc.org/sqlite (impl=" + sqliteImpl + ")")
		driver = "sqlite"
	}

	deliveryNormFunc, ok := authz.NormalizeFuncs[deliveryNormalize]
	if !ok {
		return errors.New("imapsql: unknown normalization function: " + deliveryNormalize)
	}
	store.deliveryNormalize = func(ctx context.Context, s string) (string, error) {
		return deliveryNormFunc(s)
	}
	if store.deliveryMap != nil {
		store.deliveryNormalize = func(ctx context.Context, email string) (string, error) {
			email, err := deliveryNormFunc(email)
			if err != nil {
				return "", err
			}
			mapped, ok, err := store.deliveryMap.Lookup(ctx, email)
			if err != nil || !ok {
				return "", userDoesNotExist(err)
			}
			return mapped, nil
		}
	}

	if authNormalize != "auto" {
		store.Log.Msg("auth_normalize in storage.imapsql is deprecated and will be removed in the next release, use storage_map in imap config instead")
	}
	authNormFunc, ok := authz.NormalizeFuncs[authNormalize]
	if !ok {
		return errors.New("imapsql: unknown normalization function: " + authNormalize)
	}
	store.authNormalize = func(ctx context.Context, s string) (string, error) {
		return authNormFunc(s)
	}
	if store.authMap != nil {
		store.Log.Msg("auth_map in storage.imapsql is deprecated and will be removed in the next release, use storage_map in imap config instead")
		store.authNormalize = func(ctx context.Context, username string) (string, error) {
			username, err := authNormFunc(username)
			if err != nil {
				return "", err
			}
			mapped, ok, err := store.authMap.Lookup(ctx, username)
			if err != nil || !ok {
				return "", userDoesNotExist(err)
			}
			return mapped, nil
		}
	}

	opts.Log = &store.Log

	if appendlimitVal == -1 {
		opts.MaxMsgBytes = nil
	} else {
		// int is 32-bit on some platforms, so cut off values we can't actually
		// use.
		if int64(uint32(appendlimitVal)) != appendlimitVal {
			return errors.New("imapsql: appendlimit value is too big")
		}
		opts.MaxMsgBytes = new(uint32)
		*opts.MaxMsgBytes = uint32(appendlimitVal)
	}
	var err error

	dsnStr := strings.Join(dsn, " ")
	if driver == "sqlite" && os.Getenv("MADDY_SQLITE_UNSAFE_SYNC_OFF") == "1" {
		// WARNING: this reduces durability and can corrupt data on crash.
		// The DSN fragments use modernc's `_pragma=NAME(VALUE)` URL-param
		// syntax (mattn's `_journal_mode=WAL&_synchronous=OFF` shape was
		// dropped together with the mattn driver itself).
		sep := "?"
		if strings.Contains(dsnStr, "?") {
			sep = "&"
		}
		dsnStr = dsnStr + sep + "_pragma=journal_mode(WAL)&_pragma=synchronous(OFF)"
	}

	if len(compression) != 0 {
		switch compression[0] {
		case "zstd", "lz4":
			opts.CompressAlgo = compression[0]
			if len(compression) == 2 {
				opts.CompressAlgoParams = compression[1]
				if _, err := strconv.Atoi(compression[1]); err != nil {
					return errors.New("imapsql: first argument for lz4 and zstd is compression level")
				}
			}
			if len(compression) > 2 {
				return errors.New("imapsql: expected at most 2 arguments")
			}
		case "off":
			if len(compression) > 1 {
				return errors.New("imapsql: expected at most 1 arguments")
			}
		default:
			return errors.New("imapsql: unknown compression algorithm")
		}
	}

	store.Back, err = imapsql.New(driver, dsnStr, ExtBlobStore{Base: blobStore, Log: store.Log}, opts)
	if err != nil {
		return fmt.Errorf("imapsql: %s", err)
	}
	store.blobStore = blobStore

	store.Log.Debugln("go-imap-sql version", imapsql.VersionStr)

	store.driver = driver
	store.dsn = dsn

	store.GORMDB, err = mdb.New(driver, dsn, store.Log.Debug)
	if err != nil {
		return fmt.Errorf("imapsql: gorm init failed: %w", err)
	}

	if err := store.initQuotaTable(); err != nil {
		return fmt.Errorf("imapsql: quota table init failed: %w", err)
	}

	if err := store.MigrateFirstLoginFromCreatedAt(); err != nil {
		store.Log.Error("failed to migrate first login times", err)
	}

	if store.unusedAccountRetention > 0 && store.authDBName == "" {
		return fmt.Errorf("imapsql: auth_db is required when unused_account_retention is set")
	}

	if store.retention > 0 {
		go store.cleanupLoop()
	}
	if store.unusedAccountRetention > 0 {
		go store.cleanupUnusedAccountsLoop()
	}

	return nil
}

func (store *Storage) IsRegistrationOpen() (bool, error) {
	if store.settingsTable == nil {
		return store.autoCreate, nil
	}

	val, ok, err := store.settingsTable.Lookup(context.TODO(), "__REGISTRATION_OPEN__")
	if err != nil {
		return false, err
	}
	if !ok {
		return store.autoCreate, nil
	}
	return val == "true", nil
}

func (store *Storage) IsJitRegistrationEnabled() (bool, error) {
	if store.settingsTable == nil {
		// Default to same as registration open if no settings table
		return store.IsRegistrationOpen()
	}

	val, ok, err := store.settingsTable.Lookup(context.TODO(), "__JIT_REGISTRATION_ENABLED__")
	if err != nil {
		return false, err
	}
	if !ok {
		// Default to same as registration open if not explicitly set
		return store.IsRegistrationOpen()
	}
	return val == "true", nil
}

func (store *Storage) initQuotaTable() error {
	if err := store.GORMDB.AutoMigrate(&mdb.Quota{}); err != nil {
		return err
	}
	if err := store.GORMDB.AutoMigrate(&mdb.BlockedUser{}); err != nil {
		return err
	}
	if err := store.GORMDB.AutoMigrate(&mdb.MessageStat{}); err != nil {
		return err
	}
	if err := store.GORMDB.AutoMigrate(&mdb.RegistrationToken{}); err != nil {
		return err
	}

	// Load persisted message counters into global atomic counters.
	var stat mdb.MessageStat
	if err := store.GORMDB.Where("name = ?", "sent_messages").First(&stat).Error; err == nil {
		module.SetSentMessages(stat.Count)
	}
	stat = mdb.MessageStat{} // reset to avoid GORM reusing the primary key in the next query
	if err := store.GORMDB.Where("name = ?", "outbound_messages").First(&stat).Error; err == nil {
		module.SetOutboundMessages(stat.Count)
	}
	stat = mdb.MessageStat{} // reset
	if err := store.GORMDB.Where("name = ?", "received_messages").First(&stat).Error; err == nil {
		module.SetReceivedMessages(stat.Count)
	}

	// Initialize the in-memory quota cache.
	store.QuotaCache = quota.New(store.Log)
	if err := store.populateQuotaCache(); err != nil {
		store.Log.Error("failed to populate quota cache, will use DB fallback", err)
	}

	// Blocked-user cache: load before RegisterBlocklistChecker.
	if err := store.reloadBlockedCache(); err != nil {
		store.Log.Error("imapsql: load blocklist cache failed (IsBlocked will use DB until reload)", err)
	}

	// Rehydrate the quota and blocklist caches on SIGUSR2 (EventReload) so
	// that CLI mutations (ban, delete, quota edits) picked up from disk
	// without restarting the daemon.
	hooks.AddHook(hooks.EventReload, func() {
		if err := store.ReloadQuotaCache(); err != nil {
			store.Log.Error("imapsql: reload quota cache on SIGUSR2 failed", err)
		}
		if err := store.reloadBlockedCache(); err != nil {
			store.Log.Error("imapsql: reload blocklist cache on SIGUSR2 failed", err)
		}
	})

	// Expose this storage's blocklist to other modules (notably
	// pass_table.AuthPlain) so JIT account creation on SMTP/IMAP auth
	// cannot silently resurrect a banned user.
	module.RegisterBlocklistChecker(store.IsBlocked)

	// Background goroutine to flush counters to DB every 30s.
	go store.flushMessageCounters()

	return nil
}

// ReloadQuotaCache implements module.QuotaCacheReloader. It rebuilds the
// in-memory quota view from the database (e.g. after CLI quota edits).
func (store *Storage) ReloadQuotaCache() error {
	if store.QuotaCache == nil {
		return nil
	}
	return store.populateQuotaCache()
}

// populateQuotaCache bulk-loads per-user storage usage and quota limits
// into the in-memory cache. Called once at startup.
func (store *Storage) populateQuotaCache() error {
	// Step 1: Get ALL registered accounts so every user has a cache entry.
	// This ensures users with 0 messages also get the default quota cached.
	allUsers, err := store.ListIMAPAccts()
	if err != nil {
		return fmt.Errorf("failed to list accounts: %w", err)
	}
	usedMap := make(map[string]int64, len(allUsers))
	for _, user := range allUsers {
		usedMap[user] = 0 // baseline; will be overwritten below if they have messages
	}

	// Step 2: Get per-user storage usage (SUM of message body lengths by UID).
	actualUsage, err := store.GetAllUsedStorage()
	if err != nil {
		return fmt.Errorf("failed to get used storage: %w", err)
	}
	for user, used := range actualUsage {
		usedMap[user] = used
	}

	// Step 3: Get per-user max quotas from the quotas table.
	var quotas []mdb.Quota
	if err := store.GORMDB.Where("username != ?", "__GLOBAL_DEFAULT__").Find(&quotas).Error; err != nil {
		return fmt.Errorf("failed to get user quotas: %w", err)
	}
	quotaMap := make(map[string]int64, len(quotas))
	for _, q := range quotas {
		quotaMap[q.Username] = q.MaxStorage
	}

	// Step 4: Get the effective default quota.
	defaultQuota := store.GetDefaultQuota()

	// Step 5: Load everything into the cache.
	store.QuotaCache.Load(usedMap, quotaMap, defaultQuota)
	return nil
}

func (store *Storage) flushMessageCounters() {
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		store.GORMDB.Where("name = ?", "sent_messages").
			Assign(mdb.MessageStat{Count: module.GetSentMessages()}).
			FirstOrCreate(&mdb.MessageStat{Name: "sent_messages"})
		store.GORMDB.Where("name = ?", "outbound_messages").
			Assign(mdb.MessageStat{Count: module.GetOutboundMessages()}).
			FirstOrCreate(&mdb.MessageStat{Name: "outbound_messages"})
		store.GORMDB.Where("name = ?", "received_messages").
			Assign(mdb.MessageStat{Count: module.GetReceivedMessages()}).
			FirstOrCreate(&mdb.MessageStat{Name: "received_messages"})
	}
}

func (store *Storage) GetQuota(username string) (used, max int64, isDefault bool, err error) {
	// Try the in-memory cache first for fast lookup.
	if store.QuotaCache != nil {
		entry, miss := store.QuotaCache.Get(username)
		if !miss {
			max = entry.MaxBytes
			// "Default" cap with 0 max means the cache was loaded with a bad
			// global (e.g. __GLOBAL_DEFAULT__ row had max=0) — report the real
			// effective default from config/DB, not 0 B.
			if entry.IsDefault && max == 0 {
				max = store.GetDefaultQuota()
			}
			return entry.UsedBytes, max, entry.IsDefault, nil
		}
		// Cache miss — fall through to DB and populate cache.
	}

	// DB fallback: get current usage.
	var result struct {
		TotalUsed int64
	}
	err = store.GORMDB.Table("msgs").
		Select("SUM(bodylen) as total_used").
		Joins("JOIN mboxes ON msgs.mboxid = mboxes.id").
		Joins("JOIN users ON mboxes.uid = users.id").
		Where("users.username = ?", username).
		Scan(&result).Error

	if err == nil {
		used = result.TotalUsed
	} else {
		used = 0
	}

	// Get max storage
	var q mdb.Quota
	err = store.GORMDB.Where("username = ?", username).First(&q).Error
	if err == nil {
		// MaxStorage==0 in the quotas table means "no per-user override — use
		// the global default" (same as quota.Cache.Load). It is not "0 B limit".
		if q.MaxStorage > 0 {
			if store.QuotaCache != nil {
				store.QuotaCache.SetMax(username, q.MaxStorage)
				store.QuotaCache.UpdateUsed(username, used)
			}
			return used, q.MaxStorage, false, nil
		}
		def := store.GetDefaultQuota()
		if store.QuotaCache != nil {
			store.QuotaCache.UpdateUsed(username, used)
			store.QuotaCache.ResetMax(username)
		}
		return used, def, true, nil
	}
	if !errors.Is(err, gorm.ErrRecordNotFound) {
		return 0, 0, false, err
	}

	// No per-user quotas row: use the same default as quota.Cache (global
	// override if positive, else default_quota from storage.imapsql).
	def := store.GetDefaultQuota()
	if store.QuotaCache != nil {
		store.QuotaCache.UpdateUsed(username, used)
	}
	return used, def, true, nil
}

// GetDefaultQuota returns the server-wide default limit: a positive
// __GLOBAL_DEFAULT__ row in the quotas table, otherwise default_quota from
// config. A __GLOBAL_DEFAULT__ or per-user row with MaxStorage==0 is treated
// as "unset" and does not override the imapsql default_quota.
func (store *Storage) GetDefaultQuota() int64 {
	var globalDef mdb.Quota
	err := store.GORMDB.Where("username = ?", "__GLOBAL_DEFAULT__").First(&globalDef).Error
	if err == nil && globalDef.MaxStorage > 0 {
		return globalDef.MaxStorage
	}
	return store.defaultQuota
}

func (store *Storage) SetDefaultQuota(max int64) error {
	err := store.GORMDB.Save(&mdb.Quota{Username: "__GLOBAL_DEFAULT__", MaxStorage: max}).Error
	if err == nil && store.QuotaCache != nil {
		store.QuotaCache.SetDefaultQuota(max)
	}
	return err
}

func (store *Storage) SetQuota(username string, max int64) error {
	var q mdb.Quota
	err := store.GORMDB.Where("username = ?", username).First(&q).Error
	if err != nil && !errors.Is(err, gorm.ErrRecordNotFound) {
		return err
	}

	if errors.Is(err, gorm.ErrRecordNotFound) {
		q = mdb.Quota{
			Username:     username,
			MaxStorage:   max,
			CreatedAt:    time.Now().Unix(),
			FirstLoginAt: 1,
		}
	} else {
		q.MaxStorage = max
	}

	err = store.GORMDB.Save(&q).Error
	if err == nil && store.QuotaCache != nil {
		store.QuotaCache.SetMax(username, max)
	}
	return err
}

// BlockUser adds a username to the blocklist, preventing re-registration.
func (store *Storage) BlockUser(username, reason string) error {
	if err := store.GORMDB.Save(&mdb.BlockedUser{Username: username, Reason: reason}).Error; err != nil {
		return err
	}
	store.blockedMu.Lock()
	if store.blockedSet == nil {
		store.blockedSet = make(map[string]struct{})
	}
	store.blockedSet[username] = struct{}{}
	store.blockedMu.Unlock()
	return nil
}

// UnblockUser removes a username from the blocklist.
func (store *Storage) UnblockUser(username string) error {
	if err := store.GORMDB.Where("username = ?", username).Delete(&mdb.BlockedUser{}).Error; err != nil {
		return err
	}
	store.blockedMu.Lock()
	if store.blockedSet != nil {
		delete(store.blockedSet, username)
	}
	store.blockedMu.Unlock()
	return nil
}

func (store *Storage) reloadBlockedCache() error {
	var names []string
	if err := store.GORMDB.Model(&mdb.BlockedUser{}).Pluck("username", &names).Error; err != nil {
		return err
	}
	nm := make(map[string]struct{}, len(names))
	for _, u := range names {
		nm[u] = struct{}{}
	}
	store.blockedMu.Lock()
	store.blockedSet = nm
	store.blockedMu.Unlock()
	return nil
}

func (store *Storage) isBlockedDB(username string) (bool, error) {
	var count int64
	err := store.GORMDB.Model(&mdb.BlockedUser{}).Where("username = ?", username).Count(&count).Error
	return count > 0, err
}

// IsBlocked checks if a username is in the blocklist. Uses an in-memory set
// filled at startup, on EventReload, and on BlockUser/UnblockUser, so the
// common path does not query the database.
func (store *Storage) IsBlocked(username string) (bool, error) {
	store.blockedMu.RLock()
	set := store.blockedSet
	store.blockedMu.RUnlock()
	if set == nil {
		return store.isBlockedDB(username)
	}
	_, ok := set[username]
	return ok, nil
}

// ListBlockedUsers returns all blocked usernames.
func (store *Storage) ListBlockedUsers() ([]module.BlockedUserEntry, error) {
	var blocked []mdb.BlockedUser
	err := store.GORMDB.Order("blocked_at DESC").Find(&blocked).Error
	if err != nil {
		return nil, err
	}
	entries := make([]module.BlockedUserEntry, len(blocked))
	for i, b := range blocked {
		entries[i] = module.BlockedUserEntry{
			Username:  b.Username,
			Reason:    b.Reason,
			BlockedAt: b.BlockedAt,
		}
	}
	return entries, nil
}

// DeleteAccount performs a full account removal:
// 1. Delete IMAP storage (account + mailboxes + messages)
// 2. Delete quota record from DB
// 3. Block the username from re-registration
func (store *Storage) DeleteAccount(username, reason string) error {
	// Step 1: Delete IMAP storage
	err := store.DeleteIMAPAcct(username)
	if err != nil {
		store.Log.Error("DeleteAccount: failed to delete IMAP account (may not exist)", err, "username", username)
		// Continue — the user might not have storage but still has credentials
	}

	// Step 2: Delete quota record
	store.GORMDB.Where("username = ?", username).Delete(&mdb.Quota{})

	// Step 3: Invalidate cache entry
	if store.QuotaCache != nil {
		store.QuotaCache.Invalidate(username)
	}

	// Step 4: Block the username
	if reason == "" {
		reason = "account deleted"
	}
	if err := store.BlockUser(username, reason); err != nil {
		return fmt.Errorf("failed to block user after deletion: %w", err)
	}

	return nil
}

func (store *Storage) ResetQuota(username string) error {
	err := store.GORMDB.Model(&mdb.Quota{}).Where("username = ?", username).Update("max_storage", nil).Error
	if err == nil && store.QuotaCache != nil {
		store.QuotaCache.ResetMax(username)
	}
	return err
}

func (store *Storage) GetAccountDate(username string) (created int64, err error) {
	var quota mdb.Quota
	err = store.GORMDB.Where("username = ?", username).First(&quota).Error
	if err == nil && quota.CreatedAt != 0 {
		return quota.CreatedAt, nil
	}

	// Fallback to oldest message if no creation date recorded
	// We'll use GORM here too for driver transparency
	var result struct {
		MinDate int64
	}
	err = store.GORMDB.Table("msgs").
		Select("MIN(date) as min_date").
		Joins("JOIN mboxes ON msgs.mboxid = mboxes.id").
		Joins("JOIN users ON mboxes.uid = users.id").
		Where("users.username = ?", username).
		Scan(&result).Error

	if err == nil && result.MinDate != 0 {
		return result.MinDate, nil
	}

	return 0, nil
}

func (store *Storage) GetAllAccountInfo() (map[string]module.AccountInfo, error) {
	var quotas []mdb.Quota
	if err := store.GORMDB.Find(&quotas).Error; err != nil {
		return nil, err
	}
	result := make(map[string]module.AccountInfo, len(quotas))
	for _, q := range quotas {
		result[q.Username] = module.AccountInfo{
			CreatedAt:    q.CreatedAt,
			FirstLoginAt: q.FirstLoginAt,
			LastLoginAt:  q.LastLoginAt,
		}
	}
	return result, nil
}

func (store *Storage) UpdateFirstLogin(username string) error {
	var quota mdb.Quota
	err := store.GORMDB.Where("username = ?", username).First(&quota).Error
	if err != nil {
		if errors.Is(err, gorm.ErrRecordNotFound) {
			return nil
		}
		return err
	}

	now := time.Now().Unix()
	quota.LastLoginAt = now

	if quota.FirstLoginAt == 1 {
		// This is the user's first login — handle deferred token consumption.
		if quota.UsedToken != "" {
			// Attempt to consume the registration token atomically.
			// UPDATE registration_tokens SET used_count = used_count + 1
			//   WHERE token = ? AND used_count < max_uses
			result := store.GORMDB.Model(&mdb.RegistrationToken{}).
				Where("token = ? AND used_count < max_uses", quota.UsedToken).
				Update("used_count", gorm.Expr("used_count + 1"))

			if result.Error != nil || result.RowsAffected == 0 {
				// Case B: Token was deleted, expired, or all slots taken.
				// Delete the account — the reservation is no longer valid.
				store.Log.Printf("late validation failed for %s (token %s): deleting account", username, quota.UsedToken)

				// Get auth DB to also delete credentials
				if store.authDBName != "" {
					if mod, err := module.GetInstance(store.authDBName); err == nil {
						if authDB, ok := mod.(module.PlainUserDB); ok {
							_ = authDB.DeleteUser(username)
						}
					}
				}

				// Delete IMAP storage (guard against nil Back in tests)
				if store.Back != nil {
					_ = store.DeleteIMAPAcct(username)
				}

				// Delete quota record
				store.GORMDB.Where("username = ?", username).Delete(&mdb.Quota{})

				// Invalidate cache
				if store.QuotaCache != nil {
					store.QuotaCache.Invalidate(username)
				}

				return fmt.Errorf("registration token no longer valid, account deleted")
			}

			// Case A: Success — token consumed. Clear UsedToken and proceed.
			quota.UsedToken = ""
		}

		quota.FirstLoginAt = now
	}

	return store.GORMDB.Save(&quota).Error
}

func (store *Storage) MigrateFirstLoginFromCreatedAt() error {
	now := time.Now().Unix()

	err := store.GORMDB.Model(&mdb.Quota{}).
		Where("created_at IS NULL OR created_at = 0").
		Update("created_at", now).Error
	if err != nil {
		return err
	}

	var count int64
	err = store.GORMDB.Model(&mdb.Quota{}).
		Where("first_login_at IS NULL OR first_login_at = 0").
		Count(&count).Error
	if err != nil {
		return fmt.Errorf("failed to check migration status: %w", err)
	}

	if count == 0 {
		return nil
	}

	err = store.GORMDB.Model(&mdb.Quota{}).
		Where("first_login_at IS NULL OR first_login_at = 0").
		Update("first_login_at", now).Error
	if err != nil {
		return err
	}

	return nil
}

func (store *Storage) PruneUnusedAccounts(retention time.Duration) error {
	cutoff := time.Now().Add(-retention).Unix()

	var quotas []mdb.Quota
	err := store.GORMDB.Model(&mdb.Quota{}).
		Where("first_login_at = 1 AND created_at < ?", cutoff).
		Find(&quotas).Error

	if err != nil {
		return fmt.Errorf("failed to query unused accounts: %w", err)
	}

	if len(quotas) == 0 {
		return nil
	}

	// Get auth DB if configured
	var authDB module.PlainUserDB
	if store.authDBName != "" {
		mod, err := module.GetInstance(store.authDBName)
		if err != nil {
			store.Log.Error("failed to get auth DB instance", err, "auth_db", store.authDBName)
		} else {
			if db, ok := mod.(module.PlainUserDB); ok {
				authDB = db
			} else {
				store.Log.Error("auth DB instance does not implement PlainUserDB", nil, "auth_db", store.authDBName)
			}
		}
	}

	deletedCount := 0
	for _, quota := range quotas {
		// Delete from storage
		if err := store.DeleteIMAPAcct(quota.Username); err != nil {
			store.Log.Error("failed to delete unused account from storage", err, "username", quota.Username)
			continue
		}

		// Delete from auth DB if configured
		if authDB != nil {
			if err := authDB.DeleteUser(quota.Username); err != nil {
				store.Log.Error("failed to delete unused account from auth DB", err, "username", quota.Username)
			}
		}

		deletedCount++
		store.Log.Debugln("deleted unused account:", quota.Username)
	}

	if deletedCount > 0 {
		store.Log.Printf("deleted %d unused account(s) (never logged in, older than %v)", deletedCount, retention)
	}

	return nil
}

func (store *Storage) GetStat() (totalStorage int64, accountsCount int, err error) {
	var total sql.NullInt64
	store.GORMDB.Table("msgs").Select("SUM(bodylen)").Scan(&total)
	totalStorage = total.Int64

	var count int64
	store.GORMDB.Table("users").Count(&count)
	accountsCount = int(count)

	return totalStorage, accountsCount, nil
}

// GetAllUsedStorage returns per-user storage usage in a single query.
func (store *Storage) GetAllUsedStorage() (map[string]int64, error) {
	type userStorage struct {
		Username  string
		TotalUsed int64
	}
	var results []userStorage
	err := store.GORMDB.Table("msgs").
		Select("users.username, SUM(msgs.bodylen) as total_used").
		Joins("JOIN mboxes ON msgs.mboxid = mboxes.id").
		Joins("JOIN users ON mboxes.uid = users.id").
		Group("users.username").
		Scan(&results).Error
	if err != nil {
		return nil, err
	}
	m := make(map[string]int64, len(results))
	for _, r := range results {
		m[r.Username] = r.TotalUsed
	}
	return m, nil
}

func (store *Storage) cleanupLoop() {
	ticker := time.NewTicker(1 * time.Hour)
	for range ticker.C {
		if err := store.PruneMessages(store.retention); err != nil {
			store.Log.Error("message cleanup failed", err)
		}
	}
}

func (store *Storage) cleanupUnusedAccountsLoop() {
	ticker := time.NewTicker(1 * time.Hour)
	for range ticker.C {
		if err := store.PruneUnusedAccounts(store.unusedAccountRetention); err != nil {
			store.Log.Error("unused account cleanup failed", err)
		}
	}
}

func (store *Storage) PruneMessages(retention time.Duration) error {
	cutoff := time.Now().Add(-retention).Unix()

	// go-imap-sql uses 'msgs' or 'messages' table.
	// Recent versions use 'msgs' table with 'date' as Unix timestamp.
	// We'll try to delete from 'msgs' where 'date' is older than cutoff.
	err := store.GORMDB.Table("msgs").Where("date < ?", cutoff).Delete(nil).Error
	if err != nil {
		// Try 'messages' table if 'msgs' fails
		err = store.GORMDB.Table("messages").Where("date < ?", cutoff).Delete(nil).Error
	}

	return err
}

func (store *Storage) EnableUpdatePipe(mode updatepipe.BackendMode) error {
	if store.updPipe != nil {
		return nil
	}

	switch store.driver {
	case "sqlite3":
		dbId := sha1.Sum([]byte(strings.Join(store.dsn, " ")))
		sockPath := filepath.Join(
			config.RuntimeDirectory,
			fmt.Sprintf("sql-%s.sock", hex.EncodeToString(dbId[:])))
		store.Log.DebugMsg("using unix socket for external updates", "path", sockPath)
		store.updPipe = &updatepipe.UnixSockPipe{
			SockPath: sockPath,
			Log:      log.Logger{Name: "storage.imapsql/updpipe", Debug: store.Log.Debug},
		}
	case "postgres":
		store.Log.DebugMsg("using PostgreSQL broker for external updates")
		ps, err := pubsub.NewPQ(strings.Join(store.dsn, " "))
		if err != nil {
			return fmt.Errorf("enable_update_pipe: %w", err)
		}
		ps.Log = log.Logger{Name: "storage.imapsql/updpipe/pubsub", Debug: store.Log.Debug}
		pipe := &updatepipe.PubSubPipe{
			PubSub: ps,
			Log:    log.Logger{Name: "storage.imapsql/updpipe", Debug: store.Log.Debug},
		}
		store.Back.UpdateManager().ExternalUnsubscribe = pipe.Unsubscribe
		store.Back.UpdateManager().ExternalSubscribe = pipe.Subscribe
		store.updPipe = pipe
	default:
		return errors.New("imapsql: driver does not have an update pipe implementation")
	}

	inbound := make(chan mess.Update, 32)
	outbound := make(chan mess.Update, 10)
	store.outboundUpds = outbound

	if mode == updatepipe.ModeReplicate {
		if err := store.updPipe.Listen(inbound); err != nil {
			store.updPipe = nil
			return err
		}
	}

	if err := store.updPipe.InitPush(); err != nil {
		store.updPipe = nil
		return err
	}

	store.Back.UpdateManager().SetExternalSink(outbound)

	store.updPushStop = make(chan struct{}, 1)
	go func() {
		defer func() {
			// Ensure we sent all outbound updates.
			for upd := range outbound {
				if err := store.updPipe.Push(upd); err != nil {
					store.Log.Error("IMAP update pipe push failed", err)
				}
			}
			store.updPushStop <- struct{}{}

			if err := recover(); err != nil {
				stack := debug.Stack()
				log.Printf("panic during imapsql update push: %v\n%s", err, stack)
			}
		}()

		for {
			select {
			case u := <-inbound:
				store.Log.DebugMsg("external update received", "type", u.Type, "key", u.Key)
				store.Back.UpdateManager().ExternalUpdate(u)
			case u, ok := <-outbound:
				if !ok {
					return
				}
				store.Log.DebugMsg("sending external update", "type", u.Type, "key", u.Key)
				if err := store.updPipe.Push(u); err != nil {
					store.Log.Error("IMAP update pipe push failed", err)
				}
			}
		}
	}()

	return nil
}

func (store *Storage) I18NLevel() int {
	return 1
}

func (store *Storage) IMAPExtensions() []string {
	return []string{"APPENDLIMIT", "MOVE", "CHILDREN", "SPECIAL-USE", "I18NLEVEL=1", "SORT", "THREAD=ORDEREDSUBJECT", "QUOTA"}
}

func (store *Storage) CreateMessageLimit() *uint32 {
	return store.Back.CreateMessageLimit()
}

func (store *Storage) GetOrCreateIMAPAcct(username string) (backend.User, error) {
	accountName, err := store.authNormalize(context.TODO(), username)
	if err != nil {
		return nil, backend.ErrInvalidCredentials
	}

	// Check if JIT registration is enabled before auto-creating accounts
	jitEnabled, err := store.IsJitRegistrationEnabled()
	if err != nil {
		return nil, err
	}

	// Check blocklist — blocked users must NOT be re-created via JIT
	if jitEnabled {
		if blocked, _ := store.IsBlocked(accountName); blocked {
			return nil, backend.ErrInvalidCredentials
		}
	}

	var u backend.User
	if jitEnabled {
		u, err = store.Back.GetOrCreateUser(accountName)
	} else {
		u, err = store.Back.GetUser(accountName)
		if err != nil {
			if errors.Is(err, imapsql.ErrUserDoesntExists) {
				return nil, backend.ErrInvalidCredentials
			}
			return nil, err
		}
	}

	if err != nil {
		return nil, err
	}

	// Ensure quota record exists with FirstLoginAt=1 for JIT users.
	// This is required to track them for pruning if they remain unused.
	var quota mdb.Quota
	err = store.GORMDB.Where("username = ?", accountName).First(&quota).Error
	if errors.Is(err, gorm.ErrRecordNotFound) {
		quota = mdb.Quota{
			Username:     accountName,
			CreatedAt:    time.Now().Unix(),
			FirstLoginAt: 1,
		}
		if err := store.GORMDB.Create(&quota).Error; err != nil {
			store.Log.Error("failed to create quota record for JIT user", err, "username", accountName)
		}
	} else if err == nil && quota.CreatedAt == 0 {
		// Fix legacy records missing CreatedAt
		quota.CreatedAt = time.Now().Unix()
		if err := store.GORMDB.Save(&quota).Error; err != nil {
			store.Log.Error("failed to fix CreatedAt for JIT user", err, "username", accountName)
		}
	}

	return u, nil
}

func (store *Storage) Lookup(ctx context.Context, key string) (string, bool, error) {
	accountName, err := store.authNormalize(ctx, key)
	if err != nil {
		return "", false, nil
	}

	usr, err := store.Back.GetUser(accountName)
	if err != nil {
		if errors.Is(err, imapsql.ErrUserDoesntExists) {
			return "", false, nil
		}
		return "", false, err
	}
	if err := usr.Logout(); err != nil {
		store.Log.Error("logout failed", err, "username", accountName)
	}

	return "", true, nil
}

func (store *Storage) Close() error {
	// Stop backend from generating new updates.
	store.Back.Close()

	// Wait for 'updates replicate' goroutine to actually stop so we will send
	// all updates before shutting down (this is especially important for
	// maddy subcommands).
	if store.updPipe != nil {
		close(store.outboundUpds)
		<-store.updPushStop

		store.updPipe.Close()
	}

	return nil
}

func (store *Storage) Login(_ *imap.ConnInfo, usenrame, password string) (backend.User, error) {
	panic("This method should not be called and is added only to satisfy backend.Backend interface")
}

func (store *Storage) SupportedThreadAlgorithms() []sortthread.ThreadAlgorithm {
	return []sortthread.ThreadAlgorithm{sortthread.OrderedSubject}
}

func init() {
	module.Register("storage.imapsql", New)
	module.Register("target.imapsql", New)
}
