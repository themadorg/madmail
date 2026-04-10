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

package chatmail

import (
	"fmt"
	"net"
	"strings"
)

// MxDelivSecurityError represents a security validation failure on the /mxdeliv endpoint.
type MxDelivSecurityError struct {
	Code    int
	Message string
}

func (e *MxDelivSecurityError) Error() string {
	return fmt.Sprintf("mxdeliv security: %s (code %d)", e.Message, e.Code)
}

// ValidateMxDelivTLS checks that the /mxdeliv request arrived over TLS.
// Unencrypted HTTP delivery between servers must be rejected.
func ValidateMxDelivTLS(isTLS bool) error {
	if !isTLS {
		return &MxDelivSecurityError{
			Code:    403,
			Message: "TLS required: /mxdeliv only accepts HTTPS connections",
		}
	}
	return nil
}

// NormalizeMailDomain returns the canonical forms of the server's mail domain.
// For IP-based domains like "[1.1.1.1]", it returns both "[1.1.1.1]" and "1.1.1.1".
// For hostname domains like "example.com", it returns just ["example.com"].
func NormalizeMailDomain(mailDomain string) []string {
	domains := []string{strings.ToLower(mailDomain)}

	// If domain is bracket-wrapped IP like [1.1.1.1], also accept bare IP
	stripped := strings.Trim(mailDomain, "[]")
	if stripped != mailDomain {
		domains = append(domains, strings.ToLower(stripped))
	} else if ip := net.ParseIP(mailDomain); ip != nil {
		// If domain is bare IP, also accept bracketed form
		domains = append(domains, "["+strings.ToLower(mailDomain)+"]")
	}

	return domains
}

// ValidateRecipientDomain checks that a recipient address belongs to this server.
// It extracts the domain part from the email address and compares it against
// all valid domain forms for this server.
//
// Valid forms for server with mailDomain "[1.1.1.1]":
//   - user@[1.1.1.1]  ← bracket-wrapped IP
//   - user@1.1.1.1    ← bare IP
//
// Valid forms for server with mailDomain "example.com":
//   - user@example.com
//
// Rejected:
//   - user@2.2.2.2    ← different server
//   - user@other.com  ← different domain
//   - admin@[1.1.1.1] ← handled by ValidateRecipientNotAdmin
func ValidateRecipientDomain(rcptTo string, validDomains []string) error {
	parts := strings.SplitN(rcptTo, "@", 2)
	if len(parts) != 2 {
		return &MxDelivSecurityError{
			Code:    400,
			Message: fmt.Sprintf("invalid recipient address format: %s", rcptTo),
		}
	}

	rcptDomain := strings.ToLower(parts[1])

	for _, validDomain := range validDomains {
		if rcptDomain == validDomain {
			return nil
		}
	}

	return &MxDelivSecurityError{
		Code:    404,
		Message: fmt.Sprintf("recipient domain %q does not belong to this server", rcptDomain),
	}
}

// ValidateRecipientNotAdmin checks that the recipient is not an admin-prefixed address.
// Attackers can try to deliver to admin@server to probe or abuse admin accounts.
// This blocks "admin" as a local part in federated delivery since no legitimate
// external server should be delivering to admin accounts.
var blockedLocalParts = []string{
	"admin",
	"root",
	"postmaster",
	"mailer-daemon",
	"abuse",
	"hostmaster",
	"webmaster",
}

func ValidateRecipientNotAdmin(rcptTo string) error {
	parts := strings.SplitN(rcptTo, "@", 2)
	if len(parts) != 2 {
		return &MxDelivSecurityError{
			Code:    400,
			Message: fmt.Sprintf("invalid recipient address format: %s", rcptTo),
		}
	}

	localPart := strings.ToLower(parts[0])

	for _, blocked := range blockedLocalParts {
		if localPart == blocked {
			return &MxDelivSecurityError{
				Code:    403,
				Message: fmt.Sprintf("delivery to %q is not allowed via federation", rcptTo),
			}
		}
	}

	return nil
}

// ValidateAllRecipients runs domain and admin-block checks on all recipients.
// Returns two slices: accepted recipients and rejected recipients with their errors.
func ValidateAllRecipients(mailTo []string, validDomains []string) (accepted []string, rejected map[string]error) {
	rejected = make(map[string]error)

	for _, rcpt := range mailTo {
		if err := ValidateRecipientDomain(rcpt, validDomains); err != nil {
			rejected[rcpt] = err
			continue
		}
		if err := ValidateRecipientNotAdmin(rcpt); err != nil {
			rejected[rcpt] = err
			continue
		}
		accepted = append(accepted, rcpt)
	}

	return accepted, rejected
}
