// Package servertracker provides an in-memory tracker for unique email servers
// seen by the SMTP endpoint. It uses salted hashing for privacy — server IPs
// and domains are never stored in cleartext. The salt is regenerated on each
// process restart, so the tracker resets naturally.
//
// The tracker writes summary counts to a JSON status file in
// RuntimeDirectory, which can be read by the CLI.
//
// It distinguishes between:
//   - Connection IPs: the remote IP of the connecting SMTP server
//   - Domain servers: senders like user@example.com (domain = example.com)
//   - IP servers: senders like user@[1.2.3.4] or user@1.2.3.4
package servertracker

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"net"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/themadorg/madmail/framework/config"
)

const statusFileName = "server_tracker.json"

// Status is the JSON structure written to the status file.
type Status struct {
	BootTime        int64 `json:"boot_time"`
	UniqueConnIPs   int   `json:"unique_conn_ips"`
	UniqueDomains   int   `json:"unique_domains"`
	UniqueIPServers int   `json:"unique_ip_servers"`
}

// Tracker holds salted hashes of unique server identifiers.
type Tracker struct {
	mu        sync.RWMutex
	salt      []byte
	bootTime  time.Time
	connIPs   map[string]struct{} // hashed connecting server IPs
	domains   map[string]struct{} // hashed domain-based senders (e.g. example.com)
	ipServers map[string]struct{} // hashed IP-based senders (e.g. [1.2.3.4])
}

var (
	global     *Tracker
	globalOnce sync.Once
)

// Global returns the singleton tracker. It is created on first call
// with a fresh random salt. Boot time is recorded at creation.
func Global() *Tracker {
	globalOnce.Do(func() {
		salt := make([]byte, 32)
		if _, err := rand.Read(salt); err != nil {
			salt = make([]byte, 32)
		}
		global = &Tracker{
			salt:      salt,
			bootTime:  time.Now(),
			connIPs:   make(map[string]struct{}),
			domains:   make(map[string]struct{}),
			ipServers: make(map[string]struct{}),
		}
		// Write initial status file (boot time + zeros)
		global.mu.Lock()
		global.writeStatusLocked()
		global.mu.Unlock()
	})
	return global
}

// hash returns a salted SHA-256 hex digest of the input.
func (t *Tracker) hash(input string) string {
	h := sha256.New()
	h.Write(t.salt)
	h.Write([]byte(input))
	return hex.EncodeToString(h.Sum(nil))
}

// isIPAddress checks if a string is an IP address or IP literal like [1.2.3.4].
func isIPAddress(s string) bool {
	clean := strings.TrimPrefix(strings.TrimSuffix(s, "]"), "[")
	return net.ParseIP(clean) != nil
}

// RecordServer records a connecting server IP and the sender's domain/IP.
// The connIP is the TCP connection's remote IP.
// The senderDomain is the domain part of the MAIL FROM address.
// Both are stored as salted hashes. Empty values are ignored.
func (t *Tracker) RecordServer(connIP, senderDomain string) {
	t.mu.Lock()
	defer t.mu.Unlock()

	if connIP != "" {
		t.connIPs[t.hash(connIP)] = struct{}{}
	}

	if senderDomain != "" {
		if isIPAddress(senderDomain) {
			t.ipServers[t.hash(senderDomain)] = struct{}{}
		} else {
			t.domains[t.hash(senderDomain)] = struct{}{}
		}
	}

	t.writeStatusLocked()
}

// GetStatus returns the current counts.
func (t *Tracker) GetStatus() Status {
	t.mu.RLock()
	defer t.mu.RUnlock()
	return Status{
		BootTime:        t.bootTime.Unix(),
		UniqueConnIPs:   len(t.connIPs),
		UniqueDomains:   len(t.domains),
		UniqueIPServers: len(t.ipServers),
	}
}

// writeStatusLocked writes the current counts to the status file.
// Must be called with t.mu held.
func (t *Tracker) writeStatusLocked() {
	dir := config.RuntimeDirectory
	if dir == "" {
		dir = "/run/maddy"
	}

	status := Status{
		BootTime:        t.bootTime.Unix(),
		UniqueConnIPs:   len(t.connIPs),
		UniqueDomains:   len(t.domains),
		UniqueIPServers: len(t.ipServers),
	}

	data, err := json.Marshal(status)
	if err != nil {
		return
	}

	path := filepath.Join(dir, statusFileName)
	_ = os.WriteFile(path, data, 0640)
}

// ReadStatusFile reads the tracker status from the status file on disk.
// This is used by the CLI to read counts from the running server.
func ReadStatusFile(runtimeDir string) (Status, error) {
	if runtimeDir == "" {
		runtimeDir = "/run/maddy"
	}

	path := filepath.Join(runtimeDir, statusFileName)
	data, err := os.ReadFile(path)
	if err != nil {
		return Status{}, err
	}

	var status Status
	if err := json.Unmarshal(data, &status); err != nil {
		return Status{}, err
	}
	return status, nil
}
