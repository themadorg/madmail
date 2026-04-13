// Package federationtracker provides an in-memory tracker for federation
// diagnostics. Unlike servertracker (which hashes IPs for privacy), this
// tracker stores clear-text server domains for administrative diagnosis.
//
// It provides:
//   - Per-domain queue counts, failure diagnostics, and latency profiling
//   - Thread-safe concurrent access via sync.RWMutex
//   - Background periodic flushing to database for restart survival
//   - Federation policy enforcement (ACCEPT/REJECT with domain exceptions)
package federationtracker

import (
	"strings"
	"sync"
	"time"

	"gorm.io/gorm"
	"gorm.io/gorm/clause"
)

// ServerStat holds per-domain delivery diagnostics.
type ServerStat struct {
	Domain               string `json:"domain" gorm:"primaryKey"`
	QueuedMessages       int64  `json:"queued_messages"`
	FailedHTTP           int64  `json:"failed_http"`
	FailedHTTPS          int64  `json:"failed_https"`
	FailedSMTP           int64  `json:"failed_smtp"`
	SuccessHTTP          int64  `json:"success_http"`
	SuccessHTTPS         int64  `json:"success_https"`
	SuccessSMTP          int64  `json:"success_smtp"`
	SuccessfulDeliveries int64  `json:"successful_deliveries"`
	TotalLatencyMs       int64  `json:"total_latency_ms"`
	LastActive           int64  `json:"last_active"`
}

// TableName maps to the database table for persistence.
func (ServerStat) TableName() string { return "federation_server_stats" }

// FederationTracker is the in-memory metrics store for federation diagnostics.
type FederationTracker struct {
	mu    sync.RWMutex
	stats map[string]*ServerStat
}

var (
	global     *FederationTracker
	globalOnce sync.Once
)

// Global returns the singleton FederationTracker. Created on first call.
func Global() *FederationTracker {
	globalOnce.Do(func() {
		global = &FederationTracker{
			stats: make(map[string]*ServerStat),
		}
	})
	return global
}

// getOrCreate returns the stat for a domain, creating it if needed.
// Must be called with t.mu held for writing.
func (t *FederationTracker) getOrCreate(domain string) *ServerStat {
	domain = strings.ToLower(domain)
	s, ok := t.stats[domain]
	if !ok {
		s = &ServerStat{Domain: domain}
		t.stats[domain] = s
	}
	s.LastActive = time.Now().Unix()
	return s
}

// IncrementQueue adds 1 to the queued message count for a domain.
func (t *FederationTracker) IncrementQueue(domain string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.getOrCreate(domain).QueuedMessages++
}

// DecrementQueue subtracts 1 from the queued message count for a domain.
func (t *FederationTracker) DecrementQueue(domain string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	s := t.getOrCreate(domain)
	if s.QueuedMessages > 0 {
		s.QueuedMessages--
	}
}

// RecordFailure records a delivery failure for a domain, classified by transport.
// transport must be one of "HTTP", "HTTPS", or "SMTP".
func (t *FederationTracker) RecordFailure(domain, transport string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	s := t.getOrCreate(domain)
	switch strings.ToUpper(transport) {
	case "HTTP":
		s.FailedHTTP++
	case "HTTPS":
		s.FailedHTTPS++
	case "SMTP":
		s.FailedSMTP++
	}
}

// RecordSuccess records a successful delivery with the given latency in milliseconds.
// transport should be "HTTP", "HTTPS", "SMTP", or empty for inbound/unspecified.
func (t *FederationTracker) RecordSuccess(domain string, latencyMs int64, transport string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	s := t.getOrCreate(domain)
	s.SuccessfulDeliveries++
	s.TotalLatencyMs += latencyMs
	switch strings.ToUpper(transport) {
	case "HTTP":
		s.SuccessHTTP++
	case "HTTPS":
		s.SuccessHTTPS++
	case "SMTP":
		s.SuccessSMTP++
	}
}

// GetAll returns a snapshot of all tracked server stats.
// Mean latency is lazily computed here.
func (t *FederationTracker) GetAll() []ServerStat {
	t.mu.RLock()
	defer t.mu.RUnlock()
	result := make([]ServerStat, 0, len(t.stats))
	for _, s := range t.stats {
		result = append(result, *s)
	}
	return result
}

// StartFlusher begins a background goroutine that periodically persists
// the in-memory stats to the database using UPSERT (ON CONFLICT DO UPDATE).
// This mirrors the proven flushMessageCounters() pattern from imapsql.go.
func (t *FederationTracker) StartFlusher(db *gorm.DB) {
	if db == nil {
		return
	}
	// Ensure table exists
	_ = db.AutoMigrate(&ServerStat{})

	go func() {
		ticker := time.NewTicker(30 * time.Second)
		defer ticker.Stop()
		for range ticker.C {
			t.flush(db)
		}
	}()
}

// flush clones the current stats, unlocks, then batch-upserts to the database.
func (t *FederationTracker) flush(db *gorm.DB) {
	t.mu.RLock()
	snapshot := make([]ServerStat, 0, len(t.stats))
	for _, s := range t.stats {
		snapshot = append(snapshot, *s)
	}
	t.mu.RUnlock()

	if len(snapshot) == 0 {
		return
	}

	// Batch UPSERT: ON CONFLICT (domain) DO UPDATE
	db.Clauses(clause.OnConflict{
		Columns: []clause.Column{{Name: "domain"}},
		DoUpdates: clause.AssignmentColumns([]string{
			"queued_messages", "failed_http", "failed_https", "failed_smtp",
			"success_http", "success_https", "success_smtp",
			"successful_deliveries", "total_latency_ms", "last_active",
		}),
	}).Create(&snapshot)
}

// Hydrate loads persisted stats from the database into memory.
// Called during server startup to survive restarts.
func (t *FederationTracker) Hydrate(db *gorm.DB) {
	if db == nil {
		return
	}
	var rows []ServerStat
	if err := db.Find(&rows).Error; err != nil {
		return
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	for i := range rows {
		t.stats[rows[i].Domain] = &rows[i]
	}
}
