package imapsql

import (
	"database/sql"
	"errors"
	"fmt"
	mathrand "math/rand"
	"strings"
	"sync"
	"time"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
	mess "github.com/foxcpp/go-imap-mess"
)

// VersionStr is a string value representing go-imap-sql version.
//
// Meant for debug logs, you may want to know which go-imap-sql version users
// have.
const VersionStr = "0.4.0"

// SchemaVersion is incremented each time DB schema changes.
const SchemaVersion = 6

var (
	ErrUserAlreadyExists = errors.New("imap: user already exists")
	ErrUserDoesntExists  = errors.New("imap: user doesn't exists")
)

type SerializationError struct {
	Err error
}

func (se SerializationError) Unwrap() error {
	return se.Err
}

func (se SerializationError) Error() string {
	return "imapsql: serialization failure, try again later"
}

type Rand interface {
	Uint32() uint32
}

type Logger interface {
	Printf(format string, v ...interface{})
	Println(v ...interface{})
	Debugf(format string, v ...interface{})
	Debugln(v ...interface{})
}

// Opts structure specifies additional settings that may be set
// for backend.
//
// Please use names to reference structure members on creation,
// fields may be reordered or added without major version increment.
type Opts struct {
	// Adding unexported name to structures makes it impossible to
	// reference fields without naming them explicitly.
	_ struct{}

	// Maximum amount of bytes that backend will accept.
	// Intended for use with APPENDLIMIT extension.
	// nil value means no limit, 0 means zero limit (no new messages allowed)
	MaxMsgBytes *uint32

	// Custom randomness source for UIDVALIDITY values generation.
	PRNG Rand

	// (SQLite3 only) Don't force WAL journaling mode.
	NoWAL bool

	// (SQLite3 only) Use different value for busy_timeout. Default is 50000.
	// To set to 0, use -1 (you probably don't want this).
	BusyTimeout int

	// (SQLite3 only) Use EXCLUSIVE locking mode.
	ExclusiveLock bool

	// (SQLite3 only) Change page cache size. Positive value indicates cache
	// size in pages, negative in KiB. If set 0 - SQLite default will be used.
	CacheSize int

	// (SQLite3 only) Repack database file into minimal amount of disk space on
	// Close.
	// It runs VACUUM and PRAGMA wal_checkpoint(TRUNCATE).
	// Failures of these operations are ignored and don't affect return value
	// of Close.
	MinimizeOnClose bool

	// Compression algorithm to use for new messages. Empty string means no compression.
	//
	// Algorithms should be registered before using RegisterCompressionAlgo.
	CompressAlgo string

	// CompressAlgoParams is passed directly to compression algorithm without changes.
	CompressAlgoParams string

	// Disable RFC 3501-conforming handling of \Recent flag. This improves
	// performance significantly.
	DisableRecent bool

	Log Logger
}

type Backend struct {
	db       db
	extStore ExternalStore
	mngr     *mess.Manager

	// Opts structure used to construct this Backend object.
	//
	// For most cases it is safe to change options while backend is serving
	// requests.
	// Options that should NOT be changed while backend is processing commands:
	// - PRNG
	// - CompressAlgoParams
	// Changes for the following options have no effect after backend initialization:
	// - CompressAlgo
	// - ExclusiveLock
	// - CacheSize
	// - NoWAL
	Opts Opts

	// database/sql.DB object created by New.
	DB *sql.DB

	prng         Rand
	compressAlgo CompressionAlgo

	// Shitton of pre-compiled SQL statements.
	userMeta           *sql.Stmt
	listUsers          *sql.Stmt
	addUser            *sql.Stmt
	delUser            *sql.Stmt
	listMboxes         *sql.Stmt
	listSubbedMboxes   *sql.Stmt
	createMboxExistsOk *sql.Stmt
	createMbox         *sql.Stmt
	deleteMbox         *sql.Stmt
	renameMbox         *sql.Stmt
	renameMboxChilds   *sql.Stmt
	getMboxAttrs       *sql.Stmt
	setSubbed          *sql.Stmt
	uidNextLocked      *sql.Stmt
	uidNext            *sql.Stmt
	hasChildren        *sql.Stmt
	uidValidity        *sql.Stmt
	msgsCount          *sql.Stmt
	recentCount        *sql.Stmt
	clearRecent        *sql.Stmt
	firstUnseenUid     *sql.Stmt
	unseenCount        *sql.Stmt
	deletedUids        *sql.Stmt
	expungeMbox        *sql.Stmt
	mboxId             *sql.Stmt
	addMsg             *sql.Stmt
	copyMsgsUid        *sql.Stmt
	copyMsgFlagsUid    *sql.Stmt
	massClearFlagsUid  *sql.Stmt
	msgFlagsUid        *sql.Stmt
	usedFlags          *sql.Stmt
	listMsgUids        *sql.Stmt
	listMsgUidsRecent  *sql.Stmt

	addRecentToLast *sql.Stmt

	// 'mark' column for messages is used to keep track of messages selected
	// by sequence numbers during operations that may cause seqence numbers to
	// change (e.g. message deletion)
	//
	// Consider following request: Delete messages with seqnum 1 and 3.
	// Naive implementation will delete 1st and then 3rd messages in mailbox.
	// However, after first operation 3rd message will become 2nd and
	// code will end up deleting the wrong message (4th actually).
	//
	// Solution is to "mark" 1st and 3rd message and then delete all "marked"
	// message.
	//
	// One could use \Deleted flag for this purpose, but this
	// requires more expensive operations at SQL engine side, so 'mark' column
	// is basically a optimization.

	// For MOVE extension
	markUid    *sql.Stmt
	delMarked  *sql.Stmt
	markedUids *sql.Stmt

	lastUid *sql.Stmt

	// For APPEND-LIMIT extension
	setUserMsgSizeLimit *sql.Stmt
	userMsgSizeLimit    *sql.Stmt
	setMboxMsgSizeLimit *sql.Stmt
	mboxMsgSizeLimit    *sql.Stmt

	searchFetchNoSeq *sql.Stmt

	flagsSearchStmtsLck   sync.RWMutex
	flagsSearchStmtsCache map[string]*sql.Stmt
	fetchStmtsLck         sync.RWMutex
	fetchStmtsCache       map[string]*sql.Stmt
	addFlagsStmtsLck      sync.RWMutex
	addFlagsStmtsCache    map[string]*sql.Stmt
	remFlagsStmtsLck      sync.RWMutex
	remFlagsStmtsCache    map[string]*sql.Stmt

	// extkeys table
	addExtKey             *sql.Stmt
	decreaseRefForMarked  *sql.Stmt
	decreaseRefForDeleted *sql.Stmt
	incrementRefUid       *sql.Stmt
	zeroRef               *sql.Stmt
	zeroRefUser           *sql.Stmt
	refUser               *sql.Stmt
	deleteZeroRef         *sql.Stmt
	deleteUserRef         *sql.Stmt
	decreaseRefForMbox    *sql.Stmt

	// Used by Delivery.SpecialMailbox.
	specialUseMbox *sql.Stmt

	setSeenFlagUid   *sql.Stmt
	increaseMsgCount *sql.Stmt
	decreaseMsgCount *sql.Stmt

	setInboxId *sql.Stmt

	cachedHeaderUid *sql.Stmt

	sqliteOptimizeLoopStop chan struct{}
}

var defaultPassHashAlgo = "bcrypt"

// New creates new Backend instance using provided configuration.
//
// driver and dsn arguments are passed directly to sql.Open.
//
// Note that it is not safe to create multiple Backend instances working with
// the single database as they need to keep some state synchronized and there
// is no measures for this implemented in go-imap-sql.
func New(driver, dsn string, extStore ExternalStore, opts Opts) (*Backend, error) {
	b := &Backend{
		fetchStmtsCache:       make(map[string]*sql.Stmt),
		flagsSearchStmtsCache: make(map[string]*sql.Stmt),
		addFlagsStmtsCache:    make(map[string]*sql.Stmt),
		remFlagsStmtsCache:    make(map[string]*sql.Stmt),

		sqliteOptimizeLoopStop: make(chan struct{}),

		extStore: extStore,
		Opts:     opts,

		mngr: mess.NewManager(opts.Log),
	}
	var err error

	if b.Opts.CompressAlgo != "" {
		impl, ok := compressionAlgos[b.Opts.CompressAlgo]
		if !ok {
			return nil, fmt.Errorf("New: unknown compression algorithm: %s", b.Opts.CompressAlgo)
		}

		b.compressAlgo = impl
	} else {
		b.compressAlgo = nullCompression{}
	}

	b.Opts = opts

	if b.Opts.Log == nil {
		b.Opts.Log = globalLogger{}
	}

	if b.Opts.PRNG != nil {
		b.prng = opts.PRNG
	} else {
		b.prng = mathrand.New(mathrand.NewSource(time.Now().Unix()))
	}

	if driver == "sqlite3" {
		dsn = b.addSqlite3Params(dsn)
	}

	b.db.driver = driver
	b.db.dsn = dsn

	b.db.DB, err = sql.Open(driver, dsn)
	if err != nil {
		return nil, wrapErr(err, "NewBackend (open)")
	}
	b.DB = b.db.DB

	ver, err := b.schemaVersion()
	if err != nil {
		return nil, wrapErr(err, "NewBackend (schemaVersion)")
	}
	// Zero version indicates "empty database".
	if ver > SchemaVersion {
		return nil, fmt.Errorf("incompatible database schema, too new (%d > %d)", ver, SchemaVersion)
	}
	if ver < SchemaVersion && ver != 0 {
		b.Opts.Log.Printf("Upgrading database schema (from %d to %d)", ver, SchemaVersion)
		if err := b.upgradeSchema(ver); err != nil {
			return nil, wrapErr(err, "NewBackend (schemaUpgrade)")
		}
	}
	if err := b.setSchemaVersion(SchemaVersion); err != nil {
		return nil, wrapErr(err, "NewBackend (setSchemaVersion)")
	}

	if err := b.configureEngine(); err != nil {
		return nil, wrapErr(err, "NewBackend (configureEngine)")
	}

	if err := b.initSchema(); err != nil {
		return nil, wrapErr(err, "NewBackend (initSchema)")
	}
	if err := b.prepareStmts(); err != nil {
		return nil, wrapErr(err, "NewBackend (prepareStmts)")
	}

	for _, item := range [...]imap.FetchItem{
		imap.FetchFlags, imap.FetchEnvelope,
		imap.FetchBodyStructure, "BODY[]", "BODY[HEADER.FIELDS (From To)]"} {

		if _, err := b.getFetchStmt([]imap.FetchItem{item}); err != nil {
			return nil, wrapErrf(err, "fetchStmt prime (%s)", item)
		}
	}

	if b.db.driver == "sqlite3" {
		go b.sqliteOptimizeLoop()
	}

	return b, nil
}

func (b *Backend) UpdateManager() *mess.Manager {
	return b.mngr
}

func (b *Backend) sqliteOptimizeLoop() {
	t := time.NewTicker(5 * time.Hour)
	defer t.Stop()
	for {
		select {
		case <-t.C:
			b.Opts.Log.Debugln("running SQLite query planer optimization...")
			b.db.Exec(`PRAGMA optimize`)
			b.Opts.Log.Debugln("completed SQLite query planer optimization")
		case <-b.sqliteOptimizeLoopStop:
			return
		}
	}
}

func (b *Backend) Close() error {
	if b.db.driver == "sqlite3" {
		// These operations are not critical, so it's not a problem if they fail.
		if b.Opts.MinimizeOnClose {
			b.db.Exec(`VACUUM`)
			b.db.Exec(`PRAGMA wal_checkpoint(TRUNCATE)`)
		}

		b.sqliteOptimizeLoopStop <- struct{}{}
		b.db.Exec(`PRAGMA optimize`)
	}

	return b.db.Close()
}

func (b *Backend) getUserMeta(tx *sql.Tx, username string) (id uint64, inboxId uint64, err error) {
	var row *sql.Row
	if tx != nil {
		row = tx.Stmt(b.userMeta).QueryRow(username)
	} else {
		row = b.userMeta.QueryRow(username)
	}
	if err := row.Scan(&id, &inboxId); err != nil {
		return 0, 0, err
	}
	return id, inboxId, nil
}

func normalizeUsername(u string) string {
	return strings.ToLower(u)
}

// CreateUser creates user account.
func (b *Backend) CreateUser(username string) error {
	_, _, err := b.createUser(nil, normalizeUsername(username))
	return err
}

func (b *Backend) createUser(tx *sql.Tx, username string) (uid, inboxId uint64, err error) {
	var shouldCommit bool
	if tx == nil {
		var err error
		tx, err = b.db.Begin(false)
		if err != nil {
			return 0, 0, wrapErr(err, "CreateUser")
		}
		defer tx.Rollback()
		shouldCommit = true
	}

	_, err = tx.Stmt(b.addUser).Exec(username)
	if err != nil {
		if isForeignKeyErr(err) {
			return 0, 0, ErrUserAlreadyExists
		}
		return 0, 0, wrapErr(err, "CreateUser (addUser)")
	}

	// TODO: Cut additional query here by using RETURNING on PostgreSQL.
	uid, _, err = b.getUserMeta(tx, username)
	if err != nil {
		return 0, 0, wrapErr(err, "CreateUser")
	}

	// Every new user needs to have at least one mailbox (INBOX).
	if _, err := tx.Stmt(b.createMbox).Exec(uid, "INBOX", b.prng.Uint32(), nil); err != nil {
		return 0, 0, wrapErr(err, "CreateUser")
	}

	// TODO: Cut another query here by using RETURNING on PostgreSQL.
	if err = tx.Stmt(b.mboxId).QueryRow(uid, "INBOX").Scan(&inboxId); err != nil {
		return 0, 0, wrapErr(err, "CreateUser")
	}
	if _, err = tx.Stmt(b.setInboxId).Exec(inboxId, uid); err != nil {
		return 0, 0, wrapErr(err, "CreateUser")
	}

	if shouldCommit {
		return uid, inboxId, tx.Commit()
	}
	return uid, inboxId, nil
}

// DeleteUser deleted user account with specified username.
//
// It is error to delete account that doesn't exist, ErrUserDoesntExists will
// be returned in this case.
func (b *Backend) DeleteUser(username string) error {
	username = strings.ToLower(username)

	tx, err := b.db.BeginLevel(sql.LevelReadCommitted, false)
	if err != nil {
		return wrapErr(err, "DeleteUser")
	}
	defer tx.Rollback()

	// TODO: These queries definitely can be merged on PostgreSQL.
	var keys []string
	rows, err := tx.Stmt(b.refUser).Query(username)
	if err != nil {
		return wrapErr(err, "DeleteUser")
	}
	defer rows.Close()
	for rows.Next() {
		var key string
		if err := rows.Scan(&key); err != nil {
			return wrapErr(err, "DeleteUser")
		}
		keys = append(keys, key)
	}

	stats, err := tx.Stmt(b.delUser).Exec(username)
	if err != nil {
		return wrapErr(err, "DeleteUser")
	}
	affected, err := stats.RowsAffected()
	if err != nil {
		return wrapErr(err, "DeleteUser")
	}
	if affected == 0 {
		return ErrUserDoesntExists
	}

	if err := b.extStore.Delete(keys); err != nil {
		return wrapErr(err, "DeleteUser")
	}

	if _, err := tx.Stmt(b.deleteUserRef).Exec(username); err != nil {
		return wrapErr(err, "DeleteUser")
	}

	return tx.Commit()
}

// ListUsers returns list of existing usernames.
//
// It may return nil slice if no users are registered.
func (b *Backend) ListUsers() ([]string, error) {
	var res []string
	rows, err := b.listUsers.Query()
	if err != nil {
		return res, wrapErr(err, "ListUsers")
	}
	for rows.Next() {
		var id uint64
		var name string
		if err := rows.Scan(&id, &name); err != nil {
			return res, wrapErr(err, "ListUsers")
		}
		res = append(res, name)
	}
	if err := rows.Err(); err != nil {
		return res, wrapErr(err, "ListUsers")
	}
	return res, nil
}

// GetUser creates backend.User object for the user credentials.
func (b *Backend) GetUser(username string) (backend.User, error) {
	username = normalizeUsername(username)

	uid, inboxId, err := b.getUserMeta(nil, username)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, ErrUserDoesntExists
		}
		return nil, err
	}
	return &User{id: uid, username: username, parent: b, inboxId: inboxId}, nil
}

// GetOrCreateUser is a convenience wrapper for GetUser and CreateUser.
//
// All database operations are executed within one transaction so
// this method is atomic as defined by used RDBMS.
// If another transaction creates the user concurrently, this function
// will retry getting the user.
func (b *Backend) GetOrCreateUser(username string) (backend.User, error) {
	username = normalizeUsername(username)

	tx, err := b.db.Begin(false)
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()

	uid, inboxId, err := b.getUserMeta(tx, username)
	if err != nil {
		if err == sql.ErrNoRows {
			b.Opts.Log.Println("auto-creating storage account", username)
			uid, inboxId, err = b.createUser(tx, username)
			if err != nil {
				// If another transaction created the user concurrently,
				// we get ErrUserAlreadyExists. Commit our transaction
				// (even though we didn't do anything) and retry getting the user.
				if err == ErrUserAlreadyExists {
					tx.Commit()
					return b.GetUser(username)
				}
				return nil, err
			}
		} else {
			return nil, err
		}
	}
	return &User{id: uid, username: username, parent: b, inboxId: inboxId}, tx.Commit()
}

func (b *Backend) Login(_ *imap.ConnInfo, username, password string) (backend.User, error) {
	u, err := b.GetOrCreateUser(username)
	if err != nil {
		return nil, err
	}
	b.Opts.Log.Debugln(username, "logged in")
	return u, nil
}

func (b *Backend) CreateMessageLimit() *uint32 {
	return b.Opts.MaxMsgBytes
}

// Change global APPEND limit, Opts.MaxMsgBytes.
//
// Provided to implement interfaces used by go-imap-backend-tests.
func (b *Backend) SetMessageLimit(val *uint32) error {
	b.Opts.MaxMsgBytes = val
	return nil
}
