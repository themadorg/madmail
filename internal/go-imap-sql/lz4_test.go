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

func initTestBackendLZ4() backendtests.Backend {
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

	b, err := New(driver, dsn, &FSStore{Root: storeDir}, Opts{
		CompressAlgo: "lz4",
		PRNG:         prng,
		Log:          DummyLogger{},
	})
	if err != nil {
		panic(err)
	}
	return b
}

func TestWithLZ4(t *testing.T) {
	backendtests.RunTests(t, initTestBackendLZ4, cleanBackend)
}
