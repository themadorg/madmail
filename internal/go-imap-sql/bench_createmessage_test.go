package imapsql

import (
	"fmt"
	"strings"
	"testing"
	"time"
)

// BenchmarkCreateMessage measures the cost of User.CreateMessage
// which internally calls GetMailbox(conn=nil) → readUids() on
// every invocation. As the mailbox grows, readUids loads all
// existing UIDs from the database, so we expect this to get
// progressively slower.
func BenchmarkCreateMessage(b *testing.B) {
	backend := initTestBackend().(*Backend)
	defer cleanBackend(backend)

	if err := backend.CreateUser("bench-user"); err != nil {
		b.Fatal("CreateUser:", err)
	}
	usr, err := backend.GetUser("bench-user")
	if err != nil {
		b.Fatal("GetUser:", err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		err := usr.CreateMessage(
			"INBOX",
			[]string{},
			time.Now(),
			strings.NewReader(testMsg),
			nil,
		)
		if err != nil {
			b.Fatal("CreateMessage:", err)
		}
	}
}

// TestCreateMessageScaling delivers messages in batches and
// prints the per-message time at different mailbox sizes so
// the cost of readUids becomes visible.
func TestCreateMessageScaling(t *testing.T) {
	backend := initTestBackend().(*Backend)
	defer cleanBackend(backend)

	if err := backend.CreateUser("scale-user"); err != nil {
		t.Fatal("CreateUser:", err)
	}
	usr, err := backend.GetUser("scale-user")
	if err != nil {
		t.Fatal("GetUser:", err)
	}

	// Measure batches: deliver `batchSize` messages, then
	// report the average time per message for that batch.
	batchSize := 500
	totalMessages := 5000

	for delivered := 0; delivered < totalMessages; delivered += batchSize {
		start := time.Now()
		for i := 0; i < batchSize; i++ {
			err := usr.CreateMessage(
				"INBOX",
				[]string{},
				time.Now(),
				strings.NewReader(testMsg),
				nil,
			)
			if err != nil {
				t.Fatal("CreateMessage:", err)
			}
		}
		elapsed := time.Since(start)
		avgPerMsg := elapsed / time.Duration(batchSize)
		totalInBox := delivered + batchSize
		t.Logf("messages %5d–%5d: batch %v  avg/msg %v",
			delivered+1, totalInBox, elapsed, avgPerMsg)
	}

	t.Log(fmt.Sprintf("delivered %d messages total", totalMessages))
}
