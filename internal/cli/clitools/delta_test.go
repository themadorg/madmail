package clitools

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// TestBspatchRoundtrip is the core integration test:
// build two small binaries, generate a bsdiff patch, apply it with our
// bspatch implementation, and verify the SHA-256 of the result matches
// the "new" binary exactly.
func TestBspatchRoundtrip(t *testing.T) {
	if _, err := exec.LookPath("bsdiff"); err != nil {
		t.Skip("bsdiff not found in PATH – skipping roundtrip test")
	}
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go not found in PATH – skipping roundtrip test")
	}

	dir := t.TempDir()

	// --- Build "version A" binary ---
	srcA := filepath.Join(dir, "main_a.go")
	binA := filepath.Join(dir, "bin_a")
	writeGoSrc(t, srcA, "version A – initial release")
	buildBin(t, srcA, binA)

	// --- Build "version B" binary (a changed string is enough to alter bytes) ---
	srcB := filepath.Join(dir, "main_b.go")
	binB := filepath.Join(dir, "bin_b")
	writeGoSrc(t, srcB, "version B – feature release with some extra text to widen the diff")
	buildBin(t, srcB, binB)

	hashB := sha256File(t, binB)
	t.Logf("SHA-256 of bin_b (expected): %s", hashB)

	// --- Generate patch A→B with system bsdiff ---
	patchFile := filepath.Join(dir, "a_to_b.patch")
	out, err := exec.Command("bsdiff", binA, binB, patchFile).CombinedOutput()
	if err != nil {
		t.Fatalf("bsdiff failed: %v\n%s", err, out)
	}
	patchStat, _ := os.Stat(patchFile)
	binAStat, _ := os.Stat(binA)
	t.Logf("bin_a size: %d bytes", binAStat.Size())
	t.Logf("patch size: %d bytes (%.1f%% of original)", patchStat.Size(),
		float64(patchStat.Size())*100/float64(binAStat.Size()))

	// --- Apply patch with our Go implementation ---
	resultFile := filepath.Join(dir, "bin_result")
	if err := ApplyDeltaPatch(binA, patchFile, resultFile); err != nil {
		t.Fatalf("ApplyDeltaPatch failed: %v", err)
	}

	hashResult := sha256File(t, resultFile)
	t.Logf("SHA-256 of result:           %s", hashResult)

	if hashResult != hashB {
		t.Errorf("HASH MISMATCH\n  want: %s\n  got:  %s", hashB, hashResult)
	} else {
		t.Logf("✅ SHA-256 match: patched binary == bin_b")
	}
}

// TestBspatchIdentity verifies that patching a file with a patch of itself
// produces the exact same bytes (A→A patch should be trivially applicable).
func TestBspatchIdentity(t *testing.T) {
	if _, err := exec.LookPath("bsdiff"); err != nil {
		t.Skip("bsdiff not found in PATH – skipping identity test")
	}

	dir := t.TempDir()
	src := filepath.Join(dir, "main.go")
	bin := filepath.Join(dir, "bin")
	writeGoSrc(t, src, "identity test binary")
	buildBin(t, src, bin)

	patch := filepath.Join(dir, "identity.patch")
	out, err := exec.Command("bsdiff", bin, bin, patch).CombinedOutput()
	if err != nil {
		t.Fatalf("bsdiff (identity) failed: %v\n%s", err, out)
	}

	result := filepath.Join(dir, "result")
	if err := ApplyDeltaPatch(bin, patch, result); err != nil {
		t.Fatalf("ApplyDeltaPatch (identity) failed: %v", err)
	}

	if sha256File(t, bin) != sha256File(t, result) {
		t.Error("identity patch produced different bytes")
	} else {
		t.Log("✅ Identity patch: result == original")
	}
}

// TestBspatchInvalidMagic checks that a garbage patch is rejected immediately.
func TestBspatchInvalidMagic(t *testing.T) {
	dir := t.TempDir()

	old := filepath.Join(dir, "old")
	patch := filepath.Join(dir, "bad.patch")
	out := filepath.Join(dir, "out")

	if err := os.WriteFile(old, []byte("hello world"), 0600); err != nil {
		t.Fatal(err)
	}
	// A 64-byte patch with wrong magic
	garbage := make([]byte, 64)
	copy(garbage, "GARBAGE!")
	if err := os.WriteFile(patch, garbage, 0600); err != nil {
		t.Fatal(err)
	}

	err := ApplyDeltaPatch(old, patch, out)
	if err == nil {
		t.Error("expected error for invalid magic, got nil")
	} else {
		t.Logf("✅ Invalid magic correctly rejected: %v", err)
	}
}

// TestBspatchTooShort checks that a patch shorter than the 32-byte header is rejected.
func TestBspatchTooShort(t *testing.T) {
	dir := t.TempDir()

	old := filepath.Join(dir, "old")
	patch := filepath.Join(dir, "short.patch")
	out := filepath.Join(dir, "out")

	if err := os.WriteFile(old, []byte("hello"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(patch, []byte("BSDIFF40"), 0600); err != nil {
		t.Fatal(err)
	}

	err := ApplyDeltaPatch(old, patch, out)
	if err == nil {
		t.Error("expected error for short patch, got nil")
	} else {
		t.Logf("✅ Short patch correctly rejected: %v", err)
	}
}

// TestBspatchMissingFiles checks that missing input files return errors.
func TestBspatchMissingFiles(t *testing.T) {
	dir := t.TempDir()

	t.Run("missing_old", func(t *testing.T) {
		err := ApplyDeltaPatch(
			filepath.Join(dir, "nonexistent_old"),
			filepath.Join(dir, "nonexistent_patch"),
			filepath.Join(dir, "out"),
		)
		if err == nil {
			t.Error("expected error for missing old file")
		} else {
			t.Logf("✅ Missing old file correctly rejected: %v", err)
		}
	})

	t.Run("missing_patch", func(t *testing.T) {
		old := filepath.Join(dir, "old")
		_ = os.WriteFile(old, []byte("data"), 0600)
		err := ApplyDeltaPatch(
			old,
			filepath.Join(dir, "nonexistent_patch"),
			filepath.Join(dir, "out"),
		)
		if err == nil {
			t.Error("expected error for missing patch file")
		} else {
			t.Logf("✅ Missing patch file correctly rejected: %v", err)
		}
	})
}

// TestBuildDeltaURL tests the URL rewriting helper in upgrade.go (not in this
// package, so we duplicate the logic here for isolated testing).
func TestBuildDeltaURLLogic(t *testing.T) {
	cases := []struct {
		full    string
		version string
		want    string
	}{
		{"http://server/madmail", "0.24.0", "http://server/madmail-delta?from=0.24.0"},
		{"https://server/madmail?foo=bar", "1.0.0", "https://server/madmail-delta?from=1.0.0"},
		{"http://server/madmail#frag", "0.1.0", "http://server/madmail-delta?from=0.1.0"},
	}
	for _, c := range cases {
		got := buildDeltaURL(c.full, c.version)
		if got != c.want {
			t.Errorf("buildDeltaURL(%q, %q) = %q, want %q", c.full, c.version, got, c.want)
		} else {
			t.Logf("✅ %q → %q", c.full, got)
		}
	}
}

// --- helpers ---

func writeGoSrc(t *testing.T, path, msg string) {
	t.Helper()
	src := fmt.Sprintf(`package main
import "fmt"
func main() { fmt.Println(%q) }
`, msg)
	if err := os.WriteFile(path, []byte(src), 0600); err != nil {
		t.Fatal(err)
	}
}

func buildBin(t *testing.T, src, out string) {
	t.Helper()
	cmd := exec.Command("go", "build", "-o", out, src)
	cmd.Env = append(os.Environ(), "CGO_ENABLED=0")
	if b, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("go build failed: %v\n%s", err, b)
	}
}

func sha256File(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("sha256File: %v", err)
	}
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

// buildDeltaURL is a local copy of the logic in upgrade.go so we can unit-test
// it from within this package without import cycles.
func buildDeltaURL(fullURL, fromVersion string) string {
	base := fullURL
	if idx := indexAny(base, "?#"); idx != -1 {
		base = base[:idx]
	}
	return base + "-delta?from=" + fromVersion
}

func indexAny(s, chars string) int {
	for i := 0; i < len(s); i++ {
		for j := 0; j < len(chars); j++ {
			if s[i] == chars[j] {
				return i
			}
		}
	}
	return -1
}
