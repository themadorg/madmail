package imapsql

import (
	"strconv"
	"strings"
)

func (b *Backend) addSqlite3Params(dsn string) string {
	if !strings.HasPrefix(dsn, "file:") {
		dsn = "file:" + dsn
	}
	if !strings.Contains(dsn, "?") {
		dsn += "?"
	} else {
		dsn += "&"
	}

	dsn += "_fk=ON&_auto_vacuum=FULL&"

	if !b.Opts.NoWAL {
		dsn += "_journal_mode=WAL&_sync=NORMAL&"
	}
	if b.Opts.ExclusiveLock {
		dsn += "_locking_mode=EXCLUSIVE&"
	}

	if b.Opts.BusyTimeout == 0 {
		b.Opts.BusyTimeout = 500000
	}
	if b.Opts.BusyTimeout == -1 {
		b.Opts.BusyTimeout = 0
	}
	dsn += "_busy_timeout=" + strconv.Itoa(b.Opts.BusyTimeout)

	return dsn
}

func (b *Backend) configureEngine() error {
	if b.db.driver == "sqlite3" || b.db.driver == "sqlite" {
		// For testing purposes, it is important that only one memory DB will
		// be used (otherwise each connection will get its own DB)
		if strings.Contains(b.db.dsn, ":memory:") {
			b.db.DB.SetMaxOpenConns(1)
		}

		if b.Opts.CacheSize != 0 {
			if _, err := b.db.Exec(`PRAGMA cache_size=` + strconv.Itoa(b.Opts.CacheSize)); err != nil {
				return err
			}
		}

		if b.extStore == nil {
			if _, err := b.db.Exec(`PRAGMA page_size=16384`); err != nil {
				return err
			}

			// Experimental. This increases write throughput at cost of small
			// pauses from time to time.
			if _, err := b.db.Exec(`PRAGMA wal_autocheckpoint=5000`); err != nil {
				return err
			}
		}
	}

	if b.db.driver == "mysql" {
		// Make MySQL more ANSI SQL compatible.
		_, err := b.db.Exec(`SET SESSION sql_mode = 'ansi,no_backslash_escapes'`)
		if err != nil {
			return err
		}

		// Turn on strict transaction isolation by default, it is overriden
		// by per-transaction isolation levels where necessary.
		_, err = b.db.Exec(`SET SESSION TRANSACTION ISOLATION LEVEL REPEATABLE READ`)
		if err != nil {
			return err
		}
	}

	return nil
}

func (b *Backend) initSchema() error {
	var err error
	_, err = b.db.Exec(`
		CREATE TABLE IF NOT EXISTS users (
			id BIGSERIAL NOT NULL PRIMARY KEY AUTOINCREMENT,
			username VARCHAR(255) NOT NULL UNIQUE,
			msgsizelimit INTEGER DEFAULT NULL,

            -- It does not reference mboxes, since otherwise there will
            -- be recursive foreign key constraint.
            inboxId BIGINT DEFAULT 0
		)`)
	if err != nil {
		return wrapErr(err, "create table users")
	}
	_, err = b.db.Exec(`
		CREATE TABLE IF NOT EXISTS mboxes (
			id BIGSERIAL NOT NULL PRIMARY KEY AUTOINCREMENT,
			uid INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
			name VARCHAR(255) NOT NULL,
			sub INTEGER NOT NULL DEFAULT 1,
			mark INTEGER NOT NULL DEFAULT 0,
			msgsizelimit INTEGER DEFAULT NULL,
			uidnext INTEGER NOT NULL DEFAULT 1,
			uidvalidity BIGINT NOT NULL,
            specialuse VARCHAR(255) DEFAULT NULL,

            msgsCount INTEGER NOT NULL DEFAULT 0,

			UNIQUE(uid, name)
		)`)
	if err != nil {
		return wrapErr(err, "create table mboxes")
	}
	_, err = b.db.Exec(`
		CREATE TABLE IF NOT EXISTS extKeys (
			id VARCHAR(255) PRIMARY KEY NOT NULL,

			-- REFERENCES constraint is commented out otherwise
			-- it will be impossible to delete user without
			-- doing multiple queries to delete mboxes and stuff
			-- or using deferred constraint checking (not supported by MySQL/MariaDB)
			uid BIGINT NOT NULL, -- REFERENCES users(id) ON DELETE RESTRICT
			refs INTEGER NOT NULL DEFAULT 1
		)`)
	if err != nil {
		return wrapErr(err, "create table extkeys")
	}

	_, err = b.db.Exec(`
        CREATE UNIQUE INDEX IF NOT EXISTS extKeys_uid_id
        ON extKeys(uid, id)`)
	// MySQL does not support "IF NOT EXISTS", but MariaDB does.
	if err != nil && b.db.driver == "mysql" {
		_, err = b.db.Exec(`
			CREATE INDEX extKeys_uid_id
			ON extKeys(uid, id)`)
		if err != nil && strings.HasPrefix(err.Error(), "Error 1061: Duplicate key name") {
			err = nil
		}
	}
	if err != nil {
		return wrapErr(err, "create index extKeys_uid_id")
	}

	_, err = b.db.Exec(`
		CREATE TABLE IF NOT EXISTS msgs (
			mboxId BIGINT NOT NULL REFERENCES mboxes(id) ON DELETE CASCADE,
			msgId BIGINT NOT NULL,
			date BIGINT NOT NULL,
			bodyLen INTEGER NOT NULL,
			mark INTEGER NOT NULL DEFAULT 0,

			bodyStructure LONGTEXT NOT NULL,
			cachedHeader LONGTEXT NOT NULL,
			extBodyKey VARCHAR(255) DEFAULT NULL REFERENCES extKeys(id) ON DELETE RESTRICT,

            seen INTEGER NOT NULL DEFAULT 0,

			compressAlgo VARCHAR(255),

			recent INTEGER NOT NULL DEFAULT 1,

			PRIMARY KEY(mboxId, msgId)
		)`)
	if err != nil {
		return wrapErr(err, "create table msgs")
	}
	_, err = b.db.Exec(`
		CREATE TABLE IF NOT EXISTS flags (
			mboxId BIGINT NOT NULL,
			msgId BIGINT NOT NULL,
			flag VARCHAR(255) NOT NULL,

			FOREIGN KEY (mboxId, msgId) REFERENCES msgs(mboxId, msgId) ON DELETE CASCADE,
			UNIQUE (mboxId, msgId, flag)
		)`)
	if err != nil {
		return wrapErr(err, "create table flags")
	}

	_, err = b.db.Exec(`
        CREATE INDEX IF NOT EXISTS seen_msgs
        ON msgs(mboxId, seen)`)
	// MySQL does not support "IF NOT EXISTS", but MariaDB does.
	if err != nil && b.db.driver == "mysql" {
		_, err = b.db.Exec(`
			CREATE INDEX seen_msgs
			ON msgs(mboxId, seen)`)
		if err != nil && strings.HasPrefix(err.Error(), "Error 1061: Duplicate key name") {
			err = nil
		}
	}
	if err != nil {
		return wrapErr(err, "create index seen_msgs")
	}

	return nil
}

func (b *Backend) prepareStmts() error {
	var err error

	b.userMeta, err = b.db.Prepare(`
		SELECT id, inboxId
		FROM users
		WHERE username = ?`)
	if err != nil {
		return wrapErr(err, "userMeta prep")
	}
	b.listUsers, err = b.db.Prepare(`
		SELECT id, username
		FROM users
		ORDER BY id`)
	if err != nil {
		return wrapErr(err, "listUsers prep")
	}
	b.addUser, err = b.db.Prepare(`
		INSERT INTO users(username)
		VALUES (?)`)
	if err != nil {
		return wrapErr(err, "addUser prep")
	}
	b.delUser, err = b.db.Prepare(`
		DELETE FROM users
		WHERE username = ?`)
	if err != nil {
		return wrapErr(err, "addUser prep")
	}
	b.listMboxes, err = b.db.Prepare(`
		SELECT id, name
		FROM mboxes
		WHERE uid = ?
		ORDER BY id`)
	if err != nil {
		return wrapErr(err, "listMboxes prep")
	}
	b.listSubbedMboxes, err = b.db.Prepare(`
		SELECT id, name
		FROM mboxes
		WHERE uid = ? AND sub = 1
		ORDER BY id`)
	if err != nil {
		return wrapErr(err, "listSubbedMboxes prep")
	}
	b.createMbox, err = b.db.Prepare(`
		INSERT INTO mboxes(uid, name, uidvalidity, specialuse)
		VALUES (?, ?, ?, ?)`)
	if err != nil {
		return wrapErr(err, "createMbox prep")
	}
	b.createMboxExistsOk, err = b.db.Prepare(`
		INSERT INTO mboxes(uid, name, uidvalidity)
		VALUES (?, ?, ?) ON CONFLICT DO NOTHING`)
	if err != nil {
		return wrapErr(err, "createMboxExistsOk prep")
	}
	b.deleteMbox, err = b.db.Prepare(`
		DELETE FROM mboxes
		WHERE uid = ? AND name = ?`)
	if err != nil {
		return wrapErr(err, "deleteMbox prep")
	}
	b.renameMbox, err = b.db.Prepare(`
		UPDATE mboxes SET name = ?
		WHERE uid = ? AND name = ?`)
	if err != nil {
		return wrapErr(err, "renameMbox prep")
	}
	if b.db.driver == "mysql" {
		b.renameMboxChilds, err = b.db.Prepare(`
		UPDATE mboxes SET name = concat(?, substr(name, ?+1))
		WHERE name LIKE ? AND uid = ?`)
	} else {
		b.renameMboxChilds, err = b.db.Prepare(`
		UPDATE mboxes SET name = ? || substr(name, ?+1)
		WHERE name LIKE ? AND uid = ?`)
	}
	if err != nil {
		return wrapErr(err, "renameMboxChilds prep")
	}
	b.getMboxAttrs, err = b.db.Prepare(`
		SELECT mark, specialuse FROM mboxes
		WHERE uid = ? AND name = ?`)
	if err != nil {
		return wrapErr(err, "getMboxAttrs prep")
	}
	b.setSubbed, err = b.db.Prepare(`
		UPDATE mboxes SET sub = ?
		WHERE uid = ? AND name = ?`)
	if err != nil {
		return wrapErr(err, "setSubbed prep")
	}
	b.hasChildren, err = b.db.Prepare(`
		SELECT count(*)
		FROM mboxes
		WHERE name LIKE ? AND uid = ?`)
	if err != nil {
		return wrapErr(err, "hasChildren prep")
	}
	b.uidNextLocked, err = b.db.Prepare(`
		SELECT uidnext
		FROM mboxes
		WHERE id = ?
		FOR UPDATE`)
	if err != nil {
		return wrapErr(err, "uidNext prep")
	}
	b.uidNext, err = b.db.Prepare(`
		SELECT uidnext
		FROM mboxes
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "uidNext prep")
	}
	if b.db.driver == "postgres" {
		b.increaseMsgCount, err = b.db.Prepare(`
		    UPDATE mboxes
		    SET uidnext = uidnext + ?,
                msgsCount = msgsCount + ?
		    WHERE id = ?
		    RETURNING uidnext - 1`)
	} else {
		b.increaseMsgCount, err = b.db.Prepare(`
		    UPDATE mboxes
		    SET uidnext = uidnext + ?,
                msgsCount = msgsCount + ?
		    WHERE id = ?`)
	}
	if err != nil {
		return wrapErr(err, "increaseMsgCount prep")
	}
	b.decreaseMsgCount, err = b.db.Prepare(`
		UPDATE mboxes
		SET msgsCount = msgsCount - ?
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "decreaseMsgCount prep")
	}
	b.uidValidity, err = b.db.Prepare(`
		SELECT uidvalidity
		FROM mboxes
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "uidvalidity prep")
	}
	b.msgsCount, err = b.db.Prepare(`
		SELECT msgsCount
		FROM mboxes
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "msgsCount prep")
	}
	b.recentCount, err = b.db.Prepare(`
		SELECT count(msgId)
		FROM msgs
		WHERE mboxId = ?
		AND recent = 1`)
	if err != nil {
		return wrapErr(err, "recentCount prep")
	}
	b.clearRecent, err = b.db.Prepare(`
		UPDATE msgs
		SET recent = 0
		WHERE mboxId = ?`)
	if err != nil {
		return wrapErr(err, "clearRecent prep")
	}
	b.firstUnseenUid, err = b.db.Prepare(`
        SELECT msgId
        FROM msgs
	  	WHERE mboxId = ?
		AND seen = 0
		LIMIT 1
        `)
	if err != nil {
		return wrapErr(err, "firstUnseenUid prep")
	}
	// Should we cache it?
	b.unseenCount, err = b.db.Prepare(`
        SELECT count(*)
        FROM msgs
	  	WHERE mboxId = ?
		AND seen = 0
        `)
	if err != nil {
		return wrapErr(err, "unseenCount prep")
	}
	b.deletedUids, err = b.db.Prepare(`
		SELECT msgId
		FROM flags
		WHERE mboxId = ?
		AND flag = '\Deleted'
		ORDER BY msgId`)
	if err != nil {
		return wrapErr(err, "deletedSeqnums prep")
	}
	b.expungeMbox, err = b.db.Prepare(`
		DELETE FROM msgs
		WHERE mboxId = ? AND msgId IN (
			SELECT msgId
			FROM flags
			WHERE mboxId = ?
			AND flag = '\Deleted'
		)`)
	if err != nil {
		return wrapErr(err, "expungeMbox prep")
	}
	b.mboxId, err = b.db.Prepare(`
		SELECT id FROM mboxes
		WHERE uid = ?
		AND name = ?
		ORDER BY id`)
	if err != nil {
		return wrapErr(err, "mboxId prep")
	}
	b.addMsg, err = b.db.Prepare(`
		INSERT INTO msgs(mboxId, msgId, date, bodyLen, bodyStructure, cachedHeader, extBodyKey, seen, compressAlgo, recent)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return wrapErr(err, "addMsg prep")
	}
	b.copyMsgsUid, err = b.db.Prepare(`
		INSERT INTO msgs
		SELECT ? AS mboxId, (
			SELECT uidnext - 1
			FROM mboxes
			WHERE id = ?
		) + row_number() OVER (ORDER BY msgId) + ?, date, bodyLen, 0 AS mark, bodyStructure, cachedHeader, extBodyKey, seen, compressAlgo, 0
		FROM msgs
		WHERE mboxId = ? AND msgId BETWEEN ? AND ? ORDER BY msgId`)
	if err != nil {
		return wrapErr(err, "copyMsgsUid prep")
	}
	b.copyMsgFlagsUid, err = b.db.Prepare(`
		INSERT INTO flags
		SELECT ?, new_msgId AS msgId, flag
		FROM flags
		INNER JOIN (
			SELECT (
				SELECT uidnext - 1
				FROM mboxes
				WHERE id = ?
			) + row_number() OVER (ORDER BY msgId) + ? AS new_msgId, msgId, mboxId
			FROM msgs
			WHERE mboxId = ?
			AND msgId BETWEEN ? AND ?
			ORDER BY msgId
		) map ON map.msgId = flags.msgId
		AND map.mboxId = flags.mboxId`)
	if err != nil {
		return wrapErr(err, "copyMsgFlagsUid prep")
	}

	b.massClearFlagsUid, err = b.db.Prepare(`
		DELETE FROM flags
		WHERE mboxId = ?
		AND msgId BETWEEN ? AND ?
		AND flag != '\Recent'`)
	if err != nil {
		return wrapErr(err, "massClearFlagsUid prep")
	}

	b.addRecentToLast, err = b.db.Prepare(`
		UPDATE msgs
		SET recent = 1
		WHERE mboxId = ?
		AND msgId = (SELECT msgId FROM msgs WHERE mboxId = ? ORDER BY msgId DESC LIMIT ?)
		`)
	if err != nil {
		return wrapErr(err, "addRecenttoLast prep")
	}

	b.markUid, err = b.db.Prepare(`
		UPDATE msgs
		SET mark = 1
		WHERE mboxId = ?
		AND msgId BETWEEN ? AND ?`)
	if err != nil {
		return wrapErr(err, "delMsgsUid prep")
	}
	b.markedUids, err = b.db.Prepare(`
		SELECT msgId, extBodyKey
		FROM msgs
		WHERE mboxId = ?
		AND mark = 1
		ORDER BY msgId`)
	if err != nil {
		return wrapErr(err, "markedUids prep")
	}

	b.delMarked, err = b.db.Prepare(`
		DELETE FROM msgs
		WHERE mark = 1`)
	if err != nil {
		return wrapErr(err, "delMarked prep")
	}

	b.setUserMsgSizeLimit, err = b.db.Prepare(`
		UPDATE users
		SET msgsizelimit = ?
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "setUserMsgSizeLimit prep")
	}
	b.userMsgSizeLimit, err = b.db.Prepare(`
		SELECT msgsizelimit
		FROM users
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "userMsgSizeLimit prep")
	}
	b.setMboxMsgSizeLimit, err = b.db.Prepare(`
		UPDATE mboxes
		SET msgsizelimit = ?
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "setUserMsgSizeLimit prep")
	}
	b.mboxMsgSizeLimit, err = b.db.Prepare(`
		SELECT msgsizelimit
		FROM mboxes
		WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "userMsgSizeLimit prep")
	}

	b.msgFlagsUid, err = b.db.Prepare(`
		SELECT msgs.msgId, ` + b.db.aggrValuesSet("flag", "{") + `
		FROM msgs
		LEFT JOIN flags
		ON flags.msgId = msgs.msgId AND flags.mboxId = msgs.mboxId AND msgs.mboxId = flags.mboxId
		WHERE msgs.mboxId = ? AND msgs.msgId BETWEEN ? AND ?
		GROUP BY msgs.mboxId, msgs.msgId
		ORDER BY msgs.msgId`)
	if err != nil {
		return wrapErr(err, "msgFlagsUid prep")
	}

	b.usedFlags, err = b.db.Prepare(`
		SELECT DISTINCT flag
		FROM flags
		WHERE mboxId = ?
		ORDER BY flag`)
	if err != nil {
		return wrapErr(err, "usedFlags prep")
	}
	b.listMsgUids, err = b.db.Prepare(`
        SELECT msgId
        FROM msgs
		WHERE mboxId = ?
		ORDER BY msgId`)
	if err != nil {
		return wrapErr(err, "listMsgUids prep")
	}
	b.listMsgUidsRecent, err = b.db.Prepare(`
        SELECT msgId, recent
        FROM msgs
        WHERE mboxId = ?
		ORDER BY msgId
		LIMIT 100000`)
	if err != nil {
		return wrapErr(err, "listMsgUidsRecent prep")
	}

	b.searchFetchNoSeq, err = b.db.Prepare(`
		SELECT msgs.msgId, date, bodyLen, extBodyKey, compressAlgo, ` + b.db.aggrValuesSet("flag", "{") + `
		FROM msgs
		LEFT JOIN flags
		ON flags.msgId = msgs.msgId AND msgs.mboxId = flags.mboxId
		WHERE msgs.mboxId = ?
		GROUP BY msgs.mboxId, msgs.msgId
		ORDER BY msgs.msgId`)
	if err != nil {
		return wrapErr(err, "searchFetchNoSeq prep")
	}

	b.addExtKey, err = b.db.Prepare(`
		INSERT INTO extKeys(id, uid, refs)
		VALUES (?, ?, ?)`)
	if err != nil {
		return wrapErr(err, "addExtKey prep")
	}
	b.decreaseRefForMarked, err = b.db.Prepare(`
		UPDATE extKeys
		SET refs = refs - 1
		WHERE uid = ?
		AND id IN (
			SELECT extBodyKey
			FROM msgs
			WHERE mboxId = ? AND mark = 1 AND extBodyKey IS NOT NULL
		)`)
	if err != nil {
		return wrapErr(err, "decreaseRefForMarked prep")
	}
	b.decreaseRefForDeleted, err = b.db.Prepare(`
		UPDATE extKeys
		SET refs = refs - 1
		WHERE uid = ?
		AND id IN (
			SELECT extBodyKey
			FROM msgs
			INNER JOIN flags
			ON msgs.mboxId = flags.mboxId
			AND msgs.msgId = flags.msgId
			AND flag = '\Deleted'
			WHERE msgs.mboxId = ?
		)`)
	if err != nil {
		return wrapErr(err, "decreaseRefForDeleted prep")
	}
	b.incrementRefUid, err = b.db.Prepare(`
		UPDATE extKeys
		SET refs = refs + 1
		WHERE uid = ?
		AND id IN (
			SELECT extBodyKey
			FROM msgs
			WHERE mboxId = ? AND msgId BETWEEN ? AND ?
			ORDER BY msgId DESC
		)`)
	if err != nil {
		return wrapErr(err, "incrementRefUid prep")
	}
	b.zeroRef, err = b.db.Prepare(`
		SELECT extBodyKey
		FROM msgs
		INNER JOIN extKeys
		ON msgs.extBodyKey = extKeys.id
		WHERE extBodyKey IS NOT NULL
		AND uid = ?
		AND mboxId = ?
		AND refs = 0`)
	if err != nil {
		return wrapErr(err, "zeroRef prep")
	}
	b.zeroRefUser, err = b.db.Prepare(`
		SELECT id
		FROM extKeys
		WHERE uid = ?
		AND refs = 0`)
	if err != nil {
		return wrapErr(err, "zeroRefUser prep")
	}
	b.refUser, err = b.db.Prepare(`
		SELECT id
		FROM extKeys
		WHERE uid = (SELECT id FROM users WHERE username = ?)`)
	if err != nil {
		return wrapErr(err, "refUser prep")
	}
	b.deleteZeroRef, err = b.db.Prepare(`
		DELETE FROM extKeys
		-- This is the hint to accelerate operation
		-- when we have many users.
		WHERE uid = ?
		AND refs = 0`)
	if err != nil {
		return wrapErr(err, "deleteZeroRef prep")
	}
	b.deleteUserRef, err = b.db.Prepare(`
		DELETE FROM extKeys
		-- This is the hint to accelerate operation
		-- when we have many users.
		WHERE uid = (SELECT id FROM users WHERE username = ?)`)
	if err != nil {
		return wrapErr(err, "deleteUserRef prep")
	}

	b.specialUseMbox, err = b.db.Prepare(`
		SELECT name, id
		FROM mboxes
		WHERE uid = ?
		AND specialuse = ?
		LIMIT 1`)
	if err != nil {
		return wrapErr(err, "specialUseMbox")
	}

	b.setSeenFlagUid, err = b.db.Prepare(`
		UPDATE msgs
		SET seen = ?
		WHERE mboxId = ?
		AND msgId BETWEEN ? AND ?`)
	if err != nil {
		return wrapErr(err, "setSeenFlagUid prep")
	}

	b.setInboxId, err = b.db.Prepare(`
        UPDATE users
        SET inboxId = ?
        WHERE id = ?`)
	if err != nil {
		return wrapErr(err, "setInboxId prep")
	}

	b.decreaseRefForMbox, err = b.db.Prepare(`
		UPDATE extKeys
		SET refs = refs - 1
		WHERE uid = ?
		AND id IN (
			SELECT extBodyKey
			FROM msgs
			WHERE mboxId = (SELECT id FROM mboxes WHERE name = ?)
		)`)
	if err != nil {
		return wrapErr(err, "decreaseRefForMbox prep")
	}

	b.lastUid, err = b.db.Prepare(`SELECT max(msgId) FROM msgs WHERE mboxId = ?`)
	if err != nil {
		return wrapErr(err, "lastUid prep")
	}

	b.cachedHeaderUid, err = b.db.Prepare(`
		SELECT msgId, cachedHeader, bodyLen, date
		FROM msgs
		WHERE msgs.mboxId = ? AND msgId BETWEEN ? AND ?
		ORDER BY msgId`)
	if err != nil {
		return wrapErr(err, "cachedHeaderUid prep")
	}

	return nil
}

func isForeignKeyErr(err error) bool {
	return strings.Contains(err.Error(), "UNIQUE") || strings.Contains(err.Error(), "Duplicate entry") || strings.Contains(err.Error(), "unique")
}
