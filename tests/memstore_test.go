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
"time"

"github.com/themadorg/madmail/tests"
)

// Test memstore storage delivery
func TestMemstoreDelivery(tt *testing.T) {
tt.Parallel()
t := tests.NewT(tt)

t.DNS(nil)
t.Port("imap")
t.Port("smtp")
t.Config(`
storage.memstore test_store {
auto_create yes
}

imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
tls off

auth dummy
storage &test_store
}

smtp tcp://127.0.0.1:{env:TEST_PORT_smtp} {
hostname maddy.test
tls off

deliver_to &test_store
}
`)
t.Run(2)
defer t.Close()

imapConn := t.Conn("imap")
defer imapConn.Close()
imapConn.ExpectPattern(`\* OK *`)
imapConn.Writeln(". LOGIN testusr@maddy.test 1234")
imapConn.ExpectPattern(". OK *")
imapConn.Writeln(". SELECT INBOX")
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`. OK *`)

smtpConn := t.Conn("smtp")
defer smtpConn.Close()
smtpConn.SMTPNegotation("localhost", nil, nil)
smtpConn.Writeln("MAIL FROM:<sender@maddy.test>")
smtpConn.ExpectPattern("2*")
smtpConn.Writeln("RCPT TO:<testusr@maddy.test>")
smtpConn.ExpectPattern("2*")
smtpConn.Writeln("DATA")
smtpConn.ExpectPattern("354 *")
smtpConn.Writeln("From: <sender@maddy.test>")
smtpConn.Writeln("To: <testusr@maddy.test>")
smtpConn.Writeln("Subject: Hi from memstore!")
smtpConn.Writeln("")
smtpConn.Writeln("Hi! This is a test message using memstore storage.")
smtpConn.Writeln(".")
smtpConn.ExpectPattern("2*")

time.Sleep(500 * time.Millisecond)

imapConn.Writeln(". NOOP")
imapConn.ExpectPattern(`\* 1 EXISTS`)
imapConn.ExpectPattern(`\* 1 RECENT`)
imapConn.ExpectPattern(". OK *")

imapConn.Writeln(". FETCH 1 (BODY.PEEK[])")
imapConn.ExpectPattern(`\* 1 FETCH (BODY\[\] {*}*`)
imapConn.Expect(`Delivered-To: testusr@maddy.test`)
imapConn.Expect(`Return-Path: <sender@maddy.test>`)
imapConn.ExpectPattern(`Received: from localhost (client.maddy.test \[` + tests.DefaultSourceIP.String() + `\]) by maddy.test`)
imapConn.ExpectPattern(` (envelope-sender <sender@maddy.test>) with ESMTP id *; *`)
imapConn.ExpectPattern(` *`)
imapConn.Expect("From: <sender@maddy.test>")
imapConn.Expect("To: <testusr@maddy.test>")
imapConn.Expect("Subject: Hi from memstore!")
imapConn.Expect("")
imapConn.Expect("Hi! This is a test message using memstore storage.")
imapConn.Expect(")")
imapConn.ExpectPattern(`. OK *`)
}

// Test delivery to multiple recipients with memstore storage
func TestMemstoreMultipleRecipients(tt *testing.T) {
tt.Parallel()
t := tests.NewT(tt)

t.DNS(nil)
t.Port("imap")
t.Port("smtp")
t.Config(`
storage.memstore test_store {
auto_create yes
}

imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
tls off

auth dummy
storage &test_store
}

smtp tcp://127.0.0.1:{env:TEST_PORT_smtp} {
hostname maddy.test
tls off

deliver_to &test_store
}
`)
t.Run(2)
defer t.Close()

// Send message to multiple recipients
smtpConn := t.Conn("smtp")
defer smtpConn.Close()
smtpConn.SMTPNegotation("localhost", nil, nil)
smtpConn.Writeln("MAIL FROM:<sender@maddy.test>")
smtpConn.ExpectPattern("2*")
smtpConn.Writeln("RCPT TO:<user1@maddy.test>")
smtpConn.ExpectPattern("2*")
smtpConn.Writeln("RCPT TO:<user2@maddy.test>")
smtpConn.ExpectPattern("2*")
smtpConn.Writeln("RCPT TO:<user3@maddy.test>")
smtpConn.ExpectPattern("2*")
smtpConn.Writeln("DATA")
smtpConn.ExpectPattern("354 *")
smtpConn.Writeln("From: <sender@maddy.test>")
smtpConn.Writeln("To: <user1@maddy.test>, <user2@maddy.test>, <user3@maddy.test>")
smtpConn.Writeln("Subject: Multi-recipient test")
smtpConn.Writeln("")
smtpConn.Writeln("This message is sent to multiple recipients.")
smtpConn.Writeln(".")
smtpConn.ExpectPattern("2*")

time.Sleep(500 * time.Millisecond)

// Check each user received the message
for _, user := range []string{"user1@maddy.test", "user2@maddy.test", "user3@maddy.test"} {
imapConn := t.Conn("imap")
imapConn.ExpectPattern(`\* OK *`)
imapConn.Writeln(". LOGIN " + user + " 1234")
imapConn.ExpectPattern(". OK *")
imapConn.Writeln(". SELECT INBOX")
imapConn.ExpectPattern(`\* 1 EXISTS`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`\* *`)
imapConn.ExpectPattern(`. OK *`)
imapConn.Close()
}
}
