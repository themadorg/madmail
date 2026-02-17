/*
Maddy Mail Server - Composable all-in-one email server.
Copyright © 2019-2020 Max Mazurov <fox.cpp@disroot.org>, Maddy Mail Server contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

// Package dns_cache implements a database-backed DNS override cache.
//
// When resolving a domain or IP for outbound mail delivery, the cache is
// consulted first. If a matching DNSOverride row exists in the database,
// its TargetHost is returned without performing any real DNS lookup.
// Otherwise the system's standard DNS resolver is used.
//
// This allows operators to:
//   - Route mail destined for a domain to a specific IP (e.g., during migration).
//   - Override IP-literal destinations (e.g., a@[1.1.1.1] → deliver to 2.2.2.2).
//   - Test mail flows against staging servers without touching system DNS.
package dns_cache

import (
	"context"
	"errors"
	"net"
	"strings"

	"github.com/themadorg/madmail/framework/log"
	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// Cache wraps a GORM database to provide DNS resolution with local overrides.
type Cache struct {
	db  *gorm.DB
	log log.Logger
}

// New creates a dns_cache.Cache from the given GORM database connection.
// It automatically runs AutoMigrate for the DNSOverride table.
func New(db *gorm.DB, logger log.Logger) (*Cache, error) {
	if err := db.AutoMigrate(&mdb.DNSOverride{}); err != nil {
		return nil, err
	}
	return &Cache{db: db, log: logger}, nil
}

// Resolve looks up the target host for the given key (domain name or IP).
//
// It ONLY returns a result when there is an explicit override in the database.
// If no override exists, it returns an empty string so the caller uses the
// original hostname for connecting (which preserves proper TLS certificate
// verification and MTA-STS compatibility).
func (c *Cache) Resolve(ctx context.Context, key string) (string, error) {
	// Normalize: strip brackets from IP-literal domains like [1.1.1.1]
	cleanKey := strings.TrimPrefix(key, "[")
	cleanKey = strings.TrimSuffix(cleanKey, "]")
	// Strip ipv6: prefix if present
	if strings.HasPrefix(strings.ToLower(cleanKey), "ipv6:") {
		cleanKey = cleanKey[5:]
	}
	cleanKey = strings.TrimSuffix(cleanKey, ".")
	cleanKey = strings.ToLower(cleanKey)

	// Check local database override
	var override mdb.DNSOverride
	err := c.db.Where("lookup_key = ?", cleanKey).First(&override).Error
	if err == nil {
		c.log.DebugMsg("DNS cache hit", "key", cleanKey, "target", override.TargetHost)
		return override.TargetHost, nil
	}
	if !errors.Is(err, gorm.ErrRecordNotFound) {
		// Actual DB error
		c.log.Error("DNS cache DB error", err, "key", cleanKey)
	}

	// No override — return empty so the caller uses the original hostname
	return "", nil
}

// ResolveMX resolves the MX host for a domain. It first checks the local
// override database. If an override exists for the domain, it returns a
// single synthetic MX record pointing to the override target with
// cacheHit=true. Otherwise it performs a standard MX lookup via the OS
// resolver and returns cacheHit=false.
func (c *Cache) ResolveMX(ctx context.Context, domain string) (records []*net.MX, cacheHit bool, err error) {
	cleanDomain := strings.TrimSuffix(strings.ToLower(domain), ".")

	// Check for override
	var override mdb.DNSOverride
	dbErr := c.db.Where("lookup_key = ?", cleanDomain).First(&override).Error
	if dbErr == nil {
		c.log.DebugMsg("DNS cache MX override", "domain", cleanDomain, "target", override.TargetHost)
		return []*net.MX{{Host: override.TargetHost, Pref: 0}}, true, nil
	}
	if !errors.Is(dbErr, gorm.ErrRecordNotFound) {
		c.log.Error("DNS cache DB error during MX lookup, falling back to OS resolver", dbErr, "domain", cleanDomain)
	}

	// Standard OS MX lookup — not from cache
	records, err = net.DefaultResolver.LookupMX(ctx, domain)
	return records, false, err
}

// --- CRUD Operations for managing overrides ---

// Set creates or updates a DNS override entry.
func (c *Cache) Set(lookupKey, targetHost, comment string) error {
	lookupKey = strings.TrimSuffix(strings.ToLower(lookupKey), ".")
	override := mdb.DNSOverride{
		LookupKey:  lookupKey,
		TargetHost: targetHost,
		Comment:    comment,
	}
	return c.db.Save(&override).Error
}

// Delete removes a DNS override entry.
func (c *Cache) Delete(lookupKey string) error {
	lookupKey = strings.TrimSuffix(strings.ToLower(lookupKey), ".")
	return c.db.Where("lookup_key = ?", lookupKey).Delete(&mdb.DNSOverride{}).Error
}

// Get retrieves a single DNS override entry.
func (c *Cache) Get(lookupKey string) (*mdb.DNSOverride, error) {
	lookupKey = strings.TrimSuffix(strings.ToLower(lookupKey), ".")
	var override mdb.DNSOverride
	err := c.db.Where("lookup_key = ?", lookupKey).First(&override).Error
	if err != nil {
		return nil, err
	}
	return &override, nil
}

// List returns all DNS override entries.
func (c *Cache) List() ([]mdb.DNSOverride, error) {
	var overrides []mdb.DNSOverride
	err := c.db.Find(&overrides).Error
	return overrides, err
}
