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

	_ "github.com/go-sql-driver/mysql"
	_ "github.com/lib/pq"
)

type Storage struct {
	Back     *imapsql.Backend
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

	defaultQuota int64
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
	cfg.DataSize("default_quota", false, false, 1073741824, &store.defaultQuota)

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

	if store.retention > 0 {
		go store.cleanupLoop()
	}

	if err := store.initQuotaTable(); err != nil {
		return fmt.Errorf("imapsql: quota table init failed: %w", err)
	}

	return nil
}

func (store *Storage) initQuotaTable() error {
	db := store.Back.DB

	_, err := db.Exec(`CREATE TABLE IF NOT EXISTS quotas (
		username TEXT PRIMARY KEY,
		max_storage INTEGER,
		created_at INTEGER
	)`)
	if err != nil {
		return err
	}

	// For existing tables before this change
	_, _ = db.Exec(`ALTER TABLE quotas ADD COLUMN created_at INTEGER`)

	return nil
}

func (store *Storage) GetQuota(username string) (used, max int64, isDefault bool, err error) {
	db := store.Back.DB

	// Get current usage
	err = db.QueryRow(`SELECT SUM(bodyLen) FROM msgs
		JOIN mboxes ON msgs.mboxId = mboxes.id
		JOIN users ON mboxes.uid = users.id
		WHERE users.username = ?`, username).Scan(&used)
	if err != nil {
		used = 0
	}

	// Get max storage
	var maxVal sql.NullInt64
	err = db.QueryRow(`SELECT max_storage FROM quotas WHERE username = ?`, username).Scan(&maxVal)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return 0, 0, false, err
	}

	if maxVal.Valid {
		return used, maxVal.Int64, false, nil
	}

	// Try to get global default override from DB
	var globalDef sql.NullInt64
	_ = db.QueryRow(`SELECT max_storage FROM quotas WHERE username = '__GLOBAL_DEFAULT__'`).Scan(&globalDef)
	if globalDef.Valid {
		return used, globalDef.Int64, true, nil
	}

	return used, store.defaultQuota, true, nil
}

func (store *Storage) GetDefaultQuota() int64 {
	db := store.Back.DB
	var globalDef sql.NullInt64
	_ = db.QueryRow(`SELECT max_storage FROM quotas WHERE username = '__GLOBAL_DEFAULT__'`).Scan(&globalDef)
	if globalDef.Valid {
		return globalDef.Int64
	}
	return store.defaultQuota
}

func (store *Storage) SetDefaultQuota(max int64) error {
	db := store.Back.DB
	_, err := db.Exec(`INSERT INTO quotas (username, max_storage) VALUES ('__GLOBAL_DEFAULT__', ?)
		ON CONFLICT(username) DO UPDATE SET max_storage = excluded.max_storage`, max)
	return err
}

func (store *Storage) SetQuota(username string, max int64) error {
	db := store.Back.DB

	_, err := db.Exec(`INSERT INTO quotas (username, max_storage) VALUES (?, ?)
		ON CONFLICT(username) DO UPDATE SET max_storage = excluded.max_storage`, username, max)
	return err
}

func (store *Storage) ResetQuota(username string) error {
	db := store.Back.DB

	_, err := db.Exec(`UPDATE quotas SET max_storage = NULL WHERE username = ?`, username)
	return err
}

func (store *Storage) GetAccountDate(username string) (created int64, err error) {
	db := store.Back.DB

	var createdVal sql.NullInt64
	err = db.QueryRow(`SELECT created_at FROM quotas WHERE username = ?`, username).Scan(&createdVal)
	if err == nil && createdVal.Valid {
		return createdVal.Int64, nil
	}

	// Fallback to oldest message if no creation date recorded
	err = db.QueryRow(`SELECT MIN(date) FROM msgs
		JOIN mboxes ON msgs.mboxId = mboxes.id
		JOIN users ON mboxes.uid = users.id
		WHERE users.username = ?`, username).Scan(&createdVal)
	if err == nil && createdVal.Valid {
		return createdVal.Int64, nil
	}

	return 0, nil
}

func (store *Storage) GetStat() (totalStorage int64, accountsCount int, err error) {
	db := store.Back.DB

	err = db.QueryRow(`SELECT SUM(bodyLen) FROM msgs`).Scan(&totalStorage)
	if err != nil {
		totalStorage = 0
	}

	err = db.QueryRow(`SELECT COUNT(*) FROM users`).Scan(&accountsCount)
	if err != nil {
		accountsCount = 0
	}

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

func (store *Storage) PruneMessages(retention time.Duration) error {
	db := store.Back.DB
	cutoff := time.Now().Add(-retention).Unix()

	// go-imap-sql uses 'msgs' or 'messages' table.
	// Recent versions use 'msgs' table with 'date' as Unix timestamp.
	// We'll try to delete from 'msgs' where 'date' is older than cutoff.
	_, err := db.Exec("DELETE FROM msgs WHERE date < ?", cutoff)
	if err != nil {
		// Try 'messages' table if 'msgs' fails
		_, err = db.Exec("DELETE FROM messages WHERE date < ?", cutoff)
	}

	// Also clean up blobs if they are in the database (not common for messages though)

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
