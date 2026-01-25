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
	"bufio"
	"bytes"
	"errors"
	"sort"
	"strings"
	"sync/atomic"
	"time"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
	"github.com/emersion/go-message/textproto"
)

// User implements backend.User
type User struct {
	storage *Storage
	account *Account
}

func (u *User) Username() string {
	return u.account.Username
}

func (u *User) Status(mboxName string, items []imap.StatusItem) (*imap.MailboxStatus, error) {
	u.account.mu.RLock()
	mbox, ok := u.account.Mailboxes[mboxName]
	u.account.mu.RUnlock()

	if !ok {
		return nil, backend.ErrNoSuchMailbox
	}

	mbox.mu.RLock()
	defer mbox.mu.RUnlock()

	status := imap.NewMailboxStatus(mbox.Name, items)
	status.Flags = []string{imap.SeenFlag, imap.AnsweredFlag, imap.FlaggedFlag, imap.DeletedFlag, imap.DraftFlag}
	status.PermanentFlags = []string{imap.SeenFlag, imap.AnsweredFlag, imap.FlaggedFlag, imap.DeletedFlag, imap.DraftFlag, `\*`}

	for _, item := range items {
		switch item {
		case imap.StatusMessages:
			status.Messages = uint32(len(mbox.Messages))
		case imap.StatusRecent:
			count := uint32(0)
			for _, ref := range mbox.Messages {
				ref.mu.RLock()
				for _, f := range ref.Flags {
					if f == imap.RecentFlag {
						count++
						break
					}
				}
				ref.mu.RUnlock()
			}
			status.Recent = count
		case imap.StatusUidNext:
			status.UidNext = mbox.UIDNext
		case imap.StatusUidValidity:
			status.UidValidity = mbox.UIDValidity
		case imap.StatusUnseen:
			count := uint32(0)
			firstUnseen := uint32(0)
			seqNum := uint32(0)
			for _, ref := range mbox.Messages {
				seqNum++
				ref.mu.RLock()
				seen := false
				for _, f := range ref.Flags {
					if f == imap.SeenFlag {
						seen = true
						break
					}
				}
				ref.mu.RUnlock()
				if !seen {
					count++
					if firstUnseen == 0 {
						firstUnseen = seqNum
					}
				}
			}
			status.Unseen = count
			status.UnseenSeqNum = firstUnseen
		}
	}

	return status, nil
}

func (u *User) SetSubscribed(mboxName string, subscribed bool) error {
	u.account.mu.RLock()
	mbox, ok := u.account.Mailboxes[mboxName]
	u.account.mu.RUnlock()

	if !ok {
		return backend.ErrNoSuchMailbox
	}

	mbox.mu.Lock()
	defer mbox.mu.Unlock()
	mbox.Subscribed = subscribed
	return nil
}

func (u *User) CreateMessage(mboxName string, flags []string, date time.Time, body imap.Literal, selectedMailbox backend.Mailbox) error {
	u.account.mu.RLock()
	mbox, ok := u.account.Mailboxes[mboxName]
	u.account.mu.RUnlock()

	if !ok {
		return backend.ErrNoSuchMailbox
	}

	// Read the message body
	var buf bytes.Buffer
	if _, err := buf.ReadFrom(body); err != nil {
		return err
	}
	bodyBytes := buf.Bytes()

	// Parse headers
	reader := bufio.NewReader(bytes.NewReader(bodyBytes))
	hdr, err := textproto.ReadHeader(reader)
	if err != nil {
		// If we can't parse headers, use empty headers
		hdr = textproto.Header{}
	}

	// Store the message
	messageID := u.storage.storeMessage(hdr, bodyBytes)

	// Add to mailbox
	mbox.mu.Lock()
	uid := mbox.UIDNext
	mbox.UIDNext++

	// Ensure Recent flag is set
	hasRecent := false
	for _, f := range flags {
		if f == imap.RecentFlag {
			hasRecent = true
			break
		}
	}
	if !hasRecent {
		flags = append(flags, imap.RecentFlag)
	}

	ref := &MessageRef{
		MessageID: messageID,
		UID:       uid,
		Flags:     flags,
	}
	mbox.Messages[uid] = ref
	mbox.mu.Unlock()

	// Update quota
	u.account.mu.Lock()
	u.account.QuotaUsed += int64(len(bodyBytes))
	u.account.mu.Unlock()

	return nil
}

func (u *User) ListMailboxes(subscribed bool) ([]imap.MailboxInfo, error) {
	u.account.mu.RLock()
	defer u.account.mu.RUnlock()

	var mailboxes []imap.MailboxInfo
	for _, mbox := range u.account.Mailboxes {
		if subscribed && !mbox.Subscribed {
			continue
		}

		mbox.mu.RLock()
		info := imap.MailboxInfo{
			Name:       mbox.Name,
			Delimiter:  ".",
			Attributes: make([]string, len(mbox.Attributes)),
		}
		copy(info.Attributes, mbox.Attributes)

		// Add HasChildren/HasNoChildren attributes
		hasChildren := false
		prefix := mbox.Name + "."
		for name := range u.account.Mailboxes {
			if strings.HasPrefix(name, prefix) {
				hasChildren = true
				break
			}
		}
		if hasChildren {
			info.Attributes = append(info.Attributes, imap.HasChildrenAttr)
		} else {
			info.Attributes = append(info.Attributes, imap.HasNoChildrenAttr)
		}
		mbox.mu.RUnlock()

		mailboxes = append(mailboxes, info)
	}

	// Sort mailboxes by name
	sort.Slice(mailboxes, func(i, j int) bool {
		return mailboxes[i].Name < mailboxes[j].Name
	})

	return mailboxes, nil
}

func (u *User) GetMailbox(name string, readOnly bool, conn backend.Conn) (*imap.MailboxStatus, backend.Mailbox, error) {
	u.account.mu.RLock()
	mbox, ok := u.account.Mailboxes[name]
	u.account.mu.RUnlock()

	if !ok {
		return nil, nil, backend.ErrNoSuchMailbox
	}

	mb := &MailboxBackend{
		storage:  u.storage,
		account:  u.account,
		mailbox:  mbox,
		readOnly: readOnly,
	}

	status, err := u.Status(name, []imap.StatusItem{
		imap.StatusMessages,
		imap.StatusRecent,
		imap.StatusUidNext,
		imap.StatusUidValidity,
		imap.StatusUnseen,
	})
	if err != nil {
		return nil, nil, err
	}

	return status, mb, nil
}

func (u *User) CreateMailbox(name string) error {
	u.account.mu.Lock()
	defer u.account.mu.Unlock()

	if _, ok := u.account.Mailboxes[name]; ok {
		return errors.New("mailbox already exists")
	}

	u.storage.mu.Lock()
	u.storage.uidValidityCounter++
	uidValidity := u.storage.uidValidityCounter
	u.storage.mu.Unlock()

	u.account.Mailboxes[name] = &Mailbox{
		Name:        name,
		Subscribed:  true,
		Messages:    make(map[uint32]*MessageRef),
		UIDNext:     1,
		UIDValidity: uidValidity,
	}

	return nil
}

func (u *User) DeleteMailbox(name string) error {
	// Cannot delete INBOX
	if strings.EqualFold(name, "INBOX") {
		return errors.New("cannot delete INBOX")
	}

	u.account.mu.Lock()
	defer u.account.mu.Unlock()

	mbox, ok := u.account.Mailboxes[name]
	if !ok {
		return backend.ErrNoSuchMailbox
	}

	mbox.mu.Lock()
	// Release all message references
	for _, ref := range mbox.Messages {
		u.storage.releaseMessage(ref.MessageID)
	}
	mbox.mu.Unlock()

	delete(u.account.Mailboxes, name)
	return nil
}

func (u *User) RenameMailbox(existingName, newName string) error {
	// Cannot rename INBOX to another name
	if strings.EqualFold(existingName, "INBOX") {
		return errors.New("cannot rename INBOX")
	}

	u.account.mu.Lock()
	defer u.account.mu.Unlock()

	mbox, ok := u.account.Mailboxes[existingName]
	if !ok {
		return backend.ErrNoSuchMailbox
	}

	if _, ok := u.account.Mailboxes[newName]; ok {
		return errors.New("mailbox already exists")
	}

	mbox.mu.Lock()
	mbox.Name = newName
	mbox.mu.Unlock()

	delete(u.account.Mailboxes, existingName)
	u.account.Mailboxes[newName] = mbox

	return nil
}

func (u *User) Logout() error {
	// Nothing to do for in-memory storage
	return nil
}

// MailboxBackend implements backend.Mailbox
type MailboxBackend struct {
	storage  *Storage
	account  *Account
	mailbox  *Mailbox
	readOnly bool
}

func (m *MailboxBackend) Name() string {
	return m.mailbox.Name
}

func (m *MailboxBackend) Close() error {
	// Nothing to do for in-memory storage
	return nil
}

func (m *MailboxBackend) Info() (*imap.MailboxInfo, error) {
	m.mailbox.mu.RLock()
	defer m.mailbox.mu.RUnlock()

	info := &imap.MailboxInfo{
		Name:       m.mailbox.Name,
		Delimiter:  ".",
		Attributes: make([]string, len(m.mailbox.Attributes)),
	}
	copy(info.Attributes, m.mailbox.Attributes)

	// Add HasChildren/HasNoChildren attributes
	m.account.mu.RLock()
	hasChildren := false
	prefix := m.mailbox.Name + "."
	for name := range m.account.Mailboxes {
		if strings.HasPrefix(name, prefix) {
			hasChildren = true
			break
		}
	}
	m.account.mu.RUnlock()

	if hasChildren {
		info.Attributes = append(info.Attributes, imap.HasChildrenAttr)
	} else {
		info.Attributes = append(info.Attributes, imap.HasNoChildrenAttr)
	}

	return info, nil
}

// Poll checks for updates (no-op for in-memory)
func (m *MailboxBackend) Poll(expunge bool) error {
	if expunge {
		return m.Expunge()
	}
	return nil
}

// getSortedUIDs returns UIDs in ascending order
func (m *MailboxBackend) getSortedUIDs() []uint32 {
	uids := make([]uint32, 0, len(m.mailbox.Messages))
	for u := range m.mailbox.Messages {
		uids = append(uids, u)
	}
	sort.Slice(uids, func(i, j int) bool { return uids[i] < uids[j] })
	return uids
}

func (m *MailboxBackend) ListMessages(uid bool, seqSet *imap.SeqSet, items []imap.FetchItem, ch chan<- *imap.Message) error {
	defer close(ch)

	m.mailbox.mu.RLock()
	defer m.mailbox.mu.RUnlock()

	uids := m.getSortedUIDs()

	for seqNum, msgUID := range uids {
		seqNum++ // Sequence numbers start at 1

		// Check if this message matches the sequence set
		var matches bool
		if uid {
			matches = seqSet.Contains(msgUID)
		} else {
			matches = seqSet.Contains(uint32(seqNum))
		}

		if !matches {
			continue
		}

		ref := m.mailbox.Messages[msgUID]
		msg, err := m.fetchMessage(ref, uint32(seqNum), items)
		if err != nil {
			continue
		}

		ch <- msg
	}

	return nil
}

func (m *MailboxBackend) fetchMessage(ref *MessageRef, seqNum uint32, items []imap.FetchItem) (*imap.Message, error) {
	val, ok := m.storage.messages.Load(ref.MessageID)
	if !ok {
		return nil, errors.New("message not found")
	}
	storedMsg := val.(*Message)

	msg := imap.NewMessage(seqNum, items)
	msg.Uid = ref.UID

	for _, item := range items {
		switch item {
		case imap.FetchEnvelope:
			msg.Envelope = parseEnvelope(storedMsg.Header)
		case imap.FetchBody, imap.FetchBodyStructure:
			// Simplified body structure
			msg.BodyStructure = &imap.BodyStructure{
				MIMEType:    "text",
				MIMESubType: "plain",
				Encoding:    "7bit",
				Size:        uint32(storedMsg.Size),
			}
		case imap.FetchFlags:
			ref.mu.RLock()
			msg.Flags = make([]string, len(ref.Flags))
			copy(msg.Flags, ref.Flags)
			ref.mu.RUnlock()
		case imap.FetchInternalDate:
			msg.InternalDate = storedMsg.Date
		case imap.FetchRFC822Size:
			msg.Size = uint32(storedMsg.Size)
		case imap.FetchUid:
			msg.Uid = ref.UID
		default:
			// Handle BODY[] fetches
			section, err := imap.ParseBodySectionName(item)
			if err != nil {
				continue
			}

			// Build the full message
			var buf bytes.Buffer
			textproto.WriteHeader(&buf, storedMsg.Header)
			buf.Write(storedMsg.Body)

			msg.Body[section] = imap.Literal(bytes.NewReader(buf.Bytes()))
		}
	}

	return msg, nil
}

func parseEnvelope(header textproto.Header) *imap.Envelope {
	env := &imap.Envelope{
		Subject:   header.Get("Subject"),
		MessageId: header.Get("Message-ID"),
	}

	if dateStr := header.Get("Date"); dateStr != "" {
		if t, err := time.Parse(time.RFC1123Z, dateStr); err == nil {
			env.Date = t
		} else if t, err := time.Parse(time.RFC822Z, dateStr); err == nil {
			env.Date = t
		}
	}

	// Parse address fields
	env.From = parseAddressList(header.Get("From"))
	env.To = parseAddressList(header.Get("To"))
	env.Cc = parseAddressList(header.Get("Cc"))
	env.Bcc = parseAddressList(header.Get("Bcc"))
	env.ReplyTo = parseAddressList(header.Get("Reply-To"))
	env.Sender = parseAddressList(header.Get("Sender"))

	return env
}

func parseAddressList(value string) []*imap.Address {
	if value == "" {
		return nil
	}

	// Simple address parsing (handles basic formats)
	var addrs []*imap.Address
	for _, part := range strings.Split(value, ",") {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}

		addr := &imap.Address{}

		// Try to parse "Name <email@domain.com>" format
		if idx := strings.LastIndex(part, "<"); idx >= 0 {
			if endIdx := strings.LastIndex(part, ">"); endIdx > idx {
				addr.PersonalName = strings.TrimSpace(part[:idx])
				email := part[idx+1 : endIdx]
				if atIdx := strings.LastIndex(email, "@"); atIdx >= 0 {
					addr.MailboxName = email[:atIdx]
					addr.HostName = email[atIdx+1:]
				}
			}
		} else if atIdx := strings.LastIndex(part, "@"); atIdx >= 0 {
			addr.MailboxName = part[:atIdx]
			addr.HostName = part[atIdx+1:]
		}

		if addr.MailboxName != "" || addr.HostName != "" {
			addrs = append(addrs, addr)
		}
	}

	return addrs
}

func (m *MailboxBackend) SearchMessages(uid bool, criteria *imap.SearchCriteria) ([]uint32, error) {
	m.mailbox.mu.RLock()
	defer m.mailbox.mu.RUnlock()

	var results []uint32

	uids := m.getSortedUIDs()

	for seqNum, msgUID := range uids {
		seqNum++ // Sequence numbers start at 1

		ref := m.mailbox.Messages[msgUID]
		if m.matchesCriteria(ref, uint32(seqNum), criteria) {
			if uid {
				results = append(results, msgUID)
			} else {
				results = append(results, uint32(seqNum))
			}
		}
	}

	return results, nil
}

func (m *MailboxBackend) matchesCriteria(ref *MessageRef, seqNum uint32, criteria *imap.SearchCriteria) bool {
	// Basic criteria matching - implement as needed
	if criteria == nil {
		return true
	}

	// Check sequence number
	if criteria.SeqNum != nil && !criteria.SeqNum.Contains(seqNum) {
		return false
	}

	// Check UID
	if criteria.Uid != nil && !criteria.Uid.Contains(ref.UID) {
		return false
	}

	// Check flags
	ref.mu.RLock()
	flags := ref.Flags
	ref.mu.RUnlock()

	for _, flag := range criteria.WithFlags {
		found := false
		for _, f := range flags {
			if strings.EqualFold(f, flag) {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}

	for _, flag := range criteria.WithoutFlags {
		for _, f := range flags {
			if strings.EqualFold(f, flag) {
				return false
			}
		}
	}

	return true
}

func (m *MailboxBackend) UpdateMessagesFlags(uid bool, seqSet *imap.SeqSet, operation imap.FlagsOp, silent bool, flags []string) error {
	m.mailbox.mu.RLock()
	defer m.mailbox.mu.RUnlock()

	uids := m.getSortedUIDs()

	for seqNum, msgUID := range uids {
		seqNum++ // Sequence numbers start at 1

		var matches bool
		if uid {
			matches = seqSet.Contains(msgUID)
		} else {
			matches = seqSet.Contains(uint32(seqNum))
		}

		if !matches {
			continue
		}

		ref := m.mailbox.Messages[msgUID]
		ref.mu.Lock()

		switch operation {
		case imap.SetFlags:
			ref.Flags = make([]string, len(flags))
			copy(ref.Flags, flags)
		case imap.AddFlags:
			for _, flag := range flags {
				found := false
				for _, f := range ref.Flags {
					if strings.EqualFold(f, flag) {
						found = true
						break
					}
				}
				if !found {
					ref.Flags = append(ref.Flags, flag)
				}
			}
		case imap.RemoveFlags:
			newFlags := make([]string, 0, len(ref.Flags))
			for _, f := range ref.Flags {
				remove := false
				for _, flag := range flags {
					if strings.EqualFold(f, flag) {
						remove = true
						break
					}
				}
				if !remove {
					newFlags = append(newFlags, f)
				}
			}
			ref.Flags = newFlags
		}

		ref.mu.Unlock()
	}

	return nil
}

func (m *MailboxBackend) CopyMessages(uid bool, seqSet *imap.SeqSet, destName string) error {
	m.account.mu.RLock()
	destMbox, ok := m.account.Mailboxes[destName]
	m.account.mu.RUnlock()

	if !ok {
		return backend.ErrNoSuchMailbox
	}

	m.mailbox.mu.RLock()
	defer m.mailbox.mu.RUnlock()

	uids := m.getSortedUIDs()

	for seqNum, msgUID := range uids {
		seqNum++ // Sequence numbers start at 1

		var matches bool
		if uid {
			matches = seqSet.Contains(msgUID)
		} else {
			matches = seqSet.Contains(uint32(seqNum))
		}

		if !matches {
			continue
		}

		ref := m.mailbox.Messages[msgUID]

		// Increment message reference count
		if val, ok := m.storage.messages.Load(ref.MessageID); ok {
			msg := val.(*Message)
			atomic.AddInt32(&msg.RefCount, 1)
		}

		// Copy to destination mailbox
		destMbox.mu.Lock()
		newUID := destMbox.UIDNext
		destMbox.UIDNext++

		ref.mu.RLock()
		newRef := &MessageRef{
			MessageID: ref.MessageID,
			UID:       newUID,
			Flags:     make([]string, len(ref.Flags)),
		}
		copy(newRef.Flags, ref.Flags)
		ref.mu.RUnlock()

		destMbox.Messages[newUID] = newRef
		destMbox.mu.Unlock()
	}

	return nil
}

func (m *MailboxBackend) Expunge() error {
	m.mailbox.mu.Lock()
	defer m.mailbox.mu.Unlock()

	// Find messages with \Deleted flag
	toDelete := make([]uint32, 0)
	for uid, ref := range m.mailbox.Messages {
		ref.mu.RLock()
		for _, f := range ref.Flags {
			if f == imap.DeletedFlag {
				toDelete = append(toDelete, uid)
				break
			}
		}
		ref.mu.RUnlock()
	}

	// Delete them
	for _, uid := range toDelete {
		ref := m.mailbox.Messages[uid]

		// Get message size for quota update
		var msgSize int64
		if val, ok := m.storage.messages.Load(ref.MessageID); ok {
			msgSize = int64(val.(*Message).Size)
		}

		// Release message reference
		m.storage.releaseMessage(ref.MessageID)

		// Remove from mailbox
		delete(m.mailbox.Messages, uid)

		// Update quota
		m.account.mu.Lock()
		m.account.QuotaUsed -= msgSize
		if m.account.QuotaUsed < 0 {
			m.account.QuotaUsed = 0
		}
		m.account.mu.Unlock()
	}

	return nil
}

// Idle allows backend to send updates without explicit Poll calls
func (m *MailboxBackend) Idle(done <-chan struct{}) {
	// For in-memory backend, we just wait for done
	<-done
}

// MoveMessages implements the MOVE extension
func (m *MailboxBackend) MoveMessages(uid bool, seqSet *imap.SeqSet, dest string) error {
	// Mark messages as deleted
	if err := m.UpdateMessagesFlags(uid, seqSet, imap.AddFlags, true, []string{imap.DeletedFlag}); err != nil {
		return err
	}

	// Copy messages
	if err := m.CopyMessages(uid, seqSet, dest); err != nil {
		return err
	}

	// Expunge
	return m.Expunge()
}

// Compile-time interface checks
var (
	_ backend.User    = (*User)(nil)
	_ backend.Mailbox = (*MailboxBackend)(nil)
)
