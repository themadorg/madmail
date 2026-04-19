// Command test-client is a tiny connectivity checker inspired by chatmail-core’s
// IMAP/SMTP setup flow: dial, optional TLS, authenticate, then disconnect cleanly.
// Intended for manual checks and CI against madmail (or any compatible server).
package main

import (
	"crypto/tls"
	"errors"
	"flag"
	"fmt"
	"net"
	"os"
	"strings"
	"time"

	"github.com/emersion/go-imap/client"
	"github.com/emersion/go-sasl"
	smtp "github.com/emersion/go-smtp"
)

type security uint8

const (
	secPlain security = iota
	secStartTLS
	secTLS
)

func parseSecurity(s string) (security, error) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "plain", "insecure", "tcp":
		return secPlain, nil
	case "starttls", "start-tls":
		return secStartTLS, nil
	case "tls", "ssl", "imaps", "smtps":
		return secTLS, nil
	default:
		return 0, fmt.Errorf("unknown security %q (use plain|starttls|tls)", s)
	}
}

func tlsConfig(host string, insecure bool) *tls.Config {
	return &tls.Config{
		ServerName:         host,
		InsecureSkipVerify: insecure,
		MinVersion:         tls.VersionTLS12,
	}
}

func serverName(addr string) string {
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return addr
	}
	return host
}

func dialIMAP(addr string, sec security, insecure bool) (*client.Client, error) {
	host := serverName(addr)
	cfg := tlsConfig(host, insecure)
	switch sec {
	case secPlain:
		return client.Dial(addr)
	case secTLS:
		return client.DialTLS(addr, cfg)
	case secStartTLS:
		c, err := client.Dial(addr)
		if err != nil {
			return nil, err
		}
		if err := c.StartTLS(cfg); err != nil {
			_ = c.Logout()
			return nil, err
		}
		return c, nil
	default:
		return nil, errors.New("imap: invalid security mode")
	}
}

func runIMAP(args []string) error {
	fs := flag.NewFlagSet("imap", flag.ExitOnError)
	addr := fs.String("addr", "127.0.0.1:143", "host:port")
	secStr := fs.String("security", "plain", "plain | starttls | tls")
	user := fs.String("user", "", "if set, run LOGIN after connect")
	pass := fs.String("password", "", "password for LOGIN")
	insecure := fs.Bool("insecure-tls", false, "skip TLS certificate verification")
	wantChatmail := fs.Bool("want-xchatmail", false, "fail if CAPABILITY does not contain XCHATMAIL")
	noop := fs.Bool("noop", false, "send IMAP NOOP after connect (or after LOGIN); for idle experiments use with -idle")
	idle := fs.Duration("idle", 0, "sleep with connection and session open before LOGOUT (e.g. 78s); requires -timeout > this value")
	timeout := fs.Duration("timeout", 30*time.Second, "overall operation timeout (must exceed -idle when set)")
	if err := fs.Parse(args); err != nil {
		return err
	}
	sec, err := parseSecurity(*secStr)
	if err != nil {
		return err
	}

	ctx := time.AfterFunc(*timeout, func() {
		fmt.Fprintf(os.Stderr, "imap: timeout after %s\n", *timeout)
		os.Exit(124)
	})
	defer ctx.Stop()

	c, err := dialIMAP(*addr, sec, *insecure)
	if err != nil {
		return fmt.Errorf("imap dial: %w", err)
	}
	defer func() { _ = c.Logout() }()

	if *user != "" {
		if err := c.Login(*user, *pass); err != nil {
			return fmt.Errorf("imap login: %w", err)
		}
	}

	if *wantChatmail {
		caps, err := c.Capability()
		if err != nil {
			return fmt.Errorf("imap capability: %w", err)
		}
		var ok bool
		for cap := range caps {
			if cap == "XCHATMAIL" {
				ok = true
				break
			}
		}
		if !ok {
			return fmt.Errorf("imap: capability XCHATMAIL not advertised")
		}
	}

	if *noop {
		if err := c.Noop(); err != nil {
			return fmt.Errorf("imap noop: %w", err)
		}
	}
	if *idle > 0 {
		time.Sleep(*idle)
	}

	fmt.Println("imap: ok")
	return nil
}

func dialSMTP(addr string, sec security, insecure bool) (*smtp.Client, error) {
	host := serverName(addr)
	cfg := tlsConfig(host, insecure)

	switch sec {
	case secPlain:
		return smtp.Dial(addr)
	case secTLS:
		return smtp.DialTLS(addr, cfg)
	case secStartTLS:
		return smtp.DialStartTLS(addr, cfg)
	default:
		return nil, errors.New("smtp: invalid security mode")
	}
}

func runSMTP(args []string) error {
	fs := flag.NewFlagSet("smtp", flag.ExitOnError)
	addr := fs.String("addr", "127.0.0.1:587", "host:port")
	secStr := fs.String("security", "plain", "plain | starttls | tls")
	user := fs.String("user", "", "if set, authenticate after EHLO")
	pass := fs.String("password", "", "password for AUTH PLAIN")
	insecure := fs.Bool("insecure-tls", false, "skip TLS certificate verification")
	timeout := fs.Duration("timeout", 30*time.Second, "overall operation timeout")
	if err := fs.Parse(args); err != nil {
		return err
	}
	sec, err := parseSecurity(*secStr)
	if err != nil {
		return err
	}

	ctx := time.AfterFunc(*timeout, func() {
		fmt.Fprintf(os.Stderr, "smtp: timeout after %s\n", *timeout)
		os.Exit(124)
	})
	defer ctx.Stop()

	c, err := dialSMTP(*addr, sec, *insecure)
	if err != nil {
		return fmt.Errorf("smtp dial: %w", err)
	}
	defer c.Close()

	if err := c.Hello("localhost"); err != nil {
		return fmt.Errorf("smtp EHLO: %w", err)
	}

	if *user != "" {
		a := sasl.NewPlainClient("", *user, *pass)
		if err := c.Auth(a); err != nil {
			return fmt.Errorf("smtp auth: %w", err)
		}
	}

	if err := c.Quit(); err != nil {
		return fmt.Errorf("smtp QUIT: %w", err)
	}

	fmt.Println("smtp: ok")
	return nil
}

func usage() {
	fmt.Fprintf(os.Stderr, `usage:
  %s imap  [-addr host:port] [-security plain|starttls|tls] [-user U] [-password P]
           [-want-xchatmail] [-noop] [-idle duration] [-insecure-tls] [-timeout duration]
  %s smtp  [-addr host:port] [-security plain|starttls|tls] [-user U] [-password P]
           [-insecure-tls] [-timeout duration]

Thin connectivity tester (chatmail-core–style handshake only).
`, os.Args[0], os.Args[0])
}

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	var err error
	switch os.Args[1] {
	case "imap":
		err = runIMAP(os.Args[2:])
	case "smtp":
		err = runSMTP(os.Args[2:])
	case "-h", "--help", "help":
		usage()
		os.Exit(0)
	default:
		fmt.Fprintf(os.Stderr, "unknown command %q\n\n", os.Args[1])
		usage()
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}
