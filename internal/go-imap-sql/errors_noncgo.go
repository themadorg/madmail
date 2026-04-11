//go:build !cgo || nosqlite3
// +build !cgo nosqlite3

package imapsql

import (
	"fmt"

	"github.com/lib/pq"
)

func isSerializationErr(err error) bool {
	if pqErr, ok := err.(*pq.Error); ok {
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
