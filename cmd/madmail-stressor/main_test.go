package main

import (
	"bufio"
	"context"
	"net"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestParseRamp(t *testing.T) {
	vals, err := parseRamp("1, 2,4,8")
	if err != nil {
		t.Fatalf("parseRamp: %v", err)
	}
	expected := []int{1, 2, 4, 8}
	if len(vals) != len(expected) {
		t.Fatalf("unexpected length: got %d want %d", len(vals), len(expected))
	}
	for i := range vals {
		if vals[i] != expected[i] {
			t.Fatalf("unexpected value[%d]: got %d want %d", i, vals[i], expected[i])
		}
	}

	if _, err := parseRamp("1,0,2"); err == nil {
		t.Fatal("expected parseRamp to fail for zero value")
	}
}

func TestReadSMTPResponseMultiLine(t *testing.T) {
	r := bufio.NewReader(strings.NewReader("250-first\r\n250 second\r\n"))
	code, msg, err := readSMTPResponse(r)
	if err != nil {
		t.Fatalf("readSMTPResponse: %v", err)
	}
	if code != 250 {
		t.Fatalf("unexpected code: got %d want 250", code)
	}
	if msg != "first\nsecond" {
		t.Fatalf("unexpected message: got %q", msg)
	}
}

func TestRunStageWithFixedMessagesPerWorker(t *testing.T) {
	addr, stop := startFakeSMTPServer(t)
	defer stop()

	cfg := Config{
		TargetAddr:        addr,
		HeloDomain:        "stress.local",
		MailFrom:          "loadtest@example.net",
		RcptTo:            "sink@example.net",
		Duration:          0,
		Concurrency:       5,
		MessagesPerWorker: 3,
		MessageBytes:      64,
		ConnectTimeout:    2 * time.Second,
		IOTimeout:         2 * time.Second,
		MaxLatencySamples: 1000,
	}

	res := runStage(context.Background(), cfg, cfg.Concurrency)
	if res.Attempts != 15 {
		t.Fatalf("unexpected attempts: got %d want 15", res.Attempts)
	}
	if res.Successes != 15 {
		t.Fatalf("unexpected successes: got %d want 15", res.Successes)
	}
	if res.Failures != 0 {
		t.Fatalf("unexpected failures: got %d want 0", res.Failures)
	}
}

func startFakeSMTPServer(t *testing.T) (string, func()) {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}

	var wg sync.WaitGroup
	stopCh := make(chan struct{})
	wg.Add(1)
	go func() {
		defer wg.Done()
		for {
			conn, err := ln.Accept()
			if err != nil {
				select {
				case <-stopCh:
					return
				default:
					return
				}
			}
			wg.Add(1)
			go func(c net.Conn) {
				defer wg.Done()
				handleFakeSMTPConn(c)
			}(conn)
		}
	}()

	stop := func() {
		close(stopCh)
		_ = ln.Close()
		wg.Wait()
	}

	return ln.Addr().String(), stop
}

func handleFakeSMTPConn(conn net.Conn) {
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(10 * time.Second))

	r := bufio.NewReader(conn)
	w := bufio.NewWriter(conn)

	_, _ = w.WriteString("220 fake-smtp\r\n")
	_ = w.Flush()

	inData := false
	for {
		line, err := r.ReadString('\n')
		if err != nil {
			return
		}
		line = strings.TrimRight(line, "\r\n")

		if inData {
			if line == "." {
				_, _ = w.WriteString("250 queued\r\n")
				_ = w.Flush()
				inData = false
			}
			continue
		}

		upper := strings.ToUpper(line)
		switch {
		case strings.HasPrefix(upper, "EHLO "), strings.HasPrefix(upper, "HELO "):
			_, _ = w.WriteString("250-fake\r\n250 ok\r\n")
		case strings.HasPrefix(upper, "MAIL FROM:"):
			_, _ = w.WriteString("250 ok\r\n")
		case strings.HasPrefix(upper, "RCPT TO:"):
			_, _ = w.WriteString("250 ok\r\n")
		case upper == "DATA":
			_, _ = w.WriteString("354 end with <CRLF>.<CRLF>\r\n")
			inData = true
		case upper == "QUIT":
			_, _ = w.WriteString("221 bye\r\n")
			_ = w.Flush()
			return
		default:
			_, _ = w.WriteString("500 unknown\r\n")
		}
		_ = w.Flush()
	}
}
