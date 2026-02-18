// Package admin implements a single-endpoint RPC-style Admin API for Madmail.
//
// Architecture: All requests are POST /api/admin with a JSON body:
//
//	{
//	    "method": "GET|POST|PUT|DELETE|PATCH",
//	    "resource": "/admin/status",
//	    "headers": { "Authorization": "Bearer <token>" },
//	    "body": {}
//	}
//
// Response format:
//
//	{
//	    "status": 200,
//	    "resource": "/admin/status",
//	    "body": { ... },
//	    "error": null
//	}
//
// This design makes the API easier to hide behind reverse proxies,
// firewalls, or custom auth layers — one port, one path, one handler.
package admin

import (
	"crypto/subtle"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/themadorg/madmail/framework/log"
)

// Request is the JSON-RPC style request envelope.
type Request struct {
	Method   string            `json:"method"`
	Resource string            `json:"resource"`
	Headers  map[string]string `json:"headers"`
	Body     json.RawMessage   `json:"body"`
}

// Response is the JSON-RPC style response envelope.
type Response struct {
	Status   int         `json:"status"`
	Resource string      `json:"resource"`
	Body     interface{} `json:"body"`
	Error    *string     `json:"error"`
}

// ResourceHandler is the signature for individual resource handlers.
// It receives the parsed request and returns a response body and status code.
type ResourceHandler func(method string, body json.RawMessage) (interface{}, int, error)

// Handler is the main Admin API handler.
type Handler struct {
	mu        sync.RWMutex
	token     string
	logger    log.Logger
	resources map[string]ResourceHandler

	// Rate limiting
	lastAttempts map[string][]time.Time
	rateMu       sync.Mutex
}

// NewHandler creates a new Admin API handler.
func NewHandler(token string, logger log.Logger) *Handler {
	h := &Handler{
		token:        token,
		logger:       logger,
		resources:    make(map[string]ResourceHandler),
		lastAttempts: make(map[string][]time.Time),
	}

	// Background cleanup of stale rate-limit entries to prevent memory leaks
	// from IPs that never return after failing auth.
	go func() {
		ticker := time.NewTicker(5 * time.Minute)
		defer ticker.Stop()
		for range ticker.C {
			h.cleanupRateLimitEntries()
		}
	}()

	return h
}

// Register adds a resource handler for the given path.
func (h *Handler) Register(path string, handler ResourceHandler) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.resources[path] = handler
}

// maxRequestSize is the maximum size of an Admin API request body (1 MB).
// This is enforced before authentication to prevent memory exhaustion DoS.
const maxRequestSize = 1 << 20 // 1 MB

// ServeHTTP implements http.Handler.
func (h *Handler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")

	// CORS headers — required for admin dashboard web app
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.Header().Set("Access-Control-Allow-Methods", "POST, OPTIONS")
	w.Header().Set("Access-Control-Allow-Headers", "Content-Type")

	// Handle CORS preflight
	if r.Method == http.MethodOptions {
		w.WriteHeader(http.StatusNoContent)
		return
	}

	// Only accept POST
	if r.Method != http.MethodPost {
		writeResponse(w, Response{
			Status: http.StatusMethodNotAllowed,
			Error:  strPtr("only POST is accepted"),
		})
		return
	}

	// Limit request body size to prevent memory exhaustion (applied before auth)
	limitedBody := io.LimitReader(r.Body, maxRequestSize+1)

	// Parse the RPC request envelope
	var req Request
	if err := json.NewDecoder(limitedBody).Decode(&req); err != nil {
		writeResponse(w, Response{
			Status: http.StatusBadRequest,
			Error:  strPtr("invalid JSON request body"),
		})
		return
	}

	// Authenticate via Bearer token in the inner headers
	if !h.authenticate(req.Headers, r.RemoteAddr) {
		// Don't echo back the resource — prevents unauthenticated probing
		writeResponse(w, Response{
			Status: http.StatusUnauthorized,
			Error:  strPtr("unauthorized"),
		})
		return
	}

	// Normalize method
	method := strings.ToUpper(req.Method)
	if method == "" {
		method = "GET"
	}

	// Dispatch to resource handler
	h.mu.RLock()
	handler, ok := h.resources[req.Resource]
	h.mu.RUnlock()

	if !ok {
		writeResponse(w, Response{
			Status:   http.StatusNotFound,
			Resource: req.Resource,
			Error:    strPtr("unknown resource: " + req.Resource),
		})
		return
	}

	body, status, err := handler(method, req.Body)
	if err != nil {
		writeResponse(w, Response{
			Status:   status,
			Resource: req.Resource,
			Error:    strPtr(err.Error()),
		})
		return
	}

	writeResponse(w, Response{
		Status:   status,
		Resource: req.Resource,
		Body:     body,
	})
}

// authenticate checks the Bearer token from the inner request headers.
// Returns false if the token is missing or invalid.
func (h *Handler) authenticate(headers map[string]string, remoteAddr string) bool {
	if h.token == "" {
		return false
	}

	authHeader := headers["Authorization"]
	if authHeader == "" {
		authHeader = headers["authorization"]
	}
	if authHeader == "" {
		return false
	}

	const prefix = "Bearer "
	if !strings.HasPrefix(authHeader, prefix) {
		return false
	}

	token := strings.TrimPrefix(authHeader, prefix)

	// Rate limiting: max 10 FAILED attempts per minute per IP
	ip := extractIP(remoteAddr)
	if !h.checkRateLimit(ip) {
		return false
	}

	// Constant-time comparison to prevent timing attacks
	if subtle.ConstantTimeCompare([]byte(token), []byte(h.token)) == 1 {
		// Successful auth: clear failed attempts for this IP
		h.clearFailedAttempts(ip)
		return true
	}

	// Failed auth: record the attempt
	h.recordFailedAttempt(ip)
	return false
}

// checkRateLimit returns true if the IP is allowed to attempt auth.
// It checks the count of recent failed attempts without incrementing.
func (h *Handler) checkRateLimit(ip string) bool {
	h.rateMu.Lock()
	defer h.rateMu.Unlock()

	now := time.Now()
	cutoff := now.Add(-1 * time.Minute)

	// Clean old entries
	attempts := h.lastAttempts[ip]
	var recent []time.Time
	for _, t := range attempts {
		if t.After(cutoff) {
			recent = append(recent, t)
		}
	}
	h.lastAttempts[ip] = recent

	return len(recent) < 10
}

// recordFailedAttempt records a failed auth attempt for rate limiting.
func (h *Handler) recordFailedAttempt(ip string) {
	h.rateMu.Lock()
	defer h.rateMu.Unlock()
	h.lastAttempts[ip] = append(h.lastAttempts[ip], time.Now())
}

// clearFailedAttempts clears the failed attempt history for an IP after successful auth.
func (h *Handler) clearFailedAttempts(ip string) {
	h.rateMu.Lock()
	defer h.rateMu.Unlock()
	delete(h.lastAttempts, ip)
}

// cleanupRateLimitEntries removes expired entries from all IPs and deletes
// empty IP keys to prevent the map from growing unboundedly.
func (h *Handler) cleanupRateLimitEntries() {
	h.rateMu.Lock()
	defer h.rateMu.Unlock()

	cutoff := time.Now().Add(-1 * time.Minute)
	for ip, attempts := range h.lastAttempts {
		var recent []time.Time
		for _, t := range attempts {
			if t.After(cutoff) {
				recent = append(recent, t)
			}
		}
		if len(recent) == 0 {
			delete(h.lastAttempts, ip)
		} else {
			h.lastAttempts[ip] = recent
		}
	}
}

// extractIP extracts the IP from a remote address like "1.2.3.4:12345" or "[::1]:12345".
func extractIP(addr string) string {
	if idx := strings.LastIndex(addr, ":"); idx != -1 {
		ip := addr[:idx]
		return strings.Trim(ip, "[]")
	}
	return addr
}

func writeResponse(w http.ResponseWriter, resp Response) {
	// Always return HTTP 200 — the real status is inside the JSON body.
	// This prevents network observers from distinguishing auth failures,
	// errors, and successes by HTTP status code alone.
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(resp)
}

func strPtr(s string) *string {
	return &s
}
