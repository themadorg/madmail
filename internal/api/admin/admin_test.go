package admin

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/themadorg/madmail/framework/log"
)

func TestHandler_InvalidMethod(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})
	req := httptest.NewRequest(http.MethodGet, "/api/admin", nil)
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	// Outer HTTP is always 200; check inner status
	if w.Code != http.StatusOK {
		t.Errorf("expected HTTP 200, got %d", w.Code)
	}
	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != http.StatusMethodNotAllowed {
		t.Errorf("expected inner status 405, got %d", resp.Status)
	}
}

func TestHandler_InvalidJSON(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBufferString("not json"))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != http.StatusBadRequest {
		t.Errorf("expected inner status 400, got %d", resp.Status)
	}
}

func TestHandler_MissingAuth(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})
	body := Request{
		Method:   "GET",
		Resource: "/admin/status",
	}
	data, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != http.StatusUnauthorized {
		t.Errorf("expected inner status 401, got %d", resp.Status)
	}
}

func TestHandler_WrongToken(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})
	body := Request{
		Method:   "GET",
		Resource: "/admin/status",
		Headers:  map[string]string{"Authorization": "Bearer wrong-token"},
	}
	data, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != http.StatusUnauthorized {
		t.Errorf("expected inner status 401, got %d", resp.Status)
	}
}

func TestHandler_CorrectAuth(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})
	h.Register("/admin/test", func(method string, body json.RawMessage) (interface{}, int, error) {
		return map[string]string{"ok": "true"}, 200, nil
	})

	body := Request{
		Method:   "GET",
		Resource: "/admin/test",
		Headers:  map[string]string{"Authorization": "Bearer test-token"},
	}
	data, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != 200 {
		t.Errorf("expected response status 200, got %d", resp.Status)
	}
	if resp.Error != nil {
		t.Errorf("expected no error, got %s", *resp.Error)
	}
	if resp.Resource != "/admin/test" {
		t.Errorf("expected resource /admin/test, got %s", resp.Resource)
	}
}

func TestHandler_UnknownResource(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})
	body := Request{
		Method:   "GET",
		Resource: "/admin/nonexistent",
		Headers:  map[string]string{"Authorization": "Bearer test-token"},
	}
	data, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != http.StatusNotFound {
		t.Errorf("expected inner status 404, got %d", resp.Status)
	}
}

func TestHandler_RateLimiting(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})

	// Exceed rate limit with wrong tokens
	for i := 0; i < 11; i++ {
		body := Request{
			Method:   "GET",
			Resource: "/admin/status",
			Headers:  map[string]string{"Authorization": "Bearer wrong"},
		}
		data, _ := json.Marshal(body)
		req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
		req.RemoteAddr = "1.2.3.4:12345"
		w := httptest.NewRecorder()
		h.ServeHTTP(w, req)
	}

	// Now even a correct token should fail from the same IP
	body := Request{
		Method:   "GET",
		Resource: "/admin/status",
		Headers:  map[string]string{"Authorization": "Bearer test-token"},
	}
	data, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
	req.RemoteAddr = "1.2.3.4:12345"
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != http.StatusUnauthorized {
		t.Errorf("expected inner status 401 after rate limit, got %d", resp.Status)
	}
}

func TestHandler_EmptyToken(t *testing.T) {
	// Handler with empty token should reject everything
	h := NewHandler("", log.Logger{})
	h.Register("/admin/test", func(method string, body json.RawMessage) (interface{}, int, error) {
		return map[string]string{"ok": "true"}, 200, nil
	})

	body := Request{
		Method:   "GET",
		Resource: "/admin/test",
		Headers:  map[string]string{"Authorization": "Bearer anything"},
	}
	data, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	var resp Response
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != http.StatusUnauthorized {
		t.Errorf("expected inner status 401 for empty token handler, got %d", resp.Status)
	}
}

func TestHandler_DefaultMethod(t *testing.T) {
	h := NewHandler("test-token", log.Logger{})
	var receivedMethod string
	h.Register("/admin/test", func(method string, body json.RawMessage) (interface{}, int, error) {
		receivedMethod = method
		return nil, 200, nil
	})

	body := Request{
		Resource: "/admin/test",
		Headers:  map[string]string{"Authorization": "Bearer test-token"},
		// No method specified
	}
	data, _ := json.Marshal(body)
	req := httptest.NewRequest(http.MethodPost, "/api/admin", bytes.NewBuffer(data))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)

	if receivedMethod != "GET" {
		t.Errorf("expected default method GET, got %s", receivedMethod)
	}
}
