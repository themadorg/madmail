package imapsql

import (
	"errors"
	"fmt"

	"github.com/lib/pq"
	sqlite "modernc.org/sqlite"
	sqlitelib "modernc.org/sqlite/lib"
)

// isSerializationErr reports whether err comes from the database as a
// retriable conflict (SQLite BUSY/LOCKED or Postgres serialization class 40).
// Callers wrap such errors in SerializationError so higher layers can retry.
//
// SQLite detection uses modernc.org/sqlite exclusively (pure-Go driver);
// the previous mattn/go-sqlite3 detection path has been removed.
func isSerializationErr(err error) bool {
	var sqliteErr *sqlite.Error
	if errors.As(err, &sqliteErr) {
		code := sqliteErr.Code()
		if code == sqlitelib.SQLITE_BUSY || code == sqlitelib.SQLITE_LOCKED {
			return true
		}
	}

	var pqErr *pq.Error
	if errors.As(err, &pqErr) {
		return pqErr.Code.Class() == "40"
	}

	return false
}

func wrapErr(err error, desc string) error {
	if err == nil {
		return nil
	}
	return fmt.Errorf(desc+": %w", err)
}

func wrapErrf(err error, format string, args ...interface{}) error {
	if err == nil {
		return nil
	}
	if isSerializationErr(err) {
		return SerializationError{Err: err}
	}

	args = append(args, err)
	return fmt.Errorf(format+": %w", args...)
}
