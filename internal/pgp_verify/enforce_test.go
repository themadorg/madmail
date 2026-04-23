/*
Maddy Mail Server - Composable all-in-one email server.
Copyright © 2019-2020 Maddy Mail Server contributors

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

package pgp_verify

import (
	"errors"
	"strings"
	"testing"

	"github.com/emersion/go-message/textproto"
	"github.com/themadorg/madmail/framework/exterrors"
)

const secureJoinBody = "--bnd\r\n" +
	"Content-Type: text/plain\r\n" +
	"\r\n" +
	"secure-join: vc-request\r\n" +
	"--bnd--\r\n"

func enforceHeader(fields map[string]string) textproto.Header {
	h := textproto.Header{}
	for k, v := range fields {
		h.Set(k, v)
	}
	return h
}

func TestEnforceEncryption_SecureJoinAccepted(t *testing.T) {
	h := enforceHeader(map[string]string{
		"Secure-Join":  "vc-request",
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	if err := EnforceEncryption(h, strings.NewReader(secureJoinBody), Options{}); err != nil {
		t.Fatalf("expected Secure-Join vc-request to be accepted, got: %v", err)
	}
}

func TestEnforceEncryption_UnencryptedRejected(t *testing.T) {
	h := enforceHeader(map[string]string{
		"From":         "alice@example.org",
		"Subject":      "test",
		"Content-Type": "text/plain",
	})
	err := EnforceEncryption(h, strings.NewReader("cleartext message"), Options{
		MailFrom:   "alice@example.org",
		Recipients: []string{"bob@example.org"},
	})
	if err == nil {
		t.Fatal("expected unencrypted plaintext to be rejected")
	}
	var smtpErr *exterrors.SMTPError
	if !errors.As(err, &smtpErr) {
		t.Fatalf("expected *exterrors.SMTPError, got %T: %v", err, err)
	}
	if smtpErr.Code != 523 {
		t.Errorf("expected SMTP 523, got %d", smtpErr.Code)
	}
	if smtpErr.EnhancedCode != (exterrors.EnhancedCode{5, 7, 1}) {
		t.Errorf("expected enhanced 5.7.1, got %v", smtpErr.EnhancedCode)
	}
}

func TestEnforceEncryption_PassthroughSender(t *testing.T) {
	h := enforceHeader(map[string]string{
		"Content-Type": "text/plain",
	})
	err := EnforceEncryption(h, strings.NewReader("cleartext"), Options{
		MailFrom:           "bot@example.org",
		PassthroughSenders: []string{"bot@example.org"},
	})
	if err != nil {
		t.Fatalf("expected passthrough sender to be accepted, got: %v", err)
	}
}

func TestEnforceEncryption_PassthroughRecipientDomain(t *testing.T) {
	h := enforceHeader(map[string]string{
		"Content-Type": "text/plain",
	})
	err := EnforceEncryption(h, strings.NewReader("cleartext"), Options{
		MailFrom:              "alice@example.org",
		Recipients:            []string{"a@partner.com", "b@partner.com"},
		PassthroughRecipients: []string{"@partner.com"},
	})
	if err != nil {
		t.Fatalf("expected passthrough domain recipients to be accepted, got: %v", err)
	}
}

func TestEnforceEncryption_PassthroughRecipientMixed(t *testing.T) {
	// One passthrough + one normal recipient must NOT short-circuit.
	h := enforceHeader(map[string]string{
		"Content-Type": "text/plain",
	})
	err := EnforceEncryption(h, strings.NewReader("cleartext"), Options{
		MailFrom:              "alice@example.org",
		Recipients:            []string{"a@partner.com", "c@other.com"},
		PassthroughRecipients: []string{"@partner.com"},
	})
	if err == nil {
		t.Fatal("expected mixed recipient batch to still be rejected")
	}
}

func TestEnforceEncryption_BounceAccepted(t *testing.T) {
	h := enforceHeader(map[string]string{
		"From":           "mailer-daemon@example.org",
		"Auto-Submitted": "auto-replied",
		"Content-Type":   `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(ignored)"), Options{
		MailFrom: "mailer-daemon@example.org",
	})
	if err != nil {
		t.Fatalf("expected mailer-daemon bounce to be accepted, got: %v", err)
	}
}

func TestEnforceEncryption_SpoofedBounceRejected(t *testing.T) {
	// Envelope claims mailer-daemon but From header is a user —
	// reject to prevent bounce-channel spoofing.
	h := enforceHeader(map[string]string{
		"From":           "alice@example.org",
		"Auto-Submitted": "auto-replied",
		"Content-Type":   `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(ignored)"), Options{
		MailFrom: "mailer-daemon@example.org",
	})
	if err == nil {
		t.Fatal("expected spoofed-From bounce to be rejected")
	}
}
