package federationtracker

import (
	"strings"
	"sync"
	"time"

	"gorm.io/gorm"
)

// FederationRule is a domain exception stored in the database.
// When policy is ACCEPT, rules act as a blocklist.
// When policy is REJECT, rules act as an allowlist.
type FederationRule struct {
	ID        uint   `gorm:"primaryKey"`
	Domain    string `gorm:"uniqueIndex"`
	CreatedAt int64
}

// PolicyStore manages federation policy and domain exceptions.
// It uses a memory-first architecture: a hot map[string]struct{} guarded
// by sync.RWMutex is checked on every message — no DB SELECT per delivery.
type PolicyStore struct {
	mu    sync.RWMutex
	rules map[string]struct{} // normalized domain -> exists
	times map[string]int64    // domain -> created_at timestamp
	db    *gorm.DB
}

var (
	policyStore     *PolicyStore
	policyStoreOnce sync.Once
)

// GlobalPolicy returns the singleton PolicyStore.
func GlobalPolicy() *PolicyStore {
	policyStoreOnce.Do(func() {
		policyStore = &PolicyStore{
			rules: make(map[string]struct{}),
			times: make(map[string]int64),
		}
	})
	return policyStore
}

// Init loads persisted federation rules from the database into memory.
// Must be called once during server startup.
func (p *PolicyStore) Init(db *gorm.DB) {
	if db == nil {
		return
	}
	p.db = db
	_ = db.AutoMigrate(&FederationRule{})

	var rows []FederationRule
	if err := db.Find(&rows).Error; err != nil {
		return
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, r := range rows {
		d := normalizeDomain(r.Domain)
		p.rules[d] = struct{}{}
		p.times[d] = r.CreatedAt
	}
}

// AddRule adds a domain exception. Writes to both DB and RAM synchronously.
// Returns the total count of rules after addition.
func (p *PolicyStore) AddRule(domain string) (int, error) {
	d := normalizeDomain(domain)
	now := time.Now().Unix()

	if p.db != nil {
		rule := FederationRule{Domain: d, CreatedAt: now}
		if err := p.db.Create(&rule).Error; err != nil {
			// Check if it's a unique constraint violation (already exists)
			if strings.Contains(err.Error(), "UNIQUE") || strings.Contains(err.Error(), "duplicate") {
				p.mu.RLock()
				count := len(p.rules)
				p.mu.RUnlock()
				return count, nil // Gracefully ignore duplicates
			}
			return 0, err
		}
	}

	p.mu.Lock()
	p.rules[d] = struct{}{}
	p.times[d] = now
	count := len(p.rules)
	p.mu.Unlock()

	return count, nil
}

// RemoveRule removes a domain exception from both DB and RAM.
// Returns the remaining count of rules.
func (p *PolicyStore) RemoveRule(domain string) (int, error) {
	d := normalizeDomain(domain)

	if p.db != nil {
		if err := p.db.Where("domain = ?", d).Delete(&FederationRule{}).Error; err != nil {
			return 0, err
		}
	}

	p.mu.Lock()
	delete(p.rules, d)
	delete(p.times, d)
	count := len(p.rules)
	p.mu.Unlock()

	return count, nil
}

// FlushRules removes ALL domain exceptions from both DB and RAM.
func (p *PolicyStore) FlushRules() error {
	if p.db != nil {
		if err := p.db.Session(&gorm.Session{AllowGlobalUpdate: true}).Delete(&FederationRule{}).Error; err != nil {
			return err
		}
	}

	p.mu.Lock()
	p.rules = make(map[string]struct{})
	p.times = make(map[string]int64)
	p.mu.Unlock()

	return nil
}

// HasRule checks if a domain is in the exception list (from RAM, no DB hit).
func (p *PolicyStore) HasRule(domain string) bool {
	d := normalizeDomain(domain)
	p.mu.RLock()
	_, exists := p.rules[d]
	p.mu.RUnlock()
	return exists
}

// ListRules returns all rules as domain -> created_at pairs from RAM.
func (p *PolicyStore) ListRules() map[string]int64 {
	p.mu.RLock()
	defer p.mu.RUnlock()
	result := make(map[string]int64, len(p.rules))
	for d := range p.rules {
		result[d] = p.times[d]
	}
	return result
}

// Count returns the number of active rules.
func (p *PolicyStore) Count() int {
	p.mu.RLock()
	defer p.mu.RUnlock()
	return len(p.rules)
}

// CheckFederationPolicy evaluates whether communication with the given domain
// is allowed based on the current policy and exception list.
//
// Returns true if communication should proceed, false if it should be blocked.
//
// Logic:
//   - ACCEPT policy: allow all EXCEPT domains in the rules list (blocklist)
//   - REJECT policy: deny all EXCEPT domains in the rules list (allowlist)
//
// Domain normalization: IP literals like [1.1.1.1] are stripped of brackets,
// and all comparisons are case-insensitive.
func CheckFederationPolicy(domain string, policy string, localDomain string, getSetting func(string) (string, bool, error)) bool {
	// Local delivery always bypasses federation policy
	d := normalizeDomain(domain)
	local := normalizeDomain(localDomain)
	if d == local {
		return true
	}

	// Determine effective policy
	effectivePolicy := strings.ToUpper(policy)
	if effectivePolicy == "" {
		// Try reading from DB setting
		if getSetting != nil {
			if val, ok, err := getSetting("__FEDERATION_POLICY__"); err == nil && ok {
				effectivePolicy = strings.ToUpper(val)
			}
		}
	}
	if effectivePolicy == "" {
		effectivePolicy = "ACCEPT" // Default: open federation
	}

	ps := GlobalPolicy()
	hasRule := ps.HasRule(d)

	switch effectivePolicy {
	case "ACCEPT":
		// Open federation: block only domains in the exception list
		return !hasRule
	case "REJECT":
		// Closed federation: allow only domains in the exception list
		return hasRule
	default:
		return true // Unknown policy, default to allow
	}
}

// normalizeDomain strips IP brackets and lowercases for consistent comparison.
func normalizeDomain(domain string) string {
	d := strings.TrimSpace(domain)
	d = strings.TrimPrefix(d, "[")
	d = strings.TrimSuffix(d, "]")
	return strings.ToLower(d)
}
