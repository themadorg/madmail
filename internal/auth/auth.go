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
	"net"
	"strings"
)

func WrapIP(domain string) string {
	trimmed := strings.Trim(domain, "[]")
	if ip := net.ParseIP(trimmed); ip != nil {
		return "[" + trimmed + "]"
	}
	return domain
}

func NormalizeUsername(username string) string {
	parts := strings.Split(username, "@")
	if len(parts) == 2 {
		return parts[0] + "@" + WrapIP(parts[1])
	}
	return WrapIP(username)
}

// ValidateLoginDomain checks that a username is in the format localpart@domain
// where domain exactly matches the expected domain (case-insensitive).
// This prevents JIT account creation for arbitrary usernames like:
//   - x@y@z (multiple @ signs)
//   - user@%5b1.2.3.4%5d (URL-encoded brackets)
//   - user@wrongdomain
//   - user@abcd (random domain)
//
// The expectedDomain should already be in the canonical form, e.g. "[1.1.1.1]".
// The username is normalized before comparison (bare IPs get brackets added).
func ValidateLoginDomain(username, expectedDomain string) error {
	if expectedDomain == "" {
		return nil // no domain restriction configured
	}

	// Reject URL-encoded characters in the username.
	// These are never valid in IMAP LOGIN usernames and indicate
	// an attempt to bypass domain validation.
	if strings.Contains(username, "%") {
		return fmt.Errorf("invalid username: contains URL-encoded characters")
	}

	// Split on @ — must have exactly one @
	parts := strings.Split(username, "@")
	if len(parts) != 2 {
		return fmt.Errorf("invalid username format: expected localpart@domain")
	}

	localpart := parts[0]
	domain := parts[1]

	// Localpart must not be empty
	if localpart == "" {
		return fmt.Errorf("invalid username: empty localpart")
	}

	// Normalize the domain (wrap bare IPs in brackets)
	domain = WrapIP(domain)

	// Compare domain against expected (case-insensitive)
	if !strings.EqualFold(domain, expectedDomain) {
		return fmt.Errorf("invalid login domain: expected @%s", expectedDomain)
	}

	return nil
}

func CheckDomainAuth(username string, perDomain bool, allowedDomains []string) (loginName string, allowed bool) {
	var accountName, domain string
	if perDomain {
		parts := strings.Split(username, "@")
		if len(parts) != 2 {
			return "", false
		}
		domain = parts[1]
		accountName = username
	} else {
		parts := strings.Split(username, "@")
		accountName = parts[0]
		if len(parts) == 2 {
			domain = parts[1]
		}
	}

	allowed = domain == ""
	if allowedDomains != nil && domain != "" {
		for _, allowedDomain := range allowedDomains {
			if strings.EqualFold(domain, allowedDomain) {
				allowed = true
			}
		}
		if !allowed {
			return "", false
		}
	}

	return accountName, allowed
}
