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

package pgp_verify

import (
	"encoding/base64"
	"io"
	"mime"
	"mime/multipart"
	"strings"

	"github.com/emersion/go-message/textproto"
)

func IsAcceptedMessage(header textproto.Header, body io.Reader) (bool, error) {
	contentType := header.Get("Content-Type")

	// 1. Check if it's a valid PGP encrypted message
	// If it's multipart/encrypted, this will read the body.
	isEncrypted, err := IsValidEncryptedMessage(contentType, body)
	if err != nil {
		return false, err
	}
	if isEncrypted {
		return true, nil
	}

	// 2. Check for Secure Join request (header and body)
	if IsSecureJoinMessage(header, body) {
		return true, nil
	}

	return false, nil
}

func IsSecureJoinMessage(header textproto.Header, body io.Reader) bool {
	secureJoinHeader := header.Get("Secure-Join")
	contentType := header.Get("Content-Type")

	// Quick check - if header indicates secure join, allow it
	if strings.EqualFold(secureJoinHeader, "vc-request") || strings.EqualFold(secureJoinHeader, "vg-request") {
		return true
	}

	// If no header, check the body (Delta Chat sometimes relies on body content)
	if !strings.HasPrefix(strings.ToLower(contentType), "multipart/") {
		return false
	}

	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil {
		return false
	}

	if mediatype != "multipart/mixed" && mediatype != "multipart/alternative" {
		return false
	}

	// Parse multipart message to look for secure-join string
	mpr := multipart.NewReader(body, params["boundary"])
	partsCount := 0

	for {
		part, err := mpr.NextPart()
		if err == io.EOF {
			break
		}
		if err != nil {
			return false
		}

		partsCount++
		if partsCount > 2 { // Don't look too far
			break
		}

		partContentType := part.Header.Get("Content-Type")
		if !strings.HasPrefix(partContentType, "text/plain") {
			continue
		}

		partBody, err := io.ReadAll(io.LimitReader(part, 8192)) // Read up to 8KB
		if err != nil {
			continue
		}

		bodyStr := strings.ToLower(strings.TrimSpace(string(partBody)))
		if strings.Contains(bodyStr, "secure-join: vc-request") ||
			strings.Contains(bodyStr, "secure-join: vg-request") ||
			strings.Contains(bodyStr, "securejoin") {
			return true
		}
	}

	return false
}

func IsValidEncryptedMessage(contentType string, body io.Reader) (bool, error) {
	// Parse content type first - this is the primary indicator
	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil {
		return false, err
	}

	// Must be multipart/encrypted for PGP encrypted messages
	if mediatype != "multipart/encrypted" {
		return false, nil
	}

	// Parse multipart message
	mpr := multipart.NewReader(body, params["boundary"])
	partsCount := 0

	for {
		part, err := mpr.NextPart()
		if err == io.EOF {
			break
		}
		if err != nil {
			return false, err
		}

		if partsCount == 0 {
			// First part should be application/pgp-encrypted
			partContentType := part.Header.Get("Content-Type")
			if !strings.HasPrefix(partContentType, "application/pgp-encrypted") {
				return false, nil
			}

			partBody, err := io.ReadAll(part)
			if err != nil {
				return false, err
			}

			if strings.TrimSpace(string(partBody)) != "Version: 1" {
				return false, nil
			}
		} else if partsCount == 1 {
			// Second part should be application/octet-stream with PGP data
			partContentType := part.Header.Get("Content-Type")
			if !strings.HasPrefix(partContentType, "application/octet-stream") {
				return false, nil
			}

			partBody, err := io.ReadAll(part)
			if err != nil {
				return false, err
			}

			if !isValidEncryptedPayload(string(partBody)) {
				return false, nil
			}
		} else {
			// More than 2 parts is invalid
			return false, nil
		}
		partsCount++
	}

	// We found a valid multipart/encrypted structure with 2 parts
	return partsCount == 2, nil
}

func isValidEncryptedPayload(payload string) bool {
	const header = "-----BEGIN PGP MESSAGE-----\r\n\r\n"
	const footer = "-----END PGP MESSAGE-----\r\n\r\n"

	hasHeader := strings.HasPrefix(payload, header)
	hasFooter := strings.HasSuffix(payload, footer)
	if !(hasHeader && hasFooter) {
		return false
	}

	startIdx := len(header)
	crc24Start := strings.LastIndex(payload, "=")
	var endIdx int
	if crc24Start < 0 {
		endIdx = len(payload) - len(footer)
	} else {
		endIdx = crc24Start
	}

	b64Encoded := payload[startIdx:endIdx]
	b64Decoded := make([]byte, base64.StdEncoding.DecodedLen(len(b64Encoded)))
	n, err := base64.StdEncoding.Decode(b64Decoded, []byte(b64Encoded))
	if err != nil {
		return false
	}
	b64Decoded = b64Decoded[:n]

	return isEncryptedOpenPGPPayload(b64Decoded)
}

func isEncryptedOpenPGPPayload(payload []byte) bool {
	i := 0
	for i < len(payload) {
		// Permit only OpenPGP formatted binary data
		if payload[i]&0xC0 != 0xC0 {
			return false
		}
		packetTypeID := payload[i] & 0x3F
		i++

		var bodyLen int
		if i >= len(payload) {
			return false
		}

		if payload[i] < 192 {
			bodyLen = int(payload[i])
			i++
		} else if payload[i] < 224 {
			if (i + 1) >= len(payload) {
				return false
			}
			bodyLen = ((int(payload[i]) - 192) << 8) + int(payload[i+1]) + 192
			i += 2
		} else if payload[i] == 255 {
			if (i + 4) >= len(payload) {
				return false
			}
			bodyLen = (int(payload[i+1]) << 24) | (int(payload[i+2]) << 16) | (int(payload[i+3]) << 8) | int(payload[i+4])
			i += 5
		} else {
			return false
		}

		i += bodyLen
		if i == len(payload) {
			// The last packet in the stream should be
			// "Symmetrically Encrypted and Integrity Protected Data Packet (SEIDP)"
			// This is the only place in this function that is allowed to return true
			return packetTypeID == 18
		} else if packetTypeID != 1 && packetTypeID != 3 {
			return false
		}
	}
	return false
}
