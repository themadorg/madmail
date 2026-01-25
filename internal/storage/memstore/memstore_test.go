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

package memstore

import (
	"bytes"
	"context"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	mess "github.com/foxcpp/go-imap-mess"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/module"
)

// newTestStorage creates a new Storage with all required fields initialized
func newTestStorage() *Storage {
	store := &Storage{
		instName:      "test",
		junkMbox:      "Junk",
		defaultQuota:  1073741824,
		appendLimit:   32 * 1024 * 1024,
		updateManager: mess.NewManager(),
	}
	store.deliveryNormalize = func(ctx context.Context, s string) (string, error) {
		return s, nil
	}
	store.authNormalize = func(ctx context.Context, s string) (string, error) {
		return s, nil
	}
	return store
}

func TestStorageBasic(t *testing.T) {
	store := newTestStorage()

	// Test creating an account
	user, err := store.GetOrCreateIMAPAcct("test@example.com")
	if err != nil {
		t.Fatalf("GetOrCreateIMAPAcct failed: %v", err)
	}

	if user.Username() != "test@example.com" {
		t.Errorf("Username() = %s, want test@example.com", user.Username())
	}

	// Test listing mailboxes
	mailboxes, err := user.ListMailboxes(false)
	if err != nil {
		t.Fatalf("ListMailboxes failed: %v", err)
	}

	if len(mailboxes) < 1 {
		t.Error("Expected at least 1 mailbox (INBOX)")
	}

	foundInbox := false
	for _, mb := range mailboxes {
		if mb.Name == "INBOX" {
			foundInbox = true
			break
		}
	}
	if !foundInbox {
		t.Error("INBOX not found in mailboxes")
	}

	// Test getting a mailbox
	status, mbox, err := user.GetMailbox("INBOX", false, nil)
	if err != nil {
		t.Fatalf("GetMailbox failed: %v", err)
	}

	if status.Name != "INBOX" {
		t.Errorf("status.Name = %s, want INBOX", status.Name)
	}

	if mbox.Name() != "INBOX" {
		t.Errorf("mbox.Name() = %s, want INBOX", mbox.Name())
	}
}

func TestMessageDeduplication(t *testing.T) {
	store := newTestStorage()
	store.autoCreate = true

	// Create test message
	header := textproto.Header{}
	header.Add("From", "sender@example.com")
	header.Add("To", "rcpt@example.com")
	header.Add("Subject", "Test Message")
	header.Add("Message-ID", "<test123@example.com>")
	header.Add("Date", time.Now().Format(time.RFC1123Z))

	bodyBytes := []byte("This is a test message body.")

	// Store the same message twice
	id1 := store.storeMessage(header, bodyBytes)
	id2 := store.storeMessage(header, bodyBytes)

	// They should have the same ID (deduplicated)
	if id1 != id2 {
		t.Errorf("Message not deduplicated: id1=%s, id2=%s", id1, id2)
	}

	// Check reference count is 2
	if val, ok := store.messages.Load(id1); ok {
		msg := val.(*Message)
		if msg.RefCount != 2 {
			t.Errorf("RefCount = %d, want 2", msg.RefCount)
		}
	} else {
		t.Error("Message not found in store")
	}

	// Release one reference
	store.releaseMessage(id1)

	// Check reference count is now 1
	if val, ok := store.messages.Load(id1); ok {
		msg := val.(*Message)
		if msg.RefCount != 1 {
			t.Errorf("RefCount = %d, want 1 after release", msg.RefCount)
		}
	} else {
		t.Error("Message should still exist with refcount 1")
	}

	// Release second reference
	store.releaseMessage(id1)

	// Message should be deleted
	if _, ok := store.messages.Load(id1); ok {
		t.Error("Message should be deleted after all references released")
	}
}

func TestDeliveryToMultipleRecipients(t *testing.T) {
	store := newTestStorage()
	store.autoCreate = true

	// Create accounts
	store.getOrCreateAccount("user1@example.com")
	store.getOrCreateAccount("user2@example.com")
	store.getOrCreateAccount("user3@example.com")

	// Start delivery
	msgMeta := &module.MsgMetadata{
		ID: "test-msg-001",
	}

	delivery, err := store.Start(context.Background(), msgMeta, "sender@example.com")
	if err != nil {
		t.Fatalf("Start failed: %v", err)
	}

	// Add recipients
	recipients := []string{"user1@example.com", "user2@example.com", "user3@example.com"}
	for _, rcpt := range recipients {
		if err := delivery.AddRcpt(context.Background(), rcpt, smtp.RcptOptions{}); err != nil {
			t.Fatalf("AddRcpt(%s) failed: %v", rcpt, err)
		}
	}

	// Create message body
	header := textproto.Header{}
	header.Add("From", "sender@example.com")
	header.Add("To", "user1@example.com, user2@example.com, user3@example.com")
	header.Add("Subject", "Test Message to Multiple Recipients")
	header.Add("Message-ID", "<multi-rcpt-test@example.com>")

	bodyBytes := []byte("This message is sent to multiple recipients.")
	body := buffer.MemoryBuffer{Slice: bodyBytes}

	if err := delivery.Body(context.Background(), header, body); err != nil {
		t.Fatalf("Body failed: %v", err)
	}

	if err := delivery.Commit(context.Background()); err != nil {
		t.Fatalf("Commit failed: %v", err)
	}

	// Check that each recipient has the message
	for _, rcpt := range recipients {
		acct := store.getAccount(rcpt)
		if acct == nil {
			t.Fatalf("Account %s not found", rcpt)
		}

		acct.mu.RLock()
		inbox, ok := acct.Mailboxes["INBOX"]
		if !ok {
			t.Fatalf("INBOX not found for %s", rcpt)
		}

		inbox.mu.RLock()
		msgCount := len(inbox.Messages)
		inbox.mu.RUnlock()
		acct.mu.RUnlock()

		if msgCount != 1 {
			t.Errorf("%s has %d messages, want 1", rcpt, msgCount)
		}
	}

	// Verify the message is stored only once (deduplication across recipients)
	// Count total messages in the global store
	msgCount := 0
	store.messages.Range(func(key, value interface{}) bool {
		msgCount++
		return true
	})

	if msgCount != 1 {
		t.Errorf("Global message store has %d messages, want 1 (deduplication)", msgCount)
	}
}

func TestConcurrentAccess(t *testing.T) {
	store := newTestStorage()
	store.autoCreate = true

	// Create initial account
	store.getOrCreateAccount("shared@example.com")

	var wg sync.WaitGroup
	numGoroutines := 100
	messagesPerGoroutine := 10
	var successCount int64

	for i := 0; i < numGoroutines; i++ {
		wg.Add(1)
		go func(goroutineID int) {
			defer wg.Done()

			for j := 0; j < messagesPerGoroutine; j++ {
				// Each goroutine delivers messages
				msgMeta := &module.MsgMetadata{
					ID: "test-" + string(rune(goroutineID)) + "-" + string(rune(j)),
				}

				delivery, err := store.Start(context.Background(), msgMeta, "sender@example.com")
				if err != nil {
					t.Logf("Start failed: %v", err)
					continue
				}

				if err := delivery.AddRcpt(context.Background(), "shared@example.com", smtp.RcptOptions{}); err != nil {
					t.Logf("AddRcpt failed: %v", err)
					continue
				}

				header := textproto.Header{}
				header.Add("From", "sender@example.com")
				header.Add("To", "shared@example.com")
				header.Add("Subject", "Concurrent Test Message")

				bodyBytes := []byte("Test message body")
				body := buffer.MemoryBuffer{Slice: bodyBytes}

				if err := delivery.Body(context.Background(), header, body); err != nil {
					t.Logf("Body failed: %v", err)
					continue
				}

				if err := delivery.Commit(context.Background()); err != nil {
					t.Logf("Commit failed: %v", err)
					continue
				}

				atomic.AddInt64(&successCount, 1)
			}
		}(i)
	}

	wg.Wait()

	t.Logf("Successfully delivered %d/%d messages concurrently",
		successCount, numGoroutines*messagesPerGoroutine)

	// Verify no data corruption
	acct := store.getAccount("shared@example.com")
	if acct == nil {
		t.Fatal("Account not found")
	}

	acct.mu.RLock()
	inbox, ok := acct.Mailboxes["INBOX"]
	acct.mu.RUnlock()

	if !ok {
		t.Fatal("INBOX not found")
	}

	inbox.mu.RLock()
	actualMsgCount := len(inbox.Messages)
	inbox.mu.RUnlock()

	if int64(actualMsgCount) != successCount {
		t.Errorf("INBOX has %d messages, expected %d", actualMsgCount, successCount)
	}
}

func TestQuota(t *testing.T) {
	store := newTestStorage()
	store.defaultQuota = 1000 // 1000 bytes
	store.autoCreate = true

	// Create account
	store.getOrCreateAccount("quota-test@example.com")

	// Set a small quota
	if err := store.SetQuota("quota-test@example.com", 500); err != nil {
		t.Fatalf("SetQuota failed: %v", err)
	}

	// Check quota
	used, max, isDefault, err := store.GetQuota("quota-test@example.com")
	if err != nil {
		t.Fatalf("GetQuota failed: %v", err)
	}

	if used != 0 {
		t.Errorf("Initial used = %d, want 0", used)
	}
	if max != 500 {
		t.Errorf("max = %d, want 500", max)
	}
	if isDefault {
		t.Error("isDefault should be false after SetQuota")
	}

	// Reset quota
	if err := store.ResetQuota("quota-test@example.com"); err != nil {
		t.Fatalf("ResetQuota failed: %v", err)
	}

	// Check quota again - should be default
	_, max, isDefault, err = store.GetQuota("quota-test@example.com")
	if err != nil {
		t.Fatalf("GetQuota after reset failed: %v", err)
	}

	if max != store.defaultQuota {
		t.Errorf("max = %d, want default %d", max, store.defaultQuota)
	}
	if !isDefault {
		t.Error("isDefault should be true after ResetQuota")
	}
}

func TestIMAPExtensions(t *testing.T) {
	store := newTestStorage()

	exts := store.IMAPExtensions()

	expectedExts := []string{"APPENDLIMIT", "MOVE", "CHILDREN", "SPECIAL-USE", "I18NLEVEL=1", "QUOTA"}
	for _, expected := range expectedExts {
		found := false
		for _, ext := range exts {
			if ext == expected {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("Missing IMAP extension: %s", expected)
		}
	}
}

func TestMailboxOperations(t *testing.T) {
	store := newTestStorage()

	// Create account and get user
	user, err := store.GetOrCreateIMAPAcct("mbox-test@example.com")
	if err != nil {
		t.Fatalf("GetOrCreateIMAPAcct failed: %v", err)
	}

	// Create a new mailbox
	if err := user.CreateMailbox("TestFolder"); err != nil {
		t.Fatalf("CreateMailbox failed: %v", err)
	}

	// List mailboxes
	mailboxes, err := user.ListMailboxes(false)
	if err != nil {
		t.Fatalf("ListMailboxes failed: %v", err)
	}

	foundTestFolder := false
	for _, mb := range mailboxes {
		if mb.Name == "TestFolder" {
			foundTestFolder = true
			break
		}
	}
	if !foundTestFolder {
		t.Error("TestFolder not found after creation")
	}

	// Rename mailbox
	if err := user.RenameMailbox("TestFolder", "RenamedFolder"); err != nil {
		t.Fatalf("RenameMailbox failed: %v", err)
	}

	// Verify rename
	mailboxes, _ = user.ListMailboxes(false)
	foundRenamed := false
	foundOld := false
	for _, mb := range mailboxes {
		if mb.Name == "RenamedFolder" {
			foundRenamed = true
		}
		if mb.Name == "TestFolder" {
			foundOld = true
		}
	}
	if !foundRenamed {
		t.Error("RenamedFolder not found after rename")
	}
	if foundOld {
		t.Error("TestFolder should not exist after rename")
	}

	// Delete mailbox
	if err := user.DeleteMailbox("RenamedFolder"); err != nil {
		t.Fatalf("DeleteMailbox failed: %v", err)
	}

	// Verify deletion
	mailboxes, _ = user.ListMailboxes(false)
	for _, mb := range mailboxes {
		if mb.Name == "RenamedFolder" {
			t.Error("RenamedFolder should not exist after deletion")
		}
	}
}

func TestCreateMessageViaIMAP(t *testing.T) {
	store := newTestStorage()

	// Create account and get user
	user, err := store.GetOrCreateIMAPAcct("append-test@example.com")
	if err != nil {
		t.Fatalf("GetOrCreateIMAPAcct failed: %v", err)
	}

	// Create a message via IMAP APPEND
	msgContent := "From: sender@example.com\r\nTo: append-test@example.com\r\nSubject: Appended Message\r\n\r\nThis is an appended message."
	flags := []string{imap.SeenFlag}
	date := time.Now()

	if err := user.CreateMessage("INBOX", flags, date, bytes.NewReader([]byte(msgContent)), nil); err != nil {
		t.Fatalf("CreateMessage failed: %v", err)
	}

	// Verify the message was created
	status, mbox, err := user.GetMailbox("INBOX", false, nil)
	if err != nil {
		t.Fatalf("GetMailbox failed: %v", err)
	}

	if status.Messages != 1 {
		t.Errorf("status.Messages = %d, want 1", status.Messages)
	}

	// List messages
	ch := make(chan *imap.Message, 10)
	seqSet := new(imap.SeqSet)
	seqSet.AddRange(1, 1)

	go func() {
		if err := mbox.ListMessages(false, seqSet, []imap.FetchItem{imap.FetchFlags, imap.FetchUid}, ch); err != nil {
			t.Errorf("ListMessages failed: %v", err)
		}
	}()

	msg := <-ch
	if msg == nil {
		t.Fatal("No message received")
	}

	// Check flags include \Seen
	foundSeen := false
	for _, f := range msg.Flags {
		if f == imap.SeenFlag {
			foundSeen = true
			break
		}
	}
	if !foundSeen {
		t.Error("Message should have \\Seen flag")
	}

	// Close the mailbox
	if err := mbox.Close(); err != nil {
		t.Errorf("Close failed: %v", err)
	}
}
