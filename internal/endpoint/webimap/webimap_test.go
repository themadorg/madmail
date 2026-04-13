package webimap

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/emersion/go-imap"
	imapbackend "github.com/emersion/go-imap/backend"
	"github.com/themadorg/madmail/framework/log"
)

// ---- mock types ----

type mockAuthDB struct {
	users    map[string]string // email -> password
	settings map[string]string
}

func (m *mockAuthDB) AuthPlain(username, password string) error {
	if pw, ok := m.users[username]; ok && pw == password {
		return nil
	}
	return imapbackend.ErrInvalidCredentials
}
func (m *mockAuthDB) ListUsers() ([]string, error)                     { return nil, nil }
func (m *mockAuthDB) CreateUser(username, password string) error        { return nil }
func (m *mockAuthDB) SetUserPassword(username, password string) error   { return nil }
func (m *mockAuthDB) DeleteUser(username string) error                  { return nil }
func (m *mockAuthDB) IsRegistrationOpen() (bool, error)                 { return true, nil }
func (m *mockAuthDB) SetRegistrationOpen(open bool) error               { return nil }
func (m *mockAuthDB) IsJitRegistrationEnabled() (bool, error)           { return false, nil }
func (m *mockAuthDB) SetJitRegistrationEnabled(enabled bool) error      { return nil }
func (m *mockAuthDB) IsTurnEnabled() (bool, error)                      { return false, nil }
func (m *mockAuthDB) SetTurnEnabled(enabled bool) error                 { return nil }
func (m *mockAuthDB) IsLoggingDisabled() (bool, error)                  { return false, nil }
func (m *mockAuthDB) SetLoggingDisabled(disabled bool) error            { return nil }
func (m *mockAuthDB) GetSetting(key string) (string, bool, error) {
	if v, ok := m.settings[key]; ok {
		return v, true, nil
	}
	return "", false, nil
}
func (m *mockAuthDB) SetSetting(key, value string) error {
	if m.settings == nil {
		m.settings = make(map[string]string)
	}
	m.settings[key] = value
	return nil
}
func (m *mockAuthDB) DeleteSetting(key string) error {
	delete(m.settings, key)
	return nil
}

type mockStorage struct {
	users map[string]*mockUser
}

func (m *mockStorage) GetOrCreateIMAPAcct(username string) (imapbackend.User, error) {
	if u, ok := m.users[username]; ok {
		return u, nil
	}
	return nil, imapbackend.ErrInvalidCredentials
}

func (m *mockStorage) GetIMAPAcct(username string) (imapbackend.User, error) {
	return m.GetOrCreateIMAPAcct(username)
}

func (m *mockStorage) IMAPExtensions() []string {
	return nil
}

type mockUser struct {
	username  string
	mailboxes map[string]*mockMailbox
}

func (u *mockUser) Username() string { return u.username }
func (u *mockUser) ListMailboxes(subscribed bool) ([]imap.MailboxInfo, error) {
	result := make([]imap.MailboxInfo, 0, len(u.mailboxes))
	for name, mbox := range u.mailboxes {
		result = append(result, imap.MailboxInfo{
			Name:       name,
			Attributes: mbox.attributes,
		})
	}
	return result, nil
}

func (u *mockUser) GetMailbox(name string, readOnly bool, conn imapbackend.Conn) (*imap.MailboxStatus, imapbackend.Mailbox, error) {
	mbox, ok := u.mailboxes[name]
	if !ok {
		return nil, nil, imapbackend.ErrNoSuchMailbox
	}
	status := &imap.MailboxStatus{
		Name:     name,
		Messages: uint32(len(mbox.messages)),
		Unseen:   0,
	}
	return status, mbox, nil
}

func (u *mockUser) CreateMailbox(name string) error              { return nil }
func (u *mockUser) DeleteMailbox(name string) error              { return nil }
func (u *mockUser) RenameMailbox(existing, newName string) error  { return nil }
func (u *mockUser) SetSubscribed(name string, subscribed bool) error { return nil }
func (u *mockUser) Logout() error                                { return nil }

func (u *mockUser) Status(name string, items []imap.StatusItem) (*imap.MailboxStatus, error) {
	mbox, ok := u.mailboxes[name]
	if !ok {
		return nil, imapbackend.ErrNoSuchMailbox
	}
	status := imap.NewMailboxStatus(name, items)
	status.Messages = uint32(len(mbox.messages))
	status.Unseen = 0
	for _, msg := range mbox.messages {
		seen := false
		for _, f := range msg.flags {
			if f == imap.SeenFlag {
				seen = true
				break
			}
		}
		if !seen {
			status.Unseen++
		}
	}
	return status, nil
}

func (u *mockUser) CreateMessage(mbox string, flags []string, date time.Time, body imap.Literal, mboxObj imapbackend.Mailbox) error {
	return nil
}

type mockMessage struct {
	uid   uint32
	flags []string
	body  string
	env   *imap.Envelope
	date  time.Time
}

type mockMailbox struct {
	name       string
	attributes []string
	messages   []*mockMessage
}

func (m *mockMailbox) Name() string { return m.name }
func (m *mockMailbox) Info() (*imap.MailboxInfo, error) {
	return &imap.MailboxInfo{Name: m.name, Attributes: m.attributes}, nil
}
func (m *mockMailbox) Status(items []imap.StatusItem) (*imap.MailboxStatus, error) {
	return imap.NewMailboxStatus(m.name, items), nil
}
func (m *mockMailbox) SetSubscribed(subscribed bool) error { return nil }
func (m *mockMailbox) Check() error                        { return nil }

func (m *mockMailbox) ListMessages(uid bool, seqSet *imap.SeqSet, items []imap.FetchItem, ch chan<- *imap.Message) error {
	defer close(ch)
	for i, msg := range m.messages {
		var match bool
		if uid {
			match = seqSet.Contains(msg.uid)
		} else {
			match = seqSet.Contains(uint32(i + 1))
		}
		if !match {
			continue
		}

		imapMsg := &imap.Message{
			SeqNum:       uint32(i + 1),
			Uid:          msg.uid,
			Flags:        msg.flags,
			Size:         uint32(len(msg.body)),
			InternalDate: msg.date,
			Envelope:     msg.env,
			Body:         make(map[*imap.BodySectionName]imap.Literal),
		}

		// Check if body was requested
		for _, item := range items {
			if strings.HasPrefix(string(item), "BODY") {
				section := &imap.BodySectionName{Peek: true}
				imapMsg.Body[section] = strings.NewReader(msg.body)
			}
		}

		ch <- imapMsg
	}
	return nil
}

func (m *mockMailbox) SearchMessages(uid bool, criteria *imap.SearchCriteria) ([]uint32, error) {
	return nil, nil
}
func (m *mockMailbox) CreateMessage(flags []string, date time.Time, body imap.Literal) error {
	return nil
}
func (m *mockMailbox) UpdateMessagesFlags(uid bool, seqSet *imap.SeqSet, op imap.FlagsOp, silent bool, flags []string) error {
	return nil
}
func (m *mockMailbox) CopyMessages(uid bool, seqSet *imap.SeqSet, destName string) error {
	return nil
}
func (m *mockMailbox) Expunge() error { return nil }
func (m *mockMailbox) Close() error   { return nil }
func (m *mockMailbox) Poll(expunge bool) error { return nil }
func (m *mockMailbox) Idle(done <-chan struct{}) {}

// ---- test setup ----

func newTestHandler() (*Handler, *http.ServeMux) {
	now := time.Now()
	authDB := &mockAuthDB{
		users: map[string]string{
			"test@example.com": "secret123",
		},
	}
	storage := &mockStorage{
		users: map[string]*mockUser{
			"test@example.com": {
				username: "test@example.com",
				mailboxes: map[string]*mockMailbox{
					"INBOX": {
						name: "INBOX",
						messages: []*mockMessage{
							{
								uid:   1,
								flags: []string{},
								body:  "Subject: Hello\r\n\r\nThis is a test message.",
								date:  now.Add(-1 * time.Hour),
								env: &imap.Envelope{
									Date:      now.Add(-1 * time.Hour),
									Subject:   "Hello",
									From:      []*imap.Address{{PersonalName: "Alice", MailboxName: "alice", HostName: "example.com"}},
									To:        []*imap.Address{{PersonalName: "Test", MailboxName: "test", HostName: "example.com"}},
									MessageId: "<msg1@example.com>",
								},
							},
							{
								uid:   2,
								flags: []string{imap.SeenFlag},
								body:  "Subject: World\r\n\r\nSecond message.",
								date:  now,
								env: &imap.Envelope{
									Date:    now,
									Subject: "World",
									From:    []*imap.Address{{PersonalName: "Bob", MailboxName: "bob", HostName: "example.com"}},
									To:      []*imap.Address{{PersonalName: "Test", MailboxName: "test", HostName: "example.com"}},
								},
							},
						},
					},
					"Sent": {
						name:       "Sent",
						attributes: []string{imap.SentAttr},
						messages:   []*mockMessage{},
					},
				},
			},
		},
	}
	h := &Handler{
		AuthDB:            authDB,
		Storage:           storage,
		Logger:            log.Logger{Name: "webimap-test"},
		WebIMAPEnabledKey: "webimap_enabled",
		WebSMTPEnabledKey: "websmtp_enabled",
	}
	authDB.SetSetting("webimap_enabled", "true")
	authDB.SetSetting("websmtp_enabled", "true")
	mux := http.NewServeMux()
	h.Register(mux, "/webimap")
	return h, mux
}

func doRequest(mux *http.ServeMux, method, url string, body io.Reader, email, password string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(method, url, body)
	if email != "" {
		req.Header.Set("X-Email", email)
	}
	if password != "" {
		req.Header.Set("X-Password", password)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)
	return w
}

// ---- test functions ----

func TestMailboxes_Unauthorized(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "GET", "/webimap/mailboxes", nil, "", "")
	if resp.Code != http.StatusUnauthorized {
		t.Errorf("expected 401, got %d", resp.Code)
	}
}

func TestMailboxes_WrongPassword(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "GET", "/webimap/mailboxes", nil, "test@example.com", "wrong")
	if resp.Code != http.StatusUnauthorized {
		t.Errorf("expected 401, got %d", resp.Code)
	}
}

func TestMailboxes_Success(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "GET", "/webimap/mailboxes", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", resp.Code, resp.Body.String())
	}

	var mailboxes []MailboxInfo
	if err := json.Unmarshal(resp.Body.Bytes(), &mailboxes); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}

	if len(mailboxes) != 2 {
		t.Errorf("expected 2 mailboxes, got %d", len(mailboxes))
	}

	found := false
	for _, mb := range mailboxes {
		if mb.Name == "INBOX" {
			found = true
			if mb.Messages != 2 {
				t.Errorf("INBOX expected 2 messages, got %d", mb.Messages)
			}
		}
	}
	if !found {
		t.Error("INBOX not found in mailboxes list")
	}
}

func TestMessages_Success(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "GET", "/webimap/messages?mailbox=INBOX", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", resp.Code, resp.Body.String())
	}

	var msgs []MessageSummary
	if err := json.Unmarshal(resp.Body.Bytes(), &msgs); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}

	if len(msgs) != 2 {
		t.Errorf("expected 2 messages, got %d", len(msgs))
	}

	if msgs[0].UID != 1 {
		t.Errorf("expected first message UID=1, got %d", msgs[0].UID)
	}
	if msgs[0].Envelope.Subject != "Hello" {
		t.Errorf("expected subject 'Hello', got '%s'", msgs[0].Envelope.Subject)
	}
}

func TestMessages_SinceUID(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "GET", "/webimap/messages?mailbox=INBOX&since_uid=1", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", resp.Code, resp.Body.String())
	}

	var msgs []MessageSummary
	if err := json.Unmarshal(resp.Body.Bytes(), &msgs); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}

	if len(msgs) != 1 {
		t.Errorf("expected 1 message (UID>1), got %d", len(msgs))
	}
	if len(msgs) > 0 && msgs[0].UID != 2 {
		t.Errorf("expected UID=2, got %d", msgs[0].UID)
	}
}

func TestGetMessage_Success(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "GET", "/webimap/message/1?mailbox=INBOX", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", resp.Code, resp.Body.String())
	}

	var msg MessageDetail
	if err := json.Unmarshal(resp.Body.Bytes(), &msg); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}

	if msg.UID != 1 {
		t.Errorf("expected UID=1, got %d", msg.UID)
	}
	if msg.Body == "" {
		t.Error("expected non-empty body")
	}
	if msg.Envelope.Subject != "Hello" {
		t.Errorf("expected subject 'Hello', got '%s'", msg.Envelope.Subject)
	}
}

func TestGetMessage_NotFound(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "GET", "/webimap/message/999?mailbox=INBOX", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", resp.Code)
	}
}

func TestDeleteMessage(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "DELETE", "/webimap/message/1?mailbox=INBOX", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", resp.Code, resp.Body.String())
	}
}

func TestUpdateFlags(t *testing.T) {
	_, mux := newTestHandler()
	body := `{"mailbox":"INBOX","uid":1,"flags":["\\Seen"],"op":"add"}`
	resp := doRequest(mux, "POST", "/webimap/message/flags", strings.NewReader(body), "test@example.com", "secret123")
	if resp.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", resp.Code, resp.Body.String())
	}
}

func TestUpdateFlags_InvalidOp(t *testing.T) {
	_, mux := newTestHandler()
	body := `{"mailbox":"INBOX","uid":1,"flags":["\\Seen"],"op":"invalid"}`
	resp := doRequest(mux, "POST", "/webimap/message/flags", strings.NewReader(body), "test@example.com", "secret123")
	if resp.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", resp.Code)
	}
}

func TestMailboxes_MethodNotAllowed(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "POST", "/webimap/mailboxes", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusMethodNotAllowed {
		t.Errorf("expected 405, got %d", resp.Code)
	}
}

func TestCORS_Preflight(t *testing.T) {
	_, mux := newTestHandler()
	resp := doRequest(mux, "OPTIONS", "/webimap/mailboxes", nil, "", "")
	if resp.Code != http.StatusNoContent {
		t.Errorf("expected 204, got %d", resp.Code)
	}
	if h := resp.Header().Get("Access-Control-Allow-Origin"); h != "*" {
		t.Errorf("expected CORS *, got '%s'", h)
	}
}

func TestMessages_DefaultMailbox(t *testing.T) {
	_, mux := newTestHandler()
	// No mailbox param → defaults to INBOX
	resp := doRequest(mux, "GET", "/webimap/messages", nil, "test@example.com", "secret123")
	if resp.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", resp.Code, resp.Body.String())
	}

	var msgs []MessageSummary
	if err := json.Unmarshal(resp.Body.Bytes(), &msgs); err != nil {
		t.Fatalf("failed to decode: %v", err)
	}
	if len(msgs) != 2 {
		t.Errorf("expected 2 messages in default INBOX, got %d", len(msgs))
	}
}
