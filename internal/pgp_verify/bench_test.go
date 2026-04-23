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
	"bytes"
	"encoding/base64"
	"fmt"
	"math/rand"
	"strings"
	"testing"

	"github.com/emersion/go-message/textproto"
)

// makeArmoredPGP builds a syntactically valid multipart/encrypted body
// whose SEIPD packet carries payloadBytes of random data. It does not
// produce a real encrypted message — the walker only cares about
// OpenPGP packet framing, so random bytes are sufficient to exercise
// the decoder + walker + discard loop end-to-end.
func makeArmoredPGP(payloadBytes int) (boundary string, body []byte) {
	boundary = "pgp-test-boundary"

	// Build a single SEIPD (tag 18) packet with a 5-octet length header
	// and `payloadBytes` random body. Tag byte for new-format, type 18:
	// 0b11000000 | 18 = 0xD2.
	buf := make([]byte, 0, payloadBytes+5+1)
	buf = append(buf, 0xD2)
	buf = append(buf, 0xFF)
	buf = append(buf,
		byte(payloadBytes>>24),
		byte(payloadBytes>>16),
		byte(payloadBytes>>8),
		byte(payloadBytes),
	)
	payload := make([]byte, payloadBytes)
	rng := rand.New(rand.NewSource(1))
	_, _ = rng.Read(payload)
	buf = append(buf, payload...)

	// ASCII-armor wrap with 64-column line folding.
	b64 := base64.StdEncoding.EncodeToString(buf)
	var armored strings.Builder
	armored.WriteString("-----BEGIN PGP MESSAGE-----\r\n")
	armored.WriteString("Version: Test\r\n")
	armored.WriteString("\r\n")
	for i := 0; i < len(b64); i += 64 {
		end := i + 64
		if end > len(b64) {
			end = len(b64)
		}
		armored.WriteString(b64[i:end])
		armored.WriteString("\r\n")
	}
	armored.WriteString("=AAAA\r\n")
	armored.WriteString("-----END PGP MESSAGE-----\r\n")

	var mime bytes.Buffer
	fmt.Fprintf(&mime, "--%s\r\n", boundary)
	mime.WriteString("Content-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&mime, "--%s\r\n", boundary)
	mime.WriteString("Content-Type: application/octet-stream\r\n\r\n")
	mime.WriteString(armored.String())
	fmt.Fprintf(&mime, "--%s--\r\n", boundary)
	return boundary, mime.Bytes()
}

// BenchmarkEnforceEncryption_Armored5MB measures the hot path for
// large encrypted uploads. This is the scenario the user hit: the
// attachment is large, the policy accepts it, but the old
// implementation memcpy'd the body 8-9 times before returning.
func BenchmarkEnforceEncryption_Armored5MB(b *testing.B) {
	boundary, body := makeArmoredPGP(5 * 1024 * 1024)
	h := textproto.Header{}
	h.Set("Content-Type", "multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=\""+boundary+"\"")

	b.SetBytes(int64(len(body)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := EnforceEncryption(h, bytes.NewReader(body), Options{
			MailFrom:   "alice@example.org",
			Recipients: []string{"bob@example.org"},
		}); err != nil {
			b.Fatalf("unexpected rejection: %v", err)
		}
	}
}

// BenchmarkEnforceEncryption_CleartextReject measures the fast reject
// path: Content-Type alone is enough to decide, and the body is never
// read. With the new streaming implementation this is O(1) regardless
// of body size.
func BenchmarkEnforceEncryption_CleartextReject(b *testing.B) {
	body := bytes.Repeat([]byte("cleartext content "), 1<<16) // ≈1 MiB
	h := textproto.Header{}
	h.Set("Content-Type", "text/plain")
	h.Set("From", "alice@example.org")

	b.SetBytes(int64(len(body)))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		err := EnforceEncryption(h, bytes.NewReader(body), Options{
			MailFrom:   "alice@example.org",
			Recipients: []string{"bob@example.org"},
		})
		if err == nil {
			b.Fatal("expected cleartext to be rejected")
		}
	}
}
