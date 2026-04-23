package main

// Register the pure-Go modernc.org/sqlite driver as "sqlite" in database/sql.
// The previous cgo-gated mattn/go-sqlite3 import was removed because madmail
// targets CGO_ENABLED=0 builds exclusively.
import _ "modernc.org/sqlite"
