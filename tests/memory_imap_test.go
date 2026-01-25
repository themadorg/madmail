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

// Memory-backed version of TestIMAPEndpointAuthMap
func TestMemoryIMAPEndpointAuthMap(tt *testing.T) {
	tt.Parallel()
	t := tests.NewT(tt)

	t.DNS(nil)
	t.Port("imap")
	t.Config(`
		storage.memory test_store {
			auto_create yes
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off

			auth_map email_localpart
			auth pass_table memory {
			}
			storage &test_store
		}
	`)
	t.Run(1)
	defer t.Close()

	imapConn := t.Conn("imap")
	defer imapConn.Close()
	imapConn.ExpectPattern(`\* OK *`)
	imapConn.Writeln(". LOGIN user@example.org 123")
	imapConn.ExpectPattern(". OK *")
	imapConn.Writeln(". SELECT INBOX")
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`\* *`)
	imapConn.ExpectPattern(`. OK *`)
}

// Memory-backed version of TestIMAPEndpointStorageMap
func TestMemoryIMAPEndpointStorageMap(tt *testing.T) {
	tt.Parallel()
	t := tests.NewT(tt)

	t.DNS(nil)
	t.Port("imap")
	t.Config(`
		storage.memory test_store {
			auto_create yes
		}

		imap tcp://127.0.0.1:{env:TEST_PORT_imap} {
			tls off

			storage_map email_localpart

			auth_map email_localpart
			auth pass_table memory {
			}
			storage &test_store
		}
	`)
	t.Run(1)
	defer t.Close()

	imapConn := t.Conn("imap")
	defer imapConn.Close()
	imapConn.ExpectPattern(`\* OK *`)
	imapConn.Writeln(". LOGIN user@example.org 123")
	imapConn.ExpectPattern(". OK *")
	imapConn.Writeln(". CREATE testbox")
	imapConn.ExpectPattern(". OK *")

	imapConn2 := t.Conn("imap")
	defer imapConn2.Close()
	imapConn2.ExpectPattern(`\* OK *`)
	imapConn2.Writeln(". LOGIN user@example.com 123")
	imapConn2.ExpectPattern(". OK *")
	imapConn2.Writeln(`. LIST "" "*"`)
	imapConn2.ExpectPattern(`\* LIST *`)
	imapConn2.ExpectPattern(`\* LIST *`)
	imapConn2.ExpectPattern(". OK *")
}
