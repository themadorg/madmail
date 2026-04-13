package webimap

import (
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/themadorg/madmail/framework/log"
)

func TestHandleSend_ForceAuthenticatedUser(t *testing.T) {
	authDB := &mockAuthDB{
		users: map[string]string{
			"alice@example.com": "secret123",
		},
	}
	// Enable WebSMTP
	authDB.SetSetting("websmtp_enabled", "true")

	storage := &mockStorage{
		users: map[string]*mockUser{
			"alice@example.com": {
				username: "alice@example.com",
				mailboxes: map[string]*mockMailbox{
					"INBOX": {name: "INBOX"},
				},
			},
		},
	}

	h := &Handler{
		AuthDB:            authDB,
		Storage:           storage,
		Logger:            log.Logger{Name: "webimap-test"},
		WebSMTPEnabledKey: "websmtp_enabled",
		MailDomain:        "example.com",
	}

	mux := http.NewServeMux()
	h.Register(mux, "/webimap")

	t.Run("Valid request", func(t *testing.T) {
		body := `{"to":["bob@example.com"],"body":"Content-Type: text/plain\r\nFrom: alice@example.com\r\nTo: bob@example.com\r\nSubject: Test\r\n\r\nHello"}`
		resp := doRequestWithSettings(mux, "POST", "/webimap/send", strings.NewReader(body), "alice@example.com", "secret123")
		
		// It should fail with "Encryption Needed" because it's text/plain
		if !strings.Contains(resp.Body.String(), "Encryption Needed") {
			t.Errorf("Expected 'Encryption Needed' error, got %d: %s", resp.Code, resp.Body.String())
		}
	})

	t.Run("Spoofing 'from' in JSON", func(t *testing.T) {
		body := `{"from":"bob@example.com","to":["bob@example.com"],"body":"Content-Type: text/plain\r\nFrom: alice@example.com\r\nTo: bob@example.com\r\nSubject: Test\r\n\r\nHello"}`
		resp := doRequestWithSettings(mux, "POST", "/webimap/send", strings.NewReader(body), "alice@example.com", "secret123")
		
		if !strings.Contains(resp.Body.String(), "Encryption Needed") {
			t.Errorf("Expected 'Encryption Needed' error (meaning 'from' check passed), got %d: %s", resp.Code, resp.Body.String())
		}
	})

	t.Run("Spoofing 'From' header in body", func(t *testing.T) {
		body := `{"to":["bob@example.com"],"body":"Content-Type: text/plain\r\nFrom: bob@example.com\r\nTo: bob@example.com\r\nSubject: Spoofed\r\n\r\nHello"}`
		resp := doRequestWithSettings(mux, "POST", "/webimap/send", strings.NewReader(body), "alice@example.com", "secret123")
		
		if !strings.Contains(resp.Body.String(), "From header in message (bob@example.com) does not match authenticated user (alice@example.com)") {
			t.Errorf("Expected From header mismatch error, got %d: %s", resp.Code, resp.Body.String())
		}
	})

	t.Run("Multiple From addresses", func(t *testing.T) {
		body := `{"to":["bob@example.com"],"body":"Content-Type: text/plain\r\nFrom: alice@example.com, bob@example.com\r\nTo: bob@example.com\r\nSubject: Multi\r\n\r\nHello"}`
		resp := doRequestWithSettings(mux, "POST", "/webimap/send", strings.NewReader(body), "alice@example.com", "secret123")

		if !strings.Contains(resp.Body.String(), "multiple addresses in From header are not allowed") {
			t.Errorf("Expected multiple From rejection error, got %d: %s", resp.Code, resp.Body.String())
		}
	})
}

// helper
func doRequestWithSettings(mux *http.ServeMux, method, url string, body io.Reader, email, password string) *httptest.ResponseRecorder {
	return doRequest(mux, method, url, body, email, password)
}
