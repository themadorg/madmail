package servertracker

import (
	"testing"
	"time"

	"github.com/themadorg/madmail/framework/config"
)

func TestAddBounded(t *testing.T) {
	t.Parallel()

	set := map[string]struct{}{}
	addBounded(set, "a", 2)
	addBounded(set, "b", 2)
	addBounded(set, "c", 2)
	addBounded(set, "b", 2) // duplicate should be a no-op

	if len(set) != 2 {
		t.Fatalf("unexpected set size: got %d, want 2", len(set))
	}
	if _, ok := set["a"]; !ok {
		t.Fatal("expected key a to exist")
	}
	if _, ok := set["b"]; !ok {
		t.Fatal("expected key b to exist")
	}
}

func TestRecordServerRespectsMaxEntries(t *testing.T) {
	t.Parallel()

	prevRuntimeDir := config.RuntimeDirectory
	config.RuntimeDirectory = t.TempDir()
	t.Cleanup(func() {
		config.RuntimeDirectory = prevRuntimeDir
	})

	tracker := &Tracker{
		salt:       []byte("salt"),
		bootTime:   time.Now(),
		maxEntries: 2,
		connIPs:    map[string]struct{}{},
		domains:    map[string]struct{}{},
		ipServers:  map[string]struct{}{},
	}

	tracker.RecordServer("1.1.1.1", "one.example")
	tracker.RecordServer("2.2.2.2", "two.example")
	tracker.RecordServer("3.3.3.3", "three.example")

	if got := len(tracker.connIPs); got != 2 {
		t.Fatalf("unexpected tracked conn IP count: got %d, want 2", got)
	}
	if got := len(tracker.domains); got != 2 {
		t.Fatalf("unexpected tracked domain count: got %d, want 2", got)
	}
}
