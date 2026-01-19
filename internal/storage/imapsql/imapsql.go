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
	"encoding/hex"
	"errors"
	"fmt"
	"path/filepath"
	"runtime/debug"
	"strconv"
	"strings"
	"time"

	"github.com/emersion/go-imap"
	sortthread "github.com/emersion/go-imap-sortthread"
	"github.com/emersion/go-imap/backend"
	mess "github.com/foxcpp/go-imap-mess"
	imapsql "github.com/foxcpp/go-imap-sql"
	"github.com/themadorg/madmail/framework/config"
	modconfig "github.com/themadorg/madmail/framework/config/module"
	"github.com/themadorg/madmail/framework/dns"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/authz"
	"github.com/themadorg/madmail/internal/updatepipe"
	"github.com/themadorg/madmail/internal/updatepipe/pubsub"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"

	_ "github.com/go-sql-driver/mysql"
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
}

func (store *Storage) Name() string {
	return "imapsql"
}

func (store *Storage) InstanceName() string {
	return store.instName
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

	if driver == "sqlite3" {
		if sqliteImpl == "modernc" {
			store.Log.Println("using transpiled SQLite (modernc.org/sqlite), this is experimental")
			driver = "sqlite"
		} else if sqliteImpl == "cgo" {
			store.Log.Debugln("using cgo SQLite")
		} else if sqliteImpl == "missing" {
			return errors.New("imapsql: SQLite is not supported, recompile without no_sqlite3 tag set")
		}
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

	store.Back, err = imapsql.New(driver, dsnStr, ExtBlobStore{Base: blobStore}, opts)
	if err != nil {
		return fmt.Errorf("imapsql: %s", err)
	}

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

func (store *Storage) initQuotaTable() error {
	return store.GORMDB.AutoMigrate(&mdb.Quota{})
}

func (store *Storage) GetQuota(username string) (used, max int64, isDefault bool, err error) {
	// Get current usage
	// We'll use GORM here too for driver transparency
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
	var quota mdb.Quota
	err = store.GORMDB.Where("username = ?", username).First(&quota).Error
	if err == nil {
		return used, quota.MaxStorage, false, nil
	}
	if !errors.Is(err, gorm.ErrRecordNotFound) {
		return 0, 0, false, err
	}

	// Try to get global default override from DB
	var globalDef mdb.Quota
	err = store.GORMDB.Where("username = ?", "__GLOBAL_DEFAULT__").First(&globalDef).Error
	if err == nil {
		return used, globalDef.MaxStorage, true, nil
	}

	return used, store.defaultQuota, true, nil
}

func (store *Storage) GetDefaultQuota() int64 {
	var globalDef mdb.Quota
	err := store.GORMDB.Where("username = ?", "__GLOBAL_DEFAULT__").First(&globalDef).Error
	if err == nil {
		return globalDef.MaxStorage
	}
	return store.defaultQuota
}

func (store *Storage) SetDefaultQuota(max int64) error {
	return store.GORMDB.Save(&mdb.Quota{Username: "__GLOBAL_DEFAULT__", MaxStorage: max}).Error
}

func (store *Storage) SetQuota(username string, max int64) error {
	var quota mdb.Quota
	err := store.GORMDB.Where("username = ?", username).First(&quota).Error
	if err != nil && !errors.Is(err, gorm.ErrRecordNotFound) {
		return err
	}

	if errors.Is(err, gorm.ErrRecordNotFound) {
		quota = mdb.Quota{
			Username:     username,
			MaxStorage:   max,
			CreatedAt:    time.Now().Unix(),
			FirstLoginAt: 1,
		}
	} else {
		quota.MaxStorage = max
	}

	return store.GORMDB.Save(&quota).Error
}

func (store *Storage) ResetQuota(username string) error {
	return store.GORMDB.Model(&mdb.Quota{}).Where("username = ?", username).Update("max_storage", nil).Error
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

func (store *Storage) UpdateFirstLogin(username string) error {
	var quota mdb.Quota
	err := store.GORMDB.Where("username = ?", username).First(&quota).Error
	if err != nil {
		if errors.Is(err, gorm.ErrRecordNotFound) {
			return nil
		}
		return err
	}

	if quota.FirstLoginAt == 1 {
		quota.FirstLoginAt = time.Now().Unix()
		return store.GORMDB.Save(&quota).Error
	}

	return nil
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
	}

	if deletedCount > 0 {
		store.Log.Printf("deleted %d unused account(s) (never logged in, older than %v)", deletedCount, retention)
	}

	return nil
}

func (store *Storage) GetStat() (totalStorage int64, accountsCount int, err error) {
	store.GORMDB.Table("msgs").Select("SUM(bodylen)").Scan(&totalStorage)
	if totalStorage == 0 {
		// body_len might be bodylen or body_len depending on schema version
		store.GORMDB.Table("msgs").Select("SUM(body_len)").Scan(&totalStorage)
	}

	var count int64
	store.GORMDB.Table("users").Count(&count)
	accountsCount = int(count)

	return totalStorage, accountsCount, nil
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

	return store.Back.GetOrCreateUser(accountName)
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
