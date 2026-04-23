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

package webimap

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/mail"
	"strings"

	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/pgp_verify"
)

// ---- WebSMTP: send via HTTP ----

// SendRequest is the JSON body or the raw-mode request for sending email.
type SendRequest struct {
	From string   `json:"from"`
	To   []string `json:"to"`
	Body string   `json:"body"` // raw RFC5322 message (with headers + body)
}

// handleSend accepts POST /websmtp/send with authenticated user.
// Body is a raw RFC5322 email message (headers + CRLF + body).
// The sender (X-Email) must match the From in the message.
func (h *Handler) handleSend(w http.ResponseWriter, r *http.Request) {
	setCORS(w)
	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}
	if r.Method != http.MethodPost {
		h.writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	// Check if WebSMTP is enabled
	if !h.isEnabled(h.WebSMTPEnabledKey) {
		h.writeError(w, http.StatusNotFound, "not found")
		return
	}

	_, email, authErr := h.authenticate(r)
	if authErr != nil {
		h.writeError(w, http.StatusUnauthorized, authErr.Error())
		return
	}

	// Parse JSON body with raw message
	var req SendRequest
	bodyReader := r.Body
	if h.MaxMsgSize > 0 {
		bodyReader = http.MaxBytesReader(w, r.Body, h.MaxMsgSize)
	}
	rawBody, err := io.ReadAll(bodyReader)
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "failed to read body or body too large")
		return
	}
	if err := json.Unmarshal(rawBody, &req); err != nil {
		h.writeError(w, http.StatusBadRequest, "invalid JSON: "+err.Error())
		return
	}

	// Always use the authenticated user as the sender
	req.From = email

	if len(req.To) == 0 {
		h.writeError(w, http.StatusBadRequest, "missing recipients")
		return
	}

	if err := h.deliverMessage(r.Context(), req.From, req.To, req.Body); err != nil {
		h.Logger.Error("send failed", err)
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeJSON(w, http.StatusOK, map[string]string{"status": "sent"})
}

// recipientDomain extracts the domain part from an email address.
// Handles both "user@domain" and "user@[1.2.3.4]" formats.
func recipientDomain(addr string) string {
	at := strings.LastIndex(addr, "@")
	if at < 0 {
		return ""
	}
	return strings.ToLower(addr[at+1:])
}

// deliverMessage is the shared send implementation used by both the REST
// endpoint and the WebSocket "send" action.  It splits recipients into
// local (same MailDomain → Storage) and remote (→ RemoteTarget), runs
// PGP verification, and delivers to both targets.
func (h *Handler) deliverMessage(ctx context.Context, from string, to []string, rawBody string) error {
	// ---- Parse & verify the RFC 5322 message ----
	header, err := textproto.ReadHeader(bufio.NewReader(bytes.NewReader([]byte(rawBody))))
	if err != nil {
		return fmt.Errorf("failed to parse email headers")
	}

	// Verify that the From header address matches the authenticated sender
	fromHdr := header.Get("From")
	if fromHdr != "" {
		list, err := mail.ParseAddressList(fromHdr)
		if err != nil {
			return fmt.Errorf("malformed From header: %w", err)
		}
		if len(list) == 0 {
			return fmt.Errorf("empty From header")
		}
		if len(list) > 1 {
			return fmt.Errorf("multiple addresses in From header are not allowed")
		}
		if !strings.EqualFold(list[0].Address, from) {
			return fmt.Errorf("From header in message (%s) does not match authenticated user (%s)", list[0].Address, from)
		}
	}

	// Also check Sender header if present
	if senderHdr := header.Get("Sender"); senderHdr != "" {
		sender, err := mail.ParseAddress(senderHdr)
		if err != nil {
			return fmt.Errorf("malformed Sender header: %w", err)
		}
		if !strings.EqualFold(sender.Address, from) {
			return fmt.Errorf("Sender header in message (%s) does not match authenticated user (%s)", sender.Address, from)
		}
	}

	rawMsg := []byte(rawBody)
	bodySep := bytes.Index(rawMsg, []byte("\r\n\r\n"))
	if bodySep < 0 {
		bodySep = bytes.Index(rawMsg, []byte("\n\n"))
	}
	var remainingBody []byte
	if bodySep >= 0 {
		offset := bodySep + 4
		if rawMsg[bodySep] == '\n' {
			offset = bodySep + 2
		}
		remainingBody = rawMsg[offset:]
	}

	// Shared PGP-only gate — identical to SMTP submission, HTTP
	// MX-Deliv, IMAP APPEND, CLI imap-msgs add, and
	// check.pgp_encryption. WebIMAP sessions are authenticated
	// users sending from their own account, so we seed MailFrom
	// with the authenticated address.
	if err := pgp_verify.EnforceEncryption(header, bytes.NewReader(remainingBody), pgp_verify.Options{
		MailFrom:   from,
		Recipients: to,
	}); err != nil {
		return fmt.Errorf("Encryption Needed: only PGP-encrypted messages and SecureJoin handshakes are accepted: %w", err)
	}

	// ---- Split recipients into local vs remote ----
	localDomain := strings.ToLower(h.MailDomain)
	var localRcpts, remoteRcpts []string
	for _, rcpt := range to {
		domain := recipientDomain(rcpt)
		if domain == localDomain || localDomain == "" {
			localRcpts = append(localRcpts, rcpt)
		} else {
			remoteRcpts = append(remoteRcpts, rcpt)
		}
	}

	// ---- Deliver to local recipients via Storage ----
	if len(localRcpts) > 0 {
		dt, ok := h.Storage.(module.DeliveryTarget)
		if !ok {
			return fmt.Errorf("local delivery not supported")
		}
		if err := h.deliverToTarget(ctx, dt, from, localRcpts, header, remainingBody); err != nil {
			return fmt.Errorf("local delivery failed: %s", err.Error())
		}
	}

	// ---- Deliver to remote recipients via RemoteTarget ----
	if len(remoteRcpts) > 0 {
		if h.RemoteTarget == nil {
			return fmt.Errorf("remote delivery not configured — cannot send to external domains")
		}
		if err := h.deliverToTarget(ctx, h.RemoteTarget, from, remoteRcpts, header, remainingBody); err != nil {
			return fmt.Errorf("remote delivery failed: %s", err.Error())
		}
	}

	module.IncrementReceivedMessages()
	return nil
}

// deliverToTarget performs delivery through a single DeliveryTarget (local or remote).
func (h *Handler) deliverToTarget(
	ctx context.Context,
	dt module.DeliveryTarget,
	from string,
	rcpts []string,
	header textproto.Header,
	body []byte,
) error {
	msgID, _ := module.GenerateMsgID()
	msgMeta := &module.MsgMetadata{
		ID:       msgID,
		SMTPOpts: smtp.MailOptions{},
	}

	delivery, err := dt.Start(ctx, msgMeta, from)
	if err != nil {
		return fmt.Errorf("failed to start delivery: %w", err)
	}
	defer func() {
		if abortErr := delivery.Abort(ctx); abortErr != nil {
			if !strings.Contains(abortErr.Error(), "transaction has already been committed") {
				h.Logger.Error("failed to abort delivery", abortErr)
			}
		}
	}()

	anyAccepted := false
	for _, to := range rcpts {
		if addErr := delivery.AddRcpt(ctx, to, smtp.RcptOptions{}); addErr != nil {
			h.Logger.Error("failed to add recipient", addErr, "to", to)
		} else {
			anyAccepted = true
		}
	}
	if !anyAccepted {
		return fmt.Errorf("no valid recipients")
	}

	buf := buffer.MemoryBuffer{Slice: body}
	if err := delivery.Body(ctx, header, buf); err != nil {
		return fmt.Errorf("delivery failed: %w", err)
	}
	if err := delivery.Commit(ctx); err != nil {
		return fmt.Errorf("commit failed: %w", err)
	}

	return nil
}
