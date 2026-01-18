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

package pgp_encryption

import (
	"context"
	"net/mail"
	"slices"
	"strings"

	"github.com/emersion/go-message/textproto"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/exterrors"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/pgp_verify"
	"github.com/themadorg/madmail/internal/target"
)

const modName = "check.pgp_encryption"

type Check struct {
	instName              string
	log                   log.Logger
	passthroughSenders    []string
	passthroughRecipients []string
	requireEncryption     bool
	allowSecureJoin       bool
}

func New(_, instName string, _, inlineArgs []string) (module.Module, error) {
	c := &Check{
		instName:          instName,
		log:               log.Logger{Name: modName, Debug: log.DefaultLogger.Debug},
		requireEncryption: true,
		allowSecureJoin:   true,
	}
	return c, nil
}

func (c *Check) Name() string {
	return modName
}

func (c *Check) InstanceName() string {
	return c.instName
}

func (c *Check) Init(cfg *config.Map) error {
	cfg.Bool("require_encryption", false, true, &c.requireEncryption)
	cfg.Bool("allow_secure_join", false, true, &c.allowSecureJoin)
	cfg.StringList("passthrough_senders", false, false, nil, &c.passthroughSenders)
	cfg.StringList("passthrough_recipients", false, false, nil, &c.passthroughRecipients)
	if _, err := cfg.Process(); err != nil {
		return err
	}
	return nil
}

type state struct {
	c           *Check
	msgMeta     *module.MsgMetadata
	log         log.Logger
	mailFrom    string
	mimeFrom    string
	rcptTos     []string
	secureJoin  string
	subject     string
	contentType string
}

func (c *Check) CheckStateForMsg(ctx context.Context, msgMeta *module.MsgMetadata) (module.CheckState, error) {
	return &state{
		c:       c,
		msgMeta: msgMeta,
		log:     target.DeliveryLogger(c.log, msgMeta),
	}, nil
}

func (s *state) CheckConnection(ctx context.Context) module.CheckResult {
	return module.CheckResult{}
}

func (s *state) CheckSender(ctx context.Context, mailFrom string) module.CheckResult {
	s.mailFrom = mailFrom
	return module.CheckResult{}
}

func (s *state) CheckRcpt(ctx context.Context, rcptTo string) module.CheckResult {
	s.rcptTos = append(s.rcptTos, rcptTo)
	return module.CheckResult{}
}

func (s *state) CheckBody(ctx context.Context, header textproto.Header, body buffer.Buffer) module.CheckResult {
	if !s.c.requireEncryption {
		return module.CheckResult{}
	}

	// Extract headers
	s.subject = header.Get("Subject")
	s.contentType = header.Get("Content-Type")
	s.mimeFrom = header.Get("From")
	s.secureJoin = header.Get("Secure-Join")
	autoSubmitted := header.Get("Auto-Submitted")

	// Check if sender is in passthrough list
	if slices.Contains(s.c.passthroughSenders, s.mailFrom) {
		s.log.Msg("sender in passthrough list, allowing message", "sender", s.mailFrom)
		return module.CheckResult{}
	}

	// Allow auto-submitted messages from mailer-daemon (like bounce messages)
	if autoSubmitted != "" && autoSubmitted != "no" {
		if s.mimeFrom != "" {
			mimeFromAddr, err := mail.ParseAddress(s.mimeFrom)
			if err == nil && strings.HasPrefix(strings.ToLower(mimeFromAddr.Address), "mailer-daemon@") {
				if strings.HasPrefix(s.contentType, "multipart/report") {
					s.log.Msg("allowing auto-submitted mailer-daemon message", "from", s.mimeFrom)
					return module.CheckResult{}
				}
			}
		}
	}

	// Validate MIME From header matches envelope sender
	if s.mimeFrom != "" {
		mimeFromAddr, err := mail.ParseAddress(s.mimeFrom)
		if err != nil {
			return module.CheckResult{
				Reject: true,
				Reason: &exterrors.SMTPError{
					Code:         554,
					EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
					Message:      "Invalid From header",
					Reason:       "invalid mime from",
					CheckName:    "pgp_encryption",
					Err:          err,
				},
			}
		}
		if !strings.EqualFold(mimeFromAddr.Address, s.mailFrom) {
			return module.CheckResult{
				Reject: true,
				Reason: &exterrors.SMTPError{
					Code:         554,
					EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
					Message:      "From header does not match envelope sender",
					Reason:       "from mismatch",
					CheckName:    "pgp_encryption",
				},
			}
		}
	}

	// Check each recipient
	for _, recipient := range s.rcptTos {
		// Check if recipient matches passthrough patterns
		if s.recipientMatchesPassthrough(recipient) {
			continue
		}

		// Parse recipient domain (for validation)
		rcptParts := strings.Split(recipient, "@")
		if len(rcptParts) != 2 {
			return module.CheckResult{
				Reject: true,
				Reason: &exterrors.SMTPError{
					Code:         554,
					EnhancedCode: exterrors.EnhancedCode{5, 6, 0},
					Message:      "Invalid recipient address format",
					Reason:       "invalid recipient format",
					CheckName:    "pgp_encryption",
				},
			}
		}

		// For chatmail: ALL messages (same domain or different domain) must be encrypted
		// when require_encryption is enabled, since this check runs in the submission
		// endpoint where all senders are authenticated local users.
		// Check if message is encrypted
		r, err := body.Open()
		if err != nil {
			return module.CheckResult{
				Reject: true,
				Reason: &exterrors.SMTPError{
					Code:         451,
					EnhancedCode: exterrors.EnhancedCode{4, 0, 0},
					Message:      "Cannot read message body",
					Reason:       "body read error",
					CheckName:    "pgp_encryption",
					Err:          err,
				},
			}
		}
		defer r.Close()

		isEncrypted, err := pgp_verify.IsValidEncryptedMessage(s.contentType, r)
		if err != nil {
			return module.CheckResult{
				Reject: true,
				Reason: &exterrors.SMTPError{
					Code:         451,
					EnhancedCode: exterrors.EnhancedCode{4, 0, 0},
					Message:      "Error validating message encryption",
					Reason:       "encryption validation error",
					CheckName:    "pgp_encryption",
					Err:          err,
				},
			}
		}

		if !isEncrypted {
			s.log.DebugMsg("message not encrypted, checking for secure join", "content_type", s.contentType, "secure_join", s.secureJoin)
			// Check if this is a secure join request - be more permissive here
			if s.c.allowSecureJoin {
				// Re-open body as IsValidEncryptedMessage consumed it
				r2, err := body.Open()
				if err == nil {
					defer r2.Close()
					header := textproto.Header{}
					header.Set("Content-Type", s.contentType)
					header.Set("Secure-Join", s.secureJoin)
					if pgp_verify.IsSecureJoinMessage(header, r2) {
						s.log.Msg("allowing secure join request", "recipient", recipient, "secure_join", s.secureJoin)
						continue
					}
				} else {
					s.log.Error("failed to re-open body for secure join check", err)
				}
			}

			s.log.Msg("rejecting unencrypted message", "recipient", recipient, "sender", s.mailFrom, "content_type", s.contentType, "secure_join", s.secureJoin)
			// Reject unencrypted message
			return module.CheckResult{
				Reject: true,
				Reason: &exterrors.SMTPError{
					Code:         523, // Use 523 like in the Python code
					EnhancedCode: exterrors.EnhancedCode{5, 7, 1},
					Message:      "Encryption Needed: Invalid Unencrypted Mail",
					Reason:       "unencrypted message",
					CheckName:    "pgp_encryption",
					Misc: map[string]interface{}{
						"recipient": recipient,
						"sender":    s.mailFrom,
					},
				},
			}
		}
	}

	return module.CheckResult{}
}

// recipientMatchesPassthrough checks if recipient matches any passthrough pattern
func (s *state) recipientMatchesPassthrough(recipient string) bool {
	for _, addr := range s.c.passthroughRecipients {
		if strings.EqualFold(recipient, addr) {
			s.log.Msg("recipient matches exact passthrough", "recipient", recipient, "pattern", addr)
			return true
		}
		// Support domain-wide passthrough (e.g., "@example.com")
		if strings.HasPrefix(addr, "@") && strings.HasSuffix(strings.ToLower(recipient), strings.ToLower(addr)) {
			s.log.Msg("recipient matches domain passthrough", "recipient", recipient, "pattern", addr)
			return true
		}
	}
	return false
}

func (s *state) Close() error {
	return nil
}

var (
	_ module.Check      = &Check{}
	_ module.CheckState = &state{}
)

func init() {
	module.Register(modName, New)
}
