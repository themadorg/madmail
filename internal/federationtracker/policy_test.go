package federationtracker

import (
	"sync"
	"testing"
)

// newTestPolicyStore creates an isolated PolicyStore for testing,
// bypassing the global singleton.
func newTestPolicyStore() *PolicyStore {
	return &PolicyStore{
		rules: make(map[string]struct{}),
		times: make(map[string]int64),
	}
}

// --- normalizeDomain ---

func TestNormalizeDomain(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"example.com", "example.com"},
		{"EXAMPLE.COM", "example.com"},
		{"Example.Com", "example.com"},
		{"  example.com  ", "example.com"},
		{"[1.2.3.4]", "1.2.3.4"},
		{"[2001:db8::1]", "2001:db8::1"},
		{"", ""},
		{"[MIXED.Case]", "mixed.case"},
	}
	for _, tt := range tests {
		got := normalizeDomain(tt.input)
		if got != tt.want {
			t.Errorf("normalizeDomain(%q) = %q, want %q", tt.input, got, tt.want)
		}
	}
}

// --- PolicyStore CRUD ---

func TestPolicyStore_AddRemoveHas(t *testing.T) {
	ps := newTestPolicyStore()

	// Initially empty
	if ps.HasRule("example.com") {
		t.Fatal("expected no rule for example.com")
	}
	if ps.Count() != 0 {
		t.Fatalf("expected count 0, got %d", ps.Count())
	}

	// Add a rule
	count, err := ps.AddRule("example.com")
	if err != nil {
		t.Fatalf("AddRule: %v", err)
	}
	if count != 1 {
		t.Fatalf("expected count 1 after add, got %d", count)
	}
	if !ps.HasRule("example.com") {
		t.Fatal("expected rule for example.com after add")
	}

	// Case-insensitive check
	if !ps.HasRule("EXAMPLE.COM") {
		t.Fatal("HasRule should be case-insensitive")
	}
	if !ps.HasRule("Example.Com") {
		t.Fatal("HasRule should be case-insensitive (mixed)")
	}

	// Add another
	count, err = ps.AddRule("other.org")
	if err != nil {
		t.Fatalf("AddRule: %v", err)
	}
	if count != 2 {
		t.Fatalf("expected count 2, got %d", count)
	}

	// Remove first
	remaining, err := ps.RemoveRule("example.com")
	if err != nil {
		t.Fatalf("RemoveRule: %v", err)
	}
	if remaining != 1 {
		t.Fatalf("expected 1 remaining, got %d", remaining)
	}
	if ps.HasRule("example.com") {
		t.Fatal("expected no rule for example.com after remove")
	}
	// Second rule still exists
	if !ps.HasRule("other.org") {
		t.Fatal("expected rule for other.org to remain")
	}
}

func TestPolicyStore_AddDuplicate_NoDBGraceful(t *testing.T) {
	ps := newTestPolicyStore()

	// Without a DB, adding the same domain twice just overwrites.
	// Count should still be 1.
	ps.AddRule("dup.com")
	count, err := ps.AddRule("dup.com")
	if err != nil {
		t.Fatalf("AddRule duplicate: %v", err)
	}
	if count != 1 {
		t.Fatalf("expected count 1 for duplicate add, got %d", count)
	}
}

func TestPolicyStore_IPLiteralNormalization(t *testing.T) {
	ps := newTestPolicyStore()

	// Add with brackets
	ps.AddRule("[1.2.3.4]")

	// Should match without brackets
	if !ps.HasRule("1.2.3.4") {
		t.Fatal("IP literal should match without brackets")
	}
	// And with brackets
	if !ps.HasRule("[1.2.3.4]") {
		t.Fatal("IP literal should match with brackets")
	}
}

func TestPolicyStore_FlushRules(t *testing.T) {
	ps := newTestPolicyStore()
	ps.AddRule("a.com")
	ps.AddRule("b.com")
	ps.AddRule("c.com")

	if ps.Count() != 3 {
		t.Fatalf("expected 3 rules, got %d", ps.Count())
	}

	err := ps.FlushRules()
	if err != nil {
		t.Fatalf("FlushRules: %v", err)
	}
	if ps.Count() != 0 {
		t.Fatalf("expected 0 rules after flush, got %d", ps.Count())
	}
	if ps.HasRule("a.com") {
		t.Fatal("expected no rules after flush")
	}
}

func TestPolicyStore_ListRules(t *testing.T) {
	ps := newTestPolicyStore()
	ps.AddRule("x.com")
	ps.AddRule("y.com")

	rules := ps.ListRules()
	if len(rules) != 2 {
		t.Fatalf("expected 2 rules in list, got %d", len(rules))
	}
	if _, ok := rules["x.com"]; !ok {
		t.Fatal("expected x.com in list")
	}
	if _, ok := rules["y.com"]; !ok {
		t.Fatal("expected y.com in list")
	}
}

func TestPolicyStore_RemoveNonExistent(t *testing.T) {
	ps := newTestPolicyStore()
	ps.AddRule("exists.com")

	remaining, err := ps.RemoveRule("nope.com")
	if err != nil {
		t.Fatalf("RemoveRule non-existent: %v", err)
	}
	// Should still have the one rule that wasn't removed
	if remaining != 1 {
		t.Fatalf("expected 1 remaining after removing non-existent, got %d", remaining)
	}
}

// --- CheckFederationPolicy (unit-level, using the global singleton) ---
// These tests reset the global singleton's rules before each sub-test.

func resetGlobalPolicy() {
	ps := GlobalPolicy()
	ps.mu.Lock()
	ps.rules = make(map[string]struct{})
	ps.times = make(map[string]int64)
	ps.mu.Unlock()
}

func TestCheckFederationPolicy_AcceptMode_NoBloclist(t *testing.T) {
	resetGlobalPolicy()

	// ACCEPT with no rules: everything is allowed
	if !CheckFederationPolicy("remote.com", "ACCEPT", "local.com", nil) {
		t.Fatal("ACCEPT mode with no rules should allow remote.com")
	}
}

func TestCheckFederationPolicy_AcceptMode_Blocked(t *testing.T) {
	resetGlobalPolicy()
	GlobalPolicy().AddRule("bad.com")

	// ACCEPT mode: bad.com is in the blocklist
	if CheckFederationPolicy("bad.com", "ACCEPT", "local.com", nil) {
		t.Fatal("ACCEPT mode should BLOCK bad.com when it's in the rules")
	}
	// Other domains still allowed
	if !CheckFederationPolicy("good.com", "ACCEPT", "local.com", nil) {
		t.Fatal("ACCEPT mode should ALLOW good.com when it's not in rules")
	}
}

func TestCheckFederationPolicy_RejectMode_NoAllowlist(t *testing.T) {
	resetGlobalPolicy()

	// REJECT with no rules: everything is blocked
	if CheckFederationPolicy("remote.com", "REJECT", "local.com", nil) {
		t.Fatal("REJECT mode with no rules should block remote.com")
	}
}

func TestCheckFederationPolicy_RejectMode_Allowed(t *testing.T) {
	resetGlobalPolicy()
	GlobalPolicy().AddRule("trusted.com")

	// REJECT mode: trusted.com is in the allowlist
	if !CheckFederationPolicy("trusted.com", "REJECT", "local.com", nil) {
		t.Fatal("REJECT mode should ALLOW trusted.com when it's in the rules")
	}
	// Other domains blocked
	if CheckFederationPolicy("untrusted.com", "REJECT", "local.com", nil) {
		t.Fatal("REJECT mode should BLOCK untrusted.com when it's not in rules")
	}
}

func TestCheckFederationPolicy_LocalDomainBypass(t *testing.T) {
	resetGlobalPolicy()

	// Local domain always allowed, even in REJECT mode with no allowlist
	if !CheckFederationPolicy("myserver.com", "REJECT", "myserver.com", nil) {
		t.Fatal("local domain should always be allowed (REJECT mode)")
	}
	if !CheckFederationPolicy("myserver.com", "ACCEPT", "myserver.com", nil) {
		t.Fatal("local domain should always be allowed (ACCEPT mode)")
	}
}

func TestCheckFederationPolicy_LocalDomainBypass_CaseInsensitive(t *testing.T) {
	resetGlobalPolicy()

	if !CheckFederationPolicy("MyServer.COM", "REJECT", "myserver.com", nil) {
		t.Fatal("local domain bypass should be case-insensitive")
	}
}

func TestCheckFederationPolicy_IPLiteralBypass(t *testing.T) {
	resetGlobalPolicy()

	// Local domain as IP literal
	if !CheckFederationPolicy("[127.0.0.1]", "REJECT", "127.0.0.1", nil) {
		t.Fatal("local IP literal should be allowed")
	}
	if !CheckFederationPolicy("127.0.0.1", "REJECT", "[127.0.0.1]", nil) {
		t.Fatal("local IP literal bracket mismatch should still match")
	}
}

func TestCheckFederationPolicy_DefaultPolicy(t *testing.T) {
	resetGlobalPolicy()

	// Empty policy string should default to ACCEPT
	if !CheckFederationPolicy("remote.com", "", "local.com", nil) {
		t.Fatal("default (empty) policy should act as ACCEPT and allow remote.com")
	}
}

func TestCheckFederationPolicy_PolicyFromSetting(t *testing.T) {
	resetGlobalPolicy()

	// Mock getSetting function that returns REJECT
	getSetting := func(key string) (string, bool, error) {
		if key == "__FEDERATION_POLICY__" {
			return "REJECT", true, nil
		}
		return "", false, nil
	}

	// With empty policy string, should read from getSetting -> REJECT
	// No rules means everything blocked
	if CheckFederationPolicy("remote.com", "", "local.com", getSetting) {
		t.Fatal("should use getSetting to resolve policy as REJECT")
	}
}

func TestCheckFederationPolicy_ExplicitPolicyOverridesSetting(t *testing.T) {
	resetGlobalPolicy()

	// Mock getSetting that returns REJECT
	getSetting := func(key string) (string, bool, error) {
		if key == "__FEDERATION_POLICY__" {
			return "REJECT", true, nil
		}
		return "", false, nil
	}

	// Explicitly passing ACCEPT should override the getSetting
	if !CheckFederationPolicy("remote.com", "ACCEPT", "local.com", getSetting) {
		t.Fatal("explicit ACCEPT should override getSetting returning REJECT")
	}
}

func TestCheckFederationPolicy_UnknownPolicyAllows(t *testing.T) {
	resetGlobalPolicy()

	// Unknown policy defaults to allow
	if !CheckFederationPolicy("remote.com", "BANANA", "local.com", nil) {
		t.Fatal("unknown policy should default to allowing")
	}
}

func TestCheckFederationPolicy_CaseInsensitiveBlocklist(t *testing.T) {
	resetGlobalPolicy()
	GlobalPolicy().AddRule("Evil.COM")

	if CheckFederationPolicy("evil.com", "ACCEPT", "local.com", nil) {
		t.Fatal("blocklist check should be case-insensitive")
	}
	if CheckFederationPolicy("EVIL.COM", "ACCEPT", "local.com", nil) {
		t.Fatal("blocklist check should be case-insensitive (uppercase)")
	}
}

// --- Concurrency ---

func TestPolicyStore_ConcurrentAccess(t *testing.T) {
	ps := newTestPolicyStore()

	var wg sync.WaitGroup
	// Concurrent writers
	for i := 0; i < 50; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			domain := "concurrent-" + string(rune('a'+n%26)) + ".com"
			ps.AddRule(domain)
		}(i)
	}
	// Concurrent readers
	for i := 0; i < 50; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			ps.HasRule("concurrent-a.com")
			ps.Count()
			ps.ListRules()
		}()
	}
	wg.Wait()

	// Should not have panicked; count should be consistent
	count := ps.Count()
	if count == 0 {
		t.Fatal("expected some rules after concurrent adds")
	}
	if count > 26 {
		t.Fatalf("expected at most 26 unique rules, got %d", count)
	}
}
