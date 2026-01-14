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

package chatmail

import (
	"bufio"
	"bytes"
	"crypto/rand"
	"crypto/tls"
	"database/sql"
	"embed"
	"encoding/json"
	"errors"
	"fmt"
	"html/template"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/config"
	tls2 "github.com/themadorg/madmail/framework/config/tls"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth/pass_table"
	"github.com/skip2/go-qrcode"
	"golang.org/x/crypto/bcrypt"
)

//go:embed www/*
var WWWFiles embed.FS

const modName = "chatmail"

type Endpoint struct {
	addrs  []string
	name   string
	logger log.Logger

	// Domain configuration
	mailDomain string // Domain for email addresses (e.g., something.com)
	mxDomain   string // MX domain for mail server (e.g., mx.something.com)
	webDomain  string // Web domain for chat interface (e.g., chat.something.com)

	authDB  module.PlainUserDB
	storage module.ManageableStorage

	listenersWg sync.WaitGroup
	serv        http.Server
	mux         *http.ServeMux

	// TLS configuration
	tlsConfig *tls.Config

	publicIP string

	// Configuration options
	usernameLength int
	passwordLength int
	turnOffTLS     bool
	alpnSMTP       string
	alpnIMAP       string
	smtpModule     module.Module
	imapModule     module.Module

	// Contact sharing
	enableContactSharing bool
	sharingDB            *sql.DB

	wwwDir string
}

type AccountResponse struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

func New(_ string, args []string) (module.Module, error) {
	name := modName
	if len(args) > 0 {
		name = args[0]
	}
	return &Endpoint{
		addrs:  args,
		name:   name,
		logger: log.Logger{Name: modName, Debug: log.DefaultLogger.Debug},
	}, nil
}

func (e *Endpoint) Init(cfg *config.Map) error {
	cfg.Bool("debug", false, false, &e.logger.Debug)
	cfg.String("mail_domain", false, true, "", &e.mailDomain)
	cfg.String("mx_domain", false, true, "", &e.mxDomain)
	cfg.String("web_domain", false, true, "", &e.webDomain)
	cfg.Int("username_length", false, false, 8, &e.usernameLength)
	cfg.Int("password_length", false, false, 16, &e.passwordLength)
	cfg.String("public_ip", false, false, "", &e.publicIP)
	cfg.Bool("turn_off_tls", false, false, &e.turnOffTLS)
	cfg.String("alpn_smtp", false, false, "", &e.alpnSMTP)
	cfg.String("alpn_imap", false, false, "", &e.alpnIMAP)
	cfg.Bool("enable_contact_sharing", false, false, &e.enableContactSharing)
	cfg.String("www_dir", false, false, "", &e.wwwDir)

	// Get references to the authentication database and storage
	var authDBName, storageName string
	cfg.String("auth_db", false, true, "", &authDBName)
	cfg.String("storage", false, true, "", &storageName)

	// TLS configuration block
	cfg.Custom("tls", false, false, nil, tls2.TLSDirective, &e.tlsConfig)

	if _, err := cfg.Process(); err != nil {
		return err
	}

	if e.mailDomain == "" {
		return fmt.Errorf("%s: mail_domain is required", modName)
	}
	if ip := net.ParseIP(e.mailDomain); ip != nil {
		e.mailDomain = "[" + e.mailDomain + "]"
	}
	if e.mxDomain == "" {
		return fmt.Errorf("%s: mx_domain is required", modName)
	}
	if e.webDomain == "" {
		return fmt.Errorf("%s: web_domain is required", modName)
	}
	if authDBName == "" {
		return fmt.Errorf("%s: auth_db is required", modName)
	}
	if storageName == "" {
		return fmt.Errorf("%s: storage is required", modName)
	}

	if e.enableContactSharing {
		dbPath := filepath.Join(config.StateDirectory, "sharing.db")
		db, err := sql.Open("sqlite3", dbPath)
		if err != nil {
			return fmt.Errorf("%s: failed to open sharing DB: %v", modName, err)
		}
		e.sharingDB = db

		_, err = db.Exec(`CREATE TABLE IF NOT EXISTS contacts (
			slug TEXT PRIMARY KEY,
			url TEXT NOT NULL,
			name TEXT,
			created_at DATETIME DEFAULT CURRENT_TIMESTAMP
		)`)
		if err != nil {
			return fmt.Errorf("%s: failed to create sharing table: %v", modName, err)
		}
	}

	// Get the authentication database instance
	authDBInst, err := module.GetInstance(authDBName)
	if err != nil {
		return fmt.Errorf("%s: failed to get auth DB instance: %v", modName, err)
	}

	var ok bool
	e.authDB, ok = authDBInst.(module.PlainUserDB)
	if !ok {
		return fmt.Errorf("%s: auth DB must implement PlainUserDB interface", modName)
	}

	// Get the storage instance
	storageInst, err := module.GetInstance(storageName)
	if err != nil {
		return fmt.Errorf("%s: failed to get storage instance: %v", modName, err)
	}

	e.storage, ok = storageInst.(module.ManageableStorage)
	if !ok {
		return fmt.Errorf("%s: storage must implement ManageableStorage interface", modName)
	}

	e.mux = http.NewServeMux()
	// Priority 1: API endpoints
	e.mux.HandleFunc("/new", e.handleNewAccount)
	e.mux.HandleFunc("/qr", e.handleQRCode)
	e.mux.HandleFunc("/madmail", e.handleBinaryDownload)
	e.mux.HandleFunc("/mxdeliv", e.handleReceiveEmail)

	if e.enableContactSharing {
		e.mux.HandleFunc("/share", e.handleContactShare)
	}

	if e.alpnSMTP != "" {
		mod, err := module.GetInstance(e.alpnSMTP)
		if err != nil {
			return fmt.Errorf("%s: failed to get ALPN SMTP module (%s): %v", modName, e.alpnSMTP, err)
		}
		e.smtpModule = mod
	}
	if e.alpnIMAP != "" {
		mod, err := module.GetInstance(e.alpnIMAP)
		if err != nil {
			return fmt.Errorf("%s: failed to get ALPN IMAP module (%s): %v", modName, e.alpnIMAP, err)
		}
		e.imapModule = mod
	}
	// Priority 2: Static files and templates
	e.mux.HandleFunc("/", e.handleStaticFiles)
	e.serv.Handler = e.mux

	for _, a := range e.addrs {
		endp, err := config.ParseEndpoint(a)
		if err != nil {
			return fmt.Errorf("%s: malformed endpoint: %v", modName, err)
		}

		l, err := net.Listen(endp.Network(), endp.Address())
		if err != nil {
			return fmt.Errorf("%s: %v", modName, err)
		}

		e.listenersWg.Add(1)
		go func() {
			scheme := "http"
			if endp.IsTLS() {
				scheme = "https"
			}
			e.logger.Printf("listening on %s (%s)", endp.String(), scheme)

			if endp.IsTLS() && (e.smtpModule != nil || e.imapModule != nil) {
				e.serveALPNMultiplexed(l)
			} else {
				if endp.IsTLS() {
					l = tls.NewListener(l, e.tlsConfig)
				}
				err := e.serv.Serve(l)
				if err != nil && !errors.Is(err, http.ErrServerClosed) {
					e.logger.Error("serve failed", err, "endpoint", a)
				}
			}
			e.listenersWg.Done()
		}()
	}

	return nil
}

func (e *Endpoint) Name() string {
	return modName
}

func (e *Endpoint) InstanceName() string {
	return e.name
}

func (e *Endpoint) Close() error {
	if err := e.serv.Close(); err != nil {
		return err
	}
	e.listenersWg.Wait()
	return nil
}

// generateRandomString generates a random string of specified length using alphanumeric characters
func (e *Endpoint) generateRandomString(length int) (string, error) {
	const charset = "abcdefghijklmnopqrstuvwxyz0123456789"
	b := make([]byte, length)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	for i := range b {
		b[i] = charset[b[i]%byte(len(charset))]
	}
	return string(b), nil
}

// generateRandomPassword generates a random password with special characters
func (e *Endpoint) generateRandomPassword(length int) (string, error) {
	const charset = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=[]{}|;:,.<>?"
	b := make([]byte, length)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	for i := range b {
		b[i] = charset[b[i]%byte(len(charset))]
	}
	return string(b), nil
}

func (e *Endpoint) handleNewAccount(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Generate random username
	username, err := e.generateRandomString(e.usernameLength)
	if err != nil {
		e.logger.Error("failed to generate username", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Generate random password
	password, err := e.generateRandomPassword(e.passwordLength)
	if err != nil {
		e.logger.Error("failed to generate password", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Create full email address
	email := username + "@" + e.mailDomain

	// Create user in authentication database
	if authHash, ok := e.authDB.(*pass_table.Auth); ok {
		// Use bcrypt for password hashing
		err = authHash.CreateUserHash(email, password, "bcrypt", pass_table.HashOpts{
			BcryptCost: bcrypt.DefaultCost,
		})
	} else {
		err = e.authDB.CreateUser(email, password)
	}

	if err != nil {
		// Check if user already exists and retry
		if strings.Contains(err.Error(), "already exist") {
			// Retry with new username
			e.handleNewAccount(w, r)
			return
		}
		e.logger.Error("failed to create user in auth DB", err, "email", email)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Create IMAP account in storage
	err = e.storage.CreateIMAPAcct(email)
	if err != nil {
		e.logger.Error("failed to create IMAP account", err, "email", email)
		// Try to clean up the auth entry
		if delErr := e.authDB.DeleteUser(email); delErr != nil {
			e.logger.Error("failed to cleanup auth entry after storage failure", delErr, "email", email)
		}
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Return the generated credentials
	response := AccountResponse{
		Email:    email,
		Password: password,
	}

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(response); err != nil {
		e.logger.Error("failed to encode response", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	e.logger.Printf("created new account: %s", email)
}

func (e *Endpoint) handleStaticFiles(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Clean the path to prevent directory traversal
	path := strings.TrimPrefix(r.URL.Path, "/")
	if path == "" {
		path = "index.html"
	}

	if path == "docs/serve" {
		e.serveTemplate(w, r, "docs_serve.html", nil)
		return
	}

	// Try to read the file
	fileData, err := e.readFile(path)
	if err != nil {
		e.logger.Debugf("failed to read file: %s, error: %v", path, err)
		// If not a file, check if it's a contact slug
		if e.enableContactSharing && path != "index.html" {
			var url, name string
			err := e.sharingDB.QueryRow("SELECT url, name FROM contacts WHERE slug = ?", path).Scan(&url, &name)
			if err == nil {
				e.renderContactView(w, r, path, url, name)
				return
			}
		}
		// File not found, return 404
		http.NotFound(w, r)
		return
	}

	// Determine content type based on file extension
	var contentType string
	switch {
	case strings.HasSuffix(path, ".html"):
		contentType = "text/html; charset=utf-8"
	case strings.HasSuffix(path, ".css"):
		contentType = "text/css"
	case strings.HasSuffix(path, ".js"):
		contentType = "application/javascript"
	case strings.HasSuffix(path, ".png"):
		contentType = "image/png"
	case strings.HasSuffix(path, ".jpg") || strings.HasSuffix(path, ".jpeg"):
		contentType = "image/jpeg"
	case strings.HasSuffix(path, ".gif"):
		contentType = "image/gif"
	case strings.HasSuffix(path, ".svg"):
		contentType = "image/svg+xml"
	case strings.HasSuffix(path, ".ico"):
		contentType = "image/x-icon"
	default:
		contentType = "application/octet-stream"
	}

	// For HTML files, process them as templates
	if strings.HasSuffix(path, ".html") {
		tmpl, err := template.New(path).Funcs(template.FuncMap{
			"upper":       strings.ToUpper,
			"safeURL":     func(s string) template.URL { return template.URL(s) },
			"cleanDomain": func(s string) string { return strings.Trim(s, "[]") },
		}).Parse(string(fileData))
		if err != nil {
			e.logger.Error("failed to parse template", err, "file", path)
			http.Error(w, "Internal server error", http.StatusInternalServerError)
			return
		}

		// Template data
		data := struct {
			MailDomain string
			MXDomain   string
			WebDomain  string
			PublicIP   string
			TurnOffTLS bool
			Version    string
		}{
			MailDomain: e.mailDomain,
			MXDomain:   e.mxDomain,
			WebDomain:  e.webDomain,
			PublicIP:   e.publicIP,
			TurnOffTLS: e.turnOffTLS,
			Version:    config.Version,
		}

		w.Header().Set("Content-Type", contentType)
		if err := tmpl.Execute(w, data); err != nil {
			e.logger.Error("failed to execute template", err, "file", path)
			http.Error(w, "Internal server error", http.StatusInternalServerError)
			return
		}
	} else {
		// For non-HTML files, serve them as-is
		w.Header().Set("Content-Type", contentType)
		if _, err := w.Write(fileData); err != nil {
			e.logger.Error("failed to write file data", err, "file", path)
			return
		}
	}
}

func (e *Endpoint) handleQRCode(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Get the data parameter from query string
	data := r.URL.Query().Get("data")
	if data == "" {
		http.Error(w, "Missing 'data' parameter", http.StatusBadRequest)
		return
	}

	// Generate QR code
	qrCode, err := qrcode.Encode(data, qrcode.Medium, 256)
	if err != nil {
		e.logger.Error("failed to generate QR code", err, "data", data)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Set headers for PNG image
	w.Header().Set("Content-Type", "image/png")
	w.Header().Set("Cache-Control", "no-cache, no-store, must-revalidate")
	w.Header().Set("Pragma", "no-cache")
	w.Header().Set("Expires", "0")

	// Write the QR code image
	if _, err := w.Write(qrCode); err != nil {
		e.logger.Error("failed to write QR code response", err)
		return
	}
}

func (e *Endpoint) handleBinaryDownload(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	executablePath, err := os.Executable()
	if err != nil {
		e.logger.Error("failed to get executable path", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Set headers for binary download
	w.Header().Set("Content-Type", "application/octet-stream")
	filename := filepath.Base(executablePath)
	w.Header().Set("Content-Disposition", fmt.Sprintf("attachment; filename=%s", filename))

	http.ServeFile(w, r, executablePath)
}

func (e *Endpoint) handleReceiveEmail(w http.ResponseWriter, r *http.Request) {
	e.logger.Msg("HTTP delivery request received", "remote", r.RemoteAddr, "path", r.URL.Path)
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	mailFrom := r.Header.Get("X-Mail-From")
	mailTo := r.Header.Values("X-Mail-To")

	if len(mailTo) == 0 {
		e.logger.Error("missing X-Mail-To header", nil)
		http.Error(w, "Missing X-Mail-To header", http.StatusBadRequest)
		return
	}

	bodyData, err := io.ReadAll(r.Body)
	if err != nil {
		e.logger.Error("failed to read body", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	dt, ok := e.storage.(module.DeliveryTarget)
	if !ok {
		e.logger.Error("storage does not implement DeliveryTarget", nil)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	msgID, _ := module.GenerateMsgID()
	msgMeta := &module.MsgMetadata{
		ID:       msgID,
		SMTPOpts: smtp.MailOptions{},
	}

	delivery, err := dt.Start(r.Context(), msgMeta, mailFrom)
	if err != nil {
		e.logger.Error("failed to start delivery", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}
	defer delivery.Abort(r.Context())

	anyAccepted := false
	for _, to := range mailTo {
		if err := delivery.AddRcpt(r.Context(), to, smtp.RcptOptions{}); err != nil {
			e.logger.Error("failed to add recipient", err, "to", to)
		} else {
			anyAccepted = true
		}
	}

	if !anyAccepted {
		http.Error(w, "No valid recipients", http.StatusNotFound)
		return
	}

	br := bufio.NewReader(bytes.NewReader(bodyData))
	header, err := textproto.ReadHeader(br)
	if err != nil {
		e.logger.Error("failed to parse header", err)
		http.Error(w, "Bad request", http.StatusBadRequest)
		return
	}

	remainingBody, _ := io.ReadAll(br)
	b := buffer.MemoryBuffer{Slice: remainingBody}

	if err := delivery.Body(r.Context(), header, b); err != nil {
		e.logger.Error("failed to deliver body", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	if err := delivery.Commit(r.Context()); err != nil {
		e.logger.Error("failed to commit delivery", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	scheme := "http"
	if r.TLS != nil {
		scheme = "https"
	}
	e.logger.Msg("received email via "+scheme, "from", mailFrom, "to", mailTo)
	w.WriteHeader(http.StatusOK)
}

func (e *Endpoint) serveALPNMultiplexed(l net.Listener) {
	httpL := &multiplexedListener{
		Listener: l,
		conns:    make(chan net.Conn, 128),
	}

	smtpChan := make(chan net.Conn, 128)
	imapChan := make(chan net.Conn, 128)

	if e.smtpModule != nil {
		if smtpS, ok := e.smtpModule.(interface{ Serve(net.Listener) error }); ok {
			smtpL := &multiplexedListener{Listener: l, conns: smtpChan}
			go func() {
				if err := smtpS.Serve(smtpL); err != nil && !errors.Is(err, net.ErrClosed) {
					e.logger.Error("SMTP serve failed", err)
				}
			}()
		}
	}

	if e.imapModule != nil {
		if imapS, ok := e.imapModule.(interface{ Serve(net.Listener) error }); ok {
			imapL := &multiplexedListener{Listener: l, conns: imapChan}
			go func() {
				if err := imapS.Serve(imapL); err != nil && !errors.Is(err, net.ErrClosed) {
					e.logger.Error("IMAP serve failed", err)
				}
			}()
		}
	}

	go func() {
		err := e.serv.Serve(tls.NewListener(httpL, e.tlsConfig))
		if err != nil && !errors.Is(err, http.ErrServerClosed) {
			e.logger.Error("HTTP serve failed", err)
		}
	}()

	for {
		conn, err := l.Accept()
		if err != nil {
			if !errors.Is(err, net.ErrClosed) {
				e.logger.Error("Accept failed", err)
			}
			close(httpL.conns)
			close(smtpChan)
			close(imapChan)
			return
		}

		go e.handleALPNConn(conn, httpL.conns, smtpChan, imapChan)
	}
}

func (e *Endpoint) handleALPNConn(conn net.Conn, httpConns, smtpConns, imapConns chan<- net.Conn) {
	br := bufio.NewReader(conn)
	alpn, err := e.sniffALPN(br)
	bConn := &bufferedConn{Conn: conn, r: br}

	if err == nil {
		switch alpn {
		case "smtp":
			if e.smtpModule != nil {
				e.logger.Msg("ALPN proxy: routing to internal smtp", "remote", conn.RemoteAddr())
				smtpConns <- tls.Server(bConn, e.tlsConfig)
				return
			}
		case "imap":
			if e.imapModule != nil {
				e.logger.Msg("ALPN proxy: routing to internal imap", "remote", conn.RemoteAddr())
				imapConns <- tls.Server(bConn, e.tlsConfig)
				return
			}
		}
	}

	httpConns <- bConn
}

func (e *Endpoint) sniffALPN(br *bufio.Reader) (string, error) {
	header, err := br.Peek(5)
	if err != nil {
		return "", err
	}

	if header[0] != 0x16 { // Not a Handshake
		return "", fmt.Errorf("not a TLS handshake")
	}

	length := int(header[3])<<8 | int(header[4])
	if length > 16384 {
		return "", fmt.Errorf("handshake record too large")
	}

	data, err := br.Peek(5 + length)
	if err != nil {
		return "", err
	}

	return parseALPN(data[5:]), nil
}

func parseALPN(data []byte) string {
	if len(data) < 38 {
		return ""
	}

	offset := 4 + 2 + 32 // Type (1) + Length (3) + Version (2) + Random (32)

	// Session ID
	sessionIDLen := int(data[offset])
	offset += 1 + sessionIDLen
	if len(data) < offset+2 {
		return ""
	}

	// Cipher Suites
	cipherSuitesLen := int(data[offset])<<8 | int(data[offset+1])
	offset += 2 + cipherSuitesLen
	if len(data) < offset+1 {
		return ""
	}

	// Compression Methods
	compressionMethodsLen := int(data[offset])
	offset += 1 + compressionMethodsLen
	if len(data) < offset+2 {
		return ""
	}

	// Extensions
	extensionsLen := int(data[offset])<<8 | int(data[offset+1])
	offset += 2
	extensionsEnd := offset + extensionsLen
	if extensionsEnd > len(data) {
		extensionsEnd = len(data)
	}

	for offset+4 <= extensionsEnd {
		extType := int(data[offset])<<8 | int(data[offset+1])
		extLen := int(data[offset+2])<<8 | int(data[offset+3])
		offset += 4
		if offset+extLen > extensionsEnd {
			break
		}

		if extType == 16 { // ALPN
			alpnData := data[offset : offset+extLen]
			if len(alpnData) < 2 {
				return ""
			}
			alpnListLen := int(alpnData[0])<<8 | int(alpnData[1])
			alpnList := alpnData[2:]
			if len(alpnList) < alpnListLen {
				alpnListLen = len(alpnList)
			}

			// We just return the first one for simplicity, or "smtp"/"imap" if present
			for i := 0; i < alpnListLen; {
				protLen := int(alpnList[i])
				i++
				if i+protLen > alpnListLen {
					break
				}
				prot := string(alpnList[i : i+protLen])
				if prot == "smtp" || prot == "imap" {
					return prot
				}
				i += protLen
			}
		}
		offset += extLen
	}
	return ""
}

func (e *Endpoint) proxy(c1 net.Conn, addr string) {
	defer c1.Close()
	c2, err := net.Dial("tcp", addr)
	if err != nil {
		e.logger.Error("ALPN proxy: failed to dial target", err, "target", addr)
		return
	}
	defer c2.Close()

	done := make(chan struct{}, 2)
	go func() {
		io.Copy(c2, c1)
		done <- struct{}{}
	}()
	go func() {
		io.Copy(c1, c2)
		done <- struct{}{}
	}()
	<-done
}

type multiplexedListener struct {
	net.Listener
	conns chan net.Conn
}

func (l *multiplexedListener) Accept() (net.Conn, error) {
	c, ok := <-l.conns
	if !ok {
		return nil, net.ErrClosed
	}
	return c, nil
}

type bufferedConn struct {
	net.Conn
	r *bufio.Reader
}

func (c *bufferedConn) Read(b []byte) (int, error) {
	return c.r.Read(b)
}

func (e *Endpoint) handleContactShare(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodGet {
		e.serveTemplate(w, r, "contact_share.html", nil)
		return
	}

	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	rawURL := strings.TrimSpace(r.FormValue("url"))
	name := strings.TrimSpace(r.FormValue("name"))
	slug := strings.TrimSpace(r.FormValue("slug"))

	if rawURL == "" {
		http.Error(w, "آدرس دعوت الزامی است", http.StatusBadRequest)
		return
	}

	// Strictly accept only web link format as input
	if !strings.HasPrefix(rawURL, "https://i.delta.chat/#") {
		http.Error(w, "فقط لینک‌های دعوت وب (https://i.delta.chat/#...) پذیرفته می‌شوند.", http.StatusBadRequest)
		return
	}

	// Convert URL: https://i.delta.chat/#FINGERPRINT&params -> openpgp4fpr:FINGERPRINT#params
	content := strings.TrimPrefix(rawURL, "https://i.delta.chat/#")
	if idx := strings.Index(content, "&"); idx != -1 {
		rawURL = "openpgp4fpr:" + content[:idx] + "#" + content[idx+1:]
	} else {
		rawURL = "openpgp4fpr:" + content
	}

	// Ensure the converted URL has no spaces
	if strings.Contains(rawURL, " ") {
		http.Error(w, "آدرس دعوت دلتاچت نامعتبر است.", http.StatusBadRequest)
		return
	}

	// Slug generation/validation
	if slug == "" {
		var err error
		slug, err = e.generateRandomString(8)
		if err != nil {
			http.Error(w, "Internal error", http.StatusInternalServerError)
			return
		}
	} else {
		// Validate custom slug
		if len(slug) < 3 {
			http.Error(w, "نام مسیر باید حداقل ۳ کاراکتر باشد", http.StatusBadRequest)
			return
		}
		// Simple alphanumeric check
		for _, r := range slug {
			if !((r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')) {
				http.Error(w, "نام مسیر فقط می‌تواند شامل حروف و اعداد باشد (بدون فاصله یا علامت)", http.StatusBadRequest)
				return
			}
		}
		// Check if exists or is reserved
		reserved := map[string]bool{"share": true, "qr": true, "new": true, "madmail": true, "mxdeliv": true, "main.css": true, "index.html": true, "info.html": true, "security.html": true, "deploy.html": true}
		if reserved[slug] {
			http.Error(w, "این نام مسیر رزرو شده است", http.StatusBadRequest)
			return
		}
		var exists int
		e.sharingDB.QueryRow("SELECT COUNT(*) FROM contacts WHERE slug = ?", slug).Scan(&exists)
		if exists > 0 {
			http.Error(w, "این نام مسیر قبلاً انتخاب شده است", http.StatusBadRequest)
			return
		}
	}

	_, err := e.sharingDB.Exec("INSERT INTO contacts (slug, url, name) VALUES (?, ?, ?)", slug, rawURL, name)
	if err != nil {
		e.logger.Error("failed to store contact", err)
		http.Error(w, "Failed to create shareable link", http.StatusInternalServerError)
		return
	}

	data := struct {
		Slug string
		URL  string
		Name string
	}{
		Slug: slug,
		URL:  rawURL,
		Name: name,
	}

	if r.Header.Get("Accept") == "application/json" {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(data)
		return
	}

	e.serveTemplate(w, r, "contact_share_success.html", data)
}

func (e *Endpoint) handleContactList(w http.ResponseWriter, r *http.Request) {
	rows, err := e.sharingDB.Query("SELECT slug, url, name, created_at FROM contacts ORDER BY created_at DESC LIMIT 100")
	if err != nil {
		http.Error(w, "Internal error", http.StatusInternalServerError)
		return
	}
	defer rows.Close()

	type Contact struct {
		Slug      string
		URL       string
		Name      string
		CreatedAt string
	}
	var contacts []Contact
	for rows.Next() {
		var c Contact
		rows.Scan(&c.Slug, &c.URL, &c.Name, &c.CreatedAt)
		contacts = append(contacts, c)
	}

	e.serveTemplate(w, r, "contact_list.html", contacts)
}

func (e *Endpoint) renderContactView(w http.ResponseWriter, r *http.Request, slug, url, name string) {
	data := struct {
		Slug string
		URL  string
		Name string
	}{
		Slug: slug,
		URL:  url,
		Name: name,
	}

	e.serveTemplate(w, r, "contact_view.html", data)
}

func (e *Endpoint) readFile(name string) ([]byte, error) {
	if e.wwwDir != "" {
		data, err := os.ReadFile(filepath.Join(e.wwwDir, name))
		if err == nil {
			return data, nil
		}
		// Fallback to embedded if file not found in external dir
	}
	return WWWFiles.ReadFile("www/" + name)
}

func (e *Endpoint) serveTemplate(w http.ResponseWriter, r *http.Request, name string, customData interface{}) {
	fileData, err := e.readFile(name)
	if err != nil {
		e.logger.Error("failed to read template", err, "file", name)
		http.NotFound(w, r)
		return
	}

	tmpl, err := template.New(name).Funcs(template.FuncMap{
		"upper":       strings.ToUpper,
		"safeURL":     func(s string) template.URL { return template.URL(s) },
		"cleanDomain": func(s string) string { return strings.Trim(s, "[]") },
	}).Parse(string(fileData))
	if err != nil {
		e.logger.Error("failed to parse template", err, "file", name)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Composite data including default fields
	data := struct {
		MailDomain string
		MXDomain   string
		WebDomain  string
		PublicIP   string
		TurnOffTLS bool
		Version    string
		Custom     interface{}
	}{
		MailDomain: e.mailDomain,
		MXDomain:   e.mxDomain,
		WebDomain:  e.webDomain,
		PublicIP:   e.publicIP,
		TurnOffTLS: e.turnOffTLS,
		Version:    config.Version,
		Custom:     customData,
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	if err := tmpl.Execute(w, data); err != nil {
		e.logger.Error("failed to execute template", err, "file", name)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}
}

func init() {
	module.RegisterEndpoint(modName, New)
}
