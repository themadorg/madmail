package mess

import (
	"testing"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
)

type mailboxStub struct {
	backend.Mailbox
	conn backend.Conn
}

func (m mailboxStub) Conn() backend.Conn {
	return m.conn
}

type connStub struct{}

func (connStub) SendUpdate(_ backend.Update) error {
	return nil
}

func sharesBackingArray(a, b []uint32) bool {
	if len(a) == 0 || len(b) == 0 {
		return len(a) == len(b)
	}
	return &a[0] == &b[0]
}

func TestMailboxHandlesShareInitialUIDSnapshot(t *testing.T) {
	m := NewManager(nil)

	h1, err := m.Mailbox("k", mailboxStub{conn: connStub{}}, []uint32{1, 2, 3}, new(imap.SeqSet))
	if err != nil {
		t.Fatalf("Mailbox first handle: %v", err)
	}
	h2, err := m.Mailbox("k", mailboxStub{conn: connStub{}}, []uint32{1, 2, 3}, new(imap.SeqSet))
	if err != nil {
		t.Fatalf("Mailbox second handle: %v", err)
	}

	if !sharesBackingArray(h1.uidMap, h2.uidMap) {
		t.Fatalf("expected handles to share UID map backing array")
	}
}

func TestSyncRebindsHandleToLatestSharedSnapshot(t *testing.T) {
	m := NewManager(nil)

	h1, err := m.Mailbox("k", mailboxStub{conn: connStub{}}, []uint32{1, 2, 3}, new(imap.SeqSet))
	if err != nil {
		t.Fatalf("Mailbox first handle: %v", err)
	}
	h2, err := m.Mailbox("k", mailboxStub{conn: connStub{}}, []uint32{1, 2, 3}, new(imap.SeqSet))
	if err != nil {
		t.Fatalf("Mailbox second handle: %v", err)
	}

	_ = m.NewMessage("k", 4)

	if got := len(h1.uidMap); got != 3 {
		t.Fatalf("expected first handle to keep stale snapshot before sync, got len=%d", got)
	}
	if got := len(h2.uidMap); got != 3 {
		t.Fatalf("expected second handle to keep stale snapshot before sync, got len=%d", got)
	}

	h1.Sync(true)
	if got := len(h1.uidMap); got != 4 {
		t.Fatalf("expected first handle to refresh snapshot on sync, got len=%d", got)
	}
	if got := len(h2.uidMap); got != 3 {
		t.Fatalf("expected second handle to remain stale until sync, got len=%d", got)
	}

	h2.Sync(true)
	if got := len(h2.uidMap); got != 4 {
		t.Fatalf("expected second handle to refresh snapshot on sync, got len=%d", got)
	}
	if !sharesBackingArray(h1.uidMap, h2.uidMap) {
		t.Fatalf("expected both handles to point to shared snapshot after sync")
	}
}

func TestRemoveUpdatesSharedSnapshotAndKeepsHandleViewUntilSync(t *testing.T) {
	m := NewManager(nil)

	h1, err := m.Mailbox("k", mailboxStub{conn: connStub{}}, []uint32{1, 2, 3}, new(imap.SeqSet))
	if err != nil {
		t.Fatalf("Mailbox first handle: %v", err)
	}
	h2, err := m.Mailbox("k", mailboxStub{conn: connStub{}}, []uint32{1, 2, 3}, new(imap.SeqSet))
	if err != nil {
		t.Fatalf("Mailbox second handle: %v", err)
	}

	h1.Removed(2)

	if got := len(h1.uidMap); got != 3 {
		t.Fatalf("expected first handle to keep stale snapshot before sync, got len=%d", got)
	}
	if got := len(h2.uidMap); got != 3 {
		t.Fatalf("expected second handle to keep stale snapshot before sync, got len=%d", got)
	}

	h2.Sync(true)
	if got := len(h2.uidMap); got != 2 {
		t.Fatalf("expected second handle to refresh to compacted shared snapshot, got len=%d", got)
	}

	h1.Sync(true)
	if got := len(h1.uidMap); got != 2 {
		t.Fatalf("expected first handle to refresh to compacted shared snapshot, got len=%d", got)
	}
	if !sharesBackingArray(h1.uidMap, h2.uidMap) {
		t.Fatalf("expected both handles to point to shared compacted snapshot")
	}
}
