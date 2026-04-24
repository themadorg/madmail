package imapsql

import (
	"bufio"
	"bytes"
	"database/sql"
	"errors"
	"fmt"
	"io"
	"io/ioutil"
	"time"

	"github.com/emersion/go-imap/backend"
	"github.com/emersion/go-message/textproto"
)

var ErrDeliveryInterrupted = errors.New("sql: delivery transaction interrupted, try again later")

// NewDelivery creates a new state object for atomic delivery session.
//
// Messages added to the storage using that interface are added either to
// all recipients mailboxes or none or them.
//
// Also use of this interface is more efficient than separate GetUser/GetMailbox/CreateMessage
// calls.
//
// Note that for performance reasons, the DB is not locked while the Delivery object
// exists, but only when BodyRaw/BodyParsed is called and until Abort/Commit is called.
// This means that the recipient mailbox can be deleted between AddRcpt and Body* calls.
// In that case, either Body* or Commit will return ErrDeliveryInterrupt.
// Sender should retry delivery after a short delay.
func (b *Backend) NewDelivery() Delivery {
	return Delivery{b: b, perRcptHeader: map[string]textproto.Header{}}
}

func (d *Delivery) clean() {
	d.users = d.users[0:0]
	d.mboxes = d.mboxes[0:0]
	d.extKeys = d.extKeys[0:0]
	for k := range d.perRcptHeader {
		delete(d.perRcptHeader, k)
	}
	d.pendingNotifications = d.pendingNotifications[0:0]
}

// pendingNotification stores info needed to send IMAP IDLE notifications
// after the database transaction has been committed.
type pendingNotification struct {
	mboxId uint64
	msgId  uint32
}

type Delivery struct {
	b             *Backend
	tx            *sql.Tx
	users         []User
	mboxes        []Mailbox
	extKeys       []string
	perRcptHeader map[string]textproto.Header
	flagOverrides map[string][]string
	mboxOverrides map[string]string

	// pendingNotifications stores notifications to be sent after commit.
	// This fixes the race condition where IMAP IDLE clients receive
	// notifications before the database transaction is committed.
	pendingNotifications []pendingNotification
}

// AddRcpt adds the recipient username/mailbox pair to the delivery.
//
// If this function returns an error - further calls will still work
// correctly and there is no need to restart the delivery.
//
// The specified user account and mailbox should exist at the time AddRcpt
// is called, but it can disappear before Body* call, in which case
// Delivery will be terminated with ErrDeliveryInterrupted error.
// See Backend.StartDelivery method documentation for details.
//
// Fields from userHeader, if any, will be prepended to the message header
// *only* for that recipient. Use this to add Received and Delivered-To
// fields with recipient-specific information (e.g. its address).
func (d *Delivery) AddRcpt(username string, userHeader textproto.Header) error {
	username = normalizeUsername(username)

	uid, inboxId, err := d.b.getUserMeta(nil, username)
	if err != nil {
		if err == sql.ErrNoRows {
			return ErrUserDoesntExists
		}
		return err
	}
	d.users = append(d.users, User{id: uid, username: username, parent: d.b, inboxId: inboxId})

	d.perRcptHeader[username] = userHeader

	return nil
}

// FIXME: Fix that goddamned code duplication.

// Mailbox command changes the target mailbox for all recipients.
// It should be called before BodyParsed/BodyRaw.
//
// If it is not called, it defaults to INBOX. If mailbox doesn't
// exist for some users - it will created.
func (d *Delivery) Mailbox(name string) error {
	if cap(d.mboxes) < len(d.users) {
		d.mboxes = make([]Mailbox, 0, len(d.users))
	}

	for _, u := range d.users {
		if mboxName := d.mboxOverrides[u.username]; mboxName != "" {
			_, mbox, err := u.GetMailbox(mboxName, true, nil)
			if err == nil {
				d.mboxes = append(d.mboxes, *mbox.(*Mailbox))
				continue
			}
		}

		_, mbox, err := u.GetMailbox(name, true, nil)
		if err != nil {
			if err != backend.ErrNoSuchMailbox {
				d.mboxes = nil
				return err
			}

			if err := u.CreateMailbox(name); err != nil && err != backend.ErrMailboxAlreadyExists {
				d.mboxes = nil
				return err
			}

			_, mbox, err = u.GetMailbox(name, true, nil)
			if err != nil {
				d.mboxes = nil
				return err
			}
		}

		d.mboxes = append(d.mboxes, *mbox.(*Mailbox))
	}
	return nil
}

// SpecialMailbox is similar to Mailbox method but instead of looking up mailboxes
// by name it looks it up by the SPECIAL-USE attribute.
//
// If no such mailbox exists for some user, it will be created with
// fallbackName and requested SPECIAL-USE attribute set.
//
// The main use-case of this function is to reroute messages into Junk directory
// during multi-recipient delivery.
func (d *Delivery) SpecialMailbox(attribute, fallbackName string) error {
	if cap(d.mboxes) < len(d.users) {
		d.mboxes = make([]Mailbox, 0, len(d.users))
	}
	for _, u := range d.users {
		if mboxName := d.mboxOverrides[u.username]; mboxName != "" {
			_, mbox, err := u.GetMailbox(mboxName, true, nil)
			if err == nil {
				d.mboxes = append(d.mboxes, *mbox.(*Mailbox))
				continue
			}
		}

		var mboxId uint64
		var mboxName string
		err := d.b.specialUseMbox.QueryRow(u.id, attribute).Scan(&mboxName, &mboxId)
		if err != nil {
			if err != sql.ErrNoRows {
				d.mboxes = nil
				return err
			}

			if err := u.CreateMailboxSpecial(fallbackName, attribute); err != nil && err != backend.ErrMailboxAlreadyExists {
				d.mboxes = nil
				return err
			}

			_, mbox, err := u.GetMailbox(fallbackName, true, nil)
			if err != nil {
				d.mboxes = nil
				return err
			}
			d.mboxes = append(d.mboxes, *mbox.(*Mailbox))
			continue
		}

		d.mboxes = append(d.mboxes, Mailbox{user: u, id: mboxId, name: mboxName, parent: d.b})
	}
	return nil
}

func (d *Delivery) UserMailbox(username, mailbox string, flags []string) {
	if d.mboxOverrides == nil {
		d.mboxOverrides = make(map[string]string)
	}
	if d.flagOverrides == nil {
		d.flagOverrides = make(map[string][]string)
	}

	d.mboxOverrides[username] = mailbox
	d.flagOverrides[username] = flags
}

type memoryBuffer struct {
	slice []byte
}

func (mb memoryBuffer) Open() (io.ReadCloser, error) {
	return ioutil.NopCloser(bytes.NewReader(mb.slice)), nil
}

// BodyRaw is convenience wrapper for BodyParsed. Use it only for most simple cases (e.g. for tests).
//
// You want to use BodyParsed in most cases. It is much more efficient. BodyRaw reads the entire message
// into memory.
func (d *Delivery) BodyRaw(message io.Reader) error {
	bufferedMsg := bufio.NewReader(message)
	hdr, err := textproto.ReadHeader(bufferedMsg)
	if err != nil {
		return err
	}

	blob, err := ioutil.ReadAll(bufferedMsg)
	if err != nil {
		return err
	}

	return d.BodyParsed(hdr, len(blob), memoryBuffer{slice: blob})
}

// Buffer is the temporary storage for the message body.
type Buffer interface {
	Open() (io.ReadCloser, error)
}

func (d *Delivery) BodyParsed(header textproto.Header, bodyLen int, body Buffer) error {
	if len(d.mboxes) == 0 {
		if err := d.Mailbox("INBOX"); err != nil {
			return err
		}
	}

	// Pre-generate flag statements outside the transaction to
	// avoid SQLite deadlocks from in-transaction statement prep.
	for _, mbox := range d.mboxes {
		if len(d.flagOverrides[mbox.user.username]) != 0 {
			_, err := d.b.getFlagsAddStmt(len(d.flagOverrides[mbox.user.username]))
			if err != nil {
				return wrapErr(err, "Body")
			}
		}
	}

	// Merge per-recipient headers (now empty for dedup) once.
	header = header.Copy()
	userHeader := d.perRcptHeader[d.mboxes[0].user.username]
	for fields := userHeader.Fields(); fields.Next(); {
		header.Add(fields.Key(), fields.Value())
	}

	headerBlob := bytes.Buffer{}
	if err := textproto.WriteHeader(&headerBlob, header); err != nil {
		return wrapErr(err, "Body (WriteHeader)")
	}

	length := int64(headerBlob.Len()) + int64(bodyLen)
	bodyReader, err := body.Open()
	if err != nil {
		return err
	}

	// Write-once: parse and compress the body into the first
	// recipient's per-user directory.
	firstMbox := d.mboxes[0]
	firstRandKey, err := randomKey()
	if err != nil {
		return wrapErr(err, "Body (randomKey)")
	}
	firstKey := fmt.Sprintf("%d/%s", firstMbox.user.id, firstRandKey)

	bodyStruct, cachedHeader, err := d.b.processParsedBodyOnce(
		headerBlob.Bytes(), header, bodyReader, int64(bodyLen), firstKey,
	)
	if err != nil {
		return err
	}
	d.extKeys = append(d.extKeys, firstKey)

	date := time.Now()

	d.tx, err = d.b.db.BeginLevel(sql.LevelReadCommitted, false)
	if err != nil {
		return wrapErr(err, "Body")
	}

	// Insert the first recipient using the master blob.
	var flagsStmt *sql.Stmt
	if len(d.flagOverrides[firstMbox.user.username]) != 0 {
		flagsStmt, err = d.b.getFlagsAddStmt(len(d.flagOverrides[firstMbox.user.username]))
		if err != nil {
			return wrapErr(err, "Body")
		}
	}
	if err := d.mboxDeliveryFast(firstKey, bodyStruct, cachedHeader, length, firstMbox, date, flagsStmt); err != nil {
		return err
	}

	// Hardlink the blob for remaining recipients.
	for _, mbox := range d.mboxes[1:] {
		rcptRandKey, err := randomKey()
		if err != nil {
			return wrapErr(err, "Body (randomKey)")
		}
		rcptKey := fmt.Sprintf("%d/%s", mbox.user.id, rcptRandKey)

		if err := d.b.extStore.Link(firstKey, rcptKey); err != nil {
			return wrapErr(err, "Body (Link)")
		}
		d.extKeys = append(d.extKeys, rcptKey)

		var rcptFlagsStmt *sql.Stmt
		if len(d.flagOverrides[mbox.user.username]) != 0 {
			rcptFlagsStmt, err = d.b.getFlagsAddStmt(len(d.flagOverrides[mbox.user.username]))
			if err != nil {
				return wrapErr(err, "Body")
			}
		}
		if err := d.mboxDeliveryFast(rcptKey, bodyStruct, cachedHeader, length, mbox, date, rcptFlagsStmt); err != nil {
			return err
		}
	}

	return nil
}

// mboxDeliveryFast inserts a message into a single mailbox using
// precomputed body structure and cached header data. The blob
// identified by extBodyKey must already exist on disk.
func (d *Delivery) mboxDeliveryFast(extBodyKey string, bodyStruct, cachedHeader []byte, length int64, mbox Mailbox, date time.Time, flagsStmt *sql.Stmt) error {
	if _, err := d.tx.Stmt(d.b.addExtKey).Exec(extBodyKey, mbox.user.id, 1); err != nil {
		return wrapErr(err, "Body (addExtKey)")
	}

	// --- operations that involve mboxes table ---
	msgId, err := mbox.incrementMsgCounters(d.tx)
	if err != nil {
		return wrapErr(err, "Body (incrementMsgCounters)")
	}

	persistRecent := 1

	d.pendingNotifications = append(d.pendingNotifications, pendingNotification{
		mboxId: mbox.id,
		msgId:  msgId,
	})

	// --- operations that involve msgs table ---
	_, err = d.tx.Stmt(d.b.addMsg).Exec(
		mbox.id, msgId, date.Unix(),
		length,
		bodyStruct, cachedHeader, extBodyKey,
		0, d.b.Opts.CompressAlgo, persistRecent,
	)
	if err != nil {
		return wrapErr(err, "Body (addMsg)")
	}

	// --- operations that involve flags table ---
	flags := d.flagOverrides[mbox.user.username]
	if len(flags) != 0 {
		params := mbox.makeFlagsAddStmtArgs(flags, msgId, msgId)
		if _, err := d.tx.Stmt(flagsStmt).Exec(params...); err != nil {
			return wrapErr(err, "Body (flagsStmt)")
		}
	}

	return nil
}

func (d *Delivery) Abort() error {
	if d.tx != nil {
		if err := d.tx.Rollback(); err != nil {
			return err
		}
	}
	if len(d.extKeys) > 0 {
		if err := d.b.extStore.Delete(d.extKeys); err != nil {
			return err
		}
	}

	d.clean()
	return nil
}

// Commit finishes the delivery.
//
// If this function returns no error - the message is successfully added to the mailbox
// of *all* recipients.
//
// After Commit or Abort is called, Delivery object can be reused as if it was
// just created.
func (d *Delivery) Commit() error {
	if d.tx != nil {
		if err := d.tx.Commit(); err != nil {
			return err
		}
	}

	// Send IMAP IDLE notifications AFTER the transaction is committed.
	// This fixes the race condition where clients receive notifications
	// before the message is visible in the database.
	// We use the original NewMessage method which handles both local
	// IDLE updates and external pubsub notifications correctly.
	for _, notif := range d.pendingNotifications {
		d.b.mngr.NewMessage(notif.mboxId, notif.msgId)
	}

	d.clean()
	return nil
}

// processParsedBodyOnce parses the MIME structure, extracts cached
// metadata, and compresses the body into the blob identified by key.
// The caller provides a pre-generated key (e.g. "<uid>/<rand>").
func (b *Backend) processParsedBodyOnce(headerInput []byte, header textproto.Header, bodyLiteral io.Reader, bodyLen int64, key string) (bodyStruct, cachedHeader []byte, err error) {
	objSize := int64(len(headerInput)) + bodyLen
	if b.Opts.CompressAlgo != "" {
		objSize = -1
	}

	extWriter, err := b.extStore.Create(key, objSize)
	if err != nil {
		return nil, nil, err
	}
	defer extWriter.Close()

	compressW, err := b.compressAlgo.WrapCompress(extWriter, b.Opts.CompressAlgoParams)
	if err != nil {
		return nil, nil, err
	}
	defer compressW.Close()

	if _, err := compressW.Write(headerInput); err != nil {
		b.extStore.Delete([]string{key})
		return nil, nil, err
	}

	bufferedBody := bufio.NewReader(io.TeeReader(bodyLiteral, compressW))
	bodyStruct, cachedHeader, err = extractCachedData(header, bufferedBody)
	if err != nil {
		b.extStore.Delete([]string{key})
		return nil, nil, err
	}

	// Drain remaining body so TeeReader copies everything to the store.
	_, err = io.Copy(ioutil.Discard, bufferedBody)
	if err != nil {
		b.extStore.Delete([]string{key})
		return nil, nil, err
	}

	if err := extWriter.Sync(); err != nil {
		return nil, nil, err
	}

	return
}
