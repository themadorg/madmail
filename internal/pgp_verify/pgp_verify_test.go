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
	"errors"
	"io"
	"mime/multipart"
	nettextproto "net/textproto"
	"strings"
	"testing"

	"github.com/emersion/go-message/textproto"
)

type failReader struct{}

func (failReader) Read(_ []byte) (int, error) {
	return 0, errors.New("body read should not happen")
}

func TestIsSecureJoinMessage_Valid(t *testing.T) {
	tests := []struct {
		name           string
		secureJoinHdr  string
		contentType    string
		body           string
		expectedResult bool
	}{
		{
			name:          "Valid vc-request",
			secureJoinHdr: "vc-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vc-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: true,
		},
		{
			name:          "Valid vg-request",
			secureJoinHdr: "vg-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vg-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: true,
		},
		{
			name:          "Valid with case insensitive header",
			secureJoinHdr: "VC-REQUEST",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vc-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: true,
		},
		{
			name:          "Invalid - no secure-join header",
			secureJoinHdr: "",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vc-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: false,
		},
		{
			name:          "Invalid - wrong header value",
			secureJoinHdr: "other-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vc-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: false,
		},
		{
			name:          "Valid - multipart/alternative is accepted",
			secureJoinHdr: "vc-request",
			contentType:   "multipart/alternative; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vc-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: true,
		},
		{
			name:           "Invalid - not multipart",
			secureJoinHdr:  "vc-request",
			contentType:    "text/plain",
			body:           "secure-join: vc-request",
			expectedResult: false,
		},
		{
			name:          "Valid - multiple parts are tolerated",
			secureJoinHdr: "vc-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vc-request\r\n" +
				"--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"extra part\r\n" +
				"--boundary123--\r\n",
			expectedResult: true,
		},
		{
			name:          "Invalid - wrong part content type",
			secureJoinHdr: "vc-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/html\r\n" +
				"\r\n" +
				"secure-join: vc-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: false,
		},
		{
			name:          "Invalid - wrong body text (contains instead of exact match)",
			secureJoinHdr: "vc-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"This message contains secure-join: vc-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: false,
		},
		{
			name:          "Invalid - securejoin without proper format",
			secureJoinHdr: "vc-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"securejoin\r\n" +
				"--boundary123--\r\n",
			expectedResult: false,
		},
		{
			name:          "Valid - body can differ from header (both valid)",
			secureJoinHdr: "vc-request",
			contentType:   "multipart/mixed; boundary=\"boundary123\"",
			body: "--boundary123\r\n" +
				"Content-Type: text/plain\r\n" +
				"\r\n" +
				"secure-join: vg-request\r\n" +
				"--boundary123--\r\n",
			expectedResult: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			header := textproto.Header{}
			header.Set("Secure-Join", tt.secureJoinHdr)
			header.Set("Content-Type", tt.contentType)

			body := strings.NewReader(tt.body)
			result := IsSecureJoinMessage(header, body)

			if result != tt.expectedResult {
				t.Errorf("Expected %v, got %v", tt.expectedResult, result)
			}
		})
	}
}

func TestIsAcceptedMessageSkipsBodyReadForNonCandidate(t *testing.T) {
	t.Parallel()

	header := textproto.Header{}
	header.Set("Content-Type", "text/plain")

	accepted, err := IsAcceptedMessage(header, failReader{})
	if err != nil {
		t.Fatalf("IsAcceptedMessage returned error: %v", err)
	}
	if accepted {
		t.Fatal("expected message to be rejected")
	}
}

func TestIsAcceptedMessageSkipsBodyReadForMissingContentType(t *testing.T) {
	t.Parallel()

	header := textproto.Header{}

	accepted, err := IsAcceptedMessage(header, failReader{})
	if err != nil {
		t.Fatalf("IsAcceptedMessage returned error: %v", err)
	}
	if accepted {
		t.Fatal("expected message to be rejected")
	}
}

func TestIsAcceptedMessageSecureJoin(t *testing.T) {
	t.Parallel()

	header := textproto.Header{}
	header.Set("Secure-Join", "vc-request")
	header.Set("Content-Type", "multipart/mixed; boundary=\"boundary123\"")

	body := "--boundary123\r\n" +
		"Content-Type: text/plain\r\n" +
		"\r\n" +
		"secure-join: vc-request\r\n" +
		"--boundary123--\r\n"

	accepted, err := IsAcceptedMessage(header, strings.NewReader(body))
	if err != nil {
		t.Fatalf("IsAcceptedMessage returned error: %v", err)
	}
	if !accepted {
		t.Fatal("expected secure-join message to be accepted")
	}
}

func TestIsAcceptedMessagePropagatesReaderErrorForEncryptedCandidate(t *testing.T) {
	t.Parallel()

	header := textproto.Header{}
	header.Set("Content-Type", "multipart/encrypted; boundary=\"b\"")

	_, err := IsAcceptedMessage(header, io.LimitReader(failReader{}, 1))
	if err == nil {
		t.Fatal("expected reader error for encrypted candidate")
	}
}

func TestIsEncryptedOpenPGPPayloadReader(t *testing.T) {
	t.Parallel()

	validPayload := []byte{
		0xC1, 0x01, 0x00, // PKESK packet (type=1), len=1
		0xD2, 0x01, 0x00, // SEIPD packet (type=18), len=1
	}

	ok, err := isEncryptedOpenPGPPayloadReader(bytes.NewReader(validPayload))
	if err != nil {
		t.Fatalf("unexpected error for valid payload: %v", err)
	}
	if !ok {
		t.Fatal("expected valid payload to pass")
	}

	invalidTrailing := append(append([]byte(nil), validPayload...), 0x00)
	ok, err = isEncryptedOpenPGPPayloadReader(bytes.NewReader(invalidTrailing))
	if err != nil {
		t.Fatalf("unexpected error for invalid trailing payload: %v", err)
	}
	if ok {
		t.Fatal("expected trailing data payload to fail")
	}
}

func TestIsValidEncryptedMessageBinaryPayload(t *testing.T) {
	t.Parallel()

	validPayload := []byte{
		0xC1, 0x01, 0x00, // PKESK packet (type=1), len=1
		0xD2, 0x01, 0x00, // SEIPD packet (type=18), len=1
	}

	var body bytes.Buffer
	w := multipart.NewWriter(&body)

	part1Header := nettextproto.MIMEHeader{}
	part1Header.Set("Content-Type", "application/pgp-encrypted")
	part1, err := w.CreatePart(part1Header)
	if err != nil {
		t.Fatalf("failed to create first part: %v", err)
	}
	if _, err := part1.Write([]byte("Version: 1")); err != nil {
		t.Fatalf("failed to write first part: %v", err)
	}

	part2Header := nettextproto.MIMEHeader{}
	part2Header.Set("Content-Type", "application/octet-stream")
	part2, err := w.CreatePart(part2Header)
	if err != nil {
		t.Fatalf("failed to create second part: %v", err)
	}
	if _, err := part2.Write(validPayload); err != nil {
		t.Fatalf("failed to write second part: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("failed to close multipart writer: %v", err)
	}

	contentType := "multipart/encrypted; boundary=\"" + w.Boundary() + "\""
	ok, err := IsValidEncryptedMessage(contentType, bytes.NewReader(body.Bytes()))
	if err != nil {
		t.Fatalf("IsValidEncryptedMessage returned error: %v", err)
	}
	if !ok {
		t.Fatal("expected multipart encrypted message to be valid")
	}
}
