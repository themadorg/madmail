package chatmail

import (
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestReadIncomingHTTPMessageSuccess(t *testing.T) {
	t.Parallel()

	e := Endpoint{
		maxMessageBytes: 1024,
		msgBufferDir:    t.TempDir(),
	}

	req := httptest.NewRequest(http.MethodPost, "/mxdeliv", strings.NewReader("Subject: hello\r\nX-Test: 1\r\n\r\npayload"))
	rr := httptest.NewRecorder()

	header, bodyBuf, err := e.readIncomingHTTPMessage(rr, req)
	if err != nil {
		t.Fatalf("readIncomingHTTPMessage failed: %v", err)
	}
	defer bodyBuf.Remove()

	if got := header.Get("Subject"); got != "hello" {
		t.Fatalf("unexpected subject header: got %q", got)
	}

	r, err := bodyBuf.Open()
	if err != nil {
		t.Fatalf("body buffer open failed: %v", err)
	}
	defer r.Close()

	body, err := io.ReadAll(r)
	if err != nil {
		t.Fatalf("body read failed: %v", err)
	}
	if got := string(body); got != "payload" {
		t.Fatalf("unexpected payload: got %q", got)
	}
}

func TestReadIncomingHTTPMessageTooLarge(t *testing.T) {
	t.Parallel()

	e := Endpoint{
		maxMessageBytes: 32,
		msgBufferDir:    t.TempDir(),
	}

	reqBody := "Subject: a\r\n\r\n" + strings.Repeat("x", 128)
	req := httptest.NewRequest(http.MethodPost, "/mxdeliv", strings.NewReader(reqBody))
	rr := httptest.NewRecorder()

	_, _, err := e.readIncomingHTTPMessage(rr, req)
	if err == nil {
		t.Fatal("expected size limit error, got nil")
	}
	if !isMaxBytesError(err) {
		t.Fatalf("expected max bytes error, got: %v", err)
	}
}
