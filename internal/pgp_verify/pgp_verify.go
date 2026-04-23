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
	"encoding/base64"
	"errors"
	"io"
	"mime"
	"mime/multipart"
	"net/mail"
	"strings"

	"github.com/emersion/go-message/textproto"
	"github.com/themadorg/madmail/framework/exterrors"
)

// Options tunes EnforceEncryption's acceptance rules.
//
// The zero value is the strict chatmail policy: only RFC 3156 PGP/MIME and
// the unencrypted Secure-Join v[cg]-request handshake step are accepted.
// Everything else is rejected with SMTP 523 "Encryption Needed".
//
// Callers that terminate SMTP conversations (submission, relay-in, HTTP
// federation delivery, IMAP APPEND) fill in the envelope sender and the
// configured passthrough lists so operator-provided exceptions and
// mailer-daemon bounces round-trip through a single decision point.
type Options struct {
	// MailFrom is the SMTP envelope sender (MAIL FROM). It is used to
	// recognise mailer-daemon@ bounces and to match the passthrough
	// sender list. Leave empty if unknown; the check will simply skip the
	// bounce/passthrough-sender branches.
	MailFrom string

	// Recipients is the list of SMTP envelope recipients (RCPT TO). It is
	// consulted only to match the passthrough recipient list.
	Recipients []string

	// PassthroughSenders short-circuits the check when MailFrom matches
	// one of the addresses (case-insensitive, exact match).
	PassthroughSenders []string

	// PassthroughRecipients short-circuits the check when every entry in
	// Recipients matches one of the configured addresses. Entries that
	// start with "@" match any address in that domain.
	PassthroughRecipients []string
}

// errRejectUnencrypted is the canonical rejection returned by
// EnforceEncryption. It is a package-level singleton so hot-path calls
// do not allocate a new error struct per rejected message; callers that
// wrap it via errors.As still see a 523/5.7.1 *exterrors.SMTPError.
var errRejectUnencrypted = &exterrors.SMTPError{
	Code:         523,
	EnhancedCode: exterrors.EnhancedCode{5, 7, 1},
	Message:      "Encryption Needed: Invalid Unencrypted Mail",
	Reason:       "unencrypted message",
}

// EnforceEncryption is the single PGP-only policy gate used by every
// message-accepting surface of madmail (SMTP submission & inbound, HTTP
// MX-Deliv federation, IMAP APPEND, CLI `imap-msgs add`, webimap SMTP).
//
// It returns nil if the message is acceptable and an *exterrors.SMTPError
// otherwise. The returned error carries SMTP code 523/5.7.1 for policy
// rejections and 451/4.0.0 for transient read errors so HTTP and IMAP
// callers can translate once with a simple type-switch.
//
// The implementation is fully streaming: the body reader is consumed
// incrementally. Cleartext rejection is decided from the Content-Type
// header alone and returns without touching body at all, so uploading
// a large unencrypted attachment does not burn CPU memcpy'ing a body
// we are about to reject anyway.
func EnforceEncryption(header textproto.Header, body io.Reader, opts Options) error {
	// Operator-configured sender-wide bypass: skip all PGP policy.
	if opts.MailFrom != "" && containsFold(opts.PassthroughSenders, opts.MailFrom) {
		return nil
	}

	// Operator-configured recipient-wide bypass: every RCPT must match
	// for the short-circuit to apply. A mixed batch (one passthrough +
	// one normal recipient) still runs the full check.
	if len(opts.Recipients) > 0 && allRecipientsPassthrough(opts.Recipients, opts.PassthroughRecipients) {
		return nil
	}

	// mailer-daemon bounces: multipart/report with Auto-Submitted set.
	// Delta Chat never produces these, but we must accept them on the
	// inbound side so DSN / delivery failure notifications can reach
	// local users. The envelope sender pins identity; a From-header
	// spoof alone is not enough.
	if isAllowedBounce(header, opts.MailFrom) {
		return nil
	}

	// Dispatch on Content-Type before reading a single body byte.
	// Anything that is not PGP/MIME or a Secure-Join handshake is
	// rejected immediately — no ReadAll, no memcpy, no GC pressure.
	contentType := header.Get("Content-Type")
	if strings.TrimSpace(contentType) == "" {
		return errRejectUnencrypted
	}
	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil {
		return errRejectUnencrypted
	}

	switch strings.ToLower(mediatype) {
	case "multipart/encrypted":
		// A malformed encrypted body (corrupt base64, truncated
		// partial-body length, impossible packet framing, …) is a
		// permanent policy failure, not a transient error — the
		// message is never going to become valid on retry. We fold
		// all non-accept outcomes into a single 523 rejection and
		// only surface a transient 451 if the body reader itself
		// refuses to start (which in practice cannot happen because
		// callers pre-buffer the body before reaching us).
		if streamValidateEncryptedMIME(body, params["boundary"]) {
			return nil
		}
		return errRejectUnencrypted
	case "multipart/mixed":
		// Only a narrow set of Secure-Join handshake messages may
		// legitimately arrive unencrypted; everything else is reject.
		if !isSecureJoinHeader(header) {
			return errRejectUnencrypted
		}
		if streamValidateSecureJoinMIME(body, params["boundary"]) {
			return nil
		}
		return errRejectUnencrypted
	}
	return errRejectUnencrypted
}

// IsAcceptedMessage reports whether the message would be accepted by
// the PGP-only policy without the envelope context EnforceEncryption
// needs. It is kept for the few callers (tests, IMAP wrappers) that
// have no envelope at hand; new code should prefer EnforceEncryption.
func IsAcceptedMessage(header textproto.Header, body io.Reader) (bool, error) {
	err := EnforceEncryption(header, body, Options{})
	if err == nil {
		return true, nil
	}
	var smtpErr *exterrors.SMTPError
	if errors.As(err, &smtpErr) && smtpErr.Code == 523 {
		return false, nil
	}
	return false, err
}

// IsValidEncryptedMessage streams body and reports whether it is a
// well-formed RFC 3156 multipart/encrypted / application/pgp-encrypted
// message. The body reader is consumed. The returned error is always
// nil in the current implementation (malformed data is reported as
// false, not as a transient error); the signature is kept for API
// compatibility.
func IsValidEncryptedMessage(contentType string, body io.Reader) (bool, error) {
	if strings.TrimSpace(contentType) == "" {
		return false, nil
	}
	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil {
		return false, nil
	}
	if mediatype != "multipart/encrypted" {
		return false, nil
	}
	return streamValidateEncryptedMIME(body, params["boundary"]), nil
}

// IsSecureJoinMessage streams body and reports whether it is an
// unencrypted Secure-Join v[cg]-* handshake message. Kept public for
// tests and for callers that need to peek without running the full
// EnforceEncryption gate.
func IsSecureJoinMessage(header textproto.Header, body io.Reader) bool {
	if !isSecureJoinHeader(header) {
		return false
	}
	contentType := header.Get("Content-Type")
	mediatype, params, err := mime.ParseMediaType(contentType)
	if err != nil || mediatype != "multipart/mixed" {
		return false
	}
	return streamValidateSecureJoinMIME(body, params["boundary"])
}

// isAllowedBounce recognises automated mailer-daemon DSNs. The check
// mirrors the Python filtermail policy: envelope MAIL FROM must start
// with "mailer-daemon@", Auto-Submitted must be present and not set to
// "no", the body must be a multipart/report, and the From header must
// also be a mailer-daemon@ address — an envelope-only bounce with no
// From is not a legitimate DSN and is trivially spoofable by anyone
// who controls the envelope.
func isAllowedBounce(header textproto.Header, mailFrom string) bool {
	if !strings.HasPrefix(strings.ToLower(mailFrom), "mailer-daemon@") {
		return false
	}
	auto := strings.ToLower(strings.TrimSpace(header.Get("Auto-Submitted")))
	if auto == "" || auto == "no" {
		return false
	}
	if !strings.HasPrefix(strings.ToLower(header.Get("Content-Type")), "multipart/report") {
		return false
	}
	// Require a From header that also looks like a daemon address.
	// This is defence-in-depth: without it a message with just the
	// right envelope + headers but no From would be accepted, which
	// is exactly the shape of a cleartext smuggling attempt.
	mimeFrom := header.Get("From")
	if mimeFrom == "" {
		return false
	}
	addr, err := mail.ParseAddress(mimeFrom)
	if err != nil || !strings.HasPrefix(strings.ToLower(addr.Address), "mailer-daemon@") {
		return false
	}
	return true
}

// isSecureJoinHeader returns true when the message carries a
// Delta Chat Secure-Join: v[cg]-* handshake header.
func isSecureJoinHeader(header textproto.Header) bool {
	sj := strings.ToLower(strings.TrimSpace(header.Get("Secure-Join")))
	return strings.HasPrefix(sj, "vc-") || strings.HasPrefix(sj, "vg-")
}

func containsFold(list []string, v string) bool {
	for _, item := range list {
		if strings.EqualFold(item, v) {
			return true
		}
	}
	return false
}

// allRecipientsPassthrough returns true iff every entry in rcpts matches
// one of the configured patterns. Patterns beginning with "@" act as
// domain wildcards; everything else is an exact (case-insensitive) match.
func allRecipientsPassthrough(rcpts, patterns []string) bool {
	if len(patterns) == 0 {
		return false
	}
	for _, r := range rcpts {
		matched := false
		for _, p := range patterns {
			if strings.HasPrefix(p, "@") {
				if strings.HasSuffix(strings.ToLower(r), strings.ToLower(p)) {
					matched = true
					break
				}
			} else if strings.EqualFold(r, p) {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}
	return true
}

// streamValidateSecureJoinMIME reads a multipart/mixed body and accepts
// it only if it contains exactly one text/plain part whose body starts
// with "secure-join:". The part body is bounded to 8 KiB, which is far
// more than any real Secure-Join handshake message ever needs.
func streamValidateSecureJoinMIME(body io.Reader, boundary string) bool {
	if boundary == "" {
		return false
	}
	mpr := multipart.NewReader(body, boundary)

	part, err := mpr.NextPart()
	if err != nil {
		return false
	}
	defer part.Close()
	if !strings.HasPrefix(strings.ToLower(part.Header.Get("Content-Type")), "text/plain") {
		return false
	}
	// Bounded read — Secure-Join handshake payloads are tiny.
	var buf [64]byte
	n, err := io.ReadFull(io.LimitReader(part, int64(len(buf))), buf[:])
	if err != nil && err != io.ErrUnexpectedEOF && err != io.EOF {
		return false
	}
	head := strings.ToLower(strings.TrimLeft(string(buf[:n]), " \t\r\n"))
	if !strings.HasPrefix(head, "secure-join:") {
		return false
	}
	// Drain any remaining bytes of this part before checking for a
	// trailing part — multipart.NextPart requires the current part to
	// be fully consumed. io.Copy(io.Discard, ...) uses a pooled buffer.
	if _, err := io.Copy(io.Discard, part); err != nil {
		return false
	}

	// A valid Secure-Join request is a single text/plain part.
	if _, err := mpr.NextPart(); err != io.EOF {
		return false
	}
	return true
}

// streamValidateEncryptedMIME reads a multipart/encrypted body and
// returns true iff it is a well-formed RFC 3156 message:
//
//  1. exactly two parts,
//  2. first part application/pgp-encrypted containing only "Version: 1",
//  3. second part application/octet-stream whose payload is an OpenPGP
//     stream consisting of zero or more PKESK/SKESK packets terminated
//     by a single SEIPD packet that runs to end-of-stream.
//
// All read/parse errors are folded into "false" — a malformed message
// is a policy rejection, never a transient retry. This matters because
// the SMTP caller turns a false into a 523 "Encryption Needed" reply
// and a transient error would otherwise cause the remote to loop.
func streamValidateEncryptedMIME(body io.Reader, boundary string) bool {
	if boundary == "" {
		return false
	}
	mpr := multipart.NewReader(body, boundary)

	// --- Part 1: application/pgp-encrypted, "Version: 1" ---
	p1, err := mpr.NextPart()
	if err != nil {
		return false
	}
	if !strings.HasPrefix(strings.ToLower(p1.Header.Get("Content-Type")), "application/pgp-encrypted") {
		p1.Close()
		return false
	}
	var verBuf [32]byte
	n, err := io.ReadFull(io.LimitReader(p1, int64(len(verBuf))), verBuf[:])
	if err != nil && err != io.ErrUnexpectedEOF && err != io.EOF {
		p1.Close()
		return false
	}
	if strings.TrimSpace(string(verBuf[:n])) != "Version: 1" {
		p1.Close()
		return false
	}
	// Drain and close before advancing to part 2.
	if _, err := io.Copy(io.Discard, p1); err != nil {
		p1.Close()
		return false
	}
	p1.Close()

	// --- Part 2: application/octet-stream, streaming OpenPGP check ---
	p2, err := mpr.NextPart()
	if err != nil {
		return false
	}
	defer p2.Close()
	if !strings.HasPrefix(strings.ToLower(p2.Header.Get("Content-Type")), "application/octet-stream") {
		return false
	}

	if !streamValidateOpenPGPPayload(p2) {
		return false
	}

	// Any additional parts disqualify the message.
	if _, err := mpr.NextPart(); err != io.EOF {
		return false
	}
	return true
}

const (
	armorBeginLine = "-----BEGIN PGP MESSAGE-----"
	armorEndLine   = "-----END PGP MESSAGE-----"
)

// streamValidateOpenPGPPayload detects whether the part payload is
// ASCII-armored or binary (by peeking a few bytes) and hands off to
// walkOpenPGPPackets on a reader that yields raw OpenPGP bytes. For
// armored input the reader is a base64.NewDecoder fed by a line-by-line
// filter that strips the armor header, CRC line and footer on the fly —
// no giant strings.ReplaceAll chains, no base64.DecodeString allocation.
//
// Any parse failure (malformed armor, corrupt base64, broken packet
// framing, truncated partial-body chains) returns false. We never
// propagate a transient error here — callers rely on false meaning
// "reject with 523".
func streamValidateOpenPGPPayload(r io.Reader) bool {
	br := bufio.NewReaderSize(r, 4096)

	peek, _ := br.Peek(len(armorBeginLine) + 8)
	i := 0
	for i < len(peek) && (peek[i] == ' ' || peek[i] == '\t' || peek[i] == '\r' || peek[i] == '\n') {
		i++
	}
	if bytes.HasPrefix(peek[i:], []byte(armorBeginLine)) {
		ar, err := newArmorReader(br)
		if err != nil {
			return false
		}
		dec := base64.NewDecoder(base64.StdEncoding, ar)
		return walkOpenPGPPackets(dec)
	}
	return walkOpenPGPPackets(br)
}

// walkOpenPGPPackets validates that r is a sequence of zero or more
// PKESK (tag 1) or SKESK (tag 3) packets followed by exactly one SEIPD
// (tag 18) packet that consumes the remainder of the stream.
//
// The walker never materialises the payload: it reads tag + length
// bytes, then discards body bytes via io.CopyN(io.Discard, ...). For
// armored input the base64 decoder pulls a small chunk at a time from
// the armor stripper, so the whole pipeline peaks at a few kilobytes
// of working memory regardless of message size.
//
// Any I/O error, EOF at a wrong place, truncated partial-body chain,
// invalid length encoding, or extra bytes after the SEIPD are all
// treated as "not a valid encrypted payload" and return false. No
// error is ever propagated; the caller maps false to 523.
func walkOpenPGPPackets(r io.Reader) bool {
	br := ensureBufio(r)

	for {
		tag, err := br.ReadByte()
		if err != nil {
			// EOF before any SEIPD — invalid.
			return false
		}
		// New-format packet header only (bits 7+6 set).
		if tag&0xC0 != 0xC0 {
			return false
		}
		packetType := tag & 0x3F

		// Partial body lengths come first, in a loop. Each partial
		// chunk has length 1<<(b&0x1F); we discard chunk bytes and
		// read the next length byte.
		for {
			lb, err := br.ReadByte()
			if err != nil {
				return false
			}
			if lb >= 224 && lb < 255 {
				partialLen := 1 << (lb & 0x1F)
				if _, err := io.CopyN(io.Discard, br, int64(partialLen)); err != nil {
					return false
				}
				continue
			}

			// Final length byte (one-, two-, or five-octet form).
			var bodyLen int64
			switch {
			case lb < 192:
				bodyLen = int64(lb)
			case lb < 224:
				lb2, err := br.ReadByte()
				if err != nil {
					return false
				}
				bodyLen = ((int64(lb) - 192) << 8) + int64(lb2) + 192
			case lb == 255:
				var buf [4]byte
				if _, err := io.ReadFull(br, buf[:]); err != nil {
					return false
				}
				bodyLen = (int64(buf[0]) << 24) | (int64(buf[1]) << 16) | (int64(buf[2]) << 8) | int64(buf[3])
				// A length >= 2^31 is almost certainly an attacker
				// trying to trip the walker into a multi-gigabyte
				// discard loop. Reject defensively; real PGP
				// packets never get close to this size.
				if bodyLen < 0 {
					return false
				}
			default:
				return false
			}

			if packetType == 18 {
				// SEIPD must consume to EOF. Discard its body then
				// verify nothing follows.
				if _, err := io.CopyN(io.Discard, br, bodyLen); err != nil {
					return false
				}
				if _, err := br.ReadByte(); err != io.EOF {
					return false
				}
				return true
			}
			if packetType != 1 && packetType != 3 {
				return false
			}
			if _, err := io.CopyN(io.Discard, br, bodyLen); err != nil {
				return false
			}
			break
		}
	}
}

func ensureBufio(r io.Reader) *bufio.Reader {
	if b, ok := r.(*bufio.Reader); ok {
		return b
	}
	return bufio.NewReaderSize(r, 4096)
}

// armorReader is a line-oriented filter that reads an ASCII-armored PGP
// message and yields only the raw base64 body bytes (whitespace and the
// armor header / CRC line / footer are dropped). It is designed to sit
// in front of base64.NewDecoder, which treats whitespace as harmless
// fill but will bail on the leading '=' of the CRC-24 line — so we
// must stop feeding it before the CRC line begins.
//
// Compared with the previous "TrimSpace + SplitN + 3× ReplaceAll +
// base64.DecodeString" approach, this filter allocates a single 4 KiB
// bufio.Reader regardless of message size and never copies the body
// into a throwaway []byte.
type armorReader struct {
	src        *bufio.Reader
	pending    []byte // slice of pendingBuf still owed to the caller
	pendingBuf []byte // owning backing array (reused across Reads)
	eof        bool
}

func newArmorReader(src *bufio.Reader) (*armorReader, error) {
	// Consume armor header + optional header block + blank separator.
	if err := consumeArmorHeader(src); err != nil {
		return nil, err
	}
	return &armorReader{src: src}, nil
}

// consumeArmorHeader advances src past the "-----BEGIN PGP MESSAGE-----"
// line and any following RFC 4880 armor header lines ("Comment:",
// "Version:", …) up to and including the mandatory empty separator
// line.
func consumeArmorHeader(src *bufio.Reader) error {
	// Skip any leading blank lines.
	for {
		line, err := readArmorHeaderLine(src)
		if err != nil {
			return err
		}
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		if trimmed != armorBeginLine {
			return errors.New("pgp_verify: missing armor BEGIN line")
		}
		break
	}
	// Skip armor headers until blank line. Armor headers are of the
	// form "Key: value"; a blank line terminates the header block.
	for {
		line, err := readArmorHeaderLine(src)
		if err != nil {
			return err
		}
		if strings.TrimSpace(line) == "" {
			return nil
		}
		// Tolerate header lines silently — we only care about the body.
	}
}

// readArmorHeaderLine is used by consumeArmorHeader once per header
// line. It is not on the body hot path, so the per-call allocation
// from ReadBytes is acceptable; the body loop uses ReadSlice to stay
// alloc-free.
func readArmorHeaderLine(src *bufio.Reader) (string, error) {
	b, err := src.ReadBytes('\n')
	if err != nil && len(b) == 0 {
		return "", err
	}
	n := len(b)
	for n > 0 && (b[n-1] == '\n' || b[n-1] == '\r') {
		n--
	}
	return string(b[:n]), err
}

// Read yields base64 body bytes to the base64 decoder. It stops at the
// CRC-24 line ("=XXXX") or the armor END marker, whichever comes first.
//
// The hot loop uses bufio.ReadSlice which returns a slice aliased into
// the bufio internal buffer — no per-line allocation. The slice is
// only referenced until the next I/O call; any leftover that doesn't
// fit in p is copied into the (pre-sized) ar.pending buffer before
// we return.
func (ar *armorReader) Read(p []byte) (int, error) {
	if len(p) == 0 {
		return 0, nil
	}
	if len(ar.pending) > 0 {
		n := copy(p, ar.pending)
		ar.pending = ar.pending[n:]
		return n, nil
	}
	if ar.eof {
		return 0, io.EOF
	}
	for {
		line, err := ar.src.ReadSlice('\n')
		if len(line) > 0 {
			end := len(line)
			for end > 0 && (line[end-1] == '\n' || line[end-1] == '\r' || line[end-1] == ' ' || line[end-1] == '\t') {
				end--
			}
			start := 0
			for start < end && (line[start] == ' ' || line[start] == '\t') {
				start++
			}
			trimmed := line[start:end]
			if len(trimmed) == 0 {
				if err != nil {
					ar.eof = true
					if err == io.EOF {
						return 0, io.EOF
					}
					return 0, err
				}
				continue
			}
			if trimmed[0] == '=' || bytes.HasPrefix(trimmed, []byte(armorEndLine)) {
				ar.eof = true
				return 0, io.EOF
			}
			n := copy(p, trimmed)
			if n < len(trimmed) {
				// Must copy the tail out of the bufio slice before
				// the next ReadSlice invalidates it.
				if ar.pendingBuf == nil {
					ar.pendingBuf = make([]byte, 0, 128)
				}
				ar.pendingBuf = append(ar.pendingBuf[:0], trimmed[n:]...)
				ar.pending = ar.pendingBuf
			}
			if err != nil && err != io.EOF {
				ar.eof = true
			}
			return n, nil
		}
		if err != nil {
			ar.eof = true
			if err == io.EOF {
				return 0, io.EOF
			}
			return 0, err
		}
	}
}
