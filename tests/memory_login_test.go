//go:build integration
// +build integration

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

package tests_test

import (
	"testing"

	"github.com/themadorg/madmail/tests"
)

// TestMemorySMTPIMAPLogin tests SMTP and IMAP authentication with in-memory storage.
func TestMemorySMTPIMAPLogin(tt *testing.T) {
	tt.Parallel()
	t := tests.NewT(tt)

	t.DNS(nil)
	t.Port("smtp")
	t.Port("imap")
	t.Config(`
		storage.memory local_mailboxes {
			auto_create yes
			default_quota 1G
		}

		auth.pass_table local_authdb {
			auto_create yes
			table memory {
				entry "user1@maddy.test" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
				entry "user2@maddy.test" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
			}
		}

		hostname mx.maddy.test
		
		submission tcp://127.0.0.1:{env:TEST_PORT_smtp} {
			hostname mx.maddy.test
			tls off
			auth &local_authdb
			
			deliver_to &local_mailboxes
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off
			auth &local_authdb
			storage &local_mailboxes
		}
	`)
	t.Run(1)
	defer t.Close()

	// Test SMTP authentication
	smtpConn := t.Conn("smtp")
	defer smtpConn.Close()
	smtpConn.SMTPNegotation("localhost", nil, nil)
	smtpConn.SMTPPlainAuth("user1@maddy.test", "123", true)
	smtpConn.Writeln("QUIT")
	smtpConn.ExpectPattern("221 *")

	// Test IMAP authentication for user1
	imapConn := t.Conn("imap")
	defer imapConn.Close()
	imapConn.ExpectPattern(`\* OK *`)
	imapConn.Writeln(". LOGIN user1@maddy.test 123")
	imapConn.ExpectPattern(". OK *")
	imapConn.Writeln(". SELECT INBOX")
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`. OK *`)
	imapConn.Writeln(". LOGOUT")
	imapConn.ExpectPattern(`\* BYE *`)
	imapConn.ExpectPattern(`. OK *`)

	// Test IMAP authentication for user2
	imapConn2 := t.Conn("imap")
	defer imapConn2.Close()
	imapConn2.ExpectPattern(`\* OK *`)
	imapConn2.Writeln(". LOGIN user2@maddy.test 123")
	imapConn2.ExpectPattern(". OK *")
	imapConn2.Writeln(". SELECT INBOX")
	imapConn2.ExpectPattern(`\* *`)
	imapConn2.ExpectPattern(`\* *`)
	imapConn2.ExpectPattern(`\* *`)
	imapConn2.ExpectPattern(`\* *`)
	imapConn2.ExpectPattern(`\* *`)
	imapConn2.ExpectPattern(`\* *`)
	imapConn2.ExpectPattern(`. OK *`)
	imapConn2.Writeln(". LOGOUT")
	imapConn2.ExpectPattern(`\* BYE *`)
	imapConn2.ExpectPattern(`. OK *`)
}

// TestMemoryStorageBasic tests basic IMAP operations with in-memory storage.
func TestMemoryStorageBasic(tt *testing.T) {
	tt.Parallel()
	t := tests.NewT(tt)

	t.DNS(nil)
	t.Port("imap")
	t.Config(`
		storage.memory local_mailboxes {
			auto_create yes
			default_quota 1G
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off
			auth pass_table static {
				entry "testuser" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
			}
			storage &local_mailboxes
		}
	`)
	t.Run(1)
	defer t.Close()

	// Connect and authenticate
	imapConn := t.Conn("imap")
	defer imapConn.Close()
	imapConn.ExpectPattern(`\* OK *`)
	imapConn.Writeln(". LOGIN testuser 123")
	imapConn.ExpectPattern(". OK *")

	// Create a mailbox
	imapConn.Writeln(". CREATE Sent")
	imapConn.ExpectPattern(". OK *")

	// List mailboxes
	imapConn.Writeln(`. LIST "" "*"`)
	imapConn.ExpectPattern(`\* LIST *`)
	imapConn.ExpectPattern(`\* LIST *`)
	imapConn.ExpectPattern(". OK *")

	// Delete mailbox
	imapConn.Writeln(". DELETE Sent")
	imapConn.ExpectPattern(". OK *")

	// Logout
	imapConn.Writeln(". LOGOUT")
	imapConn.ExpectPattern(`\* BYE *`)
	imapConn.ExpectPattern(`. OK *`)
}
