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
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	imap "github.com/emersion/go-imap"
	imapbackend "github.com/emersion/go-imap/backend"
)

// User represents an in-memory IMAP user.
type User struct {
	username string
	storage  *Storage

	mu        sync.RWMutex
	mailboxes map[string]*Mailbox
}

func newUser(username string, storage *Storage) *User {
	u := &User{
		username:  username,
		storage:   storage,
		mailboxes: make(map[string]*Mailbox),
	}

	// Create default INBOX
	inbox := newMailbox("INBOX", u)
	u.mailboxes["INBOX"] = inbox

	return u
}

// Username implements imapbackend.User
func (u *User) Username() string {
	return u.username
}

// ListMailboxes implements imapbackend.User
func (u *User) ListMailboxes(subscribed bool) ([]imap.MailboxInfo, error) {
	u.mu.RLock()
	defer u.mu.RUnlock()

	mailboxes := make([]imap.MailboxInfo, 0, len(u.mailboxes))
	for _, mbox := range u.mailboxes {
		if !subscribed || mbox.subscribed {
			info := imap.MailboxInfo{
				Attributes: []string{},
				Delimiter:  "/",
				Name:       mbox.name,
			}
			mailboxes = append(mailboxes, info)
		}
	}

	return mailboxes, nil
}

// GetMailbox implements imapbackend.User
func (u *User) GetMailbox(name string, readOnly bool, conn imapbackend.Conn) (*imap.MailboxStatus, imapbackend.Mailbox, error) {
	u.mu.RLock()
	defer u.mu.RUnlock()

	mbox, exists := u.mailboxes[name]
	if !exists {
		return nil, nil, imapbackend.ErrNoSuchMailbox
	}

	// Build status
	status, err := mbox.Status([]imap.StatusItem{
		imap.StatusMessages,
		imap.StatusRecent,
		imap.StatusUnseen,
		imap.StatusUidNext,
		imap.StatusUidValidity,
	})
	if err != nil {
		return nil, nil, err
	}

	return status, mbox, nil
}

// Status implements imapbackend.User
func (u *User) Status(mbox string, items []imap.StatusItem) (*imap.MailboxStatus, error) {
	u.mu.RLock()
	defer u.mu.RUnlock()

	m, exists := u.mailboxes[mbox]
	if !exists {
		return nil, imapbackend.ErrNoSuchMailbox
	}

	return m.Status(items)
}

// SetSubscribed implements imapbackend.User
func (u *User) SetSubscribed(mbox string, subscribed bool) error {
	u.mu.RLock()
	defer u.mu.RUnlock()

	m, exists := u.mailboxes[mbox]
	if !exists {
		return imapbackend.ErrNoSuchMailbox
	}

	return m.SetSubscribed(subscribed)
}

// CreateMailbox implements imapbackend.User
func (u *User) CreateMailbox(name string) error {
	if name == "INBOX" {
		return errors.New("INBOX already exists")
	}

	u.mu.Lock()
	defer u.mu.Unlock()

	if _, exists := u.mailboxes[name]; exists {
		return errors.New("mailbox already exists")
	}

	mbox := newMailbox(name, u)
	u.mailboxes[name] = mbox

	return nil
}

// DeleteMailbox implements imapbackend.User
func (u *User) DeleteMailbox(name string) error {
	if name == "INBOX" {
		return errors.New("cannot delete INBOX")
	}

	u.mu.Lock()
	defer u.mu.Unlock()

	if _, exists := u.mailboxes[name]; !exists {
		return imapbackend.ErrNoSuchMailbox
	}

	// Check if mailbox has children
	prefix := name + "/"
	for mboxName := range u.mailboxes {
		if strings.HasPrefix(mboxName, prefix) {
			return errors.New("mailbox has children")
		}
	}

	delete(u.mailboxes, name)
	return nil
}

// RenameMailbox implements imapbackend.User
func (u *User) RenameMailbox(existingName, newName string) error {
	if existingName == "INBOX" {
		return errors.New("cannot rename INBOX")
	}

	u.mu.Lock()
	defer u.mu.Unlock()

	mbox, exists := u.mailboxes[existingName]
	if !exists {
		return imapbackend.ErrNoSuchMailbox
	}

	if _, exists := u.mailboxes[newName]; exists {
		return errors.New("destination mailbox already exists")
	}

	// Rename mailbox
	mbox.mu.Lock()
	mbox.name = newName
	mbox.mu.Unlock()

	u.mailboxes[newName] = mbox
	delete(u.mailboxes, existingName)

	// Rename children
	prefix := existingName + "/"
	newPrefix := newName + "/"
	for name, childMbox := range u.mailboxes {
		if strings.HasPrefix(name, prefix) {
			newChildName := newPrefix + strings.TrimPrefix(name, prefix)

			childMbox.mu.Lock()
			childMbox.name = newChildName
			childMbox.mu.Unlock()

			u.mailboxes[newChildName] = childMbox
			delete(u.mailboxes, name)
		}
	}

	return nil
}

// CreateMessage implements imapbackend.User
func (u *User) CreateMessage(mbox string, flags []string, date time.Time, body imap.Literal, selectedMailbox imapbackend.Mailbox) error {
	u.mu.RLock()
	m, exists := u.mailboxes[mbox]
	u.mu.RUnlock()

	if !exists {
		// Auto-create mailbox
		if err := u.CreateMailbox(mbox); err != nil {
			return fmt.Errorf("failed to create mailbox: %w", err)
		}
		u.mu.RLock()
		m = u.mailboxes[mbox]
		u.mu.RUnlock()
	}

	return m.CreateMessage(flags, date, body)
}

// Logout implements imapbackend.User  
func (u *User) Logout() error {
	// Update first login time
	u.storage.UpdateFirstLogin(u.username)
	return nil
}
