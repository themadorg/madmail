// Vendored from github.com/glebarez/sqlite v1.11.0 (MIT) with the only
// change being package rename from `sqlite` to `gormsqlite`. We vendor this
// in-tree to avoid importing github.com/glebarez/sqlite itself, which pulls
// github.com/glebarez/go-sqlite — a fork of modernc.org/sqlite. The user
// directive is to depend strictly on modernc.org/sqlite; these migrator
// helpers are pure SQL/GORM glue with no SQLite driver dependency.
package gormsqlite

import "errors"

var (
	ErrConstraintsNotImplemented = errors.New("constraints not implemented on sqlite, consider using DisableForeignKeyConstraintWhenMigrating, more details https://github.com/go-gorm/gorm/wiki/GORM-V2-Release-Note-Draft#all-new-migrator")
)
