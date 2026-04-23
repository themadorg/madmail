package ctl

// Direct-DB implementation of the `maddy accounts` subcommands.
//
// These commands deliberately avoid loading the full maddy module framework.
// That matters for two reasons:
//
//  1. Avoiding the "config block named local_authdb already exists" failure
//     that happens when the same process registers config blocks more than
//     once (e.g. status needs both the auth and the storage block).
//  2. The "No Log" policy (docs/chatmail/nolog.md): the CLI should not spew
//     debug lines every time an admin runs a read-only command. Going
//     straight to GORM via internal/db keeps the CLI silent.

import (
	"errors"
	"fmt"
	"os"
	"time"

	frameworkconfig "github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/internal/auth"
	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/db"
	"github.com/urfave/cli/v2"
	"golang.org/x/text/secure/precis"
	"gorm.io/gorm"
)

func init() {
	// Shorthand for operators who expect `maddy ban-list` (same as `maddy accounts ban-list`).
	maddycli.AddSubcommand(&cli.Command{
		Name:  "ban-list",
		Usage: "List blocklisted usernames (see also: accounts ban-list)",
		Description: `Reads the blocked_users table in the IMAP storage database directly
(same as "accounts ban-list"). Newest entries first.

Use --cfg-block and --storage-cfg-block if your config blocks are not
local_authdb / local_mailboxes.`,
		Flags: accountsAuthStorageFlags(),
		Action: func(ctx *cli.Context) error {
			return accountsBanListDirect(ctx)
		},
	})
}

// humanTime renders a unix timestamp as RFC3339 in local time.
// 0 becomes "-" (never set); 1 becomes "pending first login" (matches the
// FirstLoginAt=1 sentinel that pass_table / imapsql use for
// "registered but never logged in").
func humanTime(ts int64) string {
	switch ts {
	case 0:
		return "-"
	case 1:
		return "pending first login"
	default:
		return time.Unix(ts, 0).Format(time.RFC3339)
	}
}

// accountsDB bundles the GORM connections and table names used by the
// accounts CLI. Callers must invoke Close when done.
type accountsDB struct {
	// Auth / passwords
	Auth        *gorm.DB
	AuthTable   string // e.g. "passwords"
	KeyColumn   string
	ValueColumn string

	// Settings (may equal Auth when no separate settings_table is configured).
	Settings      *gorm.DB
	SettingsTable string
	sharedAuth    bool // true when Settings == Auth

	// Storage (imapsql) — holds the users/msgs/mboxes/quotas/blocked_users tables.
	Storage *gorm.DB
}

// Close releases all underlying database handles.
func (a *accountsDB) Close() {
	if a == nil {
		return
	}
	if a.Auth != nil {
		closeDB(a.Auth)
	}
	if a.Settings != nil && !a.sharedAuth {
		closeDB(a.Settings)
	}
	if a.Storage != nil {
		closeDB(a.Storage)
	}
}

// openAccountsDB reads maddy.conf, resolves the named auth block and storage
// block, and opens direct GORM connections without touching the module
// framework.
func openAccountsDB(c *cli.Context, authBlock, storageBlock string) (*accountsDB, error) {
	confPath := frameworkconfig.ConfigFile()
	raw, err := os.ReadFile(confPath)
	if err != nil {
		return nil, fmt.Errorf("read config %s: %w", confPath, err)
	}
	conf := string(raw)
	stateDir := getStateDir(c)

	main, settings := parseAuthPassTableByName(conf, authBlock)
	if main.Driver == "" || main.DSN == "" {
		return nil, fmt.Errorf("could not find `table sql_table` inside auth.pass_table %q in %s", authBlock, confPath)
	}
	if main.TableName == "" {
		return nil, fmt.Errorf("auth.pass_table %q: table sql_table block is missing table_name", authBlock)
	}
	main.DSN = resolveSQLiteDSN(main.Driver, main.DSN, stateDir)

	storage := parseStorageImapsqlByName(conf, storageBlock)
	if storage.Driver == "" || storage.DSN == "" {
		return nil, fmt.Errorf("could not find storage.imapsql %q in %s", storageBlock, confPath)
	}
	storage.DSN = resolveSQLiteDSN(storage.Driver, storage.DSN, stateDir)

	authGORM, err := openSQLTableDB(main)
	if err != nil {
		return nil, fmt.Errorf("open auth DB: %w", err)
	}

	a := &accountsDB{
		Auth:        authGORM,
		AuthTable:   main.TableName,
		KeyColumn:   "key",
		ValueColumn: "value",
	}

	if settings.Driver != "" && settings.DSN != "" && settings.TableName != "" {
		settings.DSN = resolveSQLiteDSN(settings.Driver, settings.DSN, stateDir)
		// Reuse the auth connection if the settings table lives in the same DB.
		if settings.Driver == main.Driver && settings.DSN == main.DSN {
			a.Settings = a.Auth
			a.sharedAuth = true
		} else {
			settingsGORM, err := openSQLTableDB(settings)
			if err != nil {
				a.Close()
				return nil, fmt.Errorf("open settings DB: %w", err)
			}
			a.Settings = settingsGORM
		}
		a.SettingsTable = settings.TableName
	} else {
		// pass_table falls back to storing settings in the primary table.
		a.Settings = a.Auth
		a.SettingsTable = main.TableName
		a.sharedAuth = true
	}

	storageGORM, err := db.New(storage.Driver, []string{storage.DSN}, false)
	if err != nil {
		a.Close()
		return nil, fmt.Errorf("open storage DB: %w", err)
	}
	a.Storage = storageGORM

	// Ensure the GORM-managed tables exist. This is a no-op after the first
	// server run but keeps the CLI usable on a freshly-provisioned install.
	_ = a.Storage.AutoMigrate(&db.Quota{}, &db.BlockedUser{}, &db.MessageStat{}, &db.RegistrationToken{})

	return a, nil
}

// settingKey looks up a string setting, returning ("", false, nil) if absent.
func (a *accountsDB) settingKey(key string) (string, bool, error) {
	var row struct {
		Value string
	}
	err := a.Settings.
		Table(a.SettingsTable).
		Select(a.ValueColumn+" AS value").
		Where(a.KeyColumn+" = ?", key).
		Take(&row).Error
	if err != nil {
		if errors.Is(err, gorm.ErrRecordNotFound) {
			return "", false, nil
		}
		return "", false, err
	}
	return row.Value, true, nil
}

// boolSetting returns true when the stored value equals "true", otherwise def.
func (a *accountsDB) boolSetting(key string, def bool) bool {
	v, ok, err := a.settingKey(key)
	if err != nil || !ok {
		return def
	}
	return v == "true"
}

// listAuthKeys returns all rows in the passwords table (including internal __ keys).
func (a *accountsDB) listAuthKeys() ([]string, error) {
	var keys []string
	err := a.Auth.Table(a.AuthTable).Pluck(a.KeyColumn, &keys).Error
	if err != nil {
		return nil, err
	}
	return keys, nil
}

// credentialKey precis-normalises a username the same way auth.pass_table does.
// Used when looking up a single user by email.
func credentialKey(username string) (string, error) {
	username = auth.NormalizeUsername(username)
	return precis.UsernameCaseMapped.CompareKey(username)
}

// hasCredentials returns true when a passwords row exists for the user.
func (a *accountsDB) hasCredentials(username string) (bool, error) {
	key, err := credentialKey(username)
	if err != nil {
		return false, err
	}
	var count int64
	err = a.Auth.Table(a.AuthTable).Where(a.KeyColumn+" = ?", key).Count(&count).Error
	return count > 0, err
}

// accountsStatusDirect is the internal/db-backed implementation of
// `maddy accounts status`. It avoids the module framework entirely.
func accountsStatusDirect(ctx *cli.Context) error {
	authBlock := ctx.String("cfg-block")
	storageBlock := ctx.String("storage-cfg-block")

	a, err := openAccountsDB(ctx, authBlock, storageBlock)
	if err != nil {
		return err
	}
	defer a.Close()

	keys, err := a.listAuthKeys()
	if err != nil {
		return fmt.Errorf("list auth rows: %w", err)
	}
	nCreds := 0
	for _, k := range keys {
		if !isInternalSettingsKey(k) {
			nCreds++
		}
	}

	// Registration semantics match pass_table:
	//   __REGISTRATION_OPEN__ — missing → false
	//   __JIT_REGISTRATION_ENABLED__ — missing → same as registration open
	regOpen := a.boolSetting("__REGISTRATION_OPEN__", false)
	jitOn := a.boolSetting("__JIT_REGISTRATION_ENABLED__", regOpen)

	var (
		total    int64
		nIMAP    int64
		nBlocked int64
	)
	a.Storage.Table("msgs").Select("COALESCE(SUM(bodylen),0)").Scan(&total)
	a.Storage.Table("users").Count(&nIMAP)
	a.Storage.Model(&db.BlockedUser{}).Count(&nBlocked)

	fmt.Printf("Auth (%s):\n", authBlock)
	fmt.Printf("  Login accounts:     %d\n", nCreds)
	fmt.Printf("  Registration open:  %v\n", regOpen)
	fmt.Printf("  JIT registration:   %v\n", jitOn)
	fmt.Printf("Storage (%s):\n", storageBlock)
	fmt.Printf("  IMAP accounts:      %d\n", nIMAP)
	fmt.Printf("  Total storage:      %s\n", formatBytes(total))
	fmt.Printf("  Blocklisted users:  %d\n", nBlocked)
	return nil
}

// accountsInfoDirect is the internal/db-backed implementation of
// `maddy accounts info USERNAME`.
func accountsInfoDirect(ctx *cli.Context) error {
	raw := ctx.Args().First()
	if raw == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}
	username := auth.NormalizeUsername(raw)

	a, err := openAccountsDB(ctx, ctx.String("cfg-block"), ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer a.Close()

	hasCreds, err := a.hasCredentials(username)
	if err != nil {
		return fmt.Errorf("lookup credentials: %w", err)
	}

	var blockedCount int64
	if err := a.Storage.Model(&db.BlockedUser{}).Where("username = ?", username).Count(&blockedCount).Error; err != nil {
		return fmt.Errorf("blocklist check: %w", err)
	}

	// IMAP mailbox presence.
	var imapCount int64
	a.Storage.Table("users").Where("username = ?", username).Count(&imapCount)

	// Quota + account dates: everything we need lives in the quotas table
	// and a SUM over msgs joined to users.
	var q db.Quota
	quotaErr := a.Storage.Where("username = ?", username).First(&q).Error
	var used int64
	a.Storage.Table("msgs").
		Select("COALESCE(SUM(bodylen),0)").
		Joins("JOIN mboxes ON msgs.mboxid = mboxes.id").
		Joins("JOIN users ON mboxes.uid = users.id").
		Where("users.username = ?", username).
		Scan(&used)

	var max int64
	isDefault := false
	if quotaErr == nil && q.MaxStorage > 0 {
		max = q.MaxStorage
	} else {
		// Fall back to __GLOBAL_DEFAULT__ row, then to "unknown".
		var globalDef db.Quota
		if err := a.Storage.Where("username = ?", "__GLOBAL_DEFAULT__").First(&globalDef).Error; err == nil {
			max = globalDef.MaxStorage
		}
		isDefault = true
	}

	fmt.Printf("Username:           %s\n", username)
	fmt.Printf("Has credentials:    %v\n", hasCreds)
	fmt.Printf("IMAP mailbox:       %v\n", imapCount > 0)
	fmt.Printf("Blocklisted:        %v\n", blockedCount > 0)
	fmt.Printf("Quota used / max:   %s / %s\n", formatBytes(used), formatBytes(max))
	fmt.Printf("Default quota flag: %v\n", isDefault)
	fmt.Printf("Created at:         %s\n", humanTime(q.CreatedAt))
	fmt.Printf("First login:        %s\n", humanTime(q.FirstLoginAt))
	fmt.Printf("Last login:         %s\n", humanTime(q.LastLoginAt))
	return nil
}

// accountsUnbanDirect removes the blocklist entry for USERNAME by deleting
// the row directly from blocked_users. It does NOT restore credentials or
// mailboxes — those were removed by `ban` / `delete`. After the mutation
// it signals the running daemon (SIGUSR2) so any in-memory state that
// derives from the blocklist (future caches, federation policy) refreshes
// immediately.
func accountsUnbanDirect(ctx *cli.Context) error {
	raw := ctx.Args().First()
	if raw == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}
	username := auth.NormalizeUsername(raw)

	if !ctx.Bool("yes") {
		if !confirmUnban(username) {
			return fmt.Errorf("cancelled")
		}
	}

	a, err := openAccountsDB(ctx, ctx.String("cfg-block"), ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer a.Close()

	// Was the user actually blocklisted? Report that up front so the
	// operator doesn't get a silent no-op.
	var before int64
	if err := a.Storage.Model(&db.BlockedUser{}).Where("username = ?", username).Count(&before).Error; err != nil {
		return fmt.Errorf("blocklist lookup: %w", err)
	}
	if before == 0 {
		fmt.Printf("%s is not on the blocklist — nothing to do.\n", username)
		return nil
	}

	res := a.Storage.Where("username = ?", username).Delete(&db.BlockedUser{})
	if res.Error != nil {
		return fmt.Errorf("unblock %s: %w", username, res.Error)
	}
	fmt.Printf("Unblocked %s (removed %d blocklist row)\n", username, res.RowsAffected)

	pids, err := reloadRunningDaemons()
	if err != nil {
		fmt.Fprintf(os.Stderr, "warning: could not enumerate running daemons: %v\n", err)
	}
	switch len(pids) {
	case 0:
		fmt.Println("No running madmail daemon found to notify — changes will apply on next start.")
	case 1:
		fmt.Printf("Signalled running madmail daemon (pid %d) to reload caches.\n", pids[0])
	default:
		fmt.Printf("Signalled %d running madmail daemons (pids %v) to reload caches.\n", len(pids), pids)
	}
	return nil
}

// confirmUnban is a tiny wrapper so the direct-DB path doesn't depend on
// clitools2 being initialised (which the other CLI commands already pull in
// via accounts_bulk.go). Keeping it local makes accounts_direct.go
// self-contained in terms of its "do the DB work and signal" flow.
func confirmUnban(username string) bool {
	fmt.Printf("Remove %s from the blocklist and allow re-registration? [y/N] ", username)
	var ans string
	_, _ = fmt.Scanln(&ans)
	switch ans {
	case "y", "Y", "yes", "YES":
		return true
	}
	return false
}

// accountsBanListDirect prints every row of blocked_users, newest first,
// reading the storage DB directly. Output is tab-separated so it plays
// nicely with `column -t`, `awk`, and pagers. Timestamps use RFC3339 so
// they survive log-forwarding / sorting without surprises.
func accountsBanListDirect(ctx *cli.Context) error {
	a, err := openAccountsDB(ctx, ctx.String("cfg-block"), ctx.String("storage-cfg-block"))
	if err != nil {
		return err
	}
	defer a.Close()

	var rows []db.BlockedUser
	if err := a.Storage.Order("blocked_at DESC").Find(&rows).Error; err != nil {
		return fmt.Errorf("list blocked users: %w", err)
	}

	if len(rows) == 0 {
		fmt.Println("No blocklisted users.")
		return nil
	}

	// Column widths sized to typical chatmail usernames
	// (9-char base + `@[ipv4]` ≈ 25–35 chars). Reason is last so
	// overflow stays on one line instead of breaking the columns.
	fmt.Printf("%-45s  %-25s  %s\n", "USERNAME", "BLOCKED AT", "REASON")
	for _, r := range rows {
		reason := r.Reason
		if reason == "" {
			reason = "-"
		}
		ts := r.BlockedAt.Format(time.RFC3339)
		fmt.Printf("%-45s  %-25s  %s\n", r.Username, ts, reason)
	}
	fmt.Printf("\n%d blocklisted user(s).\n", len(rows))
	return nil
}
