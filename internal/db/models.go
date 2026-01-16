package db

import (
	"time"
)

// Quota represents the quotas table.
type Quota struct {
	Username   string `gorm:"primaryKey"`
	MaxStorage int64
	CreatedAt  int64
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
