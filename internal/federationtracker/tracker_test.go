package federationtracker

import (
	"sync"
	"testing"
)

// newTestTracker creates an isolated FederationTracker for testing,
// bypassing the global singleton.
func newTestTracker() *FederationTracker {
	return &FederationTracker{
		stats: make(map[string]*ServerStat),
	}
}

func TestTracker_IncrementDecrementQueue(t *testing.T) {
	tr := newTestTracker()

	tr.IncrementQueue("example.com")
	tr.IncrementQueue("example.com")
	tr.IncrementQueue("example.com")

	stats := tr.GetAll()
	if len(stats) != 1 {
		t.Fatalf("expected 1 domain, got %d", len(stats))
	}
	if stats[0].QueuedMessages != 3 {
		t.Fatalf("expected 3 queued, got %d", stats[0].QueuedMessages)
	}

	tr.DecrementQueue("example.com")
	stats = tr.GetAll()
	if stats[0].QueuedMessages != 2 {
		t.Fatalf("expected 2 queued after decrement, got %d", stats[0].QueuedMessages)
	}
}

func TestTracker_DecrementNeverGoesNegative(t *testing.T) {
	tr := newTestTracker()

	tr.IncrementQueue("test.com")
	tr.DecrementQueue("test.com")
	tr.DecrementQueue("test.com") // should not go below 0
	tr.DecrementQueue("test.com") // still 0

	stats := tr.GetAll()
	if stats[0].QueuedMessages != 0 {
		t.Fatalf("queue should never go negative, got %d", stats[0].QueuedMessages)
	}
}

func TestTracker_RecordFailure(t *testing.T) {
	tr := newTestTracker()

	tr.RecordFailure("fail.com", "HTTP")
	tr.RecordFailure("fail.com", "HTTP")
	tr.RecordFailure("fail.com", "HTTPS")
	tr.RecordFailure("fail.com", "SMTP")
	tr.RecordFailure("fail.com", "smtp") // case-insensitive

	stats := tr.GetAll()
	if len(stats) != 1 {
		t.Fatalf("expected 1 domain, got %d", len(stats))
	}
	s := stats[0]
	if s.FailedHTTP != 2 {
		t.Errorf("expected 2 HTTP failures, got %d", s.FailedHTTP)
	}
	if s.FailedHTTPS != 1 {
		t.Errorf("expected 1 HTTPS failure, got %d", s.FailedHTTPS)
	}
	if s.FailedSMTP != 2 {
		t.Errorf("expected 2 SMTP failures, got %d", s.FailedSMTP)
	}
}

func TestTracker_RecordFailure_UnknownTransport(t *testing.T) {
	tr := newTestTracker()

	// Unknown transport should not crash, just not increment anything
	tr.RecordFailure("noop.com", "PIGEON")

	stats := tr.GetAll()
	if len(stats) != 1 {
		t.Fatalf("expected 1 domain, got %d", len(stats))
	}
	s := stats[0]
	if s.FailedHTTP != 0 || s.FailedHTTPS != 0 || s.FailedSMTP != 0 {
		t.Error("unknown transport should not increment any failure counter")
	}
}

func TestTracker_RecordSuccess(t *testing.T) {
	tr := newTestTracker()

	tr.RecordSuccess("ok.com", 150, "HTTPS")
	tr.RecordSuccess("ok.com", 250, "HTTP")
	tr.RecordSuccess("ok.com", 100, "SMTP")

	stats := tr.GetAll()
	s := stats[0]
	if s.SuccessfulDeliveries != 3 {
		t.Errorf("expected 3 successful, got %d", s.SuccessfulDeliveries)
	}
	if s.TotalLatencyMs != 500 {
		t.Errorf("expected 500ms total latency, got %d", s.TotalLatencyMs)
	}
	if s.SuccessHTTPS != 1 {
		t.Errorf("expected 1 HTTPS success, got %d", s.SuccessHTTPS)
	}
	if s.SuccessHTTP != 1 {
		t.Errorf("expected 1 HTTP success, got %d", s.SuccessHTTP)
	}
	if s.SuccessSMTP != 1 {
		t.Errorf("expected 1 SMTP success, got %d", s.SuccessSMTP)
	}
}

func TestTracker_RecordSuccess_EmptyTransport(t *testing.T) {
	tr := newTestTracker()

	// Empty transport (inbound) should increment total but no per-transport counter
	tr.RecordSuccess("inbound.com", 0, "")
	tr.RecordSuccess("inbound.com", 0, "")

	stats := tr.GetAll()
	s := stats[0]
	if s.SuccessfulDeliveries != 2 {
		t.Errorf("expected 2 successful, got %d", s.SuccessfulDeliveries)
	}
	if s.SuccessHTTP != 0 || s.SuccessHTTPS != 0 || s.SuccessSMTP != 0 {
		t.Error("empty transport should not increment per-transport counters")
	}
}

func TestTracker_MultipleDomains(t *testing.T) {
	tr := newTestTracker()

	tr.IncrementQueue("a.com")
	tr.IncrementQueue("b.com")
	tr.RecordSuccess("c.com", 100, "HTTPS")

	stats := tr.GetAll()
	if len(stats) != 3 {
		t.Fatalf("expected 3 domains, got %d", len(stats))
	}

	// Verify each domain is tracked independently
	m := make(map[string]*ServerStat)
	for i := range stats {
		m[stats[i].Domain] = &stats[i]
	}
	if m["a.com"].QueuedMessages != 1 {
		t.Error("a.com should have 1 queued")
	}
	if m["b.com"].QueuedMessages != 1 {
		t.Error("b.com should have 1 queued")
	}
	if m["c.com"].SuccessfulDeliveries != 1 {
		t.Error("c.com should have 1 successful")
	}
}

func TestTracker_DomainCaseNormalization(t *testing.T) {
	tr := newTestTracker()

	tr.IncrementQueue("Example.COM")
	tr.IncrementQueue("example.com")

	stats := tr.GetAll()
	if len(stats) != 1 {
		t.Fatalf("expected 1 domain (case-normalized), got %d", len(stats))
	}
	if stats[0].Domain != "example.com" {
		t.Errorf("expected normalized domain 'example.com', got %q", stats[0].Domain)
	}
	if stats[0].QueuedMessages != 2 {
		t.Errorf("expected 2 queued (merged by normalization), got %d", stats[0].QueuedMessages)
	}
}

func TestTracker_GetAllReturnsSnapshot(t *testing.T) {
	tr := newTestTracker()
	tr.IncrementQueue("snap.com")

	// Get snapshot
	snap := tr.GetAll()
	if snap[0].QueuedMessages != 1 {
		t.Fatal("snapshot should show 1 queued")
	}

	// Modify tracker after snapshot
	tr.IncrementQueue("snap.com")

	// Original snapshot should NOT change
	if snap[0].QueuedMessages != 1 {
		t.Fatal("snapshot should be immutable (1), got changed to", snap[0].QueuedMessages)
	}

	// New snapshot should show updated state
	snap2 := tr.GetAll()
	if snap2[0].QueuedMessages != 2 {
		t.Fatal("new snapshot should show 2 queued")
	}
}

func TestTracker_LastActive(t *testing.T) {
	tr := newTestTracker()

	tr.IncrementQueue("last.com")
	stats := tr.GetAll()
	if stats[0].LastActive == 0 {
		t.Fatal("LastActive should be set after IncrementQueue")
	}
}

func TestTracker_ConcurrentAccess(t *testing.T) {
	tr := newTestTracker()

	var wg sync.WaitGroup
	// Mix of concurrent operations
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			switch n % 5 {
			case 0:
				tr.IncrementQueue("concurrent.com")
			case 1:
				tr.DecrementQueue("concurrent.com")
			case 2:
				tr.RecordFailure("concurrent.com", "HTTPS")
			case 3:
				tr.RecordSuccess("concurrent.com", 50, "HTTP")
			case 4:
				tr.GetAll()
			}
		}(i)
	}
	wg.Wait()

	// Should not have panicked
	stats := tr.GetAll()
	if len(stats) == 0 {
		t.Fatal("expected at least 1 domain after concurrent operations")
	}
}
