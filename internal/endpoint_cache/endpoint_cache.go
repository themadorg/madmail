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

// Package endpoint_cache implements a database-backed endpoint override cache
// with an in-memory layer for fast lookups.
//
// On initialization, all EndpointOverride rows are loaded from the database
// into a sync.RWMutex-protected map. All read operations (Resolve, ResolveMX,
// Get, List) are served from memory — no database round-trip on the hot path.
//
// Write operations (Set, Delete) update memory first, then persist to the
// database. This ensures that the running server always has instant access to
// the latest overrides without any DB latency.
//
// For IP addresses without an override, the IP itself is returned (an IP is
// already a concrete endpoint — no resolution needed).
//
// For domain names without an override, an empty string is returned so that
// the caller falls through to OS DNS resolution.
package endpoint_cache

import (
	"context"
	"errors"
	"net"
	"strings"
	"sync"

	"github.com/themadorg/madmail/framework/log"
	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// Cache wraps a GORM database to provide endpoint resolution with local overrides.
// All reads are served from the in-memory map; writes go to memory first, then DB.
type Cache struct {
	db  *gorm.DB
	log log.Logger

	mu    sync.RWMutex
	store map[string]mdb.EndpointOverride // key = normalized lookup_key
}

// New creates an endpoint_cache.Cache from the given GORM database connection.
// It automatically runs AutoMigrate for the EndpointOverride table, then loads
// all existing overrides into memory.
func New(db *gorm.DB, logger log.Logger) (*Cache, error) {
	if err := db.AutoMigrate(&mdb.EndpointOverride{}); err != nil {
		return nil, err
	}

	c := &Cache{
		db:    db,
		log:   logger,
		store: make(map[string]mdb.EndpointOverride),
	}

	// Hydrate the in-memory store from the database at boot time.
	var overrides []mdb.EndpointOverride
	if err := db.Find(&overrides).Error; err != nil {
		return nil, err
	}
	for _, o := range overrides {
		c.store[o.LookupKey] = o
	}
	logger.DebugMsg("endpoint cache hydrated from DB", "entries", len(overrides))

	return c, nil
}

// normalizeKey strips brackets, ipv6: prefix, trailing dots, and lower-cases.
func normalizeKey(key string) string {
	k := strings.TrimPrefix(key, "[")
	k = strings.TrimSuffix(k, "]")
	if strings.HasPrefix(strings.ToLower(k), "ipv6:") {
		k = k[5:]
	}
	k = strings.TrimSuffix(k, ".")
	k = strings.ToLower(k)
	return k
}

// Resolve looks up the target host for the given key (domain name or IP).
//
// Behaviour:
//   - If an explicit override exists in memory, its TargetHost is returned.
//   - If key is an IP address (bare or bracketed) with NO override, the IP
//     itself is returned — an IP is already a concrete endpoint.
//   - If key is a domain name with NO override, an empty string is returned
//     so the caller uses the original hostname for DNS resolution (which
//     preserves proper TLS certificate verification and MTA-STS compatibility).
func (c *Cache) Resolve(ctx context.Context, key string) (string, error) {
	cleanKey := normalizeKey(key)

	// Check in-memory store
	c.mu.RLock()
	override, ok := c.store[cleanKey]
	c.mu.RUnlock()

	if ok {
		c.log.DebugMsg("endpoint cache hit", "key", cleanKey, "target", override.TargetHost)
		return override.TargetHost, nil
	}

	// No override — if it's an IP, return the IP itself (no DNS needed).
	if ip := net.ParseIP(cleanKey); ip != nil {
		return cleanKey, nil
	}

	// Domain without override — return empty so caller does normal DNS.
	return "", nil
}

// ResolveMX resolves the MX host for a domain. It first checks the in-memory
// override store. If an override exists for the domain, it returns a
// single synthetic MX record pointing to the override target with
// cacheHit=true. Otherwise it performs a standard MX lookup via the OS
// resolver and returns cacheHit=false.
func (c *Cache) ResolveMX(ctx context.Context, domain string) (records []*net.MX, cacheHit bool, err error) {
	cleanDomain := normalizeKey(domain)

	// Check in-memory store
	c.mu.RLock()
	override, ok := c.store[cleanDomain]
	c.mu.RUnlock()

	if ok {
		c.log.DebugMsg("endpoint cache MX override", "domain", cleanDomain, "target", override.TargetHost)
		return []*net.MX{{Host: override.TargetHost, Pref: 0}}, true, nil
	}

	// Standard OS MX lookup — not from cache
	records, err = net.DefaultResolver.LookupMX(ctx, domain)
	return records, false, err
}

// --- CRUD Operations for managing overrides ---

// Set creates or updates an endpoint override entry.
// Memory is updated first, then persisted to the database.
func (c *Cache) Set(lookupKey, targetHost, comment string) error {
	lookupKey = strings.TrimSuffix(strings.ToLower(lookupKey), ".")
	override := mdb.EndpointOverride{
		LookupKey:  lookupKey,
		TargetHost: targetHost,
		Comment:    comment,
	}

	// Update memory first
	c.mu.Lock()
	c.store[lookupKey] = override
	c.mu.Unlock()

	// Persist to database
	if err := c.db.Save(&override).Error; err != nil {
		// Rollback memory on DB failure
		c.mu.Lock()
		delete(c.store, lookupKey)
		c.mu.Unlock()
		return err
	}

	return nil
}

// Delete removes an endpoint override entry.
// Memory is updated first, then persisted to the database.
func (c *Cache) Delete(lookupKey string) error {
	lookupKey = strings.TrimSuffix(strings.ToLower(lookupKey), ".")

	// Save old value for rollback
	c.mu.Lock()
	old, existed := c.store[lookupKey]
	delete(c.store, lookupKey)
	c.mu.Unlock()

	// Persist to database
	if err := c.db.Where("lookup_key = ?", lookupKey).Delete(&mdb.EndpointOverride{}).Error; err != nil {
		// Rollback memory on DB failure
		if existed {
			c.mu.Lock()
			c.store[lookupKey] = old
			c.mu.Unlock()
		}
		return err
	}

	return nil
}

// Get retrieves a single endpoint override entry from memory.
func (c *Cache) Get(lookupKey string) (*mdb.EndpointOverride, error) {
	lookupKey = strings.TrimSuffix(strings.ToLower(lookupKey), ".")

	c.mu.RLock()
	override, ok := c.store[lookupKey]
	c.mu.RUnlock()

	if !ok {
		return nil, errors.New("endpoint override not found: " + lookupKey)
	}

	return &override, nil
}

// List returns all endpoint override entries from memory.
func (c *Cache) List() ([]mdb.EndpointOverride, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	overrides := make([]mdb.EndpointOverride, 0, len(c.store))
	for _, o := range c.store {
		overrides = append(overrides, o)
	}
	return overrides, nil
}
