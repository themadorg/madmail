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
	"bufio"
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"mime"
	"mime/multipart"
	"strings"

	"github.com/emersion/go-message/textproto"
	"golang.org/x/crypto/openpgp/armor"
)

func IsAcceptedMessage(header textproto.Header, body io.Reader) (bool, error) {
	contentType := header.Get("Content-Type")

	// Check if it's a valid PGP encrypted message
	isEncrypted, err := IsValidEncryptedMessage(contentType, body)
	if err != nil {
		return false, err
	}
	if isEncrypted {
		return true, nil
	}

	// Check for Secure Join based on body content. This is safe because
	// IsValidEncryptedMessage returns without consuming body for non-encrypted
	// content types.
	if IsSecureJoinMessage(header, body) {
		return true, nil
	}

	return false, nil
}

func IsSecureJoinMessage(header textproto.Header, body io.Reader) bool {
	secureJoinHeader := header.Get("Secure-Join")
	contentType := header.Get("Content-Type")

	// Allow any vc-* or vg-* step as these are part of the unencrypted handshake
	secureJoinHeader = strings.ToLower(strings.TrimSpace(secureJoinHeader))
	if !strings.HasPrefix(secureJoinHeader, "vc-") &&
		!strings.HasPrefix(secureJoinHeader, "vg-") {
		return false
	}

	// Check content type for multipart/
	if !strings.HasPrefix(strings.ToLower(contentType), "multipart/") {
		// If it's not multipart but has the header, we might still want to check
		// but Delta Chat usually sends multipart for Secure Join.
		// For now, let's keep the multipart requirement but be more permissive with parts.
		return false
	}
	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil {
		return false
	}

	// Accept multipart/mixed or multipart/alternative (handshake might vary)
	if mediatype != "multipart/mixed" && mediatype != "multipart/alternative" {
		return false
	}

	// Parse multipart message
	mpr := multipart.NewReader(body, params["boundary"])
	partsFound := 0

	for {
		part, err := mpr.NextPart()
		if err == io.EOF {
			break
		}
		if err != nil {
			return false
		}

		partsFound++
		// Only check the first part for the secure-join indicator
		if partsFound == 1 {
			partContentType := part.Header.Get("Content-Type")
			if !strings.HasPrefix(strings.ToLower(partContentType), "text/plain") {
				return false
			}

			partBody, err := io.ReadAll(io.LimitReader(part, 8192)) // Read up to 8KB
			if err != nil {
				return false
			}

			bodyStr := strings.ToLower(strings.TrimSpace(string(partBody)))
			// Ensure body contains the secure-join indicator
			if !strings.HasPrefix(bodyStr, "secure-join:") {
				return false
			}
		}
	}

	return partsFound >= 1
}

func IsValidEncryptedMessage(contentType string, body io.Reader) (bool, error) {
	const maxControlPartSize = 4096

	// Parse content type first - this is the primary indicator
	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil {
		return false, nil
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

			partBody, err := io.ReadAll(io.LimitReader(part, maxControlPartSize+1))
			if err != nil {
				return false, err
			}
			if len(partBody) > maxControlPartSize {
				return false, nil
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

			isValidPayload, err := isValidEncryptedPayload(part)
			if err != nil {
				return false, err
			}
			if !isValidPayload {
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

func isValidEncryptedPayload(payload io.Reader) (bool, error) {
	const armoredHeader = "-----BEGIN PGP MESSAGE-----"

	br := bufio.NewReader(payload)
	for {
		b, err := br.ReadByte()
		if err == io.EOF {
			return false, nil
		}
		if err != nil {
			return false, err
		}
		switch b {
		case ' ', '\t', '\r', '\n':
			continue
		default:
			if err := br.UnreadByte(); err != nil {
				return false, err
			}
			goto checkFormat
		}
	}

checkFormat:
	if peek, err := br.Peek(len(armoredHeader)); err == nil && bytes.Equal(peek, []byte(armoredHeader)) {
		block, err := armor.Decode(br)
		if err != nil || block == nil || block.Type != "PGP MESSAGE" {
			return false, nil
		}
		return isEncryptedOpenPGPPayloadReader(block.Body)
	}

	return isEncryptedOpenPGPPayloadReader(br)
}

// isEncryptedOpenPGPPayload checks the OpenPGP payload structure.
// Based on Python filtermail's check_openpgp_payload.
// OpenPGP payload must consist only of PKESK and SKESK packets
// terminated by a single SEIPD packet.
func isEncryptedOpenPGPPayload(payload []byte) bool {
	ok, err := isEncryptedOpenPGPPayloadReader(bytes.NewReader(payload))
	return err == nil && ok
}

func isEncryptedOpenPGPPayloadReader(payload io.Reader) (bool, error) {
	br := bufio.NewReader(payload)
	seenSEIPD := false

	for {
		packetHeader, err := br.ReadByte()
		if err == io.EOF {
			return seenSEIPD, nil
		}
		if err != nil {
			return false, err
		}

		// Only OpenPGP new format is allowed (0xC0 = both high bits set)
		if packetHeader&0xC0 != 0xC0 {
			return false, nil
		}

		packetTypeID := packetHeader & 0x3F
		if packetTypeID != 1 && packetTypeID != 3 && packetTypeID != 18 {
			return false, nil
		}
		if seenSEIPD {
			return false, nil
		}

		bodyLen, err := readOpenPGPPacketLength(br)
		if err != nil {
			if errors.Is(err, errPartialBodyLength) {
				return false, nil
			}
			return false, err
		}

		if _, err := io.CopyN(io.Discard, br, int64(bodyLen)); err != nil {
			if errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) {
				return false, nil
			}
			return false, err
		}

		if packetTypeID == 18 {
			seenSEIPD = true
		}
	}
}

var errPartialBodyLength = errors.New("openpgp partial body length is unsupported")

func readOpenPGPPacketLength(br *bufio.Reader) (uint32, error) {
	first, err := br.ReadByte()
	if err != nil {
		return 0, err
	}

	switch {
	case first < 192:
		return uint32(first), nil
	case first >= 192 && first <= 223:
		second, err := br.ReadByte()
		if err != nil {
			return 0, err
		}
		return uint32(first-192)<<8 + uint32(second) + 192, nil
	case first == 255:
		var v [4]byte
		if _, err := io.ReadFull(br, v[:]); err != nil {
			return 0, err
		}
		return binary.BigEndian.Uint32(v[:]), nil
	default:
		// 224-254 are partial body lengths. We don't support them because
		// they are uncommon in this use-case and complicate strict validation.
		return 0, errPartialBodyLength
	}
}
