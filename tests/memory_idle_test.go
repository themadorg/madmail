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
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/themadorg/madmail/tests"
)

// TestMemoryIDLEMultiUser tests IMAP IDLE with multiple recipients receiving a message.
func TestMemoryIDLEMultiUser(tt *testing.T) {
	tt.Parallel()
	t := tests.NewT(tt)

	t.DNS(nil)
	t.Port("smtp")
	t.Port("submission")
	t.Port("imap")
	t.Config(`
		storage.memory local_mailboxes {
			auto_create yes
			default_quota 1G
		}

		auth.pass_table local_authdb {
			auto_create yes
			table memory {
				entry "sender@maddy.test" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
				entry "recipient1@maddy.test" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
				entry "recipient2@maddy.test" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
				entry "recipient3@maddy.test" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
				entry "recipient4@maddy.test" "bcrypt:$2a$10$E.AuCH3oYbaRrETXfXwc0.4jRAQBbanpZiCfudsJz9bHzLr/qj6ti" # password: 123
			}
		}

		hostname mx.maddy.test
		
		msgpipeline local_routing {
			deliver_to &local_mailboxes
		}

		submission tcp://127.0.0.1:{env:TEST_PORT_submission} {
			hostname mx.maddy.test
			tls off
			auth &local_authdb
			
			deliver_to &local_routing
		}

		smtp tcp://127.0.0.1:{env:TEST_PORT_smtp} {
			hostname mx.maddy.test
			tls off
			
			deliver_to &local_routing
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off
			auth &local_authdb
			storage &local_mailboxes
		}
	`)
	t.Run(1)
	defer t.Close()

	// Setup 4 IMAP connections for recipients
	var wg sync.WaitGroup
	receivedMsg := make([]bool, 4)
	recipients := []string{"recipient1@maddy.test", "recipient2@maddy.test", "recipient3@maddy.test", "recipient4@maddy.test"}

	for i, recipient := range recipients {
		wg.Add(1)
		go func(idx int, rcpt string) {
			defer wg.Done()

			// Connect and login
			imapConn := t.Conn("imap")
			defer imapConn.Close()
			imapConn.ExpectPattern(`\* OK *`)
			imapConn.Writeln(". LOGIN " + rcpt + " 123")
			imapConn.ExpectPattern(". OK *")

			// Select INBOX
			imapConn.Writeln(". SELECT INBOX")
			imapConn.ExpectPattern(`\* *`)
			imapConn.ExpectPattern(`\* *`)
			imapConn.ExpectPattern(`\* *`)
			imapConn.ExpectPattern(`\* *`)
			imapConn.ExpectPattern(`\* *`)
			imapConn.ExpectPattern(`\* *`)
			imapConn.ExpectPattern(`. OK *`)

			// Enter IDLE mode
			imapConn.Writeln(". IDLE")
			imapConn.ExpectPattern(`\+ *`)

			// Wait for message notification
			timeout := time.After(30 * time.Second)
			done := make(chan bool)
			go func() {
				line, err := imapConn.Readln()
				if err == nil && strings.Contains(line, "EXISTS") {
					receivedMsg[idx] = true
				}
				done <- true
			}()

			select {
			case <-done:
				// Got message notification
			case <-timeout:
				t.Logf("Recipient %d timed out waiting for message", idx+1)
			}

			// Exit IDLE
			imapConn.Writeln("DONE")
			imapConn.ExpectPattern(`. OK *`)

			// Logout
			imapConn.Writeln(". LOGOUT")
			imapConn.ExpectPattern(`\* BYE *`)
			imapConn.ExpectPattern(`. OK *`)
		}(i, recipient)
	}

	// Give IMAP connections time to enter IDLE
	time.Sleep(2 * time.Second)

	// Send a message from sender to all 4 recipients
	smtpConn := t.Conn("submission")
	defer smtpConn.Close()
	smtpConn.SMTPNegotation("localhost", nil, nil)
	smtpConn.SMTPPlainAuth("sender@maddy.test", "123", true)
	smtpConn.Writeln("MAIL FROM:<sender@maddy.test>")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("RCPT TO:<recipient1@maddy.test>")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("RCPT TO:<recipient2@maddy.test>")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("RCPT TO:<recipient3@maddy.test>")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("RCPT TO:<recipient4@maddy.test>")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("DATA")
	smtpConn.ExpectPattern("354 *")
	smtpConn.Writeln("From: <sender@maddy.test>")
	smtpConn.Writeln("To: <recipient1@maddy.test>, <recipient2@maddy.test>, <recipient3@maddy.test>, <recipient4@maddy.test>")
	smtpConn.Writeln("Subject: Test message for IDLE")
	smtpConn.Writeln("")
	smtpConn.Writeln("This is a test message to verify IDLE notifications work.")
	smtpConn.Writeln(".")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("QUIT")
	smtpConn.ExpectPattern("221 *")

	// Wait for all IMAP connections to complete
	wg.Wait()

	// Verify all recipients received the message
	for i, received := range receivedMsg {
		if !received {
			t.Errorf("Recipient %d did not receive message notification", i+1)
		}
	}
}

// TestMemoryMessageDelivery tests basic message delivery with in-memory storage.
func TestMemoryMessageDelivery(tt *testing.T) {
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
			}
		}

		hostname mx.maddy.test
		
		msgpipeline local_routing {
			deliver_to &local_mailboxes
		}

		submission tcp://127.0.0.1:{env:TEST_PORT_smtp} {
			hostname mx.maddy.test
			tls off
			auth &local_authdb
			
			deliver_to &local_routing
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off
			auth &local_authdb
			storage &local_mailboxes
		}
	`)
	t.Run(1)
	defer t.Close()

	// Send a message via SMTP
	smtpConn := t.Conn("smtp")
	defer smtpConn.Close()
	smtpConn.SMTPNegotation("localhost", nil, nil)
	smtpConn.SMTPPlainAuth("user1@maddy.test", "123", true)
	smtpConn.Writeln("MAIL FROM:<user1@maddy.test>")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("RCPT TO:<user1@maddy.test>")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("DATA")
	smtpConn.ExpectPattern("354 *")
	smtpConn.Writeln("From: <user1@maddy.test>")
	smtpConn.Writeln("To: <user1@maddy.test>")
	smtpConn.Writeln("Subject: Test message")
	smtpConn.Writeln("")
	smtpConn.Writeln("This is a test message.")
	smtpConn.Writeln(".")
	smtpConn.ExpectPattern("250 *")
	smtpConn.Writeln("QUIT")
	smtpConn.ExpectPattern("221 *")

	// Connect to IMAP and check message was delivered
	imapConn := t.Conn("imap")
	defer imapConn.Close()
	imapConn.ExpectPattern(`\* OK *`)
	imapConn.Writeln(". LOGIN user1@maddy.test 123")
	imapConn.ExpectPattern(". OK *")

	// Select INBOX
	imapConn.Writeln(". SELECT INBOX")
	// Skip the status lines
	for i := 0; i < 6; i++ {
		line, _ := imapConn.Readln()
		// Check if this is the message count line
		if strings.Contains(line, "EXISTS") {
			// Verify we have at least 1 message
			if !strings.Contains(line, "* 1 EXISTS") && !strings.Contains(line, "* 2 EXISTS") {
				t.Errorf("Expected at least 1 message in INBOX, got: %s", line)
			}
		}
	}

	// Fetch the message to verify it's there
	imapConn.Writeln(". FETCH 1 (BODY[])")
	imapConn.ExpectPattern(`\* 1 FETCH *`)
	imapConn.ExpectPattern(`. OK *`)

	// Logout
	imapConn.Writeln(". LOGOUT")
	imapConn.ExpectPattern(`\* BYE *`)
	imapConn.ExpectPattern(`. OK *`)
}
