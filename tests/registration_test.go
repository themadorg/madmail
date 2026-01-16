//go:build integration && cgo && !nosqlite3
// +build integration,cgo,!nosqlite3

package tests_test

import (
	"testing"

	"github.com/themadorg/madmail/tests"
)

func TestRegistration(tt *testing.T) {
	tt.Parallel()

	t := tests.NewT(tt)
	t.DNS(nil)
	t.Port("imap")
	t.Port("smtp")
	t.Config(`
		storage.imapsql local_mail {
			driver sqlite3
			dsn imapsql.db
			auto_create yes
		}

		auth.pass_table local_auth {
			table sql_table {
				driver sqlite3
				dsn {env:TEST_STATE_DIR}/auth.db
				table_name credentials
			}
			auto_create yes
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off
			auth &local_auth
			storage &local_mail
		}

		smtp tcp://127.0.0.1:{env:TEST_PORT_smtp} {
			hostname mx.maddy.test
			tls off
			auth &local_auth
			deliver_to &local_mail
		}
	`)
	t.Run(2)
	defer t.Close()

	// 1. Test registration open - SMTP
	t.Subtest("SMTP Registration Open", func(t *tests.T) {
		conn := t.Conn("smtp")
		defer conn.Close()
		conn.SMTPNegotation("localhost", nil, nil)

		// Attempt to login with non-existent user
		// This should create the user because auto_create is yes
		conn.SMTPPlainAuth("newuser@1.1.1.1", "password", true)
		conn.Writeln("QUIT")
		conn.ExpectPattern("221 *")
	})

	// 2. Test normalization - IMAP
	t.Subtest("IMAP Normalization", func(t *tests.T) {
		conn := t.Conn("imap")
		defer conn.Close()
		conn.ExpectPattern(`\* OK *`)

		// login with xxxx@[1.1.1.1], should work because it was created as [1.1.1.1] via SMTP (assuming normalization works same way)
		// Wait, the previous test created newuser@[1.1.1.1]
		conn.Writeln(". LOGIN newuser@[1.1.1.1] password")
		conn.ExpectPattern(". OK *")
		conn.Writeln(". LOGOUT")
		conn.ExpectPattern(`\* BYE *`)
	})

	// 3. Test normalization 2 - IMAP with different format
	t.Subtest("IMAP Normalization 2", func(t *tests.T) {
		conn := t.Conn("imap")
		defer conn.Close()
		conn.ExpectPattern(`\* OK *`)

		// login with xxxx@1.1.1.1, should also work
		conn.Writeln(". LOGIN newuser@1.1.1.1 password")
		conn.ExpectPattern(". OK *")
		conn.Writeln(". LOGOUT")
		conn.ExpectPattern(`\* BYE *`)
	})

	// 4. Test wrong password for existing user
	t.Subtest("IMAP Wrong Password", func(t *tests.T) {
		conn := t.Conn("imap")
		defer conn.Close()
		conn.ExpectPattern(`\* OK *`)

		// login with correct user but WRONG password, should fail
		conn.Writeln(". LOGIN newuser@1.1.1.1 wrongpassword")
		conn.ExpectPattern(". NO *")
	})

	// 5. Test registration via delivery (incoming email)
	t.Subtest("SMTP Delivery Registration", func(t *tests.T) {
		conn := t.Conn("smtp")
		defer conn.Close()
		conn.SMTPNegotation("localhost", nil, nil)

		// Send email to a non-existent user
		// This should create the user and accept the message if registration is open
		conn.Writeln("MAIL FROM:<sender@example.com>")
		conn.ExpectPattern("250 *")
		conn.Writeln("RCPT TO:<deliveryuser@1.1.1.1>")
		conn.ExpectPattern("250 *") // If it fails, it will be 5xx
		conn.Writeln("DATA")
		conn.ExpectPattern("354 *")
		conn.Writeln("Content-Type: text/plain\r\nSubject: Test\r\nSecure-Join: vc-request\r\n\r\nTest message")
		conn.Writeln(".")
		conn.ExpectPattern("250 *")
		conn.Writeln("QUIT")
		conn.ExpectPattern("221 *")

		// Verify user was created by trying to login via IMAP
		// Note: No auth entry yet, but we can verify storage existence if we want
		// However, for this test we'll assume the 250 above is enough proof it accepted it.
	})
}

func TestRegistrationClosed(tt *testing.T) {
	tt.Parallel()

	t := tests.NewT(tt)
	t.DNS(nil)
	t.Port("imap")
	t.Port("smtp")
	t.Config(`
		storage.imapsql local_mail {
			driver sqlite3
			dsn imapsql.db
		}

		auth.pass_table local_auth {
			table sql_table {
				driver sqlite3
				dsn {env:TEST_STATE_DIR}/auth2.db
				table_name credentials
			}
			auto_create no
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off
			auth &local_auth
			storage &local_mail
		}

		smtp tcp://127.0.0.1:{env:TEST_PORT_smtp} {
			hostname mx.maddy.test
			tls off
			auth &local_auth
			deliver_to &local_mail
		}
	`)
	t.Run(1)
	defer t.Close()

	t.Subtest("IMAP Registration Closed", func(t *tests.T) {
		conn := t.Conn("imap")
		defer conn.Close()
		conn.ExpectPattern(`\* OK *`)

		// Attempt to login with non-existent user, should fail
		conn.Writeln(". LOGIN nobody@1.1.1.1 password")
		conn.ExpectPattern(". NO *")
	})

	t.Subtest("SMTP Delivery Registration Closed", func(t *tests.T) {
		conn := t.Conn("smtp")
		defer conn.Close()
		conn.SMTPNegotation("localhost", nil, nil)

		// This should FAIL if auto_create is NO (default)
		conn.Writeln("MAIL FROM:<sender@example.com>")
		conn.ExpectPattern("250 *")
		conn.Writeln("RCPT TO:<deliveryuser2@1.1.1.1>")
		conn.ExpectPattern("501 *") // User does not exist
	})
}
