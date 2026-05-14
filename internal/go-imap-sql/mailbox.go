package imapsql

import (
	"bufio"
	"bytes"
	"database/sql"
	"errors"
	"io"
	"io/ioutil"
	nettextproto "net/textproto"
	"time"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
	"github.com/emersion/go-imap/backend/backendutil"
	"github.com/emersion/go-message/textproto"
	mess "github.com/foxcpp/go-imap-mess"
	"github.com/mailru/easyjson/jwriter"
)

const flagsSep = "{"

// Message UIDs are assigned sequentelly, starting at 1.

type Mailbox struct {
	user     User
	name     string
	parent   *Backend
	id       uint64
	readOnly bool

	conn   backend.Conn
	handle *mess.MailboxHandle
}

func (m *Mailbox) Close() error {
	if m.conn == nil {
		return nil
	}
	return m.handle.Close()
}

func (m *Mailbox) Poll(expunge bool) error {
	m.handle.Sync(expunge)
	return nil
}

func (m *Mailbox) Name() string {
	return m.name
}

func (m *Mailbox) Conn() backend.Conn {
	return m.conn
}

func (m *Mailbox) Info() (*imap.MailboxInfo, error) {
	panic("should be removed from go-imap")
}

var standardFlags = map[string]struct{}{
	imap.SeenFlag:     {},
	imap.AnsweredFlag: {},
	imap.FlaggedFlag:  {},
	imap.DeletedFlag:  {},
	imap.DraftFlag:    {},
}

func (m *Mailbox) readUids() (uids []uint32, recent *imap.SeqSet, err error) {
	recent = new(imap.SeqSet)
	var recentCount uint32
	rows, err := m.parent.listMsgUidsRecent.Query(m.id)
	if err != nil && err != sql.ErrNoRows {
		m.parent.logMboxErr(m, err, "readUids (listMsgUidsRecent)")
		return nil, nil, wrapErrf(err, "readUids %s", m.name)
	}
	if err == nil {
		defer rows.Close()
		for rows.Next() {
			var (
				uid        uint32
				recentFlag int
			)
			if err := rows.Scan(&uid, &recentFlag); err != nil {
				m.parent.logMboxErr(m, err, "readUids (listMsgUidsRecent scan)")
				return nil, nil, wrapErrf(err, "readUids %s", m.name)
			}
			uids = append(uids, uid)
			if recentFlag == 1 {
				recentCount++
				recent.AddNum(uid)
			}
		}
	}
	return uids, recent, nil
}

func (m *Mailbox) initSelected(unsetRecent bool) (uids []uint32, recent *imap.SeqSet, status *imap.MailboxStatus, err error) {
	if m.parent.Opts.DisableRecent {
		unsetRecent = false
	}

	tx, err := m.parent.db.Begin(!unsetRecent)
	if err != nil {
		return nil, nil, nil, wrapErrf(err, "statusInit %s", m.name)
	}
	defer tx.Rollback() // nolint:errcheck

	status = imap.NewMailboxStatus(m.name, []imap.StatusItem{
		imap.StatusMessages, imap.StatusRecent, imap.StatusUidNext,
		imap.StatusUidValidity, imap.StatusUnseen})
	status.Flags = []string{
		imap.SeenFlag, imap.AnsweredFlag, imap.FlaggedFlag,
		imap.DeletedFlag, imap.DraftFlag,
	}
	status.PermanentFlags = []string{
		imap.SeenFlag, imap.AnsweredFlag, imap.FlaggedFlag,
		imap.DeletedFlag, imap.DraftFlag,
		`\*`,
	}

	rows, err := tx.Stmt(m.parent.usedFlags).Query(m.id)
	if err != nil {
		m.parent.logMboxErr(m, err, "initSelected (used flags)")
		return nil, nil, nil, wrapErrf(err, "initSelected (usedFlags) %s", m.name)
	}
	defer rows.Close()
	for rows.Next() {
		var flag string
		if err := rows.Scan(&flag); err != nil {
			m.parent.logMboxErr(m, err, "initialize (used flags)")
			return nil, nil, nil, wrapErrf(err, "initSelected (usedFlags) %s", m.name)
		}
		if _, ok := standardFlags[flag]; ok {
			continue
		}
		status.Flags = append(status.Flags, flag)
		status.PermanentFlags = append(status.PermanentFlags, flag)
	}

	var unseenUid uint32
	err = tx.Stmt(m.parent.firstUnseenUid).QueryRow(m.id).Scan(&unseenUid)
	if err != nil && err != sql.ErrNoRows {
		m.parent.logMboxErr(m, err, "initSelected (first unseen)")
		return nil, nil, nil, wrapErrf(err, "initSelected %s", m.name)
	}

	row := tx.Stmt(m.parent.unseenCount).QueryRow(m.id)
	if err := row.Scan(&status.Unseen); err != nil {
		if err != sql.ErrNoRows {
			m.parent.logMboxErr(m, err, "initSelected (unseen count)")
			return nil, nil, nil, wrapErrf(err, "initSelected %s", m.name)
		}

		// Don't return it if there is no unseen messages.
		delete(status.Items, imap.StatusUnseen)
		status.UnseenSeqNum = 0
	}
	if status.Unseen == 0 {
		delete(status.Items, imap.StatusUnseen)
		status.UnseenSeqNum = 0
	}

	recent = new(imap.SeqSet)
	var recentCount uint32
	rows, err = tx.Stmt(m.parent.listMsgUidsRecent).Query(m.id)
	if err != nil && err != sql.ErrNoRows {
		m.parent.logMboxErr(m, err, "initSelected (listMsgUidsRecent)")
		return nil, nil, nil, wrapErrf(err, "initSelected %s", m.name)
	}
	if err == nil {
		defer rows.Close()
		for rows.Next() {
			var (
				uid        uint32
				recentFlag int
			)
			if err := rows.Scan(&uid, &recentFlag); err != nil {
				m.parent.logMboxErr(m, err, "initSelected (listMsgUidsRecent scan)")
				return nil, nil, nil, wrapErrf(err, "initSelected %s", m.name)
			}
			uids = append(uids, uid)
			if uid == unseenUid {
				status.UnseenSeqNum = uint32(len(uids))
			}
			if recentFlag == 1 {
				recentCount++
				recent.AddNum(uid)
			}
		}
	}

	if len(uids) > 10000 {
		m.parent.Opts.Log.Println("loaded a large mailbox with", len(uids), "messages, beware of performance issues")
	} else if len(uids) > 100 {
		m.parent.Opts.Log.Debugln("initialized uidMap for selected mailbox:", len(uids))
	} else {
		m.parent.Opts.Log.Debugln("initialized uidMap for selected mailbox:", len(uids), uids)
	}

	status.Messages = uint32(len(uids))
	status.Recent = recentCount

	if unsetRecent {
		if _, err := tx.Stmt(m.parent.clearRecent).Exec(m.id); err != nil {
			m.parent.logMboxErr(m, err, "initSelected (clearRecent)")
		}
	}

	if err := tx.Stmt(m.parent.uidNext).QueryRow(m.id).Scan(&status.UidNext); err != nil {
		if err != sql.ErrNoRows {
			m.parent.logMboxErr(m, err, "initSelected (uidNext scan)")
			return nil, nil, nil, wrapErrf(err, "initSelected %s", m.name)
		}
		status.UidNext = 1
	}

	row = tx.Stmt(m.parent.uidValidity).QueryRow(m.id)
	if err := row.Scan(&status.UidValidity); err != nil {
		m.parent.logMboxErr(m, err, "initSelected (uidValidity)")
		return nil, nil, nil, wrapErrf(err, "initSelected (uidvalidity) %s", m.name)
	}

	if unsetRecent {
		if err := tx.Commit(); err != nil {
			m.parent.logMboxErr(m, err, "initSelected (commit)")
		}
	}

	return uids, recent, status, nil
}

func (m *Mailbox) incrementMsgCounters(tx *sql.Tx) (uint32, error) {
	// On PostgreSQL we can just do everything in one query.
	// Increment both uidNext and msgsCount and return previous uidNext.
	if m.parent.db.driver == "postgres" {
		var nextId uint32
		err := tx.Stmt(m.parent.uidNextIncrease).QueryRow(1, m.id).Scan(&nextId)
		return nextId, err
	}

	// For other DBs we fallback to using a query with explicit locking.

	res := sql.NullInt64{}
	if err := tx.Stmt(m.parent.uidNextLocked).QueryRow(m.id).Scan(&res); err != nil {
		return 0, err
	}

	if _, err := tx.Stmt(m.parent.uidNextIncrease).Exec(1, m.id); err != nil {
		return 0, err
	}

	if res.Valid {
		return uint32(res.Int64), nil
	} else {
		return 1, nil
	}
}

func (m *Mailbox) createMessageLimit(tx *sql.Tx) *uint32 {
	var res sql.NullInt64
	var row *sql.Row
	if tx == nil {
		row = m.parent.mboxMsgSizeLimit.QueryRow(m.id)
	} else {
		row = tx.Stmt(m.parent.mboxMsgSizeLimit).QueryRow(m.id)
	}
	if err := row.Scan(&res); err != nil {
		return new(uint32) // 0
	}

	if !res.Valid {
		return nil
	} else {
		val := uint32(res.Int64)
		return &val
	}
}

func (m *Mailbox) CreateMessageLimit() *uint32 {
	return m.createMessageLimit(nil)
}

func (m *Mailbox) SetMessageLimit(val *uint32) error {
	_, err := m.parent.setMboxMsgSizeLimit.Exec(val, m.id)
	return err
}

func extractCachedData(hdr textproto.Header, bufferedBody *bufio.Reader) (bodyStructBlob, cachedHeadersBlob []byte, err error) {
	hdrs := make(map[string][]string, len(cachedHeaderFields))
	for field := hdr.Fields(); field.Next(); {
		cKey := nettextproto.CanonicalMIMEHeaderKey(field.Key())
		if _, ok := cachedHeaderFields[cKey]; !ok {
			continue
		}
		hdrs[cKey] = append(hdrs[cKey], field.Value())
	}

	bodyStruct, err := backendutil.FetchBodyStructure(hdr, bufferedBody, true)
	if err != nil {
		return nil, nil, err
	}

	jw := jwriter.Writer{}
	buf := bytes.NewBuffer(make([]byte, 0, 2048))
	easyjsonMarshalBodyStruct(&jw, *bodyStruct)
	jw.DumpTo(buf)
	bodyStructBlob = buf.Bytes()

	buf = bytes.NewBuffer(make([]byte, 0, 2048))
	easyjsonMarshalCachedHeader(&jw, hdrs)
	jw.DumpTo(buf)
	cachedHeadersBlob = buf.Bytes()
	return
}

func (b *Backend) processBody(literal imap.Literal) (bodyStruct, cachedHeader []byte, extBodyKey string, err error) {
	extBodyKey, err = randomKey()
	if err != nil {
		return nil, nil, "", err
	}

	objSize := literal.Len()
	if b.Opts.CompressAlgo != "" {
		objSize = 0
	}

	extWriter, err := b.extStore.Create(extBodyKey, int64(objSize))
	if err != nil {
		return nil, nil, "", err
	}
	defer extWriter.Close()

	compressW, err := b.compressAlgo.WrapCompress(extWriter, b.Opts.CompressAlgoParams)
	if err != nil {
		return nil, nil, "", err
	}
	defer compressW.Close()

	bodyReader := io.TeeReader(literal, compressW)
	bufferedBody := bufio.NewReader(bodyReader)
	hdr, err := textproto.ReadHeader(bufferedBody)
	if err != nil {
		b.extStore.Delete([]string{extBodyKey})
		return nil, nil, "", wrapErr(err, "CreateMessage (readHeader)")
	}

	bodyStruct, cachedHeader, err = extractCachedData(hdr, bufferedBody)
	if err != nil {
		b.extStore.Delete([]string{extBodyKey})
		return nil, nil, "", wrapErr(err, "CreateMessage (extractCachedData)")
	}

	// Consume all remaining body so io.TeeReader used with external store will
	// copy everything to extWriter.
	_, err = io.Copy(ioutil.Discard, bufferedBody)
	if err != nil {
		b.extStore.Delete([]string{extBodyKey})
		return nil, nil, "", wrapErr(err, "CreateMessage (ReadAll consume)")
	}

	if err := extWriter.Sync(); err != nil {
		return nil, nil, "", wrapErr(err, "CreateMessage (Sync)")
	}

	return
}

func (m *Mailbox) checkAppendLimit(length int) error {
	mboxLimit := m.CreateMessageLimit()
	if mboxLimit != nil && uint32(length) > *mboxLimit {
		return backend.ErrTooBig
	} else if mboxLimit == nil {
		userLimit := m.user.CreateMessageLimit()
		if userLimit != nil && uint32(length) > *userLimit {
			return backend.ErrTooBig
		} else if userLimit == nil {
			if m.parent.Opts.MaxMsgBytes != nil && uint32(length) > *m.parent.Opts.MaxMsgBytes {
				return backend.ErrTooBig
			}
		}
	}
	return nil
}

func (m *Mailbox) CreateMessage(flags []string, date time.Time, fullBody imap.Literal) error {
	if err := m.checkAppendLimit(fullBody.Len()); err != nil {
		m.parent.logMboxErr(m, errors.New("appendlimit hit"), "CreateMessage (checkAppendLimit)")
		return err
	}

	if date.IsZero() {
		date = time.Now()
	}

	newFlags := make([]string, 0, len(flags))
	haveSeen := uint8(0) // it needs to be stored in SQL, hence integer
	for _, flag := range flags {
		if flag == imap.RecentFlag {
			continue
		}
		if flag == imap.SeenFlag {
			haveSeen = 1
		}
		newFlags = append(newFlags, flag)
	}
	flags = newFlags

	// Important to run before transaction, otherwise it will deadlock on
	// SQLite.
	var flagsAddStmt *sql.Stmt
	if len(flags) != 0 {
		var err error
		flagsAddStmt, err = m.parent.getFlagsAddStmt(len(flags))
		if err != nil {
			m.parent.logMboxErr(m, err, "CreateMessage (getFlagsAddStmt)")
			return wrapErr(err, "CreateMessage")
		}
	}

	tx, err := m.parent.db.BeginLevel(sql.LevelReadCommitted, false)
	if err != nil {
		m.parent.logMboxErr(m, err, "CreateMessage (tx start)")
		return wrapErr(err, "CreateMessage (tx begin)")
	}
	defer tx.Rollback() // nolint:errcheck

	msgId, err := m.incrementMsgCounters(tx)
	if err != nil {
		m.parent.logMboxErr(m, err, "CreateMessage (uidNext)")
		return wrapErr(err, "CreateMessage (uidNext)")
	}

	bodyLen := fullBody.Len()
	bodyStruct, cachedHdr, extBodyKey, err := m.parent.processBody(fullBody)
	if err != nil {
		return err
	}

	if _, err = tx.Stmt(m.parent.addExtKey).Exec(extBodyKey, m.user.id, 1); err != nil {
		if err := m.parent.extStore.Delete([]string{extBodyKey}); err != nil {
			m.parent.logMboxErr(m, err, "delete extBodyKey)")
		}
		m.parent.logMboxErr(m, err, "CreateMessage (addExtKey)")
		return wrapErr(err, "CreateMessage (addExtKey)")
	}

	recent := m.parent.mngr.NewMessage(m.id, msgId)
	recentI := 0
	if recent {
		recentI = 1
	}
	_, err = tx.Stmt(m.parent.addMsg).Exec(
		m.id, msgId, date.Unix(),
		bodyLen,
		bodyStruct, cachedHdr, extBodyKey,
		haveSeen, m.parent.Opts.CompressAlgo,
		recentI,
	)
	if err != nil {
		if err := m.parent.extStore.Delete([]string{extBodyKey}); err != nil {
			m.parent.logMboxErr(m, err, "delete extBodyKey)")
		}
		m.parent.logMboxErr(m, err, "CreateMessage (addMsg)")
		return wrapErr(err, "CreateMessage (addMsg)")
	}

	if len(flags) != 0 {
		params := m.makeFlagsAddStmtArgs(flags, msgId, msgId)
		if _, err = tx.Stmt(flagsAddStmt).Exec(params...); err != nil {
			if err := m.parent.extStore.Delete([]string{extBodyKey}); err != nil {
				m.parent.logMboxErr(m, err, "delete extBodyKey)")
			}
			m.parent.logMboxErr(m, err, "CreateMessage (flags)")
			return wrapErr(err, "CreateMessage (flags)")
		}
	}

	if err = tx.Commit(); err != nil {
		if err := m.parent.extStore.Delete([]string{extBodyKey}); err != nil {
			m.parent.logMboxErr(m, err, "delete extBodyKey)")
		}
		m.parent.logMboxErr(m, err, "CreateMessage (tx commit)")
		return wrapErr(err, "CreateMessage (tx commit)")
	}

	return nil
}

func (m *Mailbox) MoveMessages(uid bool, seqset *imap.SeqSet, dest string) error {
	defer m.handle.Sync(true)

	tx, err := m.parent.db.Begin(false)
	if err != nil {
		m.parent.logMboxErr(m, err, "MoveMessages (tx start)", uid, seqset, dest)
		return wrapErr(err, "MoveMessages (tx start)")
	}
	defer tx.Rollback() // nolint:errcheck

	seqset, err = m.handle.ResolveSeq(uid, seqset)
	if err != nil {
		return err
	}

	for _, seq := range seqset.Set {
		_, err = tx.Stmt(m.parent.markUid).Exec(m.id, seq.Start, seq.Stop)
		if err != nil {
			m.parent.logMboxErr(m, err, "MoveMessages (mark)", uid, seqset, dest)
			return wrapErr(err, "MoveMessages (mark)")
		}
	}

	// There is no way we can reassign UIDs properly in UPDATE statment so we
	// have to use INSERT + DELETE. This is still better than complete message
	// copy and removal logic, though.

	var destID uint64
	if err := tx.Stmt(m.parent.mboxId).QueryRow(m.user.id, dest).Scan(&destID); err != nil {
		if err == sql.ErrNoRows {
			return backend.ErrNoSuchMailbox
		}
		m.parent.logMboxErr(m, err, "MoveMessages (target lookup)", uid, seqset, dest)
		return wrapErr(err, "MoveMessages (target lookup)")
	}

	// Copy messages and flags...
	copiedCount := uint32(0)
	for _, seq := range seqset.Set {
		stats, err := tx.Stmt(m.parent.copyMsgsUid).Exec(destID, destID, copiedCount, m.id, seq.Start, seq.Stop)
		if err != nil {
			m.parent.logMboxErr(m, err, "MoveMessages (copy msgs)", uid, seqset, dest)
			return wrapErr(err, "MoveMessages (copy msgs)")
		}
		if _, err := tx.Stmt(m.parent.copyMsgFlagsUid).Exec(destID, destID, copiedCount, m.id, seq.Start, seq.Stop); err != nil {
			m.parent.logMboxErr(m, err, "MoveMessages (copy msg flags)", uid, seqset, dest)
			return wrapErr(err, "MoveMessages (copy msg flags)")
		}
		affected, err := stats.RowsAffected()
		if err != nil {
			m.parent.logMboxErr(m, err, "MoveMessages (rows affected)", uid, seqset, dest)
			return wrapErr(err, "MoveMessages (rows affected)")
		}
		copiedCount += uint32(affected)
	}
	m.parent.Opts.Log.Debugf("copied %v messages to mboxId=%v", copiedCount, destID)

	var expunged []uint32
	rows, err := tx.Stmt(m.parent.markedUids).Query(m.id)
	if err != nil {
		m.parent.logMboxErr(m, err, "MoveMessages (marked uids)", uid, seqset, dest)
		return wrapErr(err, "MoveMessages (marked uids)")
	}
	for rows.Next() {
		var msgId uint32
		var extKey sql.NullString
		if err := rows.Scan(&msgId, &extKey); err != nil {
			m.parent.logMboxErr(m, err, "MoveMessages (marked uids scan)", uid, seqset, dest)
			return wrapErr(err, "MoveMessages (marked uids scan)")
		}

		expunged = append(expunged, msgId)
	}

	// Delete marked messages (copies in the source mailbox)
	if _, err := tx.Stmt(m.parent.delMarked).Exec(); err != nil {
		m.parent.logMboxErr(m, err, "MoveMessages (decrease counters)", uid, seqset, dest)
		return wrapErr(err, "MoveMessages (decrease counters)")
	}

	var oldUidNext uint32
	if err := tx.Stmt(m.parent.uidNext).QueryRow(destID).Scan(&oldUidNext); err != nil {
		m.parent.logMboxErr(m, err, "MoveMessages (old uidNext)", uid, seqset, dest)
		return wrapErr(err, "MoveMessages (old uidNext)")
	}

	// Increase UIDNEXT for the target mailbox.
	if _, err := tx.Stmt(m.parent.uidNextIncrease).Exec(copiedCount, destID); err != nil {
		m.parent.logMboxErr(m, err, "MoveMessages (increase counters)", uid, seqset, dest)
		return wrapErr(err, "MoveMessages (increase counters)")
	}

	if err := tx.Commit(); err != nil {
		m.parent.logMboxErr(m, err, "MoveMessages (tx commit)", uid, seqset, dest)
		return wrapErr(err, "MoveMessages (tx commit)")
	}

	for _, uid := range expunged {
		m.handle.Removed(uid)
	}
	m.parent.mngr.NewMessages(destID, imap.SeqSet{Set: []imap.Seq{{Start: oldUidNext, Stop: oldUidNext + copiedCount - 1}}})

	return nil
}

func (m *Mailbox) CopyMessages(uid bool, seqset *imap.SeqSet, dest string) error {
	tx, err := m.parent.db.BeginLevel(sql.LevelRepeatableRead, false)
	if err != nil {
		m.parent.logMboxErr(m, err, "CopyMessages (tx start)", uid, seqset, dest)
		return wrapErr(err, "CopyMessages")
	}
	defer tx.Rollback() // nolint:errcheck

	seqset, err = m.handle.ResolveSeq(uid, seqset)
	if err != nil {
		if uid {
			return nil
		}
		return err
	}

	firstCopy, lastCopy, destID, err := m.copyMessages(tx, seqset, dest)
	if err != nil {
		if err == backend.ErrNoSuchMailbox {
			return err
		}
		m.parent.logMboxErr(m, err, "CopyMessages", uid, seqset, dest)
		return wrapErr(err, "CopyMessages")
	}

	persistRecent := m.parent.mngr.NewMessages(destID, imap.SeqSet{Set: []imap.Seq{{Start: firstCopy, Stop: lastCopy}}})
	if persistRecent {
		if _, err := tx.Stmt(m.parent.addRecentToLast).Exec(destID, destID, lastCopy-firstCopy+1); err != nil {
			m.parent.logMboxErr(m, err, "CopyMessages (persistRecent)", uid, seqset, dest)
			return wrapErr(err, "CopyMessages")
		}
	}

	if err := tx.Commit(); err != nil {
		m.parent.logMboxErr(m, err, "CopyMessages (tx commit)", uid, seqset, dest)
		return wrapErr(err, "CopyMessages")
	}

	return nil
}

func (m *Mailbox) DelMessages(uid bool, seqset *imap.SeqSet) error {
	tx, err := m.parent.db.BeginLevel(sql.LevelRepeatableRead, false)
	if err != nil {
		m.parent.logMboxErr(m, err, "DelMessages (tx start)", uid, seqset)
		return wrapErr(err, "DelMessages")
	}
	defer tx.Rollback() // nolint:errcheck

	seqset, err = m.handle.ResolveSeq(uid, seqset)
	if err != nil {
		return err
	}

	deleted, err := m.delMessages(tx, seqset)
	if err != nil {
		if err == backend.ErrNoSuchMailbox {
			return err
		}
		m.parent.logMboxErr(m, err, "DelMessages", uid, seqset)
		return wrapErr(err, "DelMessages")
	}

	if err := tx.Commit(); err != nil {
		m.parent.logMboxErr(m, err, "DelMessages (tx commit)", uid, seqset)
		return wrapErr(err, "DelMessages")
	}

	m.handle.RemovedSet(deleted)

	return nil
}

func (m *Mailbox) delMessages(tx *sql.Tx, seqset *imap.SeqSet) (imap.SeqSet, error) {
	for _, seq := range seqset.Set {
		m.parent.Opts.Log.Println("delMessages: marking SQL window range", seq.Start, seq.Stop, "for deletion")
		_, err := tx.Stmt(m.parent.markUid).Exec(m.id, seq.Start, seq.Stop)
		if err != nil {
			return imap.SeqSet{}, err
		}
	}

	var (
		deletedExtKeys []string
		deletedUids    imap.SeqSet
		deletedCount   uint32
	)

	rows, err := tx.Stmt(m.parent.markedUids).Query(m.id)
	if err != nil {
		return imap.SeqSet{}, err
	}
	for rows.Next() {
		var uid uint32
		var extKey sql.NullString
		if err := rows.Scan(&uid, &extKey); err != nil {
			return imap.SeqSet{}, err
		}
		m.parent.Opts.Log.Println("delMessages:", uid, extKey, "is marked")

		deletedExtKeys = append(deletedExtKeys, extKey.String)
		deletedUids.AddNum(uid)
		deletedCount++
	}
	if err := rows.Err(); err != nil {
		return imap.SeqSet{}, err
	}

	m.parent.Opts.Log.Println("delMessages: deleting storage keys: ", deletedExtKeys)
	if err := m.parent.extStore.Delete(deletedExtKeys); err != nil {
		return imap.SeqSet{}, err
	}

	if _, err := tx.Stmt(m.parent.delMarked).Exec(); err != nil {
		return imap.SeqSet{}, err
	}

	m.parent.Opts.Log.Println("delMessages: deleted", deletedCount, "messages")
	return deletedUids, err
}

func (m *Mailbox) copyMessages(tx *sql.Tx, seqset *imap.SeqSet, dest string) (firstCopy, lastCopy uint32, destID uint64, err error) {
	row := tx.Stmt(m.parent.mboxId).QueryRow(m.user.id, dest)
	if err := row.Scan(&destID); err != nil {
		if err == sql.ErrNoRows {
			return 0, 0, 0, backend.ErrNoSuchMailbox
		}
	}

	m.parent.Opts.Log.Debugln("copyMessages: resolved target mailbox name to", destID)

	srcId := m.id
	var totalCopied uint32
	for _, seq := range seqset.Set {
		stats, err := tx.Stmt(m.parent.copyMsgsUid).Exec(destID, destID, totalCopied, srcId, seq.Start, seq.Stop)
		if err != nil {
			return 0, 0, 0, err
		}
		if _, err := tx.Stmt(m.parent.copyMsgFlagsUid).Exec(destID, destID, totalCopied, srcId, seq.Start, seq.Stop); err != nil {
			return 0, 0, 0, err
		}

		affected, err := stats.RowsAffected()
		if err != nil {
			return 0, 0, 0, err
		}
		totalCopied += uint32(affected)
		m.parent.Opts.Log.Debugln("copyMessages: copied", affected, "messages for range", seq, "SQL:", seq.Start, seq.Stop)

		if _, err := tx.Stmt(m.parent.incrementRefUid).Exec(m.user.id, srcId, seq.Start, seq.Stop); err != nil {
			return 0, 0, 0, err
		}
	}

	var oldUidNext uint32
	if err := tx.Stmt(m.parent.uidNext).QueryRow(destID).Scan(&oldUidNext); err != nil {
		return 0, 0, 0, err
	}

	if _, err := tx.Stmt(m.parent.uidNextIncrease).Exec(totalCopied, destID); err != nil {
		return 0, 0, 0, err
	}

	return oldUidNext, oldUidNext + totalCopied - 1, destID, nil
}

func (m *Mailbox) Expunge() error {
	defer m.handle.Sync(true)

	tx, err := m.parent.db.Begin(false)
	if err != nil {
		m.parent.logMboxErr(m, err, "Expunge (tx start)")
		return wrapErr(err, "Expunge")
	}
	defer tx.Rollback() // nolint:errcheck

	var (
		uids          imap.SeqSet
		expungedCount uint32
	)
	rows, err := tx.Stmt(m.parent.deletedUids).Query(m.id)
	if err != nil {
		m.parent.logMboxErr(m, err, "Expunge (deletedUids)")
		return wrapErr(err, "Expunge")
	}
	defer rows.Close()
	for rows.Next() {
		var uid uint32
		if err := rows.Scan(&uid); err != nil {
			m.parent.logMboxErr(m, err, "Expunge (deletedUids scan)")
			return wrapErr(err, "Expunge")
		}
		uids.AddNum(uid)
		expungedCount++
	}
	if err := rows.Err(); err != nil {
		m.parent.logMboxErr(m, err, "Expunge (deletedUids)")
		return wrapErr(err, "Expunge")
	}
	m.parent.Opts.Log.Debugln("expunge: pending removal for uids", uids, expungedCount)

	rows.Close()

	keys, err := m.expungeExternal(tx)
	if err != nil {
		m.parent.logMboxErr(m, err, "Expunge (external prepare)")
		return err
	}

	_, err = tx.Stmt(m.parent.expungeMbox).Exec(m.id, m.id)
	if err != nil {
		m.parent.logMboxErr(m, err, "Expunge (expunge)")
		return wrapErr(err, "Expunge")
	}

	if _, err := tx.Stmt(m.parent.deleteZeroRef).Exec(m.user.id); err != nil {
		m.parent.logMboxErr(m, err, "Expunge (deleteZeroRef)")
		return wrapErr(err, "Expunge")
	}

	if err := tx.Commit(); err != nil {
		m.parent.logMboxErr(m, err, "Expunge (tx commit)")
		return wrapErr(err, "Expunge")
	}

	if err := m.parent.extStore.Delete(keys); err != nil {
		return wrapErr(err, "Expunge (external)")
	}

	m.handle.RemovedSet(uids)

	return nil
}

func (m *Mailbox) expungeExternal(tx *sql.Tx) ([]string, error) {
	if _, err := tx.Stmt(m.parent.decreaseRefForDeleted).Exec(m.user.id, m.id); err != nil {
		return nil, wrapErr(err, "Expunge (external decrease for deleted)")
	}

	rows, err := tx.Stmt(m.parent.zeroRef).Query(m.user.id, m.id)
	if err != nil {
		return nil, wrapErr(err, "Expunge (external zeroRef collect)")
	}
	defer rows.Close()

	keys := make([]string, 0, 16)
	for rows.Next() {
		var extKey string
		if err := rows.Scan(&extKey); err != nil {
			return nil, wrapErr(err, "Expunge (external scan)")
		}
		keys = append(keys, extKey)

	}

	return keys, nil
}

func (m *Mailbox) Idle(done <-chan struct{}) {
	m.handle.Idle(done)
}
