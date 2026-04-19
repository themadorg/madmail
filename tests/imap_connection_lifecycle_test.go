//go:build integration && cgo && !nosqlite3
// +build integration,cgo,!nosqlite3

package tests_test

import (
	"bufio"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/themadorg/madmail/tests"
)

func TestIMAPChatmailCapability(tt *testing.T) {
	t := tests.NewT(tt)
	t.DNS(nil)
	t.Port("imap")
	t.Config(`
		storage.imapsql test_store {
			driver sqlite3
			dsn imapsql.db
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off

			auth pass_table static {}
			storage &test_store
		}
	`)
	t.Run(1)
	defer t.Close()

	conn := t.Conn("imap")
	defer conn.Close()
	conn.ExpectPattern(`\* OK *`)
	conn.Writeln(". CAPABILITY")
	line := conn.ExpectPattern(`\* CAPABILITY *`)
	if !strings.Contains(line, "XCHATMAIL") {
		t.Fatalf("CAPABILITY missing XCHATMAIL: %q", line)
	}
	conn.ExpectPattern(". OK *")
	conn.Writeln(". LOGOUT")
	conn.ExpectPattern(`\* BYE *`)
	conn.ExpectPattern(". OK *")
}

func TestIMAPConnectionLifecycle_DebugCounters(tt *testing.T) {
	t := tests.NewT(tt)
	t.DNS(nil)
	imapPort := t.Port("imap")
	t.Config(`
		storage.imapsql test_store {
			driver sqlite3
			dsn imapsql.db
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off

			auth pass_table static {}
			storage &test_store
		}
	`)
	t.Run(1)
	defer t.Close()

	baselineG := runtime.NumGoroutine()
	baselineConns, err := establishedConnCountForPort(int(imapPort))
	if err != nil {
		t.Fatalf("failed to read baseline TCP connection count: %v", err)
	}
	t.Logf("baseline: goroutines=%d established_conns_for_imap_port=%d", baselineG, baselineConns)

	// Phase 1: open IMAP session as a normal client.
	conn := t.Conn("imap")
	defer conn.Close()
	conn.ExpectPattern(`\* OK *`)
	conn.Writeln(". NOOP")
	conn.ExpectPattern(". OK *")

	afterOpenG := runtime.NumGoroutine()
	afterOpenConns, err := establishedConnCountForPort(int(imapPort))
	if err != nil {
		t.Fatalf("failed to read connection count after open: %v", err)
	}
	t.Logf("after open: goroutines=%d (delta=%+d) established_conns=%d (delta=%+d)",
		afterOpenG, afterOpenG-baselineG, afterOpenConns, afterOpenConns-baselineConns)
	if afterOpenConns <= baselineConns {
		t.Fatalf("expected established IMAP connections to increase after opening a session: baseline=%d now=%d", baselineConns, afterOpenConns)
	}

	// Phase 2: clean logout.
	conn.Writeln(". LOGOUT")
	conn.ExpectPattern(`\* BYE *`)
	conn.ExpectPattern(`. OK *`)

	err = waitUntil(3*time.Second, 100*time.Millisecond, func() (bool, error) {
		cur, err := establishedConnCountForPort(int(imapPort))
		if err != nil {
			return false, err
		}
		return cur <= baselineConns, nil
	})
	if err != nil {
		t.Fatalf("expected connection count to return to baseline after LOGOUT: %v", err)
	}

	afterCleanCloseG := runtime.NumGoroutine()
	afterCleanCloseConns, err := establishedConnCountForPort(int(imapPort))
	if err != nil {
		t.Fatalf("failed to read connection count after clean close: %v", err)
	}
	t.Logf("after clean close: goroutines=%d (delta=%+d) established_conns=%d (delta=%+d)",
		afterCleanCloseG, afterCleanCloseG-baselineG, afterCleanCloseConns, afterCleanCloseConns-baselineConns)

	// Phase 3: open again and intentionally keep it open.
	holdConn := t.Conn("imap")
	defer holdConn.Close()
	holdConn.ExpectPattern(`\* OK *`)
	holdConn.Writeln(". NOOP")
	holdConn.ExpectPattern(". OK *")

	time.Sleep(1 * time.Second)
	afterHoldOpenG := runtime.NumGoroutine()
	afterHoldOpenConns, err := establishedConnCountForPort(int(imapPort))
	if err != nil {
		t.Fatalf("failed to read connection count while held open: %v", err)
	}
	t.Logf("held open: goroutines=%d (delta=%+d) established_conns=%d (delta=%+d)",
		afterHoldOpenG, afterHoldOpenG-baselineG, afterHoldOpenConns, afterHoldOpenConns-baselineConns)
	if afterHoldOpenConns <= baselineConns {
		t.Fatalf("expected held-open session to keep IMAP connection count above baseline: baseline=%d now=%d", baselineConns, afterHoldOpenConns)
	}

	// Optional Phase 4: simulate "no signal" / half-open by blackholing traffic.
	// Disabled by default because it requires root and iptables.
	if os.Getenv("MADDY_TEST_SILENT_DROP") == "1" {
		runSilentDropPhase(t, int(imapPort), baselineConns, &holdConn)
	} else {
		t.Log("silent-drop phase skipped (set MADDY_TEST_SILENT_DROP=1 to enable)")
	}
}

func TestIMAPConnectionAutoLogout_ClosesIdleConn(tt *testing.T) {
	t := tests.NewT(tt)
	t.DNS(nil)
	imapPort := t.Port("imap")
	t.Config(`
		storage.imapsql test_store {
			driver sqlite3
			dsn imapsql.db
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off
			auto_logout 2s

			auth pass_table static {}
			storage &test_store
		}
	`)
	t.Run(1)
	defer t.Close()

	baselineConns, err := establishedConnCountForPort(int(imapPort))
	if err != nil {
		t.Fatalf("failed to read baseline TCP connection count: %v", err)
	}

	conn := t.Conn("imap")
	defer conn.Close()
	conn.ExpectPattern(`\* OK *`)
	conn.Writeln(". NOOP")
	conn.ExpectPattern(". OK *")

	afterOpenConns, err := establishedConnCountForPort(int(imapPort))
	if err != nil {
		t.Fatalf("failed to read connection count after open: %v", err)
	}
	if afterOpenConns <= baselineConns {
		t.Fatalf("expected connection count to increase after open: baseline=%d now=%d", baselineConns, afterOpenConns)
	}

	// Stay idle; with auto_logout=2s the server should close this connection.
	err = waitUntil(8*time.Second, 200*time.Millisecond, func() (bool, error) {
		cur, err := establishedConnCountForPort(int(imapPort))
		if err != nil {
			return false, err
		}
		return cur <= baselineConns, nil
	})
	if err != nil {
		t.Fatalf("idle connection did not close by auto_logout deadline: %v", err)
	}
}

func runSilentDropPhase(t *tests.T, imapPort int, baselineConns int, holdConn *tests.Conn) {
	t.Helper()

	if os.Geteuid() != 0 {
		t.Log("silent-drop phase skipped: requires root")
		return
	}

	if _, err := exec.LookPath("iptables"); err != nil {
		t.Log("silent-drop phase skipped: iptables not found")
		return
	}

	tcpAddr, ok := holdConn.Conn.LocalAddr().(*net.TCPAddr)
	if !ok {
		t.Log("silent-drop phase skipped: local address is not TCP")
		return
	}
	localPort := tcpAddr.Port

	rules := [][]string{
		{"-I", "OUTPUT", "-p", "tcp", "-s", "127.0.0.1", "-d", "127.0.0.1", "--sport", strconv.Itoa(localPort), "--dport", strconv.Itoa(imapPort), "-j", "DROP"},
		{"-I", "INPUT", "-p", "tcp", "-s", "127.0.0.1", "-d", "127.0.0.1", "--sport", strconv.Itoa(imapPort), "--dport", strconv.Itoa(localPort), "-j", "DROP"},
	}

	for _, args := range rules {
		cmd := exec.Command("iptables", args...)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Logf("silent-drop phase skipped: failed to install rule %v: %v (%s)", args, err, strings.TrimSpace(string(out)))
			return
		}
	}
	defer func() {
		for i := len(rules) - 1; i >= 0; i-- {
			delArgs := append([]string{"-D"}, rules[i][1:]...)
			_ = exec.Command("iptables", delArgs...).Run()
		}
	}()

	t.Logf("silent-drop phase: traffic blackholed for client local port %d -> imap port %d", localPort, imapPort)
	time.Sleep(2 * time.Second)

	curConns, err := establishedConnCountForPort(imapPort)
	if err != nil {
		t.Fatalf("silent-drop phase: failed to read connection count: %v", err)
	}
	t.Logf("silent-drop phase: established_conns=%d (baseline=%d)", curConns, baselineConns)
	if curConns <= baselineConns {
		t.Fatalf("silent-drop phase expected connection to remain visible (no close signal yet): baseline=%d now=%d", baselineConns, curConns)
	}
}

func establishedConnCountForPort(port int) (int, error) {
	total := 0
	for _, p := range []string{"/proc/net/tcp", "/proc/net/tcp6"} {
		n, err := establishedConnCountForPortInProcNet(p, port)
		if err != nil {
			return 0, err
		}
		total += n
	}
	return total, nil
}

func establishedConnCountForPortInProcNet(procNetPath string, port int) (int, error) {
	f, err := os.Open(filepath.Clean(procNetPath))
	if err != nil {
		return 0, err
	}
	defer f.Close()

	count := 0
	s := bufio.NewScanner(f)
	first := true
	for s.Scan() {
		line := strings.TrimSpace(s.Text())
		if line == "" {
			continue
		}
		if first {
			first = false
			continue // header
		}

		fields := strings.Fields(line)
		if len(fields) < 4 {
			continue
		}

		localPort, err := parseProcNetHexPort(fields[1])
		if err != nil {
			continue
		}
		remotePort, err := parseProcNetHexPort(fields[2])
		if err != nil {
			continue
		}
		state := fields[3]

		// 01 = TCP_ESTABLISHED
		if state != "01" {
			continue
		}

		if localPort == port || remotePort == port {
			count++
		}
	}
	if err := s.Err(); err != nil {
		return 0, fmt.Errorf("scan %s: %w", procNetPath, err)
	}

	return count, nil
}

func parseProcNetHexPort(addrPortHex string) (int, error) {
	parts := strings.Split(addrPortHex, ":")
	if len(parts) != 2 {
		return 0, fmt.Errorf("unexpected addr:port format: %q", addrPortHex)
	}
	v, err := strconv.ParseUint(parts[1], 16, 16)
	if err != nil {
		return 0, err
	}
	return int(v), nil
}

func waitUntil(timeout, step time.Duration, fn func() (bool, error)) error {
	deadline := time.Now().Add(timeout)
	for {
		ok, err := fn()
		if err != nil {
			return err
		}
		if ok {
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("condition not met before timeout (%s)", timeout)
		}
		time.Sleep(step)
	}
}
