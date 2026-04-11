package imapsql

import (
	"io/ioutil"
	"log"
	"math/rand"
	"os"
	"path/filepath"
	"testing"

	backendtests "github.com/foxcpp/go-imap-backend-tests"
	_ "github.com/go-sql-driver/mysql"
	_ "github.com/lib/pq"
	_ "github.com/mattn/go-sqlite3"
)

var TestDB = os.Getenv("TEST_DB")
var TestDSN = os.Getenv("TEST_DSN")

func initTestBackend() backendtests.Backend {
	driver := TestDB
	dsn := TestDSN

	if TestDB == "" {
		driver = "sqlite3"
		dsn = ":memory:"
	}

	randSrc := rand.NewSource(0)
	prng := rand.New(randSrc)

	tempDir, err := ioutil.TempDir("", "go-imap-sql-tests-")
	if err != nil {
		panic(err)
	}

	// This is meant for DB debugging.
	if os.Getenv("PRESERVE_SQLITE3_DB") == "1" {
		log.Println("Using sqlite3 DB in temporary directory.")
		driver = "sqlite3"
		dsn = filepath.Join(tempDir, "test.db")
	}

	storeDir := filepath.Join(tempDir, "store")
	if err := os.MkdirAll(storeDir, os.ModeDir|os.ModePerm); err != nil {
		panic(err)
	}

	var log Logger
	if testing.Verbose() {
		log = globalLogger{}
	} else {
		log = DummyLogger{}
	}

	b, err := New(driver, dsn, &FSStore{Root: storeDir}, Opts{
		PRNG: prng,
		Log:  log,
	})
	if err != nil {
		panic(err)
	}
	return b
}

func cleanBackend(bi backendtests.Backend) {
	b := bi.(*Backend)
	if os.Getenv("PRESERVE_DB") != "1" && os.Getenv("PRESERVE_SQLITE3_DB") != "1" {
		// Remove things manually in the right order so we will not hit
		// foreign key constraint when dropping tables.
		if _, err := b.DB.Exec(`DELETE FROM msgs`); err != nil {
			log.Println("DELETE FROM msgs", err)
		}
		if _, err := b.DB.Exec(`DELETE FROM extKeys`); err != nil {
			log.Println("DELETE FROM extKeys", err)
		}

		if _, err := b.DB.Exec(`DROP TABLE flags`); err != nil {
			log.Println("DROP TABLE flags", err)
		}
		if _, err := b.DB.Exec(`DROP TABLE msgs`); err != nil {
			log.Println("DROP TABLE msgs", err)
		}
		if _, err := b.DB.Exec(`DROP TABLE mboxes`); err != nil {
			log.Println("DROP TABLE mboxes", err)
		}
		if _, err := b.DB.Exec(`DROP TABLE users`); err != nil {
			log.Println("DROP TABLE users", err)
		}
		if _, err := b.DB.Exec(`DROP TABLE extKeys`); err != nil {
			log.Println("DROP TABLE extKeys", err)
		}

		if err := os.RemoveAll(b.extStore.(*FSStore).Root); err != nil {
			log.Println(err)
		}
	}
	b.Close()
}

func TestWithFSStore(t *testing.T) {
	backendtests.RunTests(t, initTestBackend, cleanBackend)
}
