package pass_table

import (
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// TestJitFlight_CoalescesConcurrent verifies that N concurrent callers
// for the same key only run fn once, and all see the same error.
func TestJitFlight_CoalescesConcurrent(t *testing.T) {
	t.Parallel()
	var f jitFlight
	const n = 50
	var calls int32
	start := make(chan struct{})
	var wg sync.WaitGroup
	errs := make([]error, n)
	wantErr := errors.New("boom")
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			<-start
			errs[i] = f.Do("alice", func() error {
				atomic.AddInt32(&calls, 1)
				time.Sleep(20 * time.Millisecond)
				return wantErr
			})
		}(i)
	}
	close(start)
	wg.Wait()

	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("fn ran %d times, want exactly 1", got)
	}
	for i, e := range errs {
		if !errors.Is(e, wantErr) {
			t.Fatalf("caller %d got err %v, want %v", i, e, wantErr)
		}
	}
}

// TestJitFlight_DifferentKeysParallel verifies that distinct keys do not
// block each other — two Do() calls with different keys run concurrently.
func TestJitFlight_DifferentKeysParallel(t *testing.T) {
	t.Parallel()
	var f jitFlight
	bStarted := make(chan struct{})
	bDone := make(chan struct{})

	go func() {
		_ = f.Do("bob", func() error {
			close(bStarted)
			<-bDone
			return nil
		})
	}()

	<-bStarted
	// alice must not wait for bob; run it with a strict timeout.
	done := make(chan struct{})
	go func() {
		_ = f.Do("alice", func() error { return nil })
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(500 * time.Millisecond):
		close(bDone)
		t.Fatal("distinct keys serialised: alice blocked on bob")
	}
	close(bDone)
}

// TestJitFlight_EntryClearedAfterDone verifies the map is cleared after
// the function returns, so a subsequent call for the same key runs fn again.
func TestJitFlight_EntryClearedAfterDone(t *testing.T) {
	t.Parallel()
	var f jitFlight
	var calls int32
	fn := func() error {
		atomic.AddInt32(&calls, 1)
		return nil
	}
	if err := f.Do("eve", fn); err != nil {
		t.Fatal(err)
	}
	if err := f.Do("eve", fn); err != nil {
		t.Fatal(err)
	}
	if got := atomic.LoadInt32(&calls); got != 2 {
		t.Fatalf("fn ran %d times, want 2 (entry should be cleared between calls)", got)
	}
}
