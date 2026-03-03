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

package auth

import (
	"fmt"
	"testing"
)

func TestCheckDomainAuth(t *testing.T) {
	cases := []struct {
		rawUsername string

		perDomain      bool
		allowedDomains []string

		loginName string
	}{
		{
			rawUsername: "username",
			loginName:   "username",
		},
		{
			rawUsername:    "username",
			allowedDomains: []string{"example.org"},
			loginName:      "username",
		},
		{
			rawUsername:    "username@example.org",
			allowedDomains: []string{"example.org"},
			loginName:      "username",
		},
		{
			rawUsername:    "username@example.com",
			allowedDomains: []string{"example.org"},
		},
		{
			rawUsername:    "username",
			allowedDomains: []string{"example.org"},
			perDomain:      true,
		},
		{
			rawUsername:    "username@example.com",
			allowedDomains: []string{"example.org"},
			perDomain:      true,
		},
		{
			rawUsername:    "username@EXAMPLE.Org",
			allowedDomains: []string{"exaMPle.org"},
			perDomain:      true,
			loginName:      "username@EXAMPLE.Org",
		},
		{
			rawUsername:    "username@example.org",
			allowedDomains: []string{"example.org"},
			perDomain:      true,
			loginName:      "username@example.org",
		},
	}

	for _, case_ := range cases {
		t.Run(fmt.Sprintf("%+v", case_), func(t *testing.T) {
			loginName, allowed := CheckDomainAuth(case_.rawUsername, case_.perDomain, case_.allowedDomains)
			if case_.loginName != "" && !allowed {
				t.Fatalf("Unexpected authentication fail")
			}
			if case_.loginName == "" && allowed {
				t.Fatalf("Expected authentication fail, got %s as login name", loginName)
			}

			if loginName != case_.loginName {
				t.Errorf("Incorrect login name, got %s, wanted %s", loginName, case_.loginName)
			}
		})
	}
}

func TestValidateLoginDomain(t *testing.T) {
	tests := []struct {
		name           string
		username       string
		expectedDomain string
		wantErr        bool
	}{
		// Valid cases
		{
			name:           "valid IP bracket domain",
			username:       "xyzzy@[1.1.1.1]",
			expectedDomain: "[1.1.1.1]",
			wantErr:        false,
		},
		{
			name:           "valid bare IP normalized to bracket",
			username:       "xyzzy@1.1.1.1",
			expectedDomain: "[1.1.1.1]",
			wantErr:        false,
		},
		{
			name:           "valid regular domain",
			username:       "user@example.org",
			expectedDomain: "example.org",
			wantErr:        false,
		},
		{
			name:           "valid domain case insensitive",
			username:       "user@EXAMPLE.ORG",
			expectedDomain: "example.org",
			wantErr:        false,
		},
		{
			name:           "no domain restriction (empty expectedDomain)",
			username:       "anything@whatever",
			expectedDomain: "",
			wantErr:        false,
		},

		// Invalid cases - attack vectors the user reported
		{
			name:           "URL-encoded brackets",
			username:       "xyzzy@%5b1.1.1.1%5d",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "wrong domain",
			username:       "xyzzy@abcd",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "multiple @ signs",
			username:       "x@y@z",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "no domain part",
			username:       "justusername",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "empty localpart",
			username:       "@[1.1.1.1]",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "empty username",
			username:       "",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "different IP address",
			username:       "user@[10.0.0.1]",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "URL-encoded at sign",
			username:       "user%40@[1.1.1.1]",
			expectedDomain: "[1.1.1.1]",
			wantErr:        true,
		},
		{
			name:           "domain with wrong regular domain",
			username:       "user@evil.com",
			expectedDomain: "example.org",
			wantErr:        true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateLoginDomain(tt.username, tt.expectedDomain)
			if tt.wantErr && err == nil {
				t.Errorf("ValidateLoginDomain(%q, %q) expected error, got nil", tt.username, tt.expectedDomain)
			}
			if !tt.wantErr && err != nil {
				t.Errorf("ValidateLoginDomain(%q, %q) unexpected error: %v", tt.username, tt.expectedDomain, err)
			}
		})
	}
}
