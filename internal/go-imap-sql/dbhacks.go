package imapsql

import (
	"context"
	"database/sql"
	"strconv"
	"strings"
)

// db struct is a thin wrapper to solve the most annoying problems
// with cross-RDBMS compatibility.
type db struct {
	DB     *sql.DB
	driver string
	dsn    string
}

func (d db) Prepare(req string) (*sql.Stmt, error) {
	return d.DB.Prepare(d.rewriteSQL(req))
}

func (d db) Query(req string, args ...interface{}) (*sql.Rows, error) {
	return d.DB.Query(d.rewriteSQL(req), args...)
}

func (d db) QueryRow(req string, args ...interface{}) *sql.Row {
	return d.DB.QueryRow(d.rewriteSQL(req), args...)
}

func (d db) Exec(req string, args ...interface{}) (sql.Result, error) {
	return d.DB.Exec(d.rewriteSQL(req), args...)
}

func (d db) Begin(readOnly bool) (*sql.Tx, error) {
	return d.DB.BeginTx(context.TODO(), &sql.TxOptions{
		Isolation: sql.LevelRepeatableRead,
		ReadOnly:  readOnly,
	})
}

func (d db) BeginLevel(isolation sql.IsolationLevel, readOnly bool) (*sql.Tx, error) {
	return d.DB.BeginTx(context.TODO(), &sql.TxOptions{
		Isolation: isolation,
		ReadOnly:  readOnly,
	})
}

func (d db) Close() error {
	return d.DB.Close()
}

func (d db) rewriteSQL(req string) (res string) {
	res = strings.TrimSpace(req)
	res = strings.TrimLeft(res, "\n\t")
	if d.driver == "postgres" {
		res = ""
		placeholderIndx := 1
		for _, chr := range req {
			if chr == '?' {
				res += "$" + strconv.Itoa(placeholderIndx)
				placeholderIndx += 1
			} else {
				res += string(chr)
			}
		}
		res = strings.TrimLeft(res, "\n\t")
		if strings.HasPrefix(res, "CREATE TABLE") || strings.HasPrefix(res, "ALTER TABLE") {
			res = strings.Replace(res, "BLOB", "BYTEA", -1)
			res = strings.Replace(res, "LONGTEXT", "BYTEA", -1)
			res = strings.Replace(res, "AUTOINCREMENT", "", -1)
		}

	} else if d.driver == "sqlite3" || d.driver == "sqlite" {
		if strings.HasPrefix(res, "CREATE TABLE") || strings.HasPrefix(res, "ALTER TABLE") {
			res = strings.Replace(res, "BIGSERIAL", "INTEGER", -1)
		}
		if strings.HasSuffix(res, "ON CONFLICT DO NOTHING") && strings.HasPrefix(res, "INSERT") {
			res = strings.Replace(res, "ON CONFLICT DO NOTHING", "", -1)
			res = strings.Replace(res, "INSERT", "INSERT OR IGNORE", 1)
		}
		// SQLite3 got no notion of locking and always uses Serialized Isolation.
		if strings.HasPrefix(res, "SELECT") {
			res = strings.Replace(res, "FOR UPDATE", "", -1)
		}
	}

	//log.Println(res)

	return
}

func (db db) valuesSubquery(flagsCount int) string {
	sqlList := ""


	for i := 0; i < flagsCount; i++ {
		if db.driver == "postgres" {
			sqlList += "(?::text)" // query rewriter will make it into $N::text.
			// This is a workaround for CockroachDB's https://github.com/cockroachdb/cockroach/issues/41558
		} else {
			sqlList += "(?)"
		}
		if i+1 != flagsCount {
			sqlList += ","
		}
	}

	return "VALUES " + sqlList
}

func (db db) aggrValuesSet(expr, separator string) string {
	if db.driver == "sqlite3" || db.driver == "sqlite" {
		return "coalesce(group_concat(" + expr + ", '" + separator + "'), '')"
	}
	if db.driver == "postgres" {
		return "coalesce(string_agg(" + expr + ",'" + separator + "'), '')"
	}

	panic("Unsupported driver")
}
