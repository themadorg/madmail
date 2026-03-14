package imapsql

import (
	"database/sql"
	"errors"
	"strings"
	"time"

	"github.com/emersion/go-imap"
	"github.com/emersion/go-imap/backend"
	namespace "github.com/foxcpp/go-imap-namespace"
)

const MailboxPathSep = "."

type User struct {
	id       uint64
	username string
	inboxId  uint64
	parent   *Backend
}

func (u *User) Username() string {
	return u.username
}

func (u *User) ID() uint64 {
	return u.id
}

func (u *User) ListMailboxes(subscribed bool) ([]imap.MailboxInfo, error) {
	var (
		rows *sql.Rows
		err  error
	)
	if subscribed {
		rows, err = u.parent.listSubbedMboxes.Query(u.id)
	} else {
		rows, err = u.parent.listMboxes.Query(u.id)
	}
	if err != nil {
		u.parent.logUserErr(u, err, "ListMailboxes", subscribed)
		return nil, wrapErr(err, "ListMailboxes")
	}
	defer rows.Close()

	var res []imap.MailboxInfo
	for rows.Next() {
		info := imap.MailboxInfo{
			Attributes: nil,
			Delimiter:  MailboxPathSep,
		}

		var id uint64
		if err := rows.Scan(&id, &info.Name); err != nil {
			u.parent.logUserErr(u, err, "ListMailboxes", subscribed)
			return nil, wrapErr(err, "ListMailboxes")
		}

		res = append(res, info)
	}
	if err := rows.Err(); err != nil {
		u.parent.logUserErr(u, err, "ListMailboxes", subscribed)
		return res, wrapErr(rows.Err(), "ListMailboxes")
	}

	for i, info := range res {
		row := u.parent.getMboxAttrs.QueryRow(u.id, info.Name)
		var mark int
		var specialUse sql.NullString
		if err := row.Scan(&mark, &specialUse); err != nil {
			u.parent.logUserErr(u, err, "ListMailboxes (mbox attrs)")
			continue
		}
		if mark == 1 {
			info.Attributes = []string{imap.MarkedAttr}
		}
		if specialUse.Valid {
			info.Attributes = []string{specialUse.String}
		}

		row = u.parent.hasChildren.QueryRow(info.Name+MailboxPathSep+"%", u.id)
		childrenCount := 0
		if err := row.Scan(&childrenCount); err != nil {
			u.parent.logUserErr(u, err, "ListMailboxes (children count)")
			continue
		}
		if childrenCount != 0 {
			info.Attributes = append(info.Attributes, imap.HasChildrenAttr)
		} else {
			info.Attributes = append(info.Attributes, imap.HasNoChildrenAttr)
		}

		res[i] = info
	}

	return res, nil
}

func (u *User) GetMailbox(name string, readOnly bool, conn backend.Conn) (*imap.MailboxStatus, backend.Mailbox, error) {
	var mbox *Mailbox

	if strings.EqualFold(name, "INBOX") {
		mbox = &Mailbox{user: *u, id: u.inboxId, name: name, parent: u.parent}
	} else {
		row := u.parent.mboxId.QueryRow(u.id, name)
		id := uint64(0)
		if err := row.Scan(&id); err != nil {
			if err == sql.ErrNoRows {
				return nil, nil, backend.ErrNoSuchMailbox
			}
			u.parent.logUserErr(u, err, "GetMailbox", name)
			return nil, nil, wrapErrf(err, "GetMailbox %s", name)
		}
		mbox = &Mailbox{user: *u, id: id, name: name, parent: u.parent}
	}
	mbox.readOnly = readOnly

	if conn == nil {
		// For non-interactive sessions (conn == nil) we do not provide Uids (expensive DB query). 
		// The SMTP internal/go-imap-sql/delivery.go pipeline does not need a list of Uids. 
		// All the methods that use uidMap (Sync, ResolveSeq, UidAsSeq, MsgsCount) have conn == nil guards or are
		// only called from IMAP command handlers that require an active connection. 
		mbox.handle = u.parent.mngr.ManagementHandle(mbox.id, nil, nil)
		return nil, mbox, nil
	}

	mbox.conn = conn
	uids, recent, status, err := mbox.initSelected(!readOnly)
	if err != nil {
		u.parent.logUserErr(u, err, "GetMailbox", name)
		return nil, nil, wrapErrf(err, "GetMailbox %s", name)
	}

	handle, err := u.parent.mngr.Mailbox(mbox.id, mbox, uids, recent)
	if err != nil {
		u.parent.logUserErr(u, err, "GetMailbox handle", name)
		return nil, nil, wrapErrf(err, "GetMailbox %s (get handle)", name)
	}
	mbox.handle = handle

	return status, mbox, nil
}

func (u *User) CreateMessageLimit() *uint32 {
	res := sql.NullInt64{}
	row := u.parent.userMsgSizeLimit.QueryRow(u.id)
	if err := row.Scan(&res); err != nil {
		// Oops!
		return new(uint32)
	}

	if !res.Valid {
		return nil
	} else {
		val := uint32(res.Int64)
		return &val
	}
}

func (u *User) SetMessageLimit(val *uint32) error {
	_, err := u.parent.setUserMsgSizeLimit.Exec(val, u.id)
	return err
}

func (u *User) CreateMailbox(name string) error {
	tx, err := u.parent.db.Begin(false)
	if err != nil {
		u.parent.logUserErr(u, err, "CreateMailbox (tx start)", name)
		return wrapErrf(err, "CreateMailbox %s", name)
	}
	defer tx.Rollback() //nolint:errcheck

	if err := u.createParentDirs(tx, name); err != nil {
		u.parent.logUserErr(u, err, "CreateMailbox (parents)", name)
		return wrapErrf(err, "CreateMailbox (parents) %s", name)
	}

	if _, err := tx.Stmt(u.parent.createMbox).Exec(u.id, name, u.parent.prng.Uint32(), nil); err != nil {
		if isForeignKeyErr(err) {
			return backend.ErrMailboxAlreadyExists
		}
		u.parent.logUserErr(u, err, "CreateMailbox", name)
		return wrapErrf(err, "CreateMailbox %s", name)
	}

	err = tx.Commit()
	u.parent.logUserErr(u, err, "CreateMailbox (tx commit)", name)
	return wrapErrf(err, "CreateMailbox (tx commit) %s", name)
}

var ErrUnsupportedSpecialAttr = errors.New("imap: special attribute is not supported")

// CreateMailboxSpecial creates a mailbox with SPECIAL-USE attribute set.
func (u *User) CreateMailboxSpecial(name, specialUseAttr string) error {
	switch specialUseAttr {
	case imap.AllAttr, imap.FlaggedAttr:
		return ErrUnsupportedSpecialAttr
	case imap.ArchiveAttr, imap.DraftsAttr, imap.JunkAttr, imap.SentAttr, imap.TrashAttr:
	default:
		return ErrUnsupportedSpecialAttr
	}

	tx, err := u.parent.db.Begin(false)
	if err != nil {
		return wrapErrf(err, "CreateMailboxSpecial %s", name)
	}
	defer tx.Rollback() //nolint:errcheck

	if err := u.createParentDirs(tx, name); err != nil {
		return wrapErrf(err, "CreateMailboxSpecial (parents) %s", name)
	}

	if _, err := tx.Stmt(u.parent.createMbox).Exec(u.id, name, u.parent.prng.Uint32(), specialUseAttr); err != nil {
		if isForeignKeyErr(err) {
			return backend.ErrMailboxAlreadyExists
		}
		return wrapErrf(err, "CreateMailboxSpecial %s", name)
	}

	return wrapErrf(tx.Commit(), "CreateMailbox (tx commit) %s", name)
}

func (u *User) DeleteMailbox(name string) error {
	if strings.ToLower(name) == "inbox" {
		return errors.New("DeleteMailbox: can't delete INBOX")
	}

	tx, err := u.parent.db.BeginLevel(sql.LevelRepeatableRead, false)
	if err != nil {
		u.parent.logUserErr(u, err, "DeleteMailbox (tx start)", name)
		return wrapErrf(err, "DeleteMailbox %s", name)
	}
	defer tx.Rollback()

	if _, err := tx.Stmt(u.parent.decreaseRefForMbox).Exec(u.id, name); err != nil {
		u.parent.logUserErr(u, err, "DeleteMailbox (decrease ref)", name)
		return wrapErrf(err, "DeleteMailbox %s", name)
	}

	rows, err := tx.Stmt(u.parent.zeroRefUser).Query(u.id)
	if err != nil {
		u.parent.logUserErr(u, err, "DeleteMailbox (zero ref user)", name)
		return wrapErrf(err, "DeleteMailbox %s", name)
	}
	defer rows.Close()

	keys := make([]string, 0, 16)
	for rows.Next() {
		var extKey string
		if err := rows.Scan(&extKey); err != nil {
			u.parent.logUserErr(u, err, "DeleteMailbox (extkeys scan)", name)
			return wrapErrf(err, "DeleteMailbox %s", name)
		}
		keys = append(keys, extKey)

	}

	if err := u.parent.extStore.Delete(keys); err != nil {
		u.parent.logUserErr(u, err, "DeleteMailbox (extstore delete)", name)
		return wrapErrf(err, "DeleteMailbox %s", name)
	}

	// TODO: Grab mboxId along the way on PostgreSQL?
	stats, err := tx.Stmt(u.parent.deleteMbox).Exec(u.id, name)
	if err != nil {
		u.parent.logUserErr(u, err, "DeleteMailbox (delete mbox)", name)
		return wrapErrf(err, "DeleteMailbox %s", name)
	}
	affected, err := stats.RowsAffected()
	if err != nil {
		u.parent.logUserErr(u, err, "DeleteMailbox (stats)", name)
		return wrapErrf(err, "DeleteMailbox %s", name)
	}
	if affected == 0 {
		return backend.ErrNoSuchMailbox
	}

	if _, err := tx.Stmt(u.parent.deleteZeroRef).Exec(u.id); err != nil {
		u.parent.logUserErr(u, err, "DeleteMailbox (delete zero ref)", name)
		return wrapErrf(err, "DeleteMailbox %s", name)
	}

	err = tx.Commit()
	u.parent.logUserErr(u, err, "DeleteMailbox (tx commit)", name)
	return err
}

func (u *User) RenameMailbox(existingName, newName string) error {
	tx, err := u.parent.db.Begin(false)
	if err != nil {
		u.parent.logUserErr(u, err, "RenameMailbox (tx start)", existingName, newName)
		return wrapErrf(err, "RenameMailbox %s, %s", existingName, newName)
	}
	defer tx.Rollback() //nolint:errcheck

	if err := u.createParentDirs(tx, newName); err != nil {
		u.parent.logUserErr(u, err, "RenameMailbox (create parents)", existingName, newName)
		return wrapErrf(err, "RenameMailbox %s, %s", existingName, newName)
	}

	if _, err := tx.Stmt(u.parent.renameMbox).Exec(newName, u.id, existingName); err != nil {
		u.parent.logUserErr(u, err, "RenameMailbox", existingName, newName)
		return wrapErrf(err, "RenameMailbox %s, %s", existingName, newName)
	}

	// TODO: Check if it possible to merge these queries.
	existingPattern := existingName + MailboxPathSep + "%"
	newPrefix := newName + MailboxPathSep
	existingPrefixLen := len(existingName + MailboxPathSep)
	if _, err := tx.Stmt(u.parent.renameMboxChilds).Exec(newPrefix, existingPrefixLen, existingPattern, u.id); err != nil {
		u.parent.logUserErr(u, err, "RenameMailbox (childs)", existingName, newName)
		return wrapErrf(err, "RenameMailbox (childs) %s, %s", existingName, newName)
	}

	if strings.EqualFold(existingName, "INBOX") {
		if _, err := tx.Stmt(u.parent.createMbox).Exec(u.id, existingName, u.parent.prng.Uint32(), nil); err != nil {
			u.parent.logUserErr(u, err, "RenameMailbox (create inbox)", existingName, newName)
			return wrapErrf(err, "RenameMailbox %s, %s", existingName, newName)
		}

		// TODO: Cut a query here by using RETURNING on PostgreSQL
		var inboxId uint64
		if err = tx.Stmt(u.parent.mboxId).QueryRow(u.id, "INBOX").Scan(&inboxId); err != nil {
			u.parent.logUserErr(u, err, "RenameMailbox (query mboxid id)", existingName, newName)
			return wrapErrf(err, "RenameMailbox %s, %s", existingName, newName)
		}
		if _, err := tx.Stmt(u.parent.setInboxId).Exec(inboxId, u.id); err != nil {
			u.parent.logUserErr(u, err, "RenameMailbox (set inbox id)", existingName, newName)
			return wrapErrf(err, "RenameMailbox %s, %s", existingName, newName)
		}
	}

	err = tx.Commit()
	u.parent.logUserErr(u, err, "RenameMailbox (tx commit)", existingName, newName)
	return wrapErrf(err, "RenameMailbox %s, %s", existingName, newName)
}

func (u *User) Logout() error {
	return nil
}

func (u *User) createParentDirs(tx *sql.Tx, name string) error {
	parts := strings.Split(name, MailboxPathSep)
	curDir := ""
	for i, part := range parts[:len(parts)-1] {
		if i != 0 {
			curDir += MailboxPathSep
		}
		curDir += part

		if _, err := tx.Stmt(u.parent.createMboxExistsOk).Exec(u.id, curDir, u.parent.prng.Uint32()); err != nil {
			return err
		}
	}
	return nil
}

func (u *User) Namespaces() (personal, other, shared []namespace.Namespace, err error) {
	return []namespace.Namespace{
		{
			Prefix:    "",
			Delimiter: MailboxPathSep,
		},
	}, nil, nil, nil
}

func (u *User) CreateMessage(mboxName string, flags []string, date time.Time, fullBody imap.Literal, _ backend.Mailbox) error {
	_, box, err := u.GetMailbox(mboxName, false, nil)
	if err != nil {
		return err
	}
	defer box.Close()

	return box.(*Mailbox).CreateMessage(flags, date, fullBody)
}

func (u *User) SetSubscribed(mboxName string, sub bool) error {
	i := 0
	if sub {
		i = 1
	}

	_, err := u.parent.setSubbed.Exec(i, u.id, mboxName)
	return err
}

func (u *User) Status(mbox string, items []imap.StatusItem) (*imap.MailboxStatus, error) {
	tx, err := u.parent.db.BeginLevel(sql.LevelReadCommitted, true)
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()

	var mboxId uint64
	if err := tx.Stmt(u.parent.mboxId).QueryRow(u.id, mbox).Scan(&mboxId); err != nil {
		if err == sql.ErrNoRows {
			return nil, backend.ErrNoSuchMailbox
		}
		return nil, err
	}

	status := imap.NewMailboxStatus(mbox, items)
	for _, item := range items {
		switch item {
		case imap.StatusMessages:
			err := tx.Stmt(u.parent.msgsCount).QueryRow(mboxId).Scan(&status.Messages)
			if err != nil {
				u.parent.logUserErr(u, err, "Status: messages scan")
				return nil, errors.New("I/O error")
			}
		case imap.StatusRecent:
			err := tx.Stmt(u.parent.recentCount).QueryRow(mboxId).Scan(&status.Recent)
			if err != nil {
				u.parent.logUserErr(u, err, "Status: recent scan")
				return nil, errors.New("I/O error")
			}
		case imap.StatusUidNext:
			err := tx.Stmt(u.parent.uidNext).QueryRow(mboxId).Scan(&status.UidNext)
			if err != nil {
				u.parent.logUserErr(u, err, "Status: uidNext scan")
				return nil, errors.New("I/O error")
			}
		case imap.StatusUidValidity:
			err := tx.Stmt(u.parent.uidValidity).QueryRow(mboxId).Scan(&status.UidValidity)
			if err != nil {
				u.parent.logUserErr(u, err, "Status: uidValidity scan")
				return nil, errors.New("I/O error")
			}
		case imap.StatusUnseen:
			err := tx.Stmt(u.parent.unseenCount).QueryRow(mboxId).Scan(&status.Unseen)
			if err != nil {
				u.parent.logUserErr(u, err, "Status: unseen scan")
				delete(status.Items, imap.StatusUnseen)
				continue
			}
		case imap.StatusAppendLimit:
			var res sql.NullInt64
			row := tx.Stmt(u.parent.mboxMsgSizeLimit).QueryRow(mboxId)
			if err := row.Scan(&res); err != nil {
				u.parent.logUserErr(u, err, "Status: appendLimit scan")
				continue
			}
			if res.Valid {
				status.AppendLimit = uint32(res.Int64)
			}
		}
	}

	return status, nil
}
