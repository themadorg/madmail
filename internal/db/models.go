package db

import (
	"time"
)

// Quota represents the quotas table.
type Quota struct {
	Username     string `gorm:"primaryKey"`
	MaxStorage   int64
	CreatedAt    int64
	FirstLoginAt int64  // 1 = Registered but never logged in; >1 = Logged in
	LastLoginAt  int64
	UsedToken    string `gorm:"column:used_token;index:idx_used_token_pending,priority:1"` // The token used during /new (cleared after consumption)
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

// EndpointOverride represents a local endpoint override entry (formerly "DNS override").
// It maps a lookup key (domain name or IP address) to a target host,
// allowing outbound mail delivery to be redirected without modifying
// system DNS. For example:
//   - LookupKey="nine.testrun.org" TargetHost="1.2.3.4"  → route mail for nine.testrun.org to 1.2.3.4
//   - LookupKey="1.1.1.1"          TargetHost="2.2.2.2"  → redirect connections from 1.1.1.1 to 2.2.2.2
type EndpointOverride struct {
	LookupKey  string    `gorm:"primaryKey;column:lookup_key"` // Domain or IP to match
	TargetHost string    `gorm:"column:target_host;not null"`  // Destination host/IP to use instead
	Comment    string    `gorm:"column:comment"`               // Optional human-readable note
	CreatedAt  time.Time `gorm:"autoCreateTime"`
	UpdatedAt  time.Time `gorm:"autoUpdateTime"`
}

// TableName keeps the original table name for backward compatibility with
// existing databases that already have a "dns_overrides" table.
func (EndpointOverride) TableName() string { return "dns_overrides" }

// DNSOverride is a type alias kept for backward compatibility.
// New code should use EndpointOverride.
type DNSOverride = EndpointOverride

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

// Exchanger represents a remote pull-based email relay provider.
// Madmail will periodically poll these endpoints to download messages for itself.
type Exchanger struct {
	Name         string `gorm:"primaryKey"` // Identifier for this exchanger (e.g., onjast)
	URL          string `gorm:"not null"`   // Pull endpoint (e.g., http://onjast.com/mxdeliv)
	Enabled      bool   `gorm:"default:true"`
	PollInterval int    `gorm:"default:60"` // Polling interval in seconds
	LastPollAt   time.Time
	CreatedAt    time.Time `gorm:"autoCreateTime"`
	UpdatedAt    time.Time `gorm:"autoUpdateTime"`
}

// RegistrationToken represents a token that controls account registration.
// Tokens have a max_uses limit; consumption is deferred until first login.
type RegistrationToken struct {
	Token     string     `gorm:"primaryKey"`
	MaxUses   int        `gorm:"column:max_uses;default:1"`
	UsedCount int        `gorm:"column:used_count;default:0"` // Persisted successes (consumed on first login)
	Comment   string     `gorm:"column:comment"`
	CreatedAt time.Time  `gorm:"autoCreateTime"`
	ExpiresAt *time.Time `gorm:"column:expires_at"`
}
