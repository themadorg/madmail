package pass_table

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"

	"github.com/themadorg/madmail/framework/config"
	"golang.org/x/crypto/bcrypt"
)

// mutableTable is a thread-safe in-memory table for benchmarks.
type mutableTable struct {
	mu sync.RWMutex
	m  map[string]string
}

func newMutableTable() *mutableTable {
	return &mutableTable{m: make(map[string]string)}
}

func (t *mutableTable) Lookup(_ context.Context, key string) (string, bool, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()
	v, ok := t.m[key]
	return v, ok, nil
}

func (t *mutableTable) Keys() ([]string, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()
	keys := make([]string, 0, len(t.m))
	for k := range t.m {
		keys = append(keys, k)
	}
	return keys, nil
}

func (t *mutableTable) SetKey(key, value string) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.m[key] = value
	return nil
}

func (t *mutableTable) RemoveKey(key string) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	delete(t.m, key)
	return nil
}

// newTestAuth creates an Auth instance with a mutable in-memory table.
func newTestAuth(tb testing.TB) (*Auth, *mutableTable) {
	tb.Helper()
	addSHA256()

	mod, err := New("pass_table", "", nil, []string{"dummy"})
	if err != nil {
		tb.Fatal(err)
	}
	if err := mod.Init(config.NewMap(nil, config.Node{})); err != nil {
		tb.Fatal(err)
	}
	a := mod.(*Auth)
	tbl := newMutableTable()
	a.table = tbl
	if err := a.hydrateCredCache(); err != nil {
		tb.Fatal(err)
	}
	return a, tbl
}

// BenchmarkAuthPlain_Bcrypt measures login verification with bcrypt.
func BenchmarkAuthPlain_Bcrypt(b *testing.B) {
	a, _ := newTestAuth(b)

	if err := a.CreateUserHash("user@example.org", "testpassword123!", HashBcrypt, HashOpts{
		BcryptCost: bcrypt.DefaultCost,
	}); err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := a.AuthPlain("user@example.org", "testpassword123!"); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkAuthPlain_SHA256 measures login verification with SHA256.
func BenchmarkAuthPlain_SHA256(b *testing.B) {
	a, _ := newTestAuth(b)

	if err := a.CreateUserHash("user@example.org", "testpassword123!", HashSHA256, HashOpts{}); err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := a.AuthPlain("user@example.org", "testpassword123!"); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkAuthPlain_Argon2 measures login verification with Argon2.
func BenchmarkAuthPlain_Argon2(b *testing.B) {
	a, _ := newTestAuth(b)

	if err := a.CreateUserHash("user@example.org", "testpassword123!", HashArgon2, HashOpts{
		Argon2Time:    1,
		Argon2Memory:  16 * 1024,
		Argon2Threads: 1,
	}); err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := a.AuthPlain("user@example.org", "testpassword123!"); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkCreateUser_Bcrypt measures account creation with bcrypt hashing.
func BenchmarkCreateUser_Bcrypt(b *testing.B) {
	a, _ := newTestAuth(b)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		user := fmt.Sprintf("user%d@example.org", i)
		if err := a.CreateUserHash(user, "testpassword123!", HashBcrypt, HashOpts{
			BcryptCost: bcrypt.DefaultCost,
		}); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkCreateUser_SHA256 measures account creation with SHA256 hashing.
func BenchmarkCreateUser_SHA256(b *testing.B) {
	a, _ := newTestAuth(b)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		user := fmt.Sprintf("user%d@example.org", i)
		if err := a.CreateUserHash(user, "testpassword123!", HashSHA256, HashOpts{}); err != nil {
			b.Fatal(err)
		}
	}
}

// TestLoginScaling compares login times across hash algorithms
// with a human-readable report.
func TestLoginScaling(t *testing.T) {
	addSHA256()

	type hashSetup struct {
		name string
		algo string
		opts HashOpts
	}

	algos := []hashSetup{
		{"bcrypt-cost10", HashBcrypt, HashOpts{BcryptCost: bcrypt.DefaultCost}},
		{"bcrypt-cost4", HashBcrypt, HashOpts{BcryptCost: bcrypt.MinCost}},
		{"sha256", HashSHA256, HashOpts{}},
		{"argon2-light", HashArgon2, HashOpts{Argon2Time: 1, Argon2Memory: 16 * 1024, Argon2Threads: 1}},
	}

	iterations := 20

	for _, algo := range algos {
		a, _ := newTestAuth(t)

		if err := a.CreateUserHash("bench@example.org", "s3cureP@ss!", algo.algo, algo.opts); err != nil {
			t.Fatalf("%s: create failed: %v", algo.name, err)
		}

		// Warmup
		_ = a.AuthPlain("bench@example.org", "s3cureP@ss!")

		start := time.Now()
		for i := 0; i < iterations; i++ {
			if err := a.AuthPlain("bench@example.org", "s3cureP@ss!"); err != nil {
				t.Fatalf("%s: auth failed: %v", algo.name, err)
			}
		}
		elapsed := time.Since(start)
		avg := elapsed / time.Duration(iterations)
		t.Logf("%-16s  %d logins in %v  avg/login %v", algo.name, iterations, elapsed, avg)
	}
}
