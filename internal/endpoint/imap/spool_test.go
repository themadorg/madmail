package imap

import (
	"bytes"
	"errors"
	"io"
	"os"
	"testing"
)

type testLiteral struct {
	*bytes.Reader
}

func (l testLiteral) Len() int {
	return l.Reader.Len()
}

type errLiteral struct{}

func (errLiteral) Read(_ []byte) (int, error) {
	return 0, errors.New("boom")
}

func (errLiteral) Len() int {
	return 1
}

func TestSpoolLiteralToTempFile(t *testing.T) {
	t.Parallel()

	raw := []byte("Subject: a\r\n\r\nhello")
	f, written, err := spoolLiteralToTempFile(testLiteral{Reader: bytes.NewReader(raw)})
	if err != nil {
		t.Fatalf("spoolLiteralToTempFile failed: %v", err)
	}
	path := f.Name()
	defer func() {
		_ = f.Close()
		_ = os.Remove(path)
	}()

	if written != int64(len(raw)) {
		t.Fatalf("unexpected bytes written: got %d, want %d", written, len(raw))
	}

	got, err := io.ReadAll(f)
	if err != nil {
		t.Fatalf("failed to read spooled file: %v", err)
	}
	if !bytes.Equal(got, raw) {
		t.Fatalf("spooled payload mismatch: got %q, want %q", got, raw)
	}
}

func TestSpoolLiteralToTempFileReadError(t *testing.T) {
	t.Parallel()

	if _, _, err := spoolLiteralToTempFile(errLiteral{}); err == nil {
		t.Fatal("expected error, got nil")
	}
}
