package imapsql

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"io"
)

type ExtStoreObj interface {
	Sync() error
	io.Reader
	io.Writer
	io.Closer
}

type ExternalError struct {
	// true if error was caused by an attempt to access non-existent key.
	NonExistent bool

	Key string
	Err error
}

// Unwrap implements Unwrap() for Go 1.13 'errors'.
func (err ExternalError) Unwrap() error {
	return err.Err
}

// Cause implements Cause() for pkg/errors.
func (err ExternalError) Cause() error {
	return err.Err
}

func (err ExternalError) Error() string {
	if err.NonExistent {
		return fmt.Sprintf("external: non-existent key %s", err.Key)
	}
	return fmt.Sprintf("external: %v", err.Err)
}

/*
ExternalStore is an interface used by go-imap-sql to store message bodies
outside of main database.
*/
type ExternalStore interface {
	Create(key string, objectSize int64) (ExtStoreObj, error)

	// Open returns the ExtStoreObj that reads the message body specified by
	// passed key.
	//
	// If no such message exists - ExternalError with NonExistent = true is
	// returned.
	Open(key string) (ExtStoreObj, error)

	// Delete removes a set of keys from store. Non-existent keys are ignored.
	Delete(keys []string) error

	// Link creates a filesystem hardlink from srcKey to destKey.
	// Both keys may contain path separators for per-user directories.
	Link(srcKey, destKey string) error
}

func randomKey() (string, error) {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}
