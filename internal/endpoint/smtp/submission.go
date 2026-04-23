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

package smtp

import (
	"errors"
	"fmt"
	"io"
	"net/mail"
	"time"

	"github.com/emersion/go-message/textproto"
	"github.com/google/uuid"
	"github.com/themadorg/madmail/framework/exterrors"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/pgp_verify"
)

var (
	msgIDField = func() (string, error) {
		id, err := uuid.NewRandom()
		if err != nil {
			return "", err
		}
		return id.String(), nil
	}

	now = time.Now
)

func (s *Session) submissionPrepare(msgMeta *module.MsgMetadata, header *textproto.Header) error {
	msgMeta.DontTraceSender = true

	if header.Get("Message-ID") == "" {
		msgId, err := msgIDField()
		if err != nil {
			return errors.New("Message-ID generation failed")
		}
		s.log.Msg("adding missing Message-ID")
		header.Set("Message-ID", "<"+msgId+"@"+s.endp.serv.Domain+">")
	}

	if header.Get("From") == "" {
		return &exterrors.SMTPError{
			Code:         554,
			EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
			Message:      "Message does not contains a From header field",
			Misc: map[string]interface{}{
				"modifier": "submission_prepare",
			},
		}
	}

	for _, hdr := range [...]string{"Sender"} {
		if value := header.Get(hdr); value != "" {
			if _, err := mail.ParseAddress(value); err != nil {
				return &exterrors.SMTPError{
					Code:         554,
					EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
					Message:      fmt.Sprintf("Invalid address in %s", hdr),
					Misc: map[string]interface{}{
						"modifier": "submission_prepare",
						"addr":     value,
					},
					Err: err,
				}
			}
		}
	}
	for _, hdr := range [...]string{"To", "Cc", "Bcc", "Reply-To"} {
		if value := header.Get(hdr); value != "" {
			if _, err := mail.ParseAddressList(value); err != nil {
				return &exterrors.SMTPError{
					Code:         554,
					EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
					Message:      fmt.Sprintf("Invalid address in %s", hdr),
					Misc: map[string]interface{}{
						"modifier": "submission_prepare",
						"addr":     value,
					},
					Err: err,
				}
			}
		}
	}

	addrs, err := mail.ParseAddressList(header.Get("From"))
	if err != nil {
		return &exterrors.SMTPError{
			Code:         554,
			EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
			Message:      "Invalid address in From",
			Misc: map[string]interface{}{
				"modifier": "submission_prepare",
				"addr":     header.Get("From"),
			},
			Err: err,
		}
	}

	// https://tools.ietf.org/html/rfc5322#section-3.6.2
	// If From contains multiple addresses, Sender field must be present.
	if len(addrs) > 1 && header.Get("Sender") == "" {
		return &exterrors.SMTPError{
			Code:         554,
			EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
			Message:      "Missing Sender header field",
			Misc: map[string]interface{}{
				"modifier": "submission_prepare",
				"from":     header.Get("From"),
			},
		}
	}

	if dateHdr := header.Get("Date"); dateHdr != "" {
		_, err := parseMessageDateTime(dateHdr)
		if err != nil {
			return &exterrors.SMTPError{
				Code:    554,
				Message: "Malformed Date header",
				Misc: map[string]interface{}{
					"modifier": "submission_prepare",
					"date":     dateHdr,
				},
				Err: err,
			}
		}
	} else {
		s.log.Msg("adding missing Date header")
		header.Set("Date", now().UTC().Format("Mon, 2 Jan 2006 15:04:05 -0700"))
	}

	return nil
}

// submissionCheckBody is the PGP-only policy gate for the SMTP
// submission (port 587) path. It is invoked unconditionally: chatmail
// users must not be able to send unencrypted mail through our relay.
//
// The real decision lives in pgp_verify.EnforceEncryption, the single
// function shared with the HTTP MX-Deliv federation endpoint, IMAP
// APPEND, CLI `imap-msgs add`, and the optional check.pgp_encryption
// module — so a mail rejected on one surface is rejected the same way
// on every other. The Secure-Join v[cg]-request unencrypted handshake
// leg is the only unencrypted message this accepts.
func (s *Session) submissionCheckBody(header textproto.Header, body io.Reader) error {
	err := pgp_verify.EnforceEncryption(header, body, pgp_verify.Options{
		MailFrom:   s.mailFrom,
		Recipients: s.rcpts,
	})
	if err == nil {
		return nil
	}
	var smtpErr *exterrors.SMTPError
	if errors.As(err, &smtpErr) && smtpErr.Code == 523 {
		s.log.Msg("REJECTED: unencrypted submission",
			"msg_id", s.msgMeta.ID,
			"mail_from", s.mailFrom,
			"content_type", header.Get("Content-Type"),
			"secure_join", header.Get("Secure-Join"),
		)
	}
	return err
}
