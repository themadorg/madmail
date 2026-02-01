package mess

import (
	"strconv"
	"sync"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
)

type Logger interface {
	Printf(format string, v ...interface{})
	Println(v ...interface{})
	Debugf(format string, v ...interface{})
	Debugln(v ...interface{})
}

type Manager struct {
	Log         Logger
	handlesLock sync.RWMutex
	handles     map[interface{}]*sharedHandle

	sink chan<- Update

	ExternalSubscribe   func(key interface{})
	ExternalUnsubscribe func(key interface{})
}

func (m *Manager) logf(format string, v ...interface{}) {
	if m.Log != nil {
		m.Log.Debugf(format, v...)
	}
}

func NewManager(log Logger) *Manager {
	return &Manager{
		Log:     log,
		handles: make(map[interface{}]*sharedHandle),
	}
}

type Mailbox interface {
	backend.Mailbox
	Conn() backend.Conn
}

// ManagementHandle initializes a new message handle for the mailbox that
// is opened without an active connection, e.g. from an administrative UI or
// CLI utility.
//
// Such handle never sends updates to mbox.Conn(), only to SetExternalSink if
// set. \Recent flag for new messages will never be shown to such connections
// and it will receive no updates for mailbox changes anyway (Idle, Sync are no-op).
func (m *Manager) ManagementHandle(key interface{}, uids []uint32, recents *imap.SeqSet) *MailboxHandle {
	return &MailboxHandle{
		m:      m,
		key:    key,
		recent: recents,
		uidMap: uids,
	}
}

// Mailbox initializes a new message handle for the mailbox.
//
// key should be a server-global unique identifier for the mailbox.
// uids should contain the list of all message UIDs existing in the mailbox.
//
// recents should contain the list of message UIDs with persistent \Recent flag.
// Note that persistent \Recent should be unset once passed to Mailbox().
// In particular, two subsequent calls should not receive the same value.
func (m *Manager) Mailbox(key interface{}, mbox Mailbox, uids []uint32, recents *imap.SeqSet) (*MailboxHandle, error) {
	m.handlesLock.Lock()
	defer m.handlesLock.Unlock()

	sharedHndl, ok := m.handles[key]
	if sharedHndl == nil {
		sharedHndl = &sharedHandle{
			key:     key,
			handles: map[*MailboxHandle]struct{}{},
		}
		m.logf("go-imap-mess: Mailbox creating new sharedHandle for key=%v", key)
	}

	handle := &MailboxHandle{
		m:            m,
		key:          key,
		shared:       sharedHndl,
		conn:         mbox.Conn(),
		uidMap:       uids,
		recent:       recents,
		pendingFlags: make([]flagsUpdate, 0, 1),
	}
	for _, set := range recents.Set {
		for i := set.Start; i <= set.Stop; i++ {
			handle.recentCount++
		}
	}

	sharedHndl.handlesLock.Lock()
	sharedHndl.handles[handle] = struct{}{}
	numHandles := len(sharedHndl.handles)
	sharedHndl.handlesLock.Unlock()
	m.logf("go-imap-mess: Mailbox registered handle for key=%v, total_handles=%d", key, numHandles)
	if !ok {
		m.handles[key] = sharedHndl
		if m.ExternalSubscribe != nil {
			m.ExternalSubscribe(key)
		}
	}

	return handle, nil
}

// NewMessages performs necessary updates dispatching when
// new messages are added to the mailbox.
//
// Return value indicates whether backend should store
// a persistent \Recent flag in DB for further retrieval
// (see Mailbox)
func (m *Manager) NewMessages(key interface{}, uid imap.SeqSet) (storeRecent bool) {
	if m.sink != nil {
		m.sink <- Update{
			Type:   UpdNewMessage,
			Key:    key,
			SeqSet: uid.String(),
		}
	}

	return m.newMessages(key, uid)
}

func (m *Manager) newMessages(key interface{}, uid imap.SeqSet) (storeRecent bool) {
	m.handlesLock.RLock()
	defer m.handlesLock.RUnlock()

	handle := m.handles[key]
	if handle == nil {
		return false
	}

	handle.handlesLock.RLock()
	defer handle.handlesLock.RUnlock()

	addedRecent := false
	for hndl := range handle.handles {
		hndl.lock.Lock()
		hndl.pendingCreated.AddSet(&uid)
		if !addedRecent {
			hndl.recent.AddSet(&uid)
			for _, set := range uid.Set {
				for i := set.Start; i <= set.Stop; i++ {
					hndl.recentCount++
				}
			}
			hndl.hasNewRecent = true
			addedRecent = true
		}
		hndl.idleUpdate()
		hndl.lock.Unlock()
	}

	return !addedRecent
}

func (m *Manager) NewMessage(key interface{}, uid uint32) (storeRecent bool) {
	m.logf("go-imap-mess: NewMessage called with key=%v uid=%d", key, uid)

	if m.sink != nil {
		m.sink <- Update{
			Type:   UpdNewMessage,
			Key:    key,
			SeqSet: strconv.FormatUint(uint64(uid), 10),
		}
	}

	m.handlesLock.RLock()
	defer m.handlesLock.RUnlock()

	handle := m.handles[key]
	if handle == nil {
		m.logf("go-imap-mess: NewMessage key=%v has NO handles registered, returning storeRecent=true", key)
		return true
	}

	handle.handlesLock.RLock()
	defer handle.handlesLock.RUnlock()

	numHandles := len(handle.handles)
	m.logf("go-imap-mess: NewMessage key=%v uid=%d found %d handles to notify", key, uid, numHandles)

	addedRecent := false
	for hndl := range handle.handles {
		hndl.lock.Lock()
		hndl.pendingCreated.AddNum(uid)
		if !addedRecent {
			hndl.recent.AddNum(uid)
			hndl.hasNewRecent = true
			hndl.recentCount++
			addedRecent = true
		}
		m.logf("go-imap-mess: NewMessage calling idleUpdate for handle key=%v uid=%d", key, uid)
		hndl.idleUpdate()
		hndl.lock.Unlock()
	}

	m.logf("go-imap-mess: NewMessage completed key=%v uid=%d storeRecent=%v", key, uid, !addedRecent)
	return !addedRecent
}

// MailboxDestroyed should be called when the specified key is no longer
// valid for the mailbox e.g. because it was renamed or deleted.
//
// The appropriate place to call the method from is
// DeleteMailbox - MailboxDestroyed should be called
// for all removed mailboxes - and RenameMailbox where
// it should be called for _both_ source and target mailbox.
//
// In all cases it is better to call MailboxDestroyed _after_
// physically deleting the mailbox.
func (m *Manager) MailboxDestroyed(key interface{}) {
	if m.sink != nil {
		m.sink <- Update{
			Type: UpdMboxDestroyed,
			Key:  key,
		}
	}

	m.mailboxDestroyed(key)
}

func (m *Manager) mailboxDestroyed(key interface{}) {
	m.handlesLock.RLock()
	defer m.handlesLock.RUnlock()

	handle := m.handles[key]
	if handle == nil {
		return
	}

	handle.handlesLock.Lock()
	handle.handles = nil
	handle.handlesLock.Unlock()

	delete(m.handles, key)

	if m.ExternalUnsubscribe != nil {
		m.ExternalUnsubscribe(key)
	}
}

func (m *Manager) removedSet(key interface{}, seq imap.SeqSet) {
	m.handlesLock.RLock()
	defer m.handlesLock.RUnlock()

	handle := m.handles[key]
	if handle == nil {
		return
	}

	handle.handlesLock.RLock()
	defer handle.handlesLock.RUnlock()

	for hndl := range handle.handles {
		hndl.lock.Lock()
		hndl.pendingExpunge.AddSet(&seq)
		hndl.idleUpdate()
		hndl.lock.Unlock()
	}
}

func (m *Manager) flagsChanged(key interface{}, uid uint32, newFlags []string) {
	m.handlesLock.RLock()
	defer m.handlesLock.RUnlock()

	handle := m.handles[key]
	if handle == nil {
		return
	}

	handle.handlesLock.RLock()
	defer handle.handlesLock.RUnlock()

	for hndl := range handle.handles {
		hndl.enqueueFlagsUpdate(uid, newFlags)
	}
}
