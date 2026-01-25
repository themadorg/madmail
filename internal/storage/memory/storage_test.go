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

package memory

import (
	"io"
	"strings"
	"testing"
	"time"

	imap "github.com/emersion/go-imap"
	"github.com/themadorg/madmail/framework/config"
)

// literalWrapper implements imap.Literal for testing
type literalWrapper struct {
	io.Reader
	length int
}

func (l *literalWrapper) Len() int {
	return l.length
}

func TestMemoryStorage_CreateAccount(t *testing.T) {
	storage, err := New("storage.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create storage: %v", err)
	}

	s := storage.(*Storage)
	cfg := config.NewMap(nil, config.Node{})
	if err := s.Init(cfg); err != nil {
		t.Fatalf("Failed to initialize storage: %v", err)
	}

	// Test CreateIMAPAcct
	if err := s.CreateIMAPAcct("test@example.com"); err != nil {
		t.Fatalf("Failed to create account: %v", err)
	}

	// Test duplicate account creation
	if err := s.CreateIMAPAcct("test@example.com"); err == nil {
		t.Fatal("Expected error when creating duplicate account")
	}

	// Test ListIMAPAccts
	accounts, err := s.ListIMAPAccts()
	if err != nil {
		t.Fatalf("Failed to list accounts: %v", err)
	}
	if len(accounts) != 1 {
		t.Fatalf("Expected 1 account, got %d", len(accounts))
	}
}

func TestMemoryStorage_AutoCreate(t *testing.T) {
	storage, err := New("storage.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create storage: %v", err)
	}

	s := storage.(*Storage)
	s.autoCreate = true

	// Test GetOrCreateIMAPAcct with auto-create
	user, err := s.GetOrCreateIMAPAcct("auto@example.com")
	if err != nil {
		t.Fatalf("Failed to get or create account: %v", err)
	}
	if user.Username() != "auto@example.com" {
		t.Fatalf("Expected username 'auto@example.com', got '%s'", user.Username())
	}

	// Verify account was created
	accounts, err := s.ListIMAPAccts()
	if err != nil {
		t.Fatalf("Failed to list accounts: %v", err)
	}
	if len(accounts) != 1 {
		t.Fatalf("Expected 1 account, got %d", len(accounts))
	}
}

func TestMemoryStorage_DeleteAccount(t *testing.T) {
	storage, err := New("storage.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create storage: %v", err)
	}

	s := storage.(*Storage)
	cfg := config.NewMap(nil, config.Node{})
	if err := s.Init(cfg); err != nil {
		t.Fatalf("Failed to initialize storage: %v", err)
	}

	// Create and delete account
	if err := s.CreateIMAPAcct("delete@example.com"); err != nil {
		t.Fatalf("Failed to create account: %v", err)
	}

	if err := s.DeleteIMAPAcct("delete@example.com"); err != nil {
		t.Fatalf("Failed to delete account: %v", err)
	}

	// Verify account was deleted
	accounts, err := s.ListIMAPAccts()
	if err != nil {
		t.Fatalf("Failed to list accounts: %v", err)
	}
	if len(accounts) != 0 {
		t.Fatalf("Expected 0 accounts, got %d", len(accounts))
	}
}

func TestMemoryStorage_Quota(t *testing.T) {
	storage, err := New("storage.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create storage: %v", err)
	}

	s := storage.(*Storage)
	cfg := config.NewMap(nil, config.Node{})
	if err := s.Init(cfg); err != nil {
		t.Fatalf("Failed to initialize storage: %v", err)
	}

	// Create account
	if err := s.CreateIMAPAcct("quota@example.com"); err != nil {
		t.Fatalf("Failed to create account: %v", err)
	}

	// Test GetQuota
	used, max, isDefault, err := s.GetQuota("quota@example.com")
	if err != nil {
		t.Fatalf("Failed to get quota: %v", err)
	}
	if used != 0 {
		t.Fatalf("Expected used quota 0, got %d", used)
	}
	if !isDefault {
		t.Fatal("Expected default quota")
	}

	// Test SetQuota
	newMax := int64(2048 * 1024 * 1024)
	if err := s.SetQuota("quota@example.com", newMax); err != nil {
		t.Fatalf("Failed to set quota: %v", err)
	}

	used, max, isDefault, err = s.GetQuota("quota@example.com")
	if err != nil {
		t.Fatalf("Failed to get quota: %v", err)
	}
	if max != newMax {
		t.Fatalf("Expected max quota %d, got %d", newMax, max)
	}
	if isDefault {
		t.Fatal("Expected custom quota")
	}

	// Test ResetQuota
	if err := s.ResetQuota("quota@example.com"); err != nil {
		t.Fatalf("Failed to reset quota: %v", err)
	}

	used, max, isDefault, err = s.GetQuota("quota@example.com")
	if err != nil {
		t.Fatalf("Failed to get quota: %v", err)
	}
	if !isDefault {
		t.Fatal("Expected default quota after reset")
	}
}

func TestMemoryUser_Mailboxes(t *testing.T) {
	storage, err := New("storage.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create storage: %v", err)
	}

	s := storage.(*Storage)
	s.autoCreate = true

	// Get user
	userIface, err := s.GetOrCreateIMAPAcct("mailbox@example.com")
	if err != nil {
		t.Fatalf("Failed to get user: %v", err)
	}

	user := userIface.(*User)

	// Test ListMailboxes (should have INBOX by default)
	mailboxes, err := user.ListMailboxes(false)
	if err != nil {
		t.Fatalf("Failed to list mailboxes: %v", err)
	}
	if len(mailboxes) != 1 {
		t.Fatalf("Expected 1 mailbox, got %d", len(mailboxes))
	}
	if mailboxes[0].Name != "INBOX" {
		t.Fatalf("Expected INBOX, got %s", mailboxes[0].Name)
	}

	// Test CreateMailbox
	if err := user.CreateMailbox("Sent"); err != nil {
		t.Fatalf("Failed to create mailbox: %v", err)
	}

	mailboxes, err = user.ListMailboxes(false)
	if err != nil {
		t.Fatalf("Failed to list mailboxes: %v", err)
	}
	if len(mailboxes) != 2 {
		t.Fatalf("Expected 2 mailboxes, got %d", len(mailboxes))
	}

	// Test GetMailbox
	sentStatus, sentMbox, err := user.GetMailbox("Sent", false, nil)
	if err != nil {
		t.Fatalf("Failed to get mailbox: %v", err)
	}
	if sentStatus.Name != "Sent" {
		t.Fatalf("Expected 'Sent', got '%s'", sentStatus.Name)
	}
	if sentMbox.Name() != "Sent" {
		t.Fatalf("Expected 'Sent', got '%s'", sentMbox.Name())
	}

	// Test DeleteMailbox
	if err := user.DeleteMailbox("Sent"); err != nil {
		t.Fatalf("Failed to delete mailbox: %v", err)
	}

	mailboxes, err = user.ListMailboxes(false)
	if err != nil {
		t.Fatalf("Failed to list mailboxes: %v", err)
	}
	if len(mailboxes) != 1 {
		t.Fatalf("Expected 1 mailbox after delete, got %d", len(mailboxes))
	}
}

func TestMemoryMailbox_Messages(t *testing.T) {
	storage, err := New("storage.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create storage: %v", err)
	}

	s := storage.(*Storage)
	s.autoCreate = true

	// Get user
	userIface, err := s.GetOrCreateIMAPAcct("msg@example.com")
	if err != nil {
		t.Fatalf("Failed to get user: %v", err)
	}

	user := userIface.(*User)

	// Get INBOX
	_, inboxIface, err := user.GetMailbox("INBOX", false, nil)
	if err != nil {
		t.Fatalf("Failed to get INBOX: %v", err)
	}

	inbox := inboxIface.(*Mailbox)

	// Test CreateMessage
	msg := []byte("From: test@example.com\r\nTo: msg@example.com\r\nSubject: Test\r\n\r\nHello World")
	literal := &literalWrapper{Reader: strings.NewReader(string(msg)), length: len(msg)}
	if err := inbox.CreateMessage([]string{}, time.Now(), literal); err != nil {
		t.Fatalf("Failed to create message: %v", err)
	}

	// Test Status
	status, err := inbox.Status([]imap.StatusItem{imap.StatusMessages, imap.StatusUnseen})
	if err != nil {
		t.Fatalf("Failed to get status: %v", err)
	}
	if status.Messages != 1 {
		t.Fatalf("Expected 1 message, got %d", status.Messages)
	}
	if status.Unseen != 1 {
		t.Fatalf("Expected 1 unseen message, got %d", status.Unseen)
	}

	// Test UpdateMessagesFlags
	seqSet := new(imap.SeqSet)
	seqSet.AddNum(1)
	if err := inbox.UpdateMessagesFlags(false, seqSet, imap.AddFlags, false, []string{imap.SeenFlag}); err != nil {
		t.Fatalf("Failed to update flags: %v", err)
	}

	// Verify flag was added
	status, err = inbox.Status([]imap.StatusItem{imap.StatusUnseen})
	if err != nil {
		t.Fatalf("Failed to get status: %v", err)
	}
	if status.Unseen != 0 {
		t.Fatalf("Expected 0 unseen messages after marking as seen, got %d", status.Unseen)
	}
}

func TestMemoryStorage_PruneUnusedAccounts(t *testing.T) {
	storage, err := New("storage.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create storage: %v", err)
	}

	s := storage.(*Storage)
	cfg := config.NewMap(nil, config.Node{})
	if err := s.Init(cfg); err != nil {
		t.Fatalf("Failed to initialize storage: %v", err)
	}

	// Create account and mark as old
	if err := s.CreateIMAPAcct("old@example.com"); err != nil {
		t.Fatalf("Failed to create account: %v", err)
	}

	// Manually set creation time to old
	s.accounts["old@example.com"].Created = time.Now().Unix() - 86400*30 // 30 days ago

	// Create recent account
	if err := s.CreateIMAPAcct("new@example.com"); err != nil {
		t.Fatalf("Failed to create account: %v", err)
	}

	// Prune accounts older than 7 days
	if err := s.PruneUnusedAccounts(7 * 24 * time.Hour); err != nil {
		t.Fatalf("Failed to prune accounts: %v", err)
	}

	// Verify old account was deleted
	accounts, err := s.ListIMAPAccts()
	if err != nil {
		t.Fatalf("Failed to list accounts: %v", err)
	}
	if len(accounts) != 1 {
		t.Fatalf("Expected 1 account after pruning, got %d", len(accounts))
	}
	if accounts[0] != "new@example.com" {
		t.Fatalf("Expected 'new@example.com', got '%s'", accounts[0])
	}
}
