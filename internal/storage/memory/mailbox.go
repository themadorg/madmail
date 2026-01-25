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
	"sync"
	"time"

	imap "github.com/emersion/go-imap"
	imapbackend "github.com/emersion/go-imap/backend"
)

// Mailbox represents an in-memory IMAP mailbox.
type Mailbox struct {
	name       string
	user       *User
	subscribed bool

	mu       sync.RWMutex
	messages []*Message
	nextUID  uint32

	// For IDLE support
	idleListeners []chan<- imapbackend.Update
}

// Message represents an in-memory message.
type Message struct {
	UID      uint32
	Date     time.Time
	Size     uint32
	Flags    []string
	Literal  []byte
	Headers  map[string][]string
	Body     []byte
	BodyStructure *imap.BodyStructure
}

func newMailbox(name string, user *User) *Mailbox {
	return &Mailbox{
		name:          name,
		user:          user,
		subscribed:    (name == "INBOX"),
		messages:      make([]*Message, 0),
		nextUID:       1,
		idleListeners: make([]chan<- imapbackend.Update, 0),
	}
}

// Name implements imapbackend.Mailbox
func (m *Mailbox) Name() string {
	return m.name
}

// Close implements imapbackend.Mailbox
func (m *Mailbox) Close() error {
	return nil
}

// Info implements imapbackend.Mailbox
func (m *Mailbox) Info() (*imap.MailboxInfo, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	info := &imap.MailboxInfo{
		Attributes: []string{},
		Delimiter:  "/",
		Name:       m.name,
	}

	return info, nil
}

// Poll implements imapbackend.Mailbox
func (m *Mailbox) Poll(expunge bool) error {
	// For in-memory implementation, no polling needed
	return nil
}

// Status implements imapbackend.Mailbox
func (m *Mailbox) Status(items []imap.StatusItem) (*imap.MailboxStatus, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	status := imap.NewMailboxStatus(m.name, items)
	status.Messages = uint32(len(m.messages))
	status.Recent = 0
	status.Unseen = 0
	status.UidNext = m.nextUID
	status.UidValidity = 1

	for _, msg := range m.messages {
		hasSeenFlag := false
		for _, flag := range msg.Flags {
			if flag == imap.SeenFlag {
				hasSeenFlag = true
				break
			}
		}
		if !hasSeenFlag {
			status.Unseen++
		}
	}

	return status, nil
}

// SetSubscribed implements imapbackend.Mailbox
func (m *Mailbox) SetSubscribed(subscribed bool) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.subscribed = subscribed
	return nil
}

// Check implements imapbackend.Mailbox
func (m *Mailbox) Check() error {
	return nil
}

// ListMessages implements imapbackend.Mailbox
func (m *Mailbox) ListMessages(uid bool, seqSet *imap.SeqSet, items []imap.FetchItem, ch chan<- *imap.Message) error {
	m.mu.RLock()
	defer m.mu.RUnlock()

	defer close(ch)

	for i, msg := range m.messages {
		seqNum := uint32(i + 1)
		var id uint32
		if uid {
			id = msg.UID
		} else {
			id = seqNum
		}

		if !seqSet.Contains(id) {
			continue
		}

		imapMsg := imap.NewMessage(seqNum, items)
		for _, item := range items {
			switch item {
			case imap.FetchEnvelope:
				imapMsg.Envelope = m.buildEnvelope(msg)
			case imap.FetchBody, imap.FetchBodyStructure:
				imapMsg.BodyStructure = msg.BodyStructure
			case imap.FetchFlags:
				imapMsg.Flags = msg.Flags
			case imap.FetchInternalDate:
				imapMsg.InternalDate = msg.Date
			case imap.FetchRFC822Size:
				imapMsg.Size = msg.Size
			case imap.FetchUid:
				imapMsg.Uid = msg.UID
			}
		}

		ch <- imapMsg
	}

	return nil
}

func (m *Mailbox) buildEnvelope(msg *Message) *imap.Envelope {
	env := &imap.Envelope{
		Date:    msg.Date,
		Subject: m.getHeader(msg, "Subject"),
	}

	if from := m.getHeader(msg, "From"); from != "" {
		env.From = []*imap.Address{{PersonalName: "", MailboxName: from}}
	}
	if to := m.getHeader(msg, "To"); to != "" {
		env.To = []*imap.Address{{PersonalName: "", MailboxName: to}}
	}

	return env
}

func (m *Mailbox) getHeader(msg *Message, name string) string {
	if values, ok := msg.Headers[name]; ok && len(values) > 0 {
		return values[0]
	}
	return ""
}

// SearchMessages implements imapbackend.Mailbox
func (m *Mailbox) SearchMessages(uid bool, criteria *imap.SearchCriteria) ([]uint32, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	var matches []uint32
	for i, msg := range m.messages {
		seqNum := uint32(i + 1)
		if m.matchesCriteria(msg, criteria) {
			if uid {
				matches = append(matches, msg.UID)
			} else {
				matches = append(matches, seqNum)
			}
		}
	}

	return matches, nil
}

func (m *Mailbox) matchesCriteria(msg *Message, criteria *imap.SearchCriteria) bool {
	// Simple implementation - just match all for now
	// In a real implementation, you would check all criteria fields
	return true
}

// CreateMessage implements imapbackend.Mailbox
func (m *Mailbox) CreateMessage(flags []string, date time.Time, body imap.Literal) error {
	bodyLen := body.Len()
	msg := &Message{
		Flags:   flags,
		Date:    date,
		Size:    uint32(bodyLen),
		Literal: make([]byte, bodyLen),
	}

	if _, err := body.Read(msg.Literal); err != nil {
		return err
	}

	return m.appendMessage(msg)
}

func (m *Mailbox) appendMessage(msg *Message) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	msg.UID = m.nextUID
	m.nextUID++
	m.messages = append(m.messages, msg)

	// Notify IDLE listeners
	update := &imapbackend.ExpungeUpdate{
		SeqNum: uint32(len(m.messages)),
	}

	for _, listener := range m.idleListeners {
		select {
		case listener <- update:
		default:
			// Don't block if listener isn't ready
		}
	}

	return nil
}

// UpdateMessagesFlags implements imapbackend.Mailbox
func (m *Mailbox) UpdateMessagesFlags(uid bool, seqSet *imap.SeqSet, operation imap.FlagsOp, silent bool, flags []string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	for i, msg := range m.messages {
		seqNum := uint32(i + 1)
		var id uint32
		if uid {
			id = msg.UID
		} else {
			id = seqNum
		}

		if !seqSet.Contains(id) {
			continue
		}

		switch operation {
		case imap.SetFlags:
			msg.Flags = flags
		case imap.AddFlags:
			for _, flag := range flags {
				if !contains(msg.Flags, flag) {
					msg.Flags = append(msg.Flags, flag)
				}
			}
		case imap.RemoveFlags:
			newFlags := make([]string, 0, len(msg.Flags))
			for _, existingFlag := range msg.Flags {
				if !contains(flags, existingFlag) {
					newFlags = append(newFlags, existingFlag)
				}
			}
			msg.Flags = newFlags
		}
	}

	return nil
}

// CopyMessages implements imapbackend.Mailbox
func (m *Mailbox) CopyMessages(uid bool, seqSet *imap.SeqSet, destName string) error {
	m.mu.RLock()
	srcMessages := make([]*Message, 0)

	for i, msg := range m.messages {
		seqNum := uint32(i + 1)
		var id uint32
		if uid {
			id = msg.UID
		} else {
			id = seqNum
		}

		if seqSet.Contains(id) {
			// Create a copy of the message
			msgCopy := &Message{
				Date:    msg.Date,
				Size:    msg.Size,
				Flags:   append([]string{}, msg.Flags...),
				Literal: append([]byte{}, msg.Literal...),
			}
			srcMessages = append(srcMessages, msgCopy)
		}
	}
	m.mu.RUnlock()

	// Get destination mailbox directly from user's mailboxes
	m.user.mu.RLock()
	destMemMbox, ok := m.user.mailboxes[destName]
	m.user.mu.RUnlock()

	if !ok {
		return errors.New("destination mailbox not found")
	}

	// Append messages to destination
	for _, msg := range srcMessages {
		if err := destMemMbox.appendMessage(msg); err != nil {
			return err
		}
	}

	return nil
}

// Expunge implements imapbackend.Mailbox
func (m *Mailbox) Expunge() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	newMessages := make([]*Message, 0, len(m.messages))
	for _, msg := range m.messages {
		if !contains(msg.Flags, imap.DeletedFlag) {
			newMessages = append(newMessages, msg)
		}
	}

	m.messages = newMessages
	return nil
}

// Idle implements imapbackend.Mailbox
func (m *Mailbox) Idle(done <-chan struct{}) {
	// Block until done channel is signaled
	<-done
}

func contains(slice []string, item string) bool {
	for _, s := range slice {
		if s == item {
			return true
		}
	}
	return false
}
