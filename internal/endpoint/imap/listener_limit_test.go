package imap

import (
	"net"
	"testing"
)

func TestLimitListener(t *testing.T) {
	t.Parallel()

	base, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("failed to open test listener: %v", err)
	}
	t.Cleanup(func() {
		_ = base.Close()
	})

	if got := limitListener(base, 0); got != base {
		t.Fatal("expected same listener when max_conns <= 0")
	}

	if got := limitListener(base, 5); got == base {
		t.Fatal("expected wrapped listener when max_conns > 0")
	}
}
