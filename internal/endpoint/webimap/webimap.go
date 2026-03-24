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

// Package webimap provides a REST HTTP interface for IMAP operations.
// Authentication is performed via X-Email and X-Password headers on each request.
// Messages can be retrieved via long polling.
package webimap

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/emersion/go-imap"
	imapbackend "github.com/emersion/go-imap/backend"
	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/gorilla/websocket"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/pgp_verify"
)

// Handler holds references to the auth DB and storage for servicing
// REST requests that map to IMAP operations.
type Handler struct {
	AuthDB  module.PlainUserDB
	Storage module.Storage
	Logger  log.Logger
}

// ---- JSON response types ----

type errorResp struct {
	Error string `json:"error"`
}

// MailboxInfo is the JSON representation of an IMAP mailbox.
type MailboxInfo struct {
	Name       string   `json:"name"`
	Attributes []string `json:"attributes,omitempty"`
	Messages   uint32   `json:"messages"`
	Unseen     uint32   `json:"unseen"`
}

// Address is the JSON representation of an email address.
type Address struct {
	Name    string `json:"name,omitempty"`
	Mailbox string `json:"mailbox"`
	Host    string `json:"host"`
}

// Envelope is the JSON representation of an IMAP envelope.
type Envelope struct {
	Date      string    `json:"date"`
	Subject   string    `json:"subject"`
	From      []Address `json:"from,omitempty"`
	To        []Address `json:"to,omitempty"`
	Cc        []Address `json:"cc,omitempty"`
	MessageID string    `json:"message_id,omitempty"`
	InReplyTo string    `json:"in_reply_to,omitempty"`
}

// MessageSummary is the JSON representation of a message in a list response.
type MessageSummary struct {
	UID      uint32   `json:"uid"`
	SeqNum   uint32   `json:"seq_num"`
	Flags    []string `json:"flags"`
	Size     uint32   `json:"size"`
	Date     string   `json:"date"`
	Envelope Envelope `json:"envelope"`
}

// MessageDetail is the full message including body.
type MessageDetail struct {
	MessageSummary
	Body string `json:"body"`
}

// FlagRequest is the JSON body for flag update requests.
type FlagRequest struct {
	Mailbox string   `json:"mailbox"`
	UID     uint32   `json:"uid"`
	Flags   []string `json:"flags"`
	Op      string   `json:"op"` // "add", "remove", "set"
}

// ---- helpers ----

func (h *Handler) writeJSON(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func (h *Handler) writeError(w http.ResponseWriter, status int, message string) {
	h.writeJSON(w, status, errorResp{Error: message})
}

func convertAddresses(addrs []*imap.Address) []Address {
	out := make([]Address, 0, len(addrs))
	for _, a := range addrs {
		out = append(out, Address{
			Name:    a.PersonalName,
			Mailbox: a.MailboxName,
			Host:    a.HostName,
		})
	}
	return out
}

func convertEnvelope(env *imap.Envelope) Envelope {
	if env == nil {
		return Envelope{}
	}
	return Envelope{
		Date:      env.Date.UTC().Format(time.RFC3339),
		Subject:   env.Subject,
		From:      convertAddresses(env.From),
		To:        convertAddresses(env.To),
		Cc:        convertAddresses(env.Cc),
		MessageID: env.MessageId,
		InReplyTo: env.InReplyTo,
	}
}

// authenticate validates the credentials from headers and returns an
// IMAP backend User on success.
func (h *Handler) authenticate(r *http.Request) (imapbackend.User, string, error) {
	email := r.Header.Get("X-Email")
	password := r.Header.Get("X-Password")

	if email == "" || password == "" {
		return nil, "", fmt.Errorf("missing credentials")
	}

	if err := h.AuthDB.AuthPlain(email, password); err != nil {
		return nil, "", fmt.Errorf("authentication failed")
	}

	user, err := h.Storage.GetOrCreateIMAPAcct(email)
	if err != nil {
		return nil, "", fmt.Errorf("storage error: %w", err)
	}

	return user, email, nil
}

// setCORS adds permissive CORS headers for the test page.
func setCORS(w http.ResponseWriter) {
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
	w.Header().Set("Access-Control-Allow-Headers", "Content-Type, X-Email, X-Password")
	w.Header().Set("Access-Control-Max-Age", "86400")
}

// Register mounts all WebIMAP and WebSMTP routes on the given mux under the specified prefix.
func (h *Handler) Register(mux *http.ServeMux, prefix string) {
	prefix = strings.TrimSuffix(prefix, "/")
	mux.HandleFunc(prefix+"/mailboxes", h.handleMailboxes)
	mux.HandleFunc(prefix+"/messages", h.handleMessages)
	mux.HandleFunc(prefix+"/message/", h.handleMessage) // /webimap/message/{uid}
	mux.HandleFunc(prefix+"/message/flags", h.handleFlags)
	mux.HandleFunc(prefix+"/ws", h.handleWebSocket)
	// WebSMTP: send email via HTTP REST (also under prefix)
	mux.HandleFunc(prefix+"/send", h.handleSend)
	// Keep legacy path for backward compatibility
	mux.HandleFunc("/websmtp/send", h.handleSend)
}

// ---- route handlers ----

func (h *Handler) handleMailboxes(w http.ResponseWriter, r *http.Request) {
	setCORS(w)
	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}
	if r.Method != http.MethodGet {
		h.writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	user, _, err := h.authenticate(r)
	if err != nil {
		h.writeError(w, http.StatusUnauthorized, err.Error())
		return
	}
	defer func() { _ = user.Logout() }()

	mboxInfos, err := user.ListMailboxes(false)
	if err != nil {
		h.Logger.Error("failed to list mailboxes", err)
		h.writeError(w, http.StatusInternalServerError, "failed to list mailboxes")
		return
	}

	result := make([]MailboxInfo, 0, len(mboxInfos))
	for _, info := range mboxInfos {
		mi := MailboxInfo{
			Name:       info.Name,
			Attributes: info.Attributes,
		}

		// Get status for message counts
		status, err := user.Status(info.Name, []imap.StatusItem{imap.StatusMessages, imap.StatusUnseen})
		if err == nil {
			mi.Messages = status.Messages
			mi.Unseen = status.Unseen
		}

		result = append(result, mi)
	}

	h.writeJSON(w, http.StatusOK, result)
}

func (h *Handler) handleMessages(w http.ResponseWriter, r *http.Request) {
	setCORS(w)
	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}
	if r.Method != http.MethodGet {
		h.writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	user, _, err := h.authenticate(r)
	if err != nil {
		h.writeError(w, http.StatusUnauthorized, err.Error())
		return
	}
	defer func() { _ = user.Logout() }()

	mailbox := r.URL.Query().Get("mailbox")
	if mailbox == "" {
		mailbox = "INBOX"
	}

	sinceUIDStr := r.URL.Query().Get("since_uid")
	sinceUID := uint32(0)
	if sinceUIDStr != "" {
		v, err := strconv.ParseUint(sinceUIDStr, 10, 32)
		if err == nil {
			sinceUID = uint32(v)
		}
	}

	waitStr := r.URL.Query().Get("wait")
	waitSec := 0
	if waitStr != "" {
		v, err := strconv.Atoi(waitStr)
		if err == nil && v >= 0 {
			waitSec = v
			if waitSec > 120 {
				waitSec = 120
			}
		}
	}

	// Fetch messages; if long polling and no new messages, retry with a delay.
	deadline := time.Now().Add(time.Duration(waitSec) * time.Second)
	for {
		msgs, err := h.fetchMessages(user, mailbox, sinceUID)
		if err != nil {
			h.Logger.Error("failed to fetch messages", err, "mailbox", mailbox)
			h.writeError(w, http.StatusInternalServerError, "failed to fetch messages")
			return
		}

		if len(msgs) > 0 || time.Now().After(deadline) {
			h.writeJSON(w, http.StatusOK, msgs)
			return
		}

		// Check if client disconnected
		select {
		case <-r.Context().Done():
			return
		case <-time.After(2 * time.Second):
			// Re-check for new messages
		}
	}
}

func (h *Handler) fetchMessages(user imapbackend.User, mailbox string, sinceUID uint32) ([]MessageSummary, error) {
	_, mbox, err := user.GetMailbox(mailbox, false, nil)
	if err != nil {
		return nil, fmt.Errorf("failed to open mailbox %s: %w", mailbox, err)
	}

	// Build sequence set: all UIDs > sinceUID
	var seqSet *imap.SeqSet
	if sinceUID > 0 {
		seqSet = new(imap.SeqSet)
		seqSet.AddRange(sinceUID+1, 0) // sinceUID+1 : *
	} else {
		seqSet = new(imap.SeqSet)
		seqSet.AddRange(1, 0) // 1:*
	}

	items := []imap.FetchItem{
		imap.FetchEnvelope,
		imap.FetchFlags,
		imap.FetchUid,
		imap.FetchRFC822Size,
		imap.FetchInternalDate,
	}

	ch := make(chan *imap.Message, 100)
	var fetchErr error
	go func() {
		fetchErr = mbox.ListMessages(true, seqSet, items, ch)
	}()

	result := make([]MessageSummary, 0)
	for msg := range ch {
		result = append(result, MessageSummary{
			UID:      msg.Uid,
			SeqNum:   msg.SeqNum,
			Flags:    msg.Flags,
			Size:     msg.Size,
			Date:     msg.InternalDate.UTC().Format(time.RFC3339),
			Envelope: convertEnvelope(msg.Envelope),
		})
	}

	if fetchErr != nil {
		return nil, fetchErr
	}

	return result, nil
}

func (h *Handler) handleMessage(w http.ResponseWriter, r *http.Request) {
	setCORS(w)
	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}

	// Parse UID from path: /webimap/message/{uid}
	parts := strings.Split(strings.TrimSuffix(r.URL.Path, "/"), "/")
	if len(parts) == 0 {
		h.writeError(w, http.StatusBadRequest, "missing UID")
		return
	}

	// Check if last part is "flags" -> redirect to flags handler
	if parts[len(parts)-1] == "flags" {
		h.handleFlags(w, r)
		return
	}

	uidStr := parts[len(parts)-1]
	uidVal, err := strconv.ParseUint(uidStr, 10, 32)
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "invalid UID")
		return
	}
	uid := uint32(uidVal)

	user, _, authErr := h.authenticate(r)
	if authErr != nil {
		h.writeError(w, http.StatusUnauthorized, authErr.Error())
		return
	}
	defer func() { _ = user.Logout() }()

	mailbox := r.URL.Query().Get("mailbox")
	if mailbox == "" {
		mailbox = "INBOX"
	}

	switch r.Method {
	case http.MethodGet:
		h.getMessage(w, user, mailbox, uid)
	case http.MethodDelete:
		h.deleteMessage(w, user, mailbox, uid)
	default:
		h.writeError(w, http.StatusMethodNotAllowed, "method not allowed")
	}
}

func (h *Handler) getMessage(w http.ResponseWriter, user imapbackend.User, mailbox string, uid uint32) {
	_, mbox, err := user.GetMailbox(mailbox, false, nil)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "failed to open mailbox")
		return
	}

	seqSet := new(imap.SeqSet)
	seqSet.AddNum(uid)

	section := &imap.BodySectionName{Peek: true}
	items := []imap.FetchItem{
		imap.FetchEnvelope,
		imap.FetchFlags,
		imap.FetchUid,
		imap.FetchRFC822Size,
		imap.FetchInternalDate,
		section.FetchItem(),
	}

	ch := make(chan *imap.Message, 1)
	var fetchErr error
	go func() {
		fetchErr = mbox.ListMessages(true, seqSet, items, ch)
	}()

	msg := <-ch
	if msg == nil {
		if fetchErr != nil {
			h.writeError(w, http.StatusInternalServerError, "fetch error: "+fetchErr.Error())
		} else {
			h.writeError(w, http.StatusNotFound, "message not found")
		}
		return
	}

	// Drain any remaining messages
	for range ch {
	}

	body := ""
	for _, literal := range msg.Body {
		if literal != nil {
			data, err := io.ReadAll(literal)
			if err == nil {
				body = string(data)
			}
		}
	}

	detail := MessageDetail{
		MessageSummary: MessageSummary{
			UID:      msg.Uid,
			SeqNum:   msg.SeqNum,
			Flags:    msg.Flags,
			Size:     msg.Size,
			Date:     msg.InternalDate.UTC().Format(time.RFC3339),
			Envelope: convertEnvelope(msg.Envelope),
		},
		Body: body,
	}
	h.writeJSON(w, http.StatusOK, detail)
}

func (h *Handler) deleteMessage(w http.ResponseWriter, user imapbackend.User, mailbox string, uid uint32) {
	_, mbox, err := user.GetMailbox(mailbox, false, nil)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "failed to open mailbox")
		return
	}

	seqSet := new(imap.SeqSet)
	seqSet.AddNum(uid)

	// Set \Deleted flag
	err = mbox.UpdateMessagesFlags(true, seqSet, imap.AddFlags, false, []string{imap.DeletedFlag})
	if err != nil {
		h.Logger.Error("failed to set deleted flag", err)
		h.writeError(w, http.StatusInternalServerError, "failed to delete message")
		return
	}

	// Expunge
	if expungeMbox, ok := mbox.(interface{ Expunge() error }); ok {
		if err := expungeMbox.Expunge(); err != nil {
			h.Logger.Error("expunge failed", err)
		}
	}

	h.writeJSON(w, http.StatusOK, map[string]string{"status": "deleted"})
}

func (h *Handler) handleFlags(w http.ResponseWriter, r *http.Request) {
	setCORS(w)
	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}
	if r.Method != http.MethodPost {
		h.writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	user, _, err := h.authenticate(r)
	if err != nil {
		h.writeError(w, http.StatusUnauthorized, err.Error())
		return
	}
	defer func() { _ = user.Logout() }()

	var req FlagRequest
	body, err := io.ReadAll(r.Body)
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "failed to read body")
		return
	}
	if err := json.Unmarshal(body, &req); err != nil {
		h.writeError(w, http.StatusBadRequest, "invalid JSON: "+err.Error())
		return
	}

	if req.Mailbox == "" {
		req.Mailbox = "INBOX"
	}

	_, mbox, err := user.GetMailbox(req.Mailbox, false, nil)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "failed to open mailbox")
		return
	}

	seqSet := new(imap.SeqSet)
	seqSet.AddNum(req.UID)

	var op imap.FlagsOp
	switch req.Op {
	case "add":
		op = imap.AddFlags
	case "remove":
		op = imap.RemoveFlags
	case "set":
		op = imap.SetFlags
	default:
		h.writeError(w, http.StatusBadRequest, "invalid op: must be add, remove, or set")
		return
	}

	if err := mbox.UpdateMessagesFlags(true, seqSet, op, false, req.Flags); err != nil {
		h.Logger.Error("failed to update flags", err)
		h.writeError(w, http.StatusInternalServerError, "failed to update flags")
		return
	}

	h.writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

// ---- WebSocket: real-time message notifications ----

var wsUpgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true }, // Allow any origin
}

// handleWebSocket upgrades to WebSocket and pushes new messages in real-time.
// Auth via query params: ?email=X&password=Y
func (h *Handler) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	email := r.URL.Query().Get("email")
	password := r.URL.Query().Get("password")
	if email == "" || password == "" {
		http.Error(w, "missing email/password query params", http.StatusUnauthorized)
		return
	}

	// Authenticate
	if err := h.AuthDB.AuthPlain(email, password); err != nil {
		http.Error(w, "authentication failed", http.StatusUnauthorized)
		return
	}
	user, err := h.Storage.GetOrCreateIMAPAcct(email)
	if err != nil {
		http.Error(w, "storage error", http.StatusInternalServerError)
		return
	}
	defer func() { _ = user.Logout() }()

	conn, err := wsUpgrader.Upgrade(w, r, nil)
	if err != nil {
		h.Logger.Error("websocket upgrade failed", err)
		return
	}
	defer conn.Close()

	mailbox := r.URL.Query().Get("mailbox")
	if mailbox == "" {
		mailbox = "INBOX"
	}

	// Start with the latest UID to avoid sending old messages
	lastUID := uint32(0)
	sinceUIDStr := r.URL.Query().Get("since_uid")
	if sinceUIDStr != "" {
		if v, err := strconv.ParseUint(sinceUIDStr, 10, 32); err == nil {
			lastUID = uint32(v)
		}
	}

	// Configure keepalive: client must send a pong within 60s
	conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	conn.SetPongHandler(func(string) error {
		conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		return nil
	})

	// Read goroutine — handles pong frames and detects client close
	done := make(chan struct{})
	go func() {
		defer close(done)
		for {
			if _, _, err := conn.ReadMessage(); err != nil {
				return
			}
			// Any client message (including ping JSON) resets the read deadline
			conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		}
	}()

	// Push loop: poll IMAP and send full messages as JSON
	ticker := time.NewTicker(2 * time.Second)
	defer ticker.Stop()

	// Ping ticker: send WebSocket ping frames every 30s
	pingTicker := time.NewTicker(30 * time.Second)
	defer pingTicker.Stop()

	for {
		select {
		case <-ticker.C:
			msgs, err := h.fetchMessages(user, mailbox, lastUID)
			if err != nil {
				h.Logger.Error("ws: failed to fetch messages", err)
				continue
			}
			for _, summary := range msgs {
				if summary.UID > lastUID {
					lastUID = summary.UID
				}
				// Fetch full message body for each new message
				detail := h.fetchFullMessage(user, mailbox, summary.UID)
				if detail == nil {
					continue
				}
				conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
				if err := conn.WriteJSON(detail); err != nil {
					// Client disconnected
					return
				}
			}
		case <-pingTicker.C:
			conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := conn.WriteMessage(websocket.PingMessage, nil); err != nil {
				return
			}
		case <-done:
			return
		case <-r.Context().Done():
			return
		}
	}
}

// fetchFullMessage returns a full MessageDetail for the given UID, or nil on error.
func (h *Handler) fetchFullMessage(user imapbackend.User, mailbox string, uid uint32) *MessageDetail {
	_, mbox, err := user.GetMailbox(mailbox, false, nil)
	if err != nil {
		return nil
	}

	seqSet := new(imap.SeqSet)
	seqSet.AddNum(uid)

	section := &imap.BodySectionName{Peek: true}
	items := []imap.FetchItem{
		imap.FetchEnvelope,
		imap.FetchFlags,
		imap.FetchUid,
		imap.FetchRFC822Size,
		imap.FetchInternalDate,
		section.FetchItem(),
	}

	ch := make(chan *imap.Message, 1)
	go func() {
		_ = mbox.ListMessages(true, seqSet, items, ch)
	}()

	msg := <-ch
	if msg == nil {
		return nil
	}
	for range ch {
	}

	body := ""
	for _, literal := range msg.Body {
		if literal != nil {
			data, err := io.ReadAll(literal)
			if err == nil {
				body = string(data)
			}
		}
	}

	return &MessageDetail{
		MessageSummary: MessageSummary{
			UID:      msg.Uid,
			SeqNum:   msg.SeqNum,
			Flags:    msg.Flags,
			Size:     msg.Size,
			Date:     msg.InternalDate.UTC().Format(time.RFC3339),
			Envelope: convertEnvelope(msg.Envelope),
		},
		Body: body,
	}
}

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

	_, email, authErr := h.authenticate(r)
	if authErr != nil {
		h.writeError(w, http.StatusUnauthorized, authErr.Error())
		return
	}

	// Parse JSON body with raw message
	var req SendRequest
	rawBody, err := io.ReadAll(r.Body)
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "failed to read body")
		return
	}
	if err := json.Unmarshal(rawBody, &req); err != nil {
		h.writeError(w, http.StatusBadRequest, "invalid JSON: "+err.Error())
		return
	}

	if req.From == "" {
		req.From = email
	}
	if len(req.To) == 0 {
		h.writeError(w, http.StatusBadRequest, "missing recipients")
		return
	}

	// Ensure the authenticated user is the sender
	if !strings.EqualFold(req.From, email) {
		h.writeError(w, http.StatusForbidden, "sender must match authenticated user")
		return
	}

	// Use the storage as a DeliveryTarget (same as /mxdeliv)
	dt, ok := h.Storage.(module.DeliveryTarget)
	if !ok {
		h.Logger.Error("storage does not implement DeliveryTarget", nil)
		h.writeError(w, http.StatusInternalServerError, "sending not supported")
		return
	}

	msgID, _ := module.GenerateMsgID()
	msgMeta := &module.MsgMetadata{
		ID:       msgID,
		SMTPOpts: smtp.MailOptions{},
	}

	delivery, err := dt.Start(r.Context(), msgMeta, req.From)
	if err != nil {
		h.Logger.Error("failed to start delivery", err)
		h.writeError(w, http.StatusInternalServerError, "failed to start delivery: "+err.Error())
		return
	}
	defer func() {
		if abortErr := delivery.Abort(r.Context()); abortErr != nil {
			if !strings.Contains(abortErr.Error(), "transaction has already been committed") {
				h.Logger.Error("failed to abort delivery", abortErr)
			}
		}
	}()

	anyAccepted := false
	for _, to := range req.To {
		if addErr := delivery.AddRcpt(r.Context(), to, smtp.RcptOptions{}); addErr != nil {
			h.Logger.Error("failed to add recipient", addErr, "to", to)
		} else {
			anyAccepted = true
		}
	}

	if !anyAccepted {
		h.writeError(w, http.StatusBadRequest, "no valid recipients")
		return
	}

	// Parse headers from raw message body
	header, err := textproto.ReadHeader(bufio.NewReader(bytes.NewReader([]byte(req.Body))))
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "failed to parse email headers")
		return
	}

	// Find where body starts (after the header)
	// ReadHeader consumed headers, so the remainder is the body.
	// We re-parse to get body portion
	rawMsg := []byte(req.Body)
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
	buf := buffer.MemoryBuffer{Slice: remainingBody}

	// Enforce PGP-only policy: only encrypted messages and SecureJoin handshakes allowed
	accepted, pgpErr := pgp_verify.IsAcceptedMessage(header, bytes.NewReader(remainingBody))
	if pgpErr != nil {
		h.Logger.Error("PGP verification failed", pgpErr)
		h.writeError(w, http.StatusBadRequest, "PGP verification error: "+pgpErr.Error())
		return
	}
	if !accepted {
		h.writeError(w, http.StatusForbidden, "Encryption Needed: only PGP-encrypted messages and SecureJoin handshakes are accepted")
		return
	}

	if err := delivery.Body(r.Context(), header, buf); err != nil {
		h.Logger.Error("failed to deliver body", err)
		h.writeError(w, http.StatusInternalServerError, "delivery failed: "+err.Error())
		return
	}

	if err := delivery.Commit(r.Context()); err != nil {
		h.Logger.Error("failed to commit delivery", err)
		h.writeError(w, http.StatusInternalServerError, "commit failed: "+err.Error())
		return
	}

	module.IncrementReceivedMessages()
	h.writeJSON(w, http.StatusOK, map[string]string{"status": "sent"})
}
