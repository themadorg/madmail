/*
Package gormsqlite is an in-tree GORM dialector that uses modernc.org/sqlite
as its SQL driver.

We maintain our own small dialector rather than pulling in
`gorm.io/driver/sqlite` (which hard-requires mattn/go-sqlite3) or
`github.com/glebarez/sqlite` (which hard-requires a fork of modernc under a
different import path). The resulting build tree references
modernc.org/sqlite exclusively for its SQLite needs.

The bulk of this dialector is structural boilerplate adapted from the
glebarez/sqlite code (Apache-2.0); the only functional change is that the
driver registration (`_ "modernc.org/sqlite"`) and Translate()'s error
type come directly from modernc.org/sqlite.
*/
package gormsqlite

import (
	"context"
	"database/sql"
	"errors"
	"strconv"

	modsqlite "modernc.org/sqlite"
	sqlitelib "modernc.org/sqlite/lib"

	"gorm.io/gorm"
	"gorm.io/gorm/callbacks"
	"gorm.io/gorm/clause"
	"gorm.io/gorm/logger"
	"gorm.io/gorm/migrator"
	"gorm.io/gorm/schema"
)

// DriverName is the database/sql driver name registered by
// modernc.org/sqlite (pure-Go SQLite).
const DriverName = "sqlite"

// Dialector is a GORM dialector backed exclusively by modernc.org/sqlite.
type Dialector struct {
	DriverName string
	DSN        string
	Conn       gorm.ConnPool
}

// Open returns a Dialector that opens the database lazily from the given DSN.
func Open(dsn string) gorm.Dialector {
	return &Dialector{DSN: dsn}
}

func (Dialector) Name() string { return "sqlite" }

func (dialector Dialector) Initialize(db *gorm.DB) (err error) {
	if dialector.DriverName == "" {
		dialector.DriverName = DriverName
	}

	if dialector.Conn != nil {
		db.ConnPool = dialector.Conn
	} else {
		conn, err := sql.Open(dialector.DriverName, dialector.DSN)
		if err != nil {
			return err
		}
		db.ConnPool = conn
	}

	var version string
	if err := db.ConnPool.QueryRowContext(context.Background(), "select sqlite_version()").Scan(&version); err != nil {
		return err
	}
	// https://www.sqlite.org/releaselog/3_35_0.html — RETURNING support arrived here.
	if compareVersion(version, "3.35.0") >= 0 {
		callbacks.RegisterDefaultCallbacks(db, &callbacks.Config{
			CreateClauses:        []string{"INSERT", "VALUES", "ON CONFLICT", "RETURNING"},
			UpdateClauses:        []string{"UPDATE", "SET", "FROM", "WHERE", "RETURNING"},
			DeleteClauses:        []string{"DELETE", "FROM", "WHERE", "RETURNING"},
			LastInsertIDReversed: true,
		})
	} else {
		callbacks.RegisterDefaultCallbacks(db, &callbacks.Config{
			LastInsertIDReversed: true,
		})
	}

	for k, v := range dialector.ClauseBuilders() {
		db.ClauseBuilders[k] = v
	}
	return nil
}

func (dialector Dialector) ClauseBuilders() map[string]clause.ClauseBuilder {
	return map[string]clause.ClauseBuilder{
		"INSERT": func(c clause.Clause, builder clause.Builder) {
			if insert, ok := c.Expression.(clause.Insert); ok {
				if stmt, ok := builder.(*gorm.Statement); ok {
					stmt.WriteString("INSERT ")
					if insert.Modifier != "" {
						stmt.WriteString(insert.Modifier)
						stmt.WriteByte(' ')
					}
					stmt.WriteString("INTO ")
					if insert.Table.Name == "" {
						stmt.WriteQuoted(stmt.Table)
					} else {
						stmt.WriteQuoted(insert.Table)
					}
					return
				}
			}
			c.Build(builder)
		},
		"LIMIT": func(c clause.Clause, builder clause.Builder) {
			if lim, ok := c.Expression.(clause.Limit); ok {
				lmt := -1
				if lim.Limit != nil && *lim.Limit >= 0 {
					lmt = *lim.Limit
				}
				if lmt >= 0 || lim.Offset > 0 {
					builder.WriteString("LIMIT ")
					builder.WriteString(strconv.Itoa(lmt))
				}
				if lim.Offset > 0 {
					builder.WriteString(" OFFSET ")
					builder.WriteString(strconv.Itoa(lim.Offset))
				}
			}
		},
		"FOR": func(c clause.Clause, builder clause.Builder) {
			if _, ok := c.Expression.(clause.Locking); ok {
				// SQLite has no row-level locking; swallow FOR UPDATE/SHARE.
				return
			}
			c.Build(builder)
		},
	}
}

func (Dialector) DefaultValueOf(field *schema.Field) clause.Expression {
	if field.AutoIncrement {
		return clause.Expr{SQL: "NULL"}
	}
	return clause.Expr{SQL: "DEFAULT"}
}

func (dialector Dialector) Migrator(db *gorm.DB) gorm.Migrator {
	return Migrator{migrator.Migrator{Config: migrator.Config{
		DB:                          db,
		Dialector:                   dialector,
		CreateIndexAfterCreateTable: true,
	}}}
}

func (Dialector) BindVarTo(writer clause.Writer, _ *gorm.Statement, _ interface{}) {
	writer.WriteByte('?')
}

func (Dialector) QuoteTo(writer clause.Writer, str string) {
	var (
		underQuoted, selfQuoted bool
		continuousBacktick      int8
		shiftDelimiter          int8
	)
	for _, v := range []byte(str) {
		switch v {
		case '`':
			continuousBacktick++
			if continuousBacktick == 2 {
				writer.WriteString("``")
				continuousBacktick = 0
			}
		case '.':
			if continuousBacktick > 0 || !selfQuoted {
				shiftDelimiter = 0
				underQuoted = false
				continuousBacktick = 0
				writer.WriteString("`")
			}
			writer.WriteByte(v)
			continue
		default:
			if shiftDelimiter-continuousBacktick <= 0 && !underQuoted {
				writer.WriteString("`")
				underQuoted = true
				if selfQuoted = continuousBacktick > 0; selfQuoted {
					continuousBacktick -= 1
				}
			}
			for ; continuousBacktick > 0; continuousBacktick -= 1 {
				writer.WriteString("``")
			}
			writer.WriteByte(v)
		}
		shiftDelimiter++
	}
	if continuousBacktick > 0 && !selfQuoted {
		writer.WriteString("``")
	}
	writer.WriteString("`")
}

func (Dialector) Explain(sql string, vars ...interface{}) string {
	return logger.ExplainSQL(sql, nil, `"`, vars...)
}

func (Dialector) DataTypeOf(field *schema.Field) string {
	switch field.DataType {
	case schema.Bool:
		return "numeric"
	case schema.Int, schema.Uint:
		if field.AutoIncrement {
			// https://www.sqlite.org/autoinc.html
			return "integer PRIMARY KEY AUTOINCREMENT"
		}
		return "integer"
	case schema.Float:
		return "real"
	case schema.String:
		return "text"
	case schema.Time:
		if val, ok := field.TagSettings["TYPE"]; ok {
			return val
		}
		return "datetime"
	case schema.Bytes:
		return "blob"
	}
	return string(field.DataType)
}

func (Dialector) SavePoint(tx *gorm.DB, name string) error {
	tx.Exec("SAVEPOINT " + name)
	return nil
}

func (Dialector) RollbackTo(tx *gorm.DB, name string) error {
	tx.Exec("ROLLBACK TO SAVEPOINT " + name)
	return nil
}

// Translate maps modernc.org/sqlite constraint errors onto GORM's canonical
// error types so consumers can errors.Is(err, gorm.ErrDuplicatedKey) etc.
func (Dialector) Translate(err error) error {
	var terr *modsqlite.Error
	if errors.As(err, &terr) {
		switch terr.Code() {
		case sqlitelib.SQLITE_CONSTRAINT_UNIQUE, sqlitelib.SQLITE_CONSTRAINT_PRIMARYKEY:
			return gorm.ErrDuplicatedKey
		case sqlitelib.SQLITE_CONSTRAINT_FOREIGNKEY:
			return gorm.ErrForeignKeyViolated
		}
	}
	return err
}

func compareVersion(v1, v2 string) int {
	n, m := len(v1), len(v2)
	i, j := 0, 0
	for i < n || j < m {
		x := 0
		for ; i < n && v1[i] != '.'; i++ {
			x = x*10 + int(v1[i]-'0')
		}
		i++
		y := 0
		for ; j < m && v2[j] != '.'; j++ {
			y = y*10 + int(v2[j]-'0')
		}
		j++
		if x > y {
			return 1
		}
		if x < y {
			return -1
		}
	}
	return 0
}
