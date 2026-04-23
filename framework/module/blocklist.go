package module

import "sync"

// BlocklistChecker is a global registry where a storage module can register
// its "is this username banned?" lookup. Auth modules (like pass_table) use
// it before JIT-creating accounts so a banned address cannot reappear via
// SMTP AUTH — mirroring the same check already done in imapsql.GetUser.
//
// The provider is set once per process from the storage module's Init().
// Lookups before registration return (false, nil) — fail-open on intent:
// if no storage has told us anything, assume no blocklist applies. The
// primary auth path (lookupCred -> credCache) already rejects unknown
// users; this hook only gates the "create on first login" path.
var (
	blocklistCheckerMu sync.RWMutex
	blocklistChecker   func(username string) (bool, error)
)

// RegisterBlocklistChecker stores the lookup function to be used by
// IsUsernameBlocked. Intended to be called from a storage module's Init(),
// e.g. storage.imapsql registers its own IsBlocked method.
func RegisterBlocklistChecker(fn func(username string) (bool, error)) {
	blocklistCheckerMu.Lock()
	defer blocklistCheckerMu.Unlock()
	blocklistChecker = fn
}

// IsUsernameBlocked returns true if the registered blocklist provider says
// the username is banned. Returns (false, nil) when no provider is set
// (unit tests, degraded configs) so callers can treat this as a soft gate.
func IsUsernameBlocked(username string) (bool, error) {
	blocklistCheckerMu.RLock()
	fn := blocklistChecker
	blocklistCheckerMu.RUnlock()
	if fn == nil {
		return false, nil
	}
	return fn(username)
}
