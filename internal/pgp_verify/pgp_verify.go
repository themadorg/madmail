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
	"bytes"
	"encoding/base64"
	"io"
	"mime"
	"mime/multipart"
	"strings"

	"github.com/emersion/go-message/textproto"
)

func IsAcceptedMessage(header textproto.Header, body io.Reader) (bool, error) {
	contentType := header.Get("Content-Type")
	secureJoin := header.Get("Secure-Join")
	secureJoinInvitenumber := header.Get("Secure-Join-Invitenumber")

	// 1. Check for Secure Join based on headers FIRST (before consuming body)
	// The Secure-Join-Invitenumber header is the primary indicator of a secure join request
	if secureJoinInvitenumber != "" {
		return true, nil
	}

	// Check Secure-Join header values
	sjLower := strings.ToLower(strings.TrimSpace(secureJoin))
	if strings.HasPrefix(sjLower, "vc-") || strings.HasPrefix(sjLower, "vg-") {
		return true, nil
	}

	// 2. Buffer the body so we can read it multiple times
	bodyData, err := io.ReadAll(body)
	if err != nil {
		return false, err
	}

	// 3. Check if it's a valid PGP encrypted message
	isEncrypted, err := IsValidEncryptedMessage(contentType, bytes.NewReader(bodyData))
	if err != nil {
		return false, err
	}
	if isEncrypted {
		return true, nil
	}

	// 4. Check for Secure Join based on body content (re-use buffered body)
	if IsSecureJoinMessage(header, bytes.NewReader(bodyData)) {
		return true, nil
	}

	return false, nil
}

func IsSecureJoinMessage(header textproto.Header, body io.Reader) bool {
	contentType := header.Get("Content-Type")

	// Check for Secure-Join-Invitenumber header (used in initial vc-request/vg-request step)
	// This is the primary indicator of a secure join request according to securejoin.rs
	secureJoinInvitenumber := header.Get("Secure-Join-Invitenumber")
	if secureJoinInvitenumber != "" {
		return true
	}

	// Also check lowercase variant
	for f := header.FieldsByKey("Secure-Join-Invitenumber"); f.Next(); {
		if f.Value() != "" {
			return true
		}
	}
	for f := header.FieldsByKey("secure-join-invitenumber"); f.Next(); {
		if f.Value() != "" {
			return true
		}
	}

	// Check for Secure-Join header with valid handshake values
	// Valid values according to Python filtermail: vc-request, vg-request
	// And from securejoin.rs: vc-auth-required, vg-auth-required, vc-request-with-auth, vg-request-with-auth,
	// vc-contact-confirm, vg-member-added
	isSecureJoinHeaderValue := func(v string) bool {
		v = strings.ToLower(strings.TrimSpace(v))
		return strings.HasPrefix(v, "vc-") || strings.HasPrefix(v, "vg-")
	}

	checkSecureJoinHeader := func(key string) bool {
		for f := header.FieldsByKey(key); f.Next(); {
			if isSecureJoinHeaderValue(f.Value()) {
				return true
			}
		}
		return false
	}

	if checkSecureJoinHeader("Secure-Join") || checkSecureJoinHeader("secure-join") {
		return true
	}

	// Body check (only if headers are missing)
	// For multipart messages, check if body contains secure-join pattern
	// This matches Python filtermail's is_securejoin() behavior
	if !strings.HasPrefix(strings.ToLower(contentType), "multipart/") {
		// For non-multipart, check if it's text/plain with secure-join content
		if strings.HasPrefix(strings.ToLower(contentType), "text/plain") {
			if body != nil {
				bodyData, err := io.ReadAll(io.LimitReader(body, 8192))
				if err == nil {
					bodyStr := strings.TrimSpace(strings.ToLower(string(bodyData)))
					// Check for exact patterns from Python filtermail
					if bodyStr == "secure-join: vc-request" || bodyStr == "secure-join: vg-request" {
						return true
					}
				}
			}
		}
		return false
	}

	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil {
		return false
	}
	_ = mediatype // not strictly needed but good to have parsed

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
		if partsCount > 1 { // Secure join requests usually only have one part according to Python filtermail
			break
		}

		partContentType := part.Header.Get("Content-Type")
		if !strings.HasPrefix(strings.ToLower(partContentType), "text/plain") {
			continue
		}

		partBody, err := io.ReadAll(io.LimitReader(part, 8192)) // Read up to 8KB
		if err != nil {
			continue
		}

		bodyStr := strings.TrimSpace(strings.ToLower(string(partBody)))
		// Check for exact patterns from Python filtermail
		if bodyStr == "secure-join: vc-request" || bodyStr == "secure-join: vg-request" {
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
			if !strings.HasPrefix(strings.ToLower(partContentType), "application/pgp-encrypted") {
				return false, nil
			}

			partBody, err := io.ReadAll(part)
			if err != nil {
				return false, err
			}

			bodyStr := strings.TrimSpace(string(partBody))
			if bodyStr != "Version: 1" {
				return false, nil
			}
		} else if partsCount == 1 {
			// Second part should be application/octet-stream with PGP data
			partContentType := part.Header.Get("Content-Type")
			if !strings.HasPrefix(strings.ToLower(partContentType), "application/octet-stream") {
				return false, nil
			}

			partBody, err := io.ReadAll(part)
			if err != nil {
				return false, err
			}

			if !isValidEncryptedPayload(partBody) {
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

func isValidEncryptedPayload(payload []byte) bool {
	p := bytes.TrimSpace(payload)
	const header = "-----BEGIN PGP MESSAGE-----"
	const footer = "-----END PGP MESSAGE-----"

	if bytes.HasPrefix(p, []byte(header)) && bytes.HasSuffix(p, []byte(footer)) {
		// Armor case
		payloadStr := string(p)
		// Find where the base64 data starts (after the armor header and optional headers)
		// Usually there is a blank line after the headers.
		parts := strings.SplitN(payloadStr, "\n\n", 2)
		if len(parts) < 2 {
			// Try with \r\n\r\n
			parts = strings.SplitN(payloadStr, "\r\n\r\n", 2)
			if len(parts) < 2 {
				return false
			}
		}

		b64WithFooter := parts[1]
		footerIdx := strings.LastIndex(b64WithFooter, footer)
		if footerIdx < 0 {
			return false
		}

		b64Content := b64WithFooter[:footerIdx]

		// Remove CRC24 checksum line (starts with = on its own line)
		// The CRC format is: \n=XXXX or \r\n=XXXX where XXXX is 4 base64 chars
		// Following Python filtermail: payload.rpartition("=")[0] but we need to be smarter
		// to avoid cutting off base64 padding
		if crcIdx := strings.LastIndex(b64Content, "\n="); crcIdx >= 0 {
			// Found CRC line, remove it
			b64Content = b64Content[:crcIdx]
		} else if crcIdx := strings.LastIndex(b64Content, "\r\n="); crcIdx >= 0 {
			b64Content = b64Content[:crcIdx]
		}

		b64Encoded := strings.ReplaceAll(b64Content, "\n", "")
		b64Encoded = strings.ReplaceAll(b64Encoded, "\r", "")
		b64Encoded = strings.ReplaceAll(b64Encoded, " ", "")

		b64Decoded, err := base64.StdEncoding.DecodeString(b64Encoded)
		if err != nil {
			return false
		}

		return isEncryptedOpenPGPPayload(b64Decoded)
	}

	// Binary case (or invalid armor which will be rejected by isEncryptedOpenPGPPayload)
	return isEncryptedOpenPGPPayload(p)
}

// isEncryptedOpenPGPPayload checks the OpenPGP payload structure.
// Based on Python filtermail's check_openpgp_payload.
// OpenPGP payload must consist only of PKESK and SKESK packets
// terminated by a single SEIPD packet.
func isEncryptedOpenPGPPayload(payload []byte) bool {
	i := 0
	if len(payload) == 0 {
		return false
	}

	for i < len(payload) {
		// Only OpenPGP new format is allowed (0xC0 = both high bits set)
		if payload[i]&0xC0 != 0xC0 {
			return false
		}

		packetTypeID := payload[i] & 0x3F
		i++
		if i >= len(payload) {
			return false
		}

		// Handle partial body lengths first (in a loop, like Python does)
		for payload[i] >= 224 && payload[i] < 255 {
			// Partial body length
			partialLen := 1 << (payload[i] & 0x1F)
			i += 1 + partialLen
			if i >= len(payload) {
				return false
			}
		}

		// Now read the final length
		var bodyLen int
		if payload[i] < 192 {
			// One-octet length
			bodyLen = int(payload[i])
			i++
		} else if payload[i] < 224 {
			// Two-octet length
			if i+1 >= len(payload) {
				return false
			}
			bodyLen = ((int(payload[i]) - 192) << 8) + int(payload[i+1]) + 192
			i += 2
		} else if payload[i] == 255 {
			// Five-octet length
			if i+4 >= len(payload) {
				return false
			}
			bodyLen = (int(payload[i+1]) << 24) | (int(payload[i+2]) << 16) | (int(payload[i+3]) << 8) | int(payload[i+4])
			i += 5
		} else {
			// Impossible, partial body length was processed above
			return false
		}

		i += bodyLen

		if i == len(payload) {
			// Last packet should be SEIPD (Symmetrically Encrypted and Integrity Protected Data Packet)
			// This is the only place where this function may return true
			return packetTypeID == 18
		} else if packetTypeID != 1 && packetTypeID != 3 {
			// All packets except the last one must be either
			// Public-Key Encrypted Session Key Packet (PKESK = 1)
			// or Symmetric-Key Encrypted Session Key Packet (SKESK = 3)
			return false
		}
	}

	return false
}
