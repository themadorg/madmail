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

// Package inmemory implements an in-memory storage module for storing
// email messages without any persistent storage (no SQLite, no filesystem).
//
// Key features:
// - Messages are stored once and referenced from multiple mailboxes (deduplication)
// - Thread-safe concurrent access supporting 10000+ concurrent clients
// - No disk I/O for message storage
//
// Interfaces implemented:
// - module.Storage
// - module.ManageableStorage
// - module.DeliveryTarget
package memstore

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"io"
	"runtime/trace"
	"sort"
	"sync"
	"sync/atomic"
	"time"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/exterrors"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/authz"
	"github.com/themadorg/madmail/internal/target"
)

// Message represents a single stored email message.
// Messages are stored once and can be referenced from multiple mailboxes.
type Message struct {
	// Body contains the raw message body
	Body []byte
	// Header contains the message headers
	Header textproto.Header
	// ContentHash is used for deduplication
	ContentHash string
	// RefCount tracks how many mailbox entries reference this message
	RefCount int32
	// Size is the message size in bytes
	Size int
	// Date is the internal date of the message
	Date time.Time
	// UID is the unique identifier within the mailbox (set per-mailbox-entry)
	UID uint32
	// SeqNum is the sequence number (dynamically computed)
	SeqNum uint32
	// Flags are the message flags
	Flags []string
	// mu protects Flags
	mu sync.RWMutex
}

// MessageRef is a reference to a message in a specific mailbox
type MessageRef struct {
	// MessageID points to the actual message in the global store
	MessageID string
	// UID is unique within the mailbox
	UID uint32
	// Flags specific to this mailbox entry
	Flags []string
	// mu protects Flags
	mu sync.RWMutex
}

// Mailbox represents an IMAP mailbox
type Mailbox struct {
	Name       string
	Subscribed bool
	// Messages maps UID -> MessageRef
	Messages map[uint32]*MessageRef
	// UIDNext is the next UID to be assigned
	UIDNext uint32
	// UIDValidity for this mailbox
	UIDValidity uint32
	// Attributes for special mailboxes
	Attributes []string
	// mu protects the mailbox
	mu sync.RWMutex
}

// Account represents a user account with mailboxes
type Account struct {
	Username string
	// Mailboxes maps mailbox name -> Mailbox
	Mailboxes map[string]*Mailbox
	// QuotaUsed is the total bytes used by this account
	QuotaUsed int64
	// QuotaMax is the quota limit for this account (0 = use default)
	QuotaMax int64
	// CreatedAt is when the account was created
	CreatedAt int64
	// FirstLoginAt is when the user first logged in (1 = never logged in)
	FirstLoginAt int64
	// mu protects the account
	mu sync.RWMutex
}

// Storage is the main in-memory storage implementation
type Storage struct {
	instName string
	Log      log.Logger

	// accounts maps username -> Account
	accounts sync.Map

	// messages is the global message store (content hash -> Message)
	messages sync.Map

	// deliveryNormalize normalizes delivery addresses
	deliveryNormalize func(context.Context, string) (string, error)
	deliveryMap       module.Table

	// authNormalize normalizes authentication usernames
	authNormalize func(context.Context, string) (string, error)
	authMap       module.Table

	// filters for IMAP message filtering
	filters module.IMAPFilter

	// junkMbox is the name of the junk mailbox
	junkMbox string

	// defaultQuota is the default quota for new accounts
	defaultQuota int64

	// globalDefaultQuota can be set dynamically
	globalDefaultQuota int64

	// autoCreate determines whether to auto-create accounts
	autoCreate bool

	// retention is the message retention period (0 = forever)
	retention time.Duration

	// unusedAccountRetention is how long to keep unused accounts
	unusedAccountRetention time.Duration

	// uidValidityCounter is used to generate unique UIDValidity values
	uidValidityCounter uint32

	// totalAccountsCount for statistics
	totalAccountsCount int64

	// totalStorageUsed for statistics
	totalStorageUsed int64

	// appendLimit is the maximum message size
	appendLimit uint32

	// mu protects global state modifications
	mu sync.RWMutex
}

func (s *Storage) Name() string {
	return "memstore"
}

func (s *Storage) InstanceName() string {
	return s.instName
}

// New creates a new in-memory storage module
func New(_, instName string, _, inlineArgs []string) (module.Module, error) {
	store := &Storage{
		instName:     instName,
		Log:          log.Logger{Name: "memstore"},
		junkMbox:     "Junk",
		defaultQuota: 1073741824, // 1 GB default
		appendLimit:  32 * 1024 * 1024,
	}
	return store, nil
}

func (s *Storage) Init(cfg *config.Map) error {
	var (
		authNormalize     string
		deliveryNormalize string
	)

	cfg.Bool("debug", true, false, &s.Log.Debug)
	cfg.String("junk_mailbox", false, false, "Junk", &s.junkMbox)
	cfg.Custom("imap_filter", false, false, func() (interface{}, error) {
		return nil, nil
	}, func(m *config.Map, node config.Node) (interface{}, error) {
		return nil, errors.New("imap_filter is not yet supported in inmemory storage")
	}, &s.filters)
	cfg.Custom("auth_map", false, false, func() (interface{}, error) {
		return nil, nil
	}, func(m *config.Map, node config.Node) (interface{}, error) {
		return nil, errors.New("auth_map is not yet supported in inmemory storage")
	}, &s.authMap)
	cfg.String("auth_normalize", false, false, "auto", &authNormalize)
	cfg.Custom("delivery_map", false, false, func() (interface{}, error) {
		return nil, nil
	}, func(m *config.Map, node config.Node) (interface{}, error) {
		return nil, errors.New("delivery_map is not yet supported in inmemory storage")
	}, &s.deliveryMap)
	cfg.String("delivery_normalize", false, false, "precis_casefold_email", &deliveryNormalize)
	cfg.Duration("retention", false, false, 0, &s.retention)
	cfg.Duration("unused_account_retention", false, false, 0, &s.unusedAccountRetention)
	cfg.DataSize("default_quota", false, false, 1073741824, &s.defaultQuota)
	cfg.Bool("auto_create", false, false, &s.autoCreate)

	var appendlimitVal int64 = 32 * 1024 * 1024
	cfg.DataSize("appendlimit", false, false, 32*1024*1024, &appendlimitVal)

	if _, err := cfg.Process(); err != nil {
		return err
	}

	if appendlimitVal > 0 && appendlimitVal <= int64(^uint32(0)) {
		s.appendLimit = uint32(appendlimitVal)
	}

	// Setup normalization functions
	deliveryNormFunc, ok := authz.NormalizeFuncs[deliveryNormalize]
	if !ok {
		return errors.New("inmemory: unknown normalization function: " + deliveryNormalize)
	}
	s.deliveryNormalize = func(ctx context.Context, email string) (string, error) {
		return deliveryNormFunc(email)
	}

	authNormFunc, ok := authz.NormalizeFuncs[authNormalize]
	if !ok {
		return errors.New("inmemory: unknown normalization function: " + authNormalize)
	}
	s.authNormalize = func(ctx context.Context, username string) (string, error) {
		return authNormFunc(username)
	}

	// Start cleanup goroutines if retention is set
	if s.retention > 0 {
		go s.cleanupLoop()
	}
	if s.unusedAccountRetention > 0 {
		go s.cleanupUnusedAccountsLoop()
	}

	s.Log.Printf("in-memory storage initialized (default quota: %d bytes, append limit: %d bytes)",
		s.defaultQuota, s.appendLimit)

	return nil
}

func (s *Storage) cleanupLoop() {
	ticker := time.NewTicker(1 * time.Hour)
	for range ticker.C {
		if err := s.pruneMessages(s.retention); err != nil {
			s.Log.Error("message cleanup failed", err)
		}
	}
}

func (s *Storage) cleanupUnusedAccountsLoop() {
	ticker := time.NewTicker(1 * time.Hour)
	for range ticker.C {
		if err := s.PruneUnusedAccounts(s.unusedAccountRetention); err != nil {
			s.Log.Error("unused account cleanup failed", err)
		}
	}
}

func (s *Storage) pruneMessages(retention time.Duration) error {
	cutoff := time.Now().Add(-retention)
	s.accounts.Range(func(key, value interface{}) bool {
		acct := value.(*Account)
		acct.mu.Lock()
		defer acct.mu.Unlock()

		for _, mbox := range acct.Mailboxes {
			mbox.mu.Lock()
			for uid, ref := range mbox.Messages {
				if msg, ok := s.messages.Load(ref.MessageID); ok {
					m := msg.(*Message)
					if m.Date.Before(cutoff) {
						delete(mbox.Messages, uid)
						s.releaseMessage(ref.MessageID)
					}
				}
			}
			mbox.mu.Unlock()
		}
		return true
	})
	return nil
}

// releaseMessage decrements the reference count and removes the message if no longer referenced
func (s *Storage) releaseMessage(messageID string) {
	if val, ok := s.messages.Load(messageID); ok {
		msg := val.(*Message)
		if atomic.AddInt32(&msg.RefCount, -1) <= 0 {
			s.messages.Delete(messageID)
			atomic.AddInt64(&s.totalStorageUsed, -int64(msg.Size))
		}
	}
}

// getOrCreateAccount gets an existing account or creates a new one
func (s *Storage) getOrCreateAccount(username string) *Account {
	if val, ok := s.accounts.Load(username); ok {
		return val.(*Account)
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	// Double-check after acquiring lock
	if val, ok := s.accounts.Load(username); ok {
		return val.(*Account)
	}

	// Generate a unique UIDValidity
	s.uidValidityCounter++
	uidValidity := s.uidValidityCounter

	acct := &Account{
		Username:     username,
		Mailboxes:    make(map[string]*Mailbox),
		CreatedAt:    time.Now().Unix(),
		FirstLoginAt: 1, // 1 means never logged in
	}

	// Create default mailboxes
	defaultMailboxes := []struct {
		name  string
		attrs []string
	}{
		{"INBOX", nil},
		{"Drafts", []string{imap.DraftsAttr}},
		{"Sent", []string{imap.SentAttr}},
		{"Trash", []string{imap.TrashAttr}},
		{s.junkMbox, []string{imap.JunkAttr}},
	}

	for _, mb := range defaultMailboxes {
		acct.Mailboxes[mb.name] = &Mailbox{
			Name:        mb.name,
			Subscribed:  true,
			Messages:    make(map[uint32]*MessageRef),
			UIDNext:     1,
			UIDValidity: uidValidity,
			Attributes:  mb.attrs,
		}
	}

	s.accounts.Store(username, acct)
	atomic.AddInt64(&s.totalAccountsCount, 1)

	return acct
}

// getAccount gets an existing account (returns nil if not found)
func (s *Storage) getAccount(username string) *Account {
	if val, ok := s.accounts.Load(username); ok {
		return val.(*Account)
	}
	return nil
}

// I18NLevel returns the internationalization level supported
func (s *Storage) I18NLevel() int {
	return 1
}

// IMAPExtensions returns the list of IMAP extensions supported
func (s *Storage) IMAPExtensions() []string {
	return []string{"APPENDLIMIT", "MOVE", "CHILDREN", "SPECIAL-USE", "I18NLEVEL=1", "QUOTA"}
}

// CreateMessageLimit returns the maximum message size
func (s *Storage) CreateMessageLimit() *uint32 {
	return &s.appendLimit
}

// GetOrCreateIMAPAcct returns or creates an IMAP account
func (s *Storage) GetOrCreateIMAPAcct(username string) (backend.User, error) {
	accountName, err := s.authNormalize(context.TODO(), username)
	if err != nil {
		return nil, backend.ErrInvalidCredentials
	}

	acct := s.getOrCreateAccount(accountName)
	return &User{storage: s, account: acct}, nil
}

// GetIMAPAcct returns an existing IMAP account
func (s *Storage) GetIMAPAcct(username string) (backend.User, error) {
	accountName, err := s.authNormalize(context.TODO(), username)
	if err != nil {
		return nil, backend.ErrInvalidCredentials
	}

	acct := s.getAccount(accountName)
	if acct == nil {
		return nil, backend.ErrInvalidCredentials
	}
	return &User{storage: s, account: acct}, nil
}

// ListIMAPAccts lists all IMAP accounts
func (s *Storage) ListIMAPAccts() ([]string, error) {
	var accounts []string
	s.accounts.Range(func(key, value interface{}) bool {
		accounts = append(accounts, key.(string))
		return true
	})
	sort.Strings(accounts)
	return accounts, nil
}

// CreateIMAPAcct creates a new IMAP account
func (s *Storage) CreateIMAPAcct(username string) error {
	s.getOrCreateAccount(username)
	return nil
}

// DeleteIMAPAcct deletes an IMAP account
func (s *Storage) DeleteIMAPAcct(username string) error {
	if val, ok := s.accounts.Load(username); ok {
		acct := val.(*Account)
		acct.mu.Lock()
		// Release all messages
		for _, mbox := range acct.Mailboxes {
			mbox.mu.Lock()
			for _, ref := range mbox.Messages {
				s.releaseMessage(ref.MessageID)
			}
			mbox.mu.Unlock()
		}
		acct.mu.Unlock()
		s.accounts.Delete(username)
		atomic.AddInt64(&s.totalAccountsCount, -1)
	}
	return nil
}

// PurgeIMAPMsgs removes all messages from an account
func (s *Storage) PurgeIMAPMsgs(username string) error {
	if val, ok := s.accounts.Load(username); ok {
		acct := val.(*Account)
		acct.mu.Lock()
		defer acct.mu.Unlock()

		for _, mbox := range acct.Mailboxes {
			mbox.mu.Lock()
			for _, ref := range mbox.Messages {
				s.releaseMessage(ref.MessageID)
			}
			mbox.Messages = make(map[uint32]*MessageRef)
			mbox.mu.Unlock()
		}
		acct.QuotaUsed = 0
	}
	return nil
}

// GetQuota returns the quota for an account
func (s *Storage) GetQuota(username string) (used, max int64, isDefault bool, err error) {
	if val, ok := s.accounts.Load(username); ok {
		acct := val.(*Account)
		acct.mu.RLock()
		defer acct.mu.RUnlock()

		if acct.QuotaMax > 0 {
			return acct.QuotaUsed, acct.QuotaMax, false, nil
		}
	}

	// Return default quota
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.globalDefaultQuota > 0 {
		return 0, s.globalDefaultQuota, true, nil
	}
	return 0, s.defaultQuota, true, nil
}

// SetQuota sets the quota for an account
func (s *Storage) SetQuota(username string, max int64) error {
	acct := s.getOrCreateAccount(username)
	acct.mu.Lock()
	defer acct.mu.Unlock()
	acct.QuotaMax = max
	return nil
}

// ResetQuota resets the quota for an account to default
func (s *Storage) ResetQuota(username string) error {
	if val, ok := s.accounts.Load(username); ok {
		acct := val.(*Account)
		acct.mu.Lock()
		defer acct.mu.Unlock()
		acct.QuotaMax = 0
	}
	return nil
}

// GetAccountDate returns the creation date of an account
func (s *Storage) GetAccountDate(username string) (created int64, err error) {
	if val, ok := s.accounts.Load(username); ok {
		acct := val.(*Account)
		acct.mu.RLock()
		defer acct.mu.RUnlock()
		return acct.CreatedAt, nil
	}
	return 0, nil
}

// UpdateFirstLogin updates the first login time for an account
func (s *Storage) UpdateFirstLogin(username string) error {
	if val, ok := s.accounts.Load(username); ok {
		acct := val.(*Account)
		acct.mu.Lock()
		defer acct.mu.Unlock()
		if acct.FirstLoginAt == 1 {
			acct.FirstLoginAt = time.Now().Unix()
		}
	}
	return nil
}

// PruneUnusedAccounts removes accounts that have never been logged into
func (s *Storage) PruneUnusedAccounts(retention time.Duration) error {
	cutoff := time.Now().Add(-retention).Unix()
	var toDelete []string

	s.accounts.Range(func(key, value interface{}) bool {
		acct := value.(*Account)
		acct.mu.RLock()
		if acct.FirstLoginAt == 1 && acct.CreatedAt < cutoff {
			toDelete = append(toDelete, key.(string))
		}
		acct.mu.RUnlock()
		return true
	})

	for _, username := range toDelete {
		if err := s.DeleteIMAPAcct(username); err != nil {
			s.Log.Error("failed to delete unused account", err, "username", username)
		} else {
			s.Log.Debugln("deleted unused account:", username)
		}
	}

	if len(toDelete) > 0 {
		s.Log.Printf("deleted %d unused account(s)", len(toDelete))
	}

	return nil
}

// GetDefaultQuota returns the default quota
func (s *Storage) GetDefaultQuota() int64 {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.globalDefaultQuota > 0 {
		return s.globalDefaultQuota
	}
	return s.defaultQuota
}

// SetDefaultQuota sets the default quota
func (s *Storage) SetDefaultQuota(max int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.globalDefaultQuota = max
	return nil
}

// GetStat returns storage statistics
func (s *Storage) GetStat() (totalStorage int64, accountsCount int, err error) {
	return atomic.LoadInt64(&s.totalStorageUsed), int(atomic.LoadInt64(&s.totalAccountsCount)), nil
}

// Close closes the storage (no-op for in-memory)
func (s *Storage) Close() error {
	s.Log.Println("in-memory storage closed")
	return nil
}

// IsRegistrationOpen returns whether registration is open
func (s *Storage) IsRegistrationOpen() (bool, error) {
	return s.autoCreate, nil
}

// IsJitRegistrationEnabled returns whether JIT registration is enabled
func (s *Storage) IsJitRegistrationEnabled() (bool, error) {
	return s.autoCreate, nil
}

// Login is not used (required by backend.Backend interface)
func (s *Storage) Login(_ *imap.ConnInfo, username, password string) (backend.User, error) {
	panic("This method should not be called and is added only to satisfy backend.Backend interface")
}

// hashMessage computes a content hash for deduplication
func hashMessage(header textproto.Header, body []byte) string {
	h := sha256.New()

	// Include key headers in the hash
	for _, key := range []string{"Message-ID", "Date", "From", "To", "Subject"} {
		if val := header.Get(key); val != "" {
			h.Write([]byte(key + ":" + val + "\n"))
		}
	}
	h.Write(body)
	return hex.EncodeToString(h.Sum(nil))
}

// storeMessage stores a message and returns its ID (with deduplication)
// It creates `refCount` references (default 1).
func (s *Storage) storeMessage(header textproto.Header, body []byte) string {
	return s.storeMessageWithRefCount(header, body, 1)
}

// storeMessageWithRefCount stores a message with a specific initial reference count
// This is useful for multi-recipient delivery where we know upfront how many references we need
func (s *Storage) storeMessageWithRefCount(header textproto.Header, body []byte, refCount int32) string {
	contentHash := hashMessage(header, body)

	// Create new message with the specified ref count
	msg := &Message{
		Body:        body,
		Header:      header,
		ContentHash: contentHash,
		RefCount:    refCount,
		Size:        len(body),
		Date:        time.Now(),
	}

	// Store message (use LoadOrStore to handle race condition)
	actual, loaded := s.messages.LoadOrStore(contentHash, msg)
	if loaded {
		// Another goroutine stored it first (or a previous delivery had this message)
		// Add our references to the existing message
		existingMsg := actual.(*Message)
		atomic.AddInt32(&existingMsg.RefCount, refCount)
	} else {
		atomic.AddInt64(&s.totalStorageUsed, int64(len(body)))
	}

	return contentHash
}


// Delivery implementation for module.DeliveryTarget

type delivery struct {
	store    *Storage
	msgMeta  *module.MsgMetadata
	mailFrom string

	addedRcpts map[string]struct{}
}

func (d *delivery) String() string {
	return d.store.Name() + ":" + d.store.InstanceName()
}

func userDoesNotExist(actual error) error {
	return &exterrors.SMTPError{
		Code:         501,
		EnhancedCode: exterrors.EnhancedCode{5, 1, 1},
		Message:      "User does not exist",
		TargetName:   "inmemory",
		Err:          actual,
	}
}

func (d *delivery) AddRcpt(ctx context.Context, rcptTo string, _ smtp.RcptOptions) error {
	defer trace.StartRegion(ctx, "inmemory/AddRcpt").End()

	accountName, err := d.store.deliveryNormalize(ctx, rcptTo)
	if err != nil {
		return userDoesNotExist(err)
	}

	if _, ok := d.addedRcpts[accountName]; ok {
		return nil
	}

	// Auto-create account if enabled
	if d.store.autoCreate {
		d.store.getOrCreateAccount(accountName)
	} else {
		// Check if account exists
		if d.store.getAccount(accountName) == nil {
			return userDoesNotExist(errors.New("account does not exist"))
		}
	}

	d.addedRcpts[accountName] = struct{}{}
	return nil
}

func (d *delivery) Body(ctx context.Context, header textproto.Header, body buffer.Buffer) error {
	defer trace.StartRegion(ctx, "inmemory/Body").End()

	// Read the body into memory
	reader, err := body.Open()
	if err != nil {
		return err
	}
	defer reader.Close()

	bodyBytes, err := io.ReadAll(reader)
	if err != nil {
		return err
	}

	// Add Return-Path header
	headerCopy := header.Copy()
	headerCopy.Add("Return-Path", "<"+target.SanitizeForHeader(d.mailFrom)+">")

	// First pass: check quotas and count valid recipients
	validRecipients := make([]string, 0, len(d.addedRcpts))
	for rcpt := range d.addedRcpts {
		acct := d.store.getAccount(rcpt)
		if acct == nil {
			continue
		}

		// Quota check
		used, max, _, err := d.store.GetQuota(rcpt)
		if err != nil {
			d.store.Log.Error("Failed to get quota for recipient", err, "rcpt", rcpt)
			continue
		}

		if max > 0 && used+int64(len(bodyBytes)) > max {
			// Skip this recipient due to quota exceeded, but continue with others
			d.store.Log.Debugf("Quota exceeded for %s: used=%d, max=%d, msgSize=%d", rcpt, used, max, len(bodyBytes))
			continue
		}

		validRecipients = append(validRecipients, rcpt)
	}

	// If no valid recipients after quota checks, return early
	if len(validRecipients) == 0 {
		return nil
	}

	// Store the message once (with deduplication) with the correct reference count
	// Only count the recipients that passed quota checks
	messageID := d.store.storeMessageWithRefCount(headerCopy, bodyBytes, int32(len(validRecipients)))

	// Second pass: deliver to valid recipients only
	for _, rcpt := range validRecipients {
		acct := d.store.getAccount(rcpt)
		if acct == nil {
			// Account was deleted between checks - release a reference
			d.store.releaseMessage(messageID)
			continue
		}

		// Determine target mailbox
		targetMailbox := "INBOX"
		if d.msgMeta.Quarantine {
			targetMailbox = d.store.junkMbox
		}

		acct.mu.Lock()
		mbox, ok := acct.Mailboxes[targetMailbox]
		if !ok {
			// Create mailbox if it doesn't exist
			d.store.mu.Lock()
			d.store.uidValidityCounter++
			uidValidity := d.store.uidValidityCounter
			d.store.mu.Unlock()

			mbox = &Mailbox{
				Name:        targetMailbox,
				Subscribed:  true,
				Messages:    make(map[uint32]*MessageRef),
				UIDNext:     1,
				UIDValidity: uidValidity,
			}
			acct.Mailboxes[targetMailbox] = mbox
		}

		mbox.mu.Lock()
		uid := mbox.UIDNext
		mbox.UIDNext++

		ref := &MessageRef{
			MessageID: messageID,
			UID:       uid,
			Flags:     []string{imap.RecentFlag},
		}
		mbox.Messages[uid] = ref

		mbox.mu.Unlock()

		// Update quota used
		acct.QuotaUsed += int64(len(bodyBytes))
		acct.mu.Unlock()
	}

	return nil
}

func (d *delivery) Abort(ctx context.Context) error {
	defer trace.StartRegion(ctx, "inmemory/Abort").End()
	return nil
}

func (d *delivery) Commit(ctx context.Context) error {
	defer trace.StartRegion(ctx, "inmemory/Commit").End()
	return nil
}

// Start begins a new delivery
func (s *Storage) Start(ctx context.Context, msgMeta *module.MsgMetadata, mailFrom string) (module.Delivery, error) {
	defer trace.StartRegion(ctx, "inmemory/Start").End()

	return &delivery{
		store:      s,
		msgMeta:    msgMeta,
		mailFrom:   mailFrom,
		addedRcpts: make(map[string]struct{}),
	}, nil
}

// Lookup checks if an account exists
func (s *Storage) Lookup(ctx context.Context, key string) (string, bool, error) {
	accountName, err := s.authNormalize(ctx, key)
	if err != nil {
		return "", false, nil
	}

	if s.getAccount(accountName) != nil {
		return "", true, nil
	}
	return "", false, nil
}

func init() {
	module.Register("storage.memstore", New)
	module.Register("target.memstore", New)
}
