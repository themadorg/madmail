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
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/emersion/go-imap"
	imapbackend "github.com/emersion/go-imap/backend"
	"github.com/gorilla/websocket"
)

// ---- WebSocket: bidirectional command protocol ----

var wsUpgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true }, // Allow any origin
}

// wsRequest is the JSON envelope the client sends over the WebSocket.
type wsRequest struct {
	ReqID  string          `json:"req_id"`  // Client-generated correlation ID
	Action string          `json:"action"`  // Command name
	Data   json.RawMessage `json:"data"`    // Action-specific payload
}

// wsResponse is the JSON envelope the server sends back.
type wsResponse struct {
	ReqID  string      `json:"req_id,omitempty"`  // Echoed from the request (empty for push)
	Action string      `json:"action"`            // "result", "error", or push action name
	Data   interface{} `json:"data"`              // Payload
}

// wsSendData is the payload for the "send" action.
type wsSendData struct {
	From string   `json:"from"`
	To   []string `json:"to"`
	Body string   `json:"body"` // raw RFC5322 message
}

// wsFetchData is the payload for the "fetch" action.
type wsFetchData struct {
	Mailbox string `json:"mailbox"`
	UID     uint32 `json:"uid"`
}

// wsListMessagesData is the payload for the "list_messages" action.
type wsListMessagesData struct {
	Mailbox  string `json:"mailbox"`
	SinceUID uint32 `json:"since_uid"`
}

// wsFlagsData is the payload for the "flags" action.
type wsFlagsData struct {
	Mailbox string   `json:"mailbox"`
	UID     uint32   `json:"uid"`
	Flags   []string `json:"flags"`
	Op      string   `json:"op"` // "add", "remove", "set"
}

// wsDeleteData is the payload for the "delete" action.
type wsDeleteData struct {
	Mailbox string `json:"mailbox"`
	UID     uint32 `json:"uid"`
}

// wsMoveData is the payload for the "move" action.
type wsMoveData struct {
	Mailbox     string `json:"mailbox"`
	DestMailbox string `json:"dest_mailbox"`
	UID         uint32 `json:"uid"`
}

// wsCopyData is the payload for the "copy" action.
type wsCopyData struct {
	Mailbox     string `json:"mailbox"`
	DestMailbox string `json:"dest_mailbox"`
	UID         uint32 `json:"uid"`
}

// wsSearchData is the payload for the "search" action.
type wsSearchData struct {
	Mailbox string `json:"mailbox"`
	Query   string `json:"query"` // Search string (matched against Subject, From, To)
}

// wsCreateMailboxData is the payload for the "create_mailbox" action.
type wsCreateMailboxData struct {
	Name string `json:"name"`
}

// wsDeleteMailboxData is the payload for the "delete_mailbox" action.
type wsDeleteMailboxData struct {
	Name string `json:"name"`
}

// wsRenameMailboxData is the payload for the "rename_mailbox" action.
type wsRenameMailboxData struct {
	OldName string `json:"old_name"`
	NewName string `json:"new_name"`
}

// handleWebSocket upgrades to WebSocket and provides a bidirectional command
// protocol on top of the connection.  In addition to responding to client
// commands, the server continuously pushes new-message notifications.
//
// Auth via query params: ?email=X&password=Y
func (h *Handler) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	// Check if WebSocket access is enabled
	if !h.isEnabled(h.WebIMAPEnabledKey) {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}

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

	watchMailbox := r.URL.Query().Get("mailbox")
	if watchMailbox == "" {
		watchMailbox = "INBOX"
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

	// Serialise writes — WebSocket connections are not safe for concurrent writes.
	writeMu := &sync.Mutex{}
	writeJSON := func(v interface{}) error {
		writeMu.Lock()
		defer writeMu.Unlock()
		conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
		return conn.WriteJSON(v)
	}
	writePing := func() error {
		writeMu.Lock()
		defer writeMu.Unlock()
		conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
		return conn.WriteMessage(websocket.PingMessage, nil)
	}

	// Read goroutine — parses JSON commands and dispatches them.
	done := make(chan struct{})
	go func() {
		defer close(done)
		for {
			_, raw, err := conn.ReadMessage()
			if err != nil {
				return
			}
			conn.SetReadDeadline(time.Now().Add(60 * time.Second))

			var req wsRequest
			if err := json.Unmarshal(raw, &req); err != nil {
				_ = writeJSON(wsResponse{Action: "error", Data: "invalid JSON"})
				continue
			}

			// Dispatch command
			h.dispatchWSCommand(r.Context(), conn, writeJSON, user, email, req)
		}
	}()

	// Push loop: poll IMAP and push new messages as "new_message" events.
	ticker := time.NewTicker(2 * time.Second)
	defer ticker.Stop()

	pingTicker := time.NewTicker(30 * time.Second)
	defer pingTicker.Stop()

	for {
		select {
		case <-ticker.C:
			msgs, err := h.fetchMessages(user, watchMailbox, lastUID)
			if err != nil {
				h.Logger.Error("ws: failed to fetch messages", err)
				continue
			}
			for _, summary := range msgs {
				if summary.UID > lastUID {
					lastUID = summary.UID
				}
				// For push notifications, only send the summary (uid + envelope).
				// The client can use "fetch" to retrieve the full body.
				if err := writeJSON(wsResponse{
					Action: "new_message",
					Data:   summary,
				}); err != nil {
					return
				}
			}
		case <-pingTicker.C:
			if err := writePing(); err != nil {
				return
			}
		case <-done:
			return
		case <-r.Context().Done():
			return
		}
	}
}

// dispatchWSCommand handles a single client command received over WebSocket.
func (h *Handler) dispatchWSCommand(
	ctx context.Context,
	conn *websocket.Conn,
	writeJSON func(interface{}) error,
	user imapbackend.User,
	email string,
	req wsRequest,
) {
	respond := func(data interface{}) {
		_ = writeJSON(wsResponse{ReqID: req.ReqID, Action: "result", Data: data})
	}
	respondErr := func(msg string) {
		_ = writeJSON(wsResponse{ReqID: req.ReqID, Action: "error", Data: msg})
	}

	switch req.Action {

	// ---- send ----
	case "send":
		// Check if WebSMTP is enabled for send operations
		if !h.isEnabled(h.WebSMTPEnabledKey) {
			respondErr("send is not enabled")
			return
		}
		var d wsSendData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid send payload: " + err.Error())
			return
		}
		if d.From == "" {
			d.From = email
		}
		if !strings.EqualFold(d.From, email) {
			respondErr("sender must match authenticated user")
			return
		}
		if len(d.To) == 0 {
			respondErr("missing recipients")
			return
		}
		h.wsHandleSend(ctx, respond, respondErr, user, d)

	// ---- fetch (single message by UID) ----
	case "fetch":
		var d wsFetchData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid fetch payload: " + err.Error())
			return
		}
		if d.Mailbox == "" {
			d.Mailbox = "INBOX"
		}
		detail := h.fetchFullMessage(user, d.Mailbox, d.UID)
		if detail == nil {
			respondErr("message not found")
			return
		}
		respond(detail)

	// ---- list_mailboxes ----
	case "list_mailboxes":
		infos, err := user.ListMailboxes(false)
		if err != nil {
			respondErr("failed to list mailboxes: " + err.Error())
			return
		}
		result := make([]MailboxInfo, 0, len(infos))
		for _, info := range infos {
			mi := MailboxInfo{Name: info.Name, Attributes: info.Attributes}
			if status, err := user.Status(info.Name, []imap.StatusItem{imap.StatusMessages, imap.StatusUnseen}); err == nil {
				mi.Messages = status.Messages
				mi.Unseen = status.Unseen
			}
			result = append(result, mi)
		}
		respond(result)

	// ---- list_messages ----
	case "list_messages":
		var d wsListMessagesData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid list_messages payload: " + err.Error())
			return
		}
		if d.Mailbox == "" {
			d.Mailbox = "INBOX"
		}
		msgs, err := h.fetchMessages(user, d.Mailbox, d.SinceUID)
		if err != nil {
			respondErr("failed to list messages: " + err.Error())
			return
		}
		respond(msgs)

	// ---- flags ----
	case "flags":
		var d wsFlagsData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid flags payload: " + err.Error())
			return
		}
		if d.Mailbox == "" {
			d.Mailbox = "INBOX"
		}
		_, mbox, err := user.GetMailbox(d.Mailbox, false, nil)
		if err != nil {
			respondErr("failed to open mailbox")
			return
		}
		seqSet := new(imap.SeqSet)
		seqSet.AddNum(d.UID)
		var op imap.FlagsOp
		switch d.Op {
		case "add":
			op = imap.AddFlags
		case "remove":
			op = imap.RemoveFlags
		case "set":
			op = imap.SetFlags
		default:
			respondErr("invalid op: must be add, remove, or set")
			return
		}
		if err := mbox.UpdateMessagesFlags(true, seqSet, op, false, d.Flags); err != nil {
			respondErr("failed to update flags: " + err.Error())
			return
		}
		respond(map[string]string{"status": "ok"})

	// ---- delete ----
	case "delete":
		var d wsDeleteData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid delete payload: " + err.Error())
			return
		}
		if d.Mailbox == "" {
			d.Mailbox = "INBOX"
		}
		_, mbox, err := user.GetMailbox(d.Mailbox, false, nil)
		if err != nil {
			respondErr("failed to open mailbox")
			return
		}
		seqSet := new(imap.SeqSet)
		seqSet.AddNum(d.UID)
		if err := mbox.UpdateMessagesFlags(true, seqSet, imap.AddFlags, false, []string{imap.DeletedFlag}); err != nil {
			respondErr("failed to delete message: " + err.Error())
			return
		}
		if expMbox, ok := mbox.(interface{ Expunge() error }); ok {
			_ = expMbox.Expunge()
		}
		respond(map[string]string{"status": "deleted"})

	// ---- move ----
	case "move":
		var d wsMoveData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid move payload: " + err.Error())
			return
		}
		if d.Mailbox == "" {
			d.Mailbox = "INBOX"
		}
		if d.DestMailbox == "" {
			respondErr("dest_mailbox is required")
			return
		}
		_, mbox, err := user.GetMailbox(d.Mailbox, false, nil)
		if err != nil {
			respondErr("failed to open source mailbox")
			return
		}
		seqSet := new(imap.SeqSet)
		seqSet.AddNum(d.UID)
		// Copy then delete
		if err := mbox.CopyMessages(true, seqSet, d.DestMailbox); err != nil {
			respondErr("failed to copy message: " + err.Error())
			return
		}
		if err := mbox.UpdateMessagesFlags(true, seqSet, imap.AddFlags, false, []string{imap.DeletedFlag}); err != nil {
			respondErr("failed to remove original: " + err.Error())
			return
		}
		if expMbox, ok := mbox.(interface{ Expunge() error }); ok {
			_ = expMbox.Expunge()
		}
		respond(map[string]string{"status": "moved"})

	// ---- copy ----
	case "copy":
		var d wsCopyData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid copy payload: " + err.Error())
			return
		}
		if d.Mailbox == "" {
			d.Mailbox = "INBOX"
		}
		if d.DestMailbox == "" {
			respondErr("dest_mailbox is required")
			return
		}
		_, mbox, err := user.GetMailbox(d.Mailbox, false, nil)
		if err != nil {
			respondErr("failed to open mailbox")
			return
		}
		seqSet := new(imap.SeqSet)
		seqSet.AddNum(d.UID)
		if err := mbox.CopyMessages(true, seqSet, d.DestMailbox); err != nil {
			respondErr("failed to copy message: " + err.Error())
			return
		}
		respond(map[string]string{"status": "copied"})

	// ---- search ----
	case "search":
		var d wsSearchData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid search payload: " + err.Error())
			return
		}
		if d.Mailbox == "" {
			d.Mailbox = "INBOX"
		}
		if d.Query == "" {
			respondErr("query is required")
			return
		}
		// Fetch all messages and filter client-side (the IMAP backend doesn't
		// expose a full-text search; we match against envelope fields).
		msgs, err := h.fetchMessages(user, d.Mailbox, 0)
		if err != nil {
			respondErr("failed to search: " + err.Error())
			return
		}
		query := strings.ToLower(d.Query)
		matched := make([]MessageSummary, 0)
		for _, m := range msgs {
			if strings.Contains(strings.ToLower(m.Envelope.Subject), query) {
				matched = append(matched, m)
				continue
			}
			for _, a := range m.Envelope.From {
				if strings.Contains(strings.ToLower(a.Mailbox+"@"+a.Host), query) ||
					strings.Contains(strings.ToLower(a.Name), query) {
					matched = append(matched, m)
					break
				}
			}
		}
		respond(matched)

	// ---- create_mailbox ----
	case "create_mailbox":
		var d wsCreateMailboxData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid create_mailbox payload: " + err.Error())
			return
		}
		if d.Name == "" {
			respondErr("name is required")
			return
		}
		if err := user.CreateMailbox(d.Name); err != nil {
			respondErr("failed to create mailbox: " + err.Error())
			return
		}
		respond(map[string]string{"status": "created"})

	// ---- delete_mailbox ----
	case "delete_mailbox":
		var d wsDeleteMailboxData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid delete_mailbox payload: " + err.Error())
			return
		}
		if d.Name == "" {
			respondErr("name is required")
			return
		}
		if err := user.DeleteMailbox(d.Name); err != nil {
			respondErr("failed to delete mailbox: " + err.Error())
			return
		}
		respond(map[string]string{"status": "deleted"})

	// ---- rename_mailbox ----
	case "rename_mailbox":
		var d wsRenameMailboxData
		if err := json.Unmarshal(req.Data, &d); err != nil {
			respondErr("invalid rename_mailbox payload: " + err.Error())
			return
		}
		if d.OldName == "" || d.NewName == "" {
			respondErr("old_name and new_name are required")
			return
		}
		if err := user.RenameMailbox(d.OldName, d.NewName); err != nil {
			respondErr("failed to rename mailbox: " + err.Error())
			return
		}
		respond(map[string]string{"status": "renamed"})

	default:
		respondErr("unknown action: " + req.Action)
	}
}

// wsHandleSend delivers an email for the authenticated user over WebSocket.
func (h *Handler) wsHandleSend(
	ctx context.Context,
	respond func(interface{}),
	respondErr func(string),
	user imapbackend.User,
	d wsSendData,
) {
	if err := h.deliverMessage(ctx, d.From, d.To, d.Body); err != nil {
		respondErr(err.Error())
		return
	}
	respond(map[string]string{"status": "sent"})
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
