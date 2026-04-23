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
	"errors"
	"fmt"
	"strings"
	"testing"

	"github.com/emersion/go-message/textproto"
	"github.com/themadorg/madmail/framework/exterrors"
)

// ── OpenPGP packet framing helpers ─────────────────────────────────

// pkt emits a new-format OpenPGP packet: tag byte (0b11xxxxxx with
// packetType in the low 6 bits) + a 5-octet body length + body.
func pkt(packetType byte, body []byte) []byte {
	var out []byte
	out = append(out, 0xC0|(packetType&0x3F))
	out = append(out, 0xFF)
	n := len(body)
	out = append(out,
		byte(n>>24), byte(n>>16), byte(n>>8), byte(n),
	)
	out = append(out, body...)
	return out
}

// pktOneOctet emits a packet with a one-octet length (for bodies < 192).
func pktOneOctet(packetType byte, body []byte) []byte {
	if len(body) >= 192 {
		panic("one-octet length only valid for body < 192 bytes")
	}
	out := []byte{0xC0 | (packetType & 0x3F), byte(len(body))}
	out = append(out, body...)
	return out
}

// pktPartial emits a packet whose body is transmitted in partial
// chunks. chunkPowers gives the partial exponents (each chunk is
// 1<<exp bytes); the caller guarantees sum(1<<exp) + finalLen ==
// len(body). Used to stress the partial-body-length walker branch.
func pktPartial(packetType byte, chunkPowers []int, body []byte) []byte {
	out := []byte{0xC0 | (packetType & 0x3F)}
	pos := 0
	for _, exp := range chunkPowers {
		clen := 1 << exp
		if pos+clen > len(body) {
			panic("partial chunk overruns body")
		}
		out = append(out, 224|byte(exp))
		out = append(out, body[pos:pos+clen]...)
		pos += clen
	}
	final := len(body) - pos
	if final >= 192 {
		panic("final partial remainder must be < 192 for this helper")
	}
	out = append(out, byte(final))
	out = append(out, body[pos:]...)
	return out
}

// armored wraps a binary OpenPGP payload in ASCII armor with the
// standard RFC 4880 framing (BEGIN line, empty header block, 64-col
// base64 body, CRC-24 line, END line).
func armored(payload []byte) []byte {
	b64 := base64.StdEncoding.EncodeToString(payload)
	var buf bytes.Buffer
	buf.WriteString("-----BEGIN PGP MESSAGE-----\r\n\r\n")
	for i := 0; i < len(b64); i += 64 {
		end := i + 64
		if end > len(b64) {
			end = len(b64)
		}
		buf.WriteString(b64[i:end])
		buf.WriteString("\r\n")
	}
	buf.WriteString("=AAAA\r\n-----END PGP MESSAGE-----\r\n")
	return buf.Bytes()
}

// pgpMIME wraps a PGP payload (binary or armored) in an RFC 3156
// multipart/encrypted envelope.
func pgpMIME(payload []byte) (boundary string, ct string, body []byte) {
	boundary = "test-bnd"
	ct = fmt.Sprintf(`multipart/encrypted; protocol="application/pgp-encrypted"; boundary=%q`, boundary)
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/octet-stream\r\n\r\n")
	buf.Write(payload)
	fmt.Fprintf(&buf, "\r\n--%s--\r\n", boundary)
	return boundary, ct, buf.Bytes()
}

// validPGPPayload returns a minimal but well-formed OpenPGP payload:
// one PKESK packet followed by a SEIPD that consumes to EOF. Used as
// the "known-good" baseline — every test that tampers with this should
// turn accept into reject.
func validPGPPayload() []byte {
	pkesk := pkt(1, bytes.Repeat([]byte{0xAA}, 96))
	seipd := pkt(18, bytes.Repeat([]byte{0xBB}, 128))
	return append(pkesk, seipd...)
}

// assertRejected fails the test if err is not a 523 rejection.
func assertRejected(t *testing.T, err error) {
	t.Helper()
	if err == nil {
		t.Fatal("expected rejection, got accepted")
	}
	var smtpErr *exterrors.SMTPError
	if !errors.As(err, &smtpErr) {
		t.Fatalf("expected *exterrors.SMTPError, got %T: %v", err, err)
	}
	if smtpErr.Code != 523 {
		t.Fatalf("expected SMTP 523, got %d (%v)", smtpErr.Code, err)
	}
}

// assertAccepted fails the test if err is non-nil.
func assertAccepted(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatalf("expected accepted, got: %v", err)
	}
}

func mkHeader(fields map[string]string) textproto.Header {
	h := textproto.Header{}
	for k, v := range fields {
		h.Set(k, v)
	}
	return h
}

// runEnforce enforces the policy with zero envelope context (the mode
// used by IMAP APPEND / CLI tools). The submission/MX-Deliv callers
// pass MailFrom/Recipients, which is covered by the passthrough and
// bounce test groups further down.
func runEnforce(ct string, body []byte) error {
	return EnforceEncryption(
		mkHeader(map[string]string{"Content-Type": ct}),
		bytes.NewReader(body),
		Options{},
	)
}

// ── Baseline sanity: the happy path accepts ────────────────────────

func TestAdversarial_Baseline_ValidPGPMIMEAccepted(t *testing.T) {
	_, ct, body := pgpMIME(validPGPPayload())
	assertAccepted(t, runEnforce(ct, body))

	_, ct, body = pgpMIME(armored(validPGPPayload()))
	assertAccepted(t, runEnforce(ct, body))
}

// ── Category A: OpenPGP packet-stream tampering ───────────────────

func TestAdversarial_TrailingPlaintextAfterSEIPD(t *testing.T) {
	// SEIPD is supposed to be the terminal packet and consume the
	// stream to EOF. Tack cleartext after the SEIPD body and the
	// walker must notice the extra byte.
	payload := append(validPGPPayload(), []byte("TRAILING PLAINTEXT SMUGGLE")...)
	_, ct, body := pgpMIME(payload)
	assertRejected(t, runEnforce(ct, body))

	_, ct, body = pgpMIME(armored(payload))
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_LeadingPlaintextBeforePKESK(t *testing.T) {
	// Anything that is not a new-format packet header byte before the
	// first PKESK fails the (tag & 0xC0 == 0xC0) check.
	payload := append([]byte("LEADING PLAINTEXT"), validPGPPayload()...)
	_, ct, body := pgpMIME(payload)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_SEIPDLengthShorterThanRemaining(t *testing.T) {
	// SEIPD claims to be 16 bytes but we append 32 bytes of body.
	// Walker reads the declared 16, then ReadByte() != EOF → reject.
	real := bytes.Repeat([]byte{0xBB}, 32)
	var seipd []byte
	seipd = append(seipd, 0xC0|18)
	seipd = append(seipd, 0xFF, 0, 0, 0, 16) // claim 16
	seipd = append(seipd, real...)           // actually 32
	_, ct, body := pgpMIME(seipd)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_SEIPDLengthLongerThanRemaining(t *testing.T) {
	// SEIPD claims 1024 bytes but only 16 are present. CopyN gets
	// ErrUnexpectedEOF → walker returns "not valid" → reject (523).
	var seipd []byte
	seipd = append(seipd, 0xC0|18)
	seipd = append(seipd, 0xFF, 0, 0, 4, 0) // claim 1024
	seipd = append(seipd, bytes.Repeat([]byte{0xBB}, 16)...)
	_, ct, body := pgpMIME(seipd)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_NoSEIPD_OnlyPKESK(t *testing.T) {
	// Two PKESKs with no SEIPD terminator.
	payload := append(pkt(1, bytes.Repeat([]byte{0xAA}, 32)),
		pkt(1, bytes.Repeat([]byte{0xAA}, 32))...)
	_, ct, body := pgpMIME(payload)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_TwoSEIPDs(t *testing.T) {
	// Two SEIPDs back-to-back: the walker must accept the first and
	// then notice there is more data after SEIPD consumed its body.
	payload := append(pkt(18, bytes.Repeat([]byte{0xBB}, 32)),
		pkt(18, bytes.Repeat([]byte{0xBB}, 32))...)
	_, ct, body := pgpMIME(payload)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_PKESKAfterSEIPD(t *testing.T) {
	payload := append(pkt(18, bytes.Repeat([]byte{0xBB}, 32)),
		pkt(1, bytes.Repeat([]byte{0xAA}, 32))...)
	_, ct, body := pgpMIME(payload)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_EmptyPayload(t *testing.T) {
	_, ct, body := pgpMIME(nil)
	assertRejected(t, runEnforce(ct, body))
	_, ct, body = pgpMIME([]byte{})
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_OldFormatPacketTag(t *testing.T) {
	// Old-format: top bit set, second bit CLEAR (tag & 0xC0 == 0x80).
	// RFC 4880 permits old-format, but our validator hard-rejects them
	// to keep the parser conservative — test documents that rule.
	oldFmt := []byte{
		0x90,       // old-format tag 4 (one-octet length)
		0x04,       // len=4
		1, 2, 3, 4, // body
	}
	oldFmt = append(oldFmt, pkt(18, bytes.Repeat([]byte{0xBB}, 32))...)
	_, ct, body := pgpMIME(oldFmt)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_ForbiddenPacketTypeBeforeSEIPD(t *testing.T) {
	// Literal data (tag 11) before SEIPD would let plaintext through
	// if the walker did not whitelist PKESK/SKESK.
	payload := append(pkt(11, []byte("HIDDEN PLAINTEXT PAYLOAD")),
		pkt(18, bytes.Repeat([]byte{0xBB}, 32))...)
	_, ct, body := pgpMIME(payload)
	assertRejected(t, runEnforce(ct, body))

	// Compressed data (tag 8) — same reasoning.
	payload = append(pkt(8, []byte("compressed junk")),
		pkt(18, bytes.Repeat([]byte{0xBB}, 32))...)
	_, ct, body = pgpMIME(payload)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_SKESKIsAccepted(t *testing.T) {
	// SKESK (tag 3) is a legitimate session-key packet type, so a
	// SKESK+SEIPD payload must be accepted.
	payload := append(pkt(3, bytes.Repeat([]byte{0x55}, 24)),
		pkt(18, bytes.Repeat([]byte{0xBB}, 32))...)
	_, ct, body := pgpMIME(payload)
	assertAccepted(t, runEnforce(ct, body))
}

func TestAdversarial_PartialBodyPKESKAccepted(t *testing.T) {
	// PKESK carried as partial body chunks (2 × 32 bytes + 8-byte
	// final remainder) plus a normal SEIPD. Exercises the partial-
	// body-length loop.
	pkBody := bytes.Repeat([]byte{0xAA}, 64+8)
	pkesk := pktPartial(1, []int{5, 5}, pkBody) // 32 + 32 + final 8
	payload := append(pkesk, pkt(18, bytes.Repeat([]byte{0xBB}, 32))...)
	_, ct, body := pgpMIME(payload)
	assertAccepted(t, runEnforce(ct, body))
}

func TestAdversarial_PartialBodyTruncated(t *testing.T) {
	// Partial chunk claims 64 bytes but stream only carries 16 before
	// the SEIPD header shows up. CopyN returns ErrUnexpectedEOF.
	var mal []byte
	mal = append(mal, 0xC0|1) // PKESK tag
	mal = append(mal, 224|6)  // partial chunk of 2^6 = 64 bytes
	mal = append(mal, bytes.Repeat([]byte{0xAA}, 16)...)
	mal = append(mal, pkt(18, bytes.Repeat([]byte{0xBB}, 16))...)
	_, ct, body := pgpMIME(mal)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_ReservedLengthByte255Mid(t *testing.T) {
	// 0xFF must introduce a 5-octet length. Before my walker fix, the
	// inner loop interpreted a bare 0xFF as both "end of partial
	// chain" AND "five-octet length prefix" depending on ordering. A
	// length byte equal to 0xFF immediately after a packet tag must
	// trigger the five-octet path, not the partial-body path.
	var mal []byte
	mal = append(mal, 0xC0|1)                          // PKESK
	mal = append(mal, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF)    // len = 4GB-ish
	mal = append(mal, bytes.Repeat([]byte{0xAA}, 8)...) // short body
	_, ct, body := pgpMIME(mal)
	assertRejected(t, runEnforce(ct, body))
}

// ── Category B: ASCII-armor tampering ──────────────────────────────

func TestAdversarial_ArmorGarbageBeforeBegin(t *testing.T) {
	// Non-blank lines before "-----BEGIN PGP MESSAGE-----" must
	// abort armor parsing → reject.
	arm := append([]byte("this is definitely not a PGP message\r\n"), armored(validPGPPayload())...)
	_, ct, body := pgpMIME(arm)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_ArmorTextAfterEnd(t *testing.T) {
	// Trailing content after END armor. Our walker stops feeding the
	// decoder at END, so the extra text should be harmless — but it
	// must not cause an accept/transient-error split by accident.
	arm := append(armored(validPGPPayload()), []byte("\r\ncleartext trailer\r\n")...)
	_, ct, body := pgpMIME(arm)
	// Extra text inside the part but outside the armor is ignored by
	// design: the octet-stream part body contains one armored block
	// that validates; whatever the part reader hands us after END
	// doesn't feed into the walker. This is the same behaviour Python
	// filtermail has.
	assertAccepted(t, runEnforce(ct, body))
}

func TestAdversarial_ArmorMissingEnd(t *testing.T) {
	// Armor BEGIN + body + CRC but no END. The armor reader stops at
	// the CRC line anyway, so the walker sees the valid packet stream
	// and returns true. Document that behaviour.
	b64 := base64.StdEncoding.EncodeToString(validPGPPayload())
	var buf bytes.Buffer
	buf.WriteString("-----BEGIN PGP MESSAGE-----\r\n\r\n")
	for i := 0; i < len(b64); i += 64 {
		end := i + 64
		if end > len(b64) {
			end = len(b64)
		}
		buf.WriteString(b64[i:end])
		buf.WriteString("\r\n")
	}
	buf.WriteString("=AAAA\r\n") // CRC but no END marker
	_, ct, body := pgpMIME(buf.Bytes())
	assertAccepted(t, runEnforce(ct, body))
}

func TestAdversarial_ArmorMissingBegin(t *testing.T) {
	// Payload looks armored (base64 + END) but lacks BEGIN. Armor
	// reader requires BEGIN as the first non-blank line.
	b64 := base64.StdEncoding.EncodeToString(validPGPPayload())
	var buf bytes.Buffer
	buf.WriteString(b64)
	buf.WriteString("\r\n=AAAA\r\n-----END PGP MESSAGE-----\r\n")
	// Since it doesn't start with "-----BEGIN PGP MESSAGE-----", the
	// walker takes the binary branch and sees base64 ASCII — the
	// first byte 0x?? does not have the new-format tag bits set, so
	// it rejects.
	_, ct, body := pgpMIME(buf.Bytes())
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_ArmorCorruptBase64(t *testing.T) {
	// Base64 with invalid characters must not be transient-classified
	// (a malformed body is a permanent reject, not a 451 retry).
	var buf bytes.Buffer
	buf.WriteString("-----BEGIN PGP MESSAGE-----\r\n\r\n")
	buf.WriteString("!!!not$$base64***\r\n")
	buf.WriteString("=AAAA\r\n-----END PGP MESSAGE-----\r\n")
	_, ct, body := pgpMIME(buf.Bytes())
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_ArmorFakeBeginInsideBody(t *testing.T) {
	// A base64 line that happens to contain the armor BEGIN string
	// doesn't matter — the decoder treats it as opaque base64 (most
	// of the line is valid base64 chars apart from '-' which base64
	// will skip or fail on). Either way, reject.
	var buf bytes.Buffer
	buf.WriteString("-----BEGIN PGP MESSAGE-----\r\n\r\n")
	buf.WriteString("-----BEGIN PGP MESSAGE-----\r\n") // injected
	buf.WriteString(base64.StdEncoding.EncodeToString(validPGPPayload()))
	buf.WriteString("\r\n=AAAA\r\n-----END PGP MESSAGE-----\r\n")
	_, ct, body := pgpMIME(buf.Bytes())
	// Either decoder rejects ('-' not in alphabet) or walker sees
	// garbled packets. We just require the answer to be "reject".
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_ArmorCRCLineOnly(t *testing.T) {
	// BEGIN + blank + CRC + END with no body. The armor reader hits
	// CRC immediately and returns EOF to the decoder, walker sees
	// empty stream → reject.
	var buf bytes.Buffer
	buf.WriteString("-----BEGIN PGP MESSAGE-----\r\n\r\n=AAAA\r\n-----END PGP MESSAGE-----\r\n")
	_, ct, body := pgpMIME(buf.Bytes())
	assertRejected(t, runEnforce(ct, body))
}

// ── Category C: Content-Type / MIME tricks ────────────────────────

func TestAdversarial_CTMissing(t *testing.T) {
	err := EnforceEncryption(textproto.Header{}, bytes.NewReader([]byte("anything")), Options{})
	assertRejected(t, err)
}

func TestAdversarial_CTMalformed(t *testing.T) {
	err := EnforceEncryption(
		mkHeader(map[string]string{"Content-Type": "this is not a Content-Type"}),
		bytes.NewReader([]byte("anything")),
		Options{},
	)
	assertRejected(t, err)
}

func TestAdversarial_CTMultipartEncryptedNoBoundary(t *testing.T) {
	err := runEnforce("multipart/encrypted", []byte("anything"))
	assertRejected(t, err)
}

func TestAdversarial_CTTextPlainWithPGPInside(t *testing.T) {
	// A text/plain body that *contains* a PGP-armored message should
	// NOT be accepted: the policy requires the MIME wrapper to be
	// multipart/encrypted. This is critical — if the check were
	// content-based instead of structure-based, a Delta Chat client
	// would not decrypt it and the user would see cleartext.
	err := runEnforce("text/plain", armored(validPGPPayload()))
	assertRejected(t, err)
}

func TestAdversarial_MIMEThreeParts(t *testing.T) {
	// Valid PGP/MIME followed by an extra part. The multipart reader
	// will find a third boundary → reject.
	boundary := "tbnd"
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/octet-stream\r\n\r\n")
	buf.Write(validPGPPayload())
	fmt.Fprintf(&buf, "\r\n--%s\r\n", boundary)
	buf.WriteString("Content-Type: text/plain\r\n\r\nSmuggled!\r\n")
	fmt.Fprintf(&buf, "--%s--\r\n", boundary)
	ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
	assertRejected(t, runEnforce(ct, buf.Bytes()))
}

func TestAdversarial_MIMEFirstPartWrongType(t *testing.T) {
	boundary := "tbnd"
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: text/plain\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/octet-stream\r\n\r\n")
	buf.Write(validPGPPayload())
	fmt.Fprintf(&buf, "\r\n--%s--\r\n", boundary)
	ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
	assertRejected(t, runEnforce(ct, buf.Bytes()))
}

func TestAdversarial_MIMEFirstPartWrongVersion(t *testing.T) {
	boundary := "tbnd"
	for _, v := range []string{"Version: 2", "Version: 1.0", "Version:1", "", "arbitrary stuff"} {
		var buf bytes.Buffer
		fmt.Fprintf(&buf, "--%s\r\n", boundary)
		buf.WriteString("Content-Type: application/pgp-encrypted\r\n\r\n" + v + "\r\n")
		fmt.Fprintf(&buf, "--%s\r\n", boundary)
		buf.WriteString("Content-Type: application/octet-stream\r\n\r\n")
		buf.Write(validPGPPayload())
		fmt.Fprintf(&buf, "\r\n--%s--\r\n", boundary)
		ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
		assertRejected(t, runEnforce(ct, buf.Bytes()))
	}
}

func TestAdversarial_MIMESecondPartWrongType(t *testing.T) {
	boundary := "tbnd"
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: text/plain\r\n\r\n") // wrong type!
	buf.Write(validPGPPayload())
	fmt.Fprintf(&buf, "\r\n--%s--\r\n", boundary)
	ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
	assertRejected(t, runEnforce(ct, buf.Bytes()))
}

func TestAdversarial_MIMEOnlyOnePart(t *testing.T) {
	boundary := "tbnd"
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&buf, "--%s--\r\n", boundary)
	ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
	assertRejected(t, runEnforce(ct, buf.Bytes()))
}

// ── Category D: Secure-Join tricks ─────────────────────────────────

func sjBody(content string) string {
	return "--bnd\r\nContent-Type: text/plain\r\n\r\n" + content + "\r\n--bnd--\r\n"
}

func TestAdversarial_SecureJoinCaseInsensitive(t *testing.T) {
	h := mkHeader(map[string]string{
		"Secure-Join":  "VC-REQUEST",
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	assertAccepted(t, EnforceEncryption(h, strings.NewReader(sjBody("SECURE-JOIN: vc-request")), Options{}))
}

func TestAdversarial_SecureJoinLeadingWhitespace(t *testing.T) {
	h := mkHeader(map[string]string{
		"Secure-Join":  "vc-request",
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	assertAccepted(t, EnforceEncryption(h, strings.NewReader(sjBody("   \tsecure-join: vc-request")), Options{}))
}

func TestAdversarial_SecureJoinBogusPrefix(t *testing.T) {
	h := mkHeader(map[string]string{
		"Secure-Join":  "vc-request",
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	// Prefixed with something else — must reject.
	assertRejected(t, EnforceEncryption(h, strings.NewReader(sjBody("Hi friend! secure-join: vc-request")), Options{}))
}

func TestAdversarial_SecureJoinNoHeaderButMixedBody(t *testing.T) {
	// multipart/mixed with a secure-join body but NO Secure-Join
	// header → reject.
	h := mkHeader(map[string]string{
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	assertRejected(t, EnforceEncryption(h, strings.NewReader(sjBody("secure-join: vc-request")), Options{}))
}

func TestAdversarial_SecureJoinHeaderButNotMultipartMixed(t *testing.T) {
	// text/plain with Secure-Join header must reject — the handshake
	// is only legit inside multipart/mixed.
	h := mkHeader(map[string]string{
		"Secure-Join":  "vc-request",
		"Content-Type": "text/plain",
	})
	assertRejected(t, EnforceEncryption(h, strings.NewReader("secure-join: vc-request"), Options{}))
}

func TestAdversarial_SecureJoinWrongHeaderValue(t *testing.T) {
	for _, bad := range []string{"xx-request", "v-request", "", "foo"} {
		h := mkHeader(map[string]string{
			"Secure-Join":  bad,
			"Content-Type": `multipart/mixed; boundary="bnd"`,
		})
		assertRejected(t, EnforceEncryption(h, strings.NewReader(sjBody("secure-join: vc-request")), Options{}))
	}
}

func TestAdversarial_SecureJoinHTMLPart(t *testing.T) {
	h := mkHeader(map[string]string{
		"Secure-Join":  "vc-request",
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	body := "--bnd\r\nContent-Type: text/html\r\n\r\n<p>secure-join: vc-request</p>\r\n--bnd--\r\n"
	assertRejected(t, EnforceEncryption(h, strings.NewReader(body), Options{}))
}

func TestAdversarial_SecureJoinMultipleParts(t *testing.T) {
	h := mkHeader(map[string]string{
		"Secure-Join":  "vc-request",
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	body := "--bnd\r\nContent-Type: text/plain\r\n\r\nsecure-join: vc-request\r\n" +
		"--bnd\r\nContent-Type: text/plain\r\n\r\nsmuggled plaintext\r\n--bnd--\r\n"
	assertRejected(t, EnforceEncryption(h, strings.NewReader(body), Options{}))
}

// ── Category E: Bounce spoofing ────────────────────────────────────

func TestAdversarial_BounceRequiresFromHeader(t *testing.T) {
	// Envelope MAIL FROM claims mailer-daemon and Auto-Submitted is
	// set, but the MIME From header is missing. A bounce without a
	// From header is not a legitimate DSN — reject.
	h := mkHeader(map[string]string{
		"Auto-Submitted": "auto-replied",
		"Content-Type":   `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(anything)"), Options{
		MailFrom: "mailer-daemon@example.org",
	})
	assertRejected(t, err)
}

func TestAdversarial_BounceFromNullAddress(t *testing.T) {
	// From: <> is common in bounces. net/mail will reject it as not
	// parseable; our check then rejects the bounce altogether.
	h := mkHeader(map[string]string{
		"From":           "<>",
		"Auto-Submitted": "auto-replied",
		"Content-Type":   `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(anything)"), Options{
		MailFrom: "mailer-daemon@example.org",
	})
	assertRejected(t, err)
}

func TestAdversarial_BounceAutoSubmittedNo(t *testing.T) {
	h := mkHeader(map[string]string{
		"From":           "mailer-daemon@example.org",
		"Auto-Submitted": "no",
		"Content-Type":   `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(anything)"), Options{
		MailFrom: "mailer-daemon@example.org",
	})
	assertRejected(t, err)
}

func TestAdversarial_BounceAutoSubmittedMissing(t *testing.T) {
	h := mkHeader(map[string]string{
		"From":         "mailer-daemon@example.org",
		"Content-Type": `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(anything)"), Options{
		MailFrom: "mailer-daemon@example.org",
	})
	assertRejected(t, err)
}

func TestAdversarial_BounceWrongContentType(t *testing.T) {
	// Claims to be a bounce but is multipart/mixed (not /report).
	// Our policy requires multipart/report for DSNs.
	h := mkHeader(map[string]string{
		"From":           "mailer-daemon@example.org",
		"Auto-Submitted": "auto-replied",
		"Content-Type":   `multipart/mixed; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(anything)"), Options{
		MailFrom: "mailer-daemon@example.org",
	})
	assertRejected(t, err)
}

func TestAdversarial_BounceEnvelopeSpoofedUser(t *testing.T) {
	// Envelope is a normal user but From claims mailer-daemon. Must
	// reject — envelope pins bounce identity.
	h := mkHeader(map[string]string{
		"From":           "mailer-daemon@example.org",
		"Auto-Submitted": "auto-replied",
		"Content-Type":   `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(anything)"), Options{
		MailFrom: "alice@example.org",
	})
	assertRejected(t, err)
}

func TestAdversarial_BounceMixedCaseEnvelope(t *testing.T) {
	// Envelope MAIL FROM in mixed case must still match.
	h := mkHeader(map[string]string{
		"From":           "MAILER-daemon@example.org",
		"Auto-Submitted": "auto-replied",
		"Content-Type":   `multipart/report; boundary="bnd"`,
	})
	err := EnforceEncryption(h, strings.NewReader("(anything)"), Options{
		MailFrom: "Mailer-Daemon@example.org",
	})
	assertAccepted(t, err)
}

// ── Category F: Passthrough bypass attempts ────────────────────────

func TestAdversarial_PassthroughSenderCaseInsensitive(t *testing.T) {
	h := mkHeader(map[string]string{"Content-Type": "text/plain"})
	err := EnforceEncryption(h, strings.NewReader("cleartext"), Options{
		MailFrom:           "Bot@Example.ORG",
		PassthroughSenders: []string{"bot@example.org"},
	})
	assertAccepted(t, err)
}

func TestAdversarial_PassthroughDomainOnlyIfAllMatch(t *testing.T) {
	h := mkHeader(map[string]string{"Content-Type": "text/plain"})
	// One recipient inside the allowed domain, one outside.
	err := EnforceEncryption(h, strings.NewReader("cleartext"), Options{
		Recipients:            []string{"a@partner.com", "b@evil.com"},
		PassthroughRecipients: []string{"@partner.com"},
	})
	assertRejected(t, err)
}

func TestAdversarial_PassthroughDomainPrefixMatchNotSubstring(t *testing.T) {
	// "@partner.com" must NOT match "a@impartner.com". Without a
	// proper @-boundary check we could leak via crafted subdomains.
	// Note: the current implementation uses HasSuffix which will
	// reject "a@impartner.com" (doesn't end with "@partner.com"
	// because "im" precedes the @). Documented here as a guard.
	h := mkHeader(map[string]string{"Content-Type": "text/plain"})
	err := EnforceEncryption(h, strings.NewReader("cleartext"), Options{
		Recipients:            []string{"a@impartner.com"},
		PassthroughRecipients: []string{"@partner.com"},
	})
	assertRejected(t, err)
}

func TestAdversarial_PassthroughRecipientsOnlyIfListed(t *testing.T) {
	// An empty PassthroughRecipients list must never short-circuit,
	// even if Recipients is empty.
	h := mkHeader(map[string]string{"Content-Type": "text/plain"})
	err := EnforceEncryption(h, strings.NewReader("cleartext"), Options{
		Recipients: []string{"a@partner.com"},
	})
	assertRejected(t, err)
}

// ── Category G: Stream-size / EOF edge cases ───────────────────────

func TestAdversarial_MultipartTruncated(t *testing.T) {
	// Multipart header says we'll have parts, but the body is cut
	// off before the first boundary. multipart.NextPart will return
	// io.EOF or io.ErrUnexpectedEOF on an incomplete stream; the
	// validator must not accept an empty encrypted envelope.
	ct := `multipart/encrypted; boundary="bnd"`
	assertRejected(t, runEnforce(ct, []byte("--bnd\r\n")))
	assertRejected(t, runEnforce(ct, []byte("")))
}

func TestAdversarial_VersionLinePadding(t *testing.T) {
	// Version line with CRLF padding and trailing whitespace — must
	// still validate, since TrimSpace covers this.
	boundary := "tbnd"
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/pgp-encrypted\r\n\r\n  Version: 1  \r\n")
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/octet-stream\r\n\r\n")
	buf.Write(validPGPPayload())
	fmt.Fprintf(&buf, "\r\n--%s--\r\n", boundary)
	ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
	assertAccepted(t, runEnforce(ct, buf.Bytes()))
}

// ── Category H: Extra sneaky attacks discovered while reviewing ─

func TestAdversarial_SEIPDZeroLengthBody(t *testing.T) {
	// SEIPD with declared length 0 and EOF immediately after. This
	// is a degenerate but syntactically well-formed packet — we
	// intentionally accept it because the OpenPGP library downstream
	// will fail to decrypt, and it isn't a plaintext-smuggling vector.
	// Documented here so the behaviour doesn't regress silently.
	seipd := []byte{0xC0 | 18, 0x00}
	_, ct, body := pgpMIME(seipd)
	assertAccepted(t, runEnforce(ct, body))
}

func TestAdversarial_PartialChunkAllTheWay(t *testing.T) {
	// Valid stream built entirely from partial-body chunks — no
	// final length byte. Spec requires a terminating final length;
	// a stream that ends inside a partial chunk must be rejected.
	var mal []byte
	mal = append(mal, 0xC0|18)                           // SEIPD tag
	mal = append(mal, 224|5)                             // partial 32
	mal = append(mal, bytes.Repeat([]byte{0xBB}, 32)...) // 32 bytes
	// no final length → stream ends → truncated
	_, ct, body := pgpMIME(mal)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_LengthOverflowAttack(t *testing.T) {
	// Five-octet length with the high bit set would overflow a
	// signed int32. Our walker uses int64 and rejects negative
	// values defensively so an attacker cannot cause a huge
	// CopyN(io.Discard, …, 2^31+ bytes).
	var mal []byte
	mal = append(mal, 0xC0|1)                        // PKESK
	mal = append(mal, 0xFF, 0x80, 0x00, 0x00, 0x00)  // 2^31
	mal = append(mal, bytes.Repeat([]byte{0xAA}, 4)...)
	_, ct, body := pgpMIME(mal)
	assertRejected(t, runEnforce(ct, body))
}

func TestAdversarial_PGPMIMEWithChatmailSecureJoinHeader(t *testing.T) {
	// A message that is BOTH multipart/encrypted AND carries a
	// Secure-Join: vc-request header must still be validated as
	// encrypted — Content-Type is the dispatch key, Secure-Join is
	// only honoured on multipart/mixed. We want to make sure adding
	// a Secure-Join header does not bypass packet validation on a
	// malformed encrypted body.
	_, ct, body := pgpMIME(bytes.Repeat([]byte("plaintext"), 8))
	h := mkHeader(map[string]string{
		"Content-Type": ct,
		"Secure-Join":  "vc-request",
	})
	err := EnforceEncryption(h, bytes.NewReader(body), Options{})
	assertRejected(t, err)
}

func TestAdversarial_ContentTypeWithTrailingJunk(t *testing.T) {
	// mime.ParseMediaType is strict about quoted-string rules; a
	// Content-Type with a stray semicolon or garbage parameters
	// should not be accidentally treated as valid multipart/encrypted.
	err := runEnforce(`multipart/encrypted; boundary="bnd"; charset=plain%bogus`,
		[]byte("--bnd\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n--bnd--\r\n"))
	// Malformed CT → ParseMediaType returns an error → reject.
	// (Even if it were accepted, the MIME body is missing part 2.)
	assertRejected(t, err)
}

func TestAdversarial_BoundaryClashWithBodyContent(t *testing.T) {
	// An armored line that happens to look like the MIME boundary
	// can terminate the octet-stream part prematurely. multipart.Reader
	// handles boundary detection so the encrypted body doesn't need
	// to be boundary-aware, but we want to confirm we don't accept a
	// case where part 2 gets truncated.
	boundary := "XBOUND"
	payload := []byte("--XBOUND\r\nContent-Type: text/plain\r\n\r\nhello\r\n") // looks like boundary
	ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/octet-stream\r\n\r\n")
	buf.Write(payload)
	fmt.Fprintf(&buf, "\r\n--%s--\r\n", boundary)
	assertRejected(t, runEnforce(ct, buf.Bytes()))
}

func TestAdversarial_LargeArmoredBodyStillStreams(t *testing.T) {
	// Regression guard for the streaming refactor: a 2 MiB valid
	// armored body must still validate (and not trip any ReadSlice
	// buffer-too-small errors). Any future change that accidentally
	// materialises the whole body would still pass this test but
	// the dedicated benchmark would regress — the pair is the guard.
	big := bytes.Repeat([]byte{0xBB}, 2*1024*1024)
	payload := append(pkt(1, bytes.Repeat([]byte{0xAA}, 32)), pkt(18, big)...)
	_, ct, body := pgpMIME(armored(payload))
	assertAccepted(t, runEnforce(ct, body))
}

func TestAdversarial_CLRLInjectionInSecureJoin(t *testing.T) {
	// Try to sneak extra headers into a Secure-Join message via
	// CRLF in the body. multipart.NewReader handles header parsing
	// so any CRLF is just part of the part body — but we still want
	// an explicit test so a regression to raw-string body matching
	// would be caught.
	h := mkHeader(map[string]string{
		"Secure-Join":  "vc-request",
		"Content-Type": `multipart/mixed; boundary="bnd"`,
	})
	body := "--bnd\r\nContent-Type: text/plain\r\n\r\n\r\nX-Injected: yes\r\nsecure-join: vc-request\r\n--bnd--\r\n"
	// The part body starts with an empty line and then an injected
	// header — it's NOT prefixed with "secure-join:", so the check
	// must reject it.
	assertRejected(t, EnforceEncryption(h, strings.NewReader(body), Options{}))
}

func TestAdversarial_PartContentTypeWithParams(t *testing.T) {
	// application/pgp-encrypted; foo=bar must still match.
	boundary := "tbnd"
	var buf bytes.Buffer
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/pgp-encrypted; charset=us-ascii\r\n\r\nVersion: 1\r\n")
	fmt.Fprintf(&buf, "--%s\r\n", boundary)
	buf.WriteString("Content-Type: application/octet-stream; name=\"encrypted.asc\"\r\n\r\n")
	buf.Write(validPGPPayload())
	fmt.Fprintf(&buf, "\r\n--%s--\r\n", boundary)
	ct := fmt.Sprintf(`multipart/encrypted; boundary=%q`, boundary)
	assertAccepted(t, runEnforce(ct, buf.Bytes()))
}
