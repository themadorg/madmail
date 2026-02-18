package db

import (
	"time"
)

// Quota represents the quotas table.
type Quota struct {
	Username     string `gorm:"primaryKey"`
	MaxStorage   int64
	CreatedAt    int64
	FirstLoginAt int64
	LastLoginAt  int64
}

// Contact represents the contacts table for contact sharing.
type Contact struct {
	Slug      string `gorm:"primaryKey"`
	URL       string `gorm:"column:url;not null"`
	Name      string
	CreatedAt time.Time `gorm:"autoCreateTime"`
}

// TableEntry represents a generic key-value entry for sql_table module.
type TableEntry struct {
	Key   string `gorm:"primaryKey"`
	Value string `gorm:"not null"`
}

// DNSOverride represents a local DNS cache override entry.
// It maps a lookup key (domain name or IP address) to a target host,
// allowing outbound mail delivery to be redirected without modifying
// system DNS. For example:
//   - LookupKey="nine.testrun.org" TargetHost="1.2.3.4"  → route mail for nine.testrun.org to 1.2.3.4
//   - LookupKey="1.1.1.1"          TargetHost="2.2.2.2"  → redirect connections from 1.1.1.1 to 2.2.2.2
type DNSOverride struct {
	LookupKey  string    `gorm:"primaryKey;column:lookup_key"` // Domain or IP to match
	TargetHost string    `gorm:"column:target_host;not null"`  // Destination host/IP to use instead
	Comment    string    `gorm:"column:comment"`               // Optional human-readable note
	CreatedAt  time.Time `gorm:"autoCreateTime"`
	UpdatedAt  time.Time `gorm:"autoUpdateTime"`
}

// BlockedUser represents a user that has been blocked from re-registering.
// When an account is deleted via the admin API/CLI, its username is added
// here to prevent the same address from being created again (via /new or JIT).
type BlockedUser struct {
	Username  string    `gorm:"primaryKey"`
	Reason    string    `gorm:"column:reason"`
	BlockedAt time.Time `gorm:"autoCreateTime"`
}

// MessageStat stores server-wide message counters.
// Each row is identified by a stat name (e.g., "sent_messages").
type MessageStat struct {
	Name  string `gorm:"primaryKey"`
	Count int64
}
