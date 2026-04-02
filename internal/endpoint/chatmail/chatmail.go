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
	"embed"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"html/template"
	"io"
	syslog "log"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/skip2/go-qrcode"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/config"
	tls2 "github.com/themadorg/madmail/framework/config/tls"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	adminapi "github.com/themadorg/madmail/internal/api/admin"
	"github.com/themadorg/madmail/internal/api/admin/resources"
	"github.com/themadorg/madmail/internal/auth/pass_table"

	"github.com/shadowsocks/go-shadowsocks2/core"
	"github.com/shadowsocks/go-shadowsocks2/socks"

	// Xray-core: embedded QUIC transport for Shadowsocks
	"github.com/xtls/xray-core/app/dispatcher"
	xlog "github.com/xtls/xray-core/app/log"
	"github.com/xtls/xray-core/app/proxyman"
	_ "github.com/xtls/xray-core/app/proxyman/inbound"
	_ "github.com/xtls/xray-core/app/proxyman/outbound"
	xclog "github.com/xtls/xray-core/common/log"
	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/common/platform/filesystem"
	"github.com/xtls/xray-core/common/protocol"
	"github.com/xtls/xray-core/common/serial"
	xcore "github.com/xtls/xray-core/core"
	// "github.com/xtls/xray-core/proxy/dokodemo" -- no longer used; WS and gRPC use native SS inbound
	"github.com/xtls/xray-core/app/router"
	"github.com/xtls/xray-core/proxy/blackhole"
	"github.com/xtls/xray-core/proxy/freedom"
	xss "github.com/xtls/xray-core/proxy/shadowsocks"
	"github.com/xtls/xray-core/transport/internet"
	"github.com/xtls/xray-core/transport/internet/grpc"
	xtls "github.com/xtls/xray-core/transport/internet/tls"
	"github.com/xtls/xray-core/transport/internet/websocket"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

//go:embed www/*
var WWWFiles embed.FS

const modName = "chatmail"

type Endpoint struct {
	addrs  []string
	name   string
	logger log.Logger

	maxMessageSize string

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
	sharingDriver        string
	sharingDSN           []string
	sharingGORM          *gorm.DB
	exchangerGORM        *gorm.DB

	wwwDir string

	// Language configuration (en, fa, ru)
	language string

	// Admin API
	adminToken string
	adminPath  string

	// Admin Web UI
	adminWebPath string

	// Shadowsocks configuration
	ssAddr             string
	ssPassword         string
	ssCipher           string
	ssAllowedPortsList []string
	ssAllowedPorts     map[string]bool

	// RAM cache for frequently accessed settings to prevent redundant DB calls
	cache struct {
		sync.RWMutex
		language               string
		registrationOpen       bool
		jitRegistrationEnabled bool
		defaultQuota           int64
		hydrated               bool
		lastChecked            time.Time
	}

	// Shadowsocks QUIC transport (v2ray-plugin)
	ssCertPath string
	ssKeyPath  string
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
	cfg.String("language", false, false, "en", &e.language)
	cfg.String("ss_addr", false, false, "", &e.ssAddr)
	cfg.String("ss_password", false, false, "", &e.ssPassword)
	cfg.String("ss_cipher", false, false, "aes-128-gcm", &e.ssCipher)
	cfg.String("ss_cert", false, false, "", &e.ssCertPath)
	cfg.String("ss_key", false, false, "", &e.ssKeyPath)
	allowedPortsList := []string{"3478", "5349"} // Default TURN ports
	cfg.StringList("ss_allowed_ports", false, false, nil, &e.ssAllowedPortsList)
	cfg.String("sharing_driver", false, false, "sqlite3", &e.sharingDriver)
	cfg.StringList("sharing_dsn", false, false, nil, &e.sharingDSN)
	cfg.String("max_message_size", false, false, "32M", &e.maxMessageSize)
	cfg.String("admin_token", false, false, "", &e.adminToken)
	cfg.String("admin_path", false, false, "/api/admin", &e.adminPath)
	cfg.String("admin_web_path", false, false, "", &e.adminWebPath)

	// Get references to the authentication database and storage
	var authDBName, storageName string
	cfg.String("auth_db", false, true, "", &authDBName)
	cfg.String("storage", false, true, "", &storageName)

	// TLS configuration block
	cfg.Custom("tls", true, false, nil, tls2.TLSDirective, &e.tlsConfig)

	if _, err := cfg.Process(); err != nil {
		return err
	}

	e.ssAllowedPorts = make(map[string]bool)
	// 1. Add ports from ss_allowed_ports config if provided
	if len(e.ssAllowedPortsList) > 0 {
		for _, p := range e.ssAllowedPortsList {
			e.ssAllowedPorts[p] = true
		}
	} else {
		// 2. Otherwise use defaults + discovered ports
		for _, p := range allowedPortsList {
			e.ssAllowedPorts[p] = true
		}
		// TODO: Discover ports from SMTP and IMAP modules if they exist in globals
		// for k, v := range cfg.Globals {
		// 	if strings.HasPrefix(k, "endpoint.smtp") || strings.HasPrefix(k, "endpoint.submission") || strings.HasPrefix(k, "endpoint.imap") {
		// 		if _, ok := v.(module.Module); ok {
		// 			// We can't easily get addresses from the module interface,
		// 			// but we can look at the config nodes in the future.
		// 			// For now, if no ss_allowed_ports is set, we use the standard defaults
		// 			// which covers 99% of maddy setups.
		// 		}
		// 	}
		// }
		// Re-enforce standard ports if nothing else
		for _, p := range []string{"25", "143", "465", "587", "993"} {
			e.ssAllowedPorts[p] = true
		}
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

	// Initialize exchanger GORM DB (uses the same credentials file as CLI by default)
	e.logger.Msg("chatmail Init: starting DB detection", "config", config.ConfigFile())
	exDriver := e.sharingDriver
	stateDir := config.StateDirectory
	if stateDir == "" {
		stateDir = "/var/lib/maddy"
	}
	exchangerDSN := []string{filepath.Join(stateDir, "credentials.db")}

	// Try to detect the auth table driver / DSN from the same maddy.conf as the CLI
	data, err := os.ReadFile(config.ConfigFile())
	if err != nil {
		e.logger.Error("failed to read config for DB detection", err)
	} else {
		inAuthBlock := false
		lines := strings.Split(string(data), "\n")
		for _, line := range lines {
			trimmed := strings.TrimSpace(line)
			if strings.HasPrefix(trimmed, "auth.pass_table local_authdb") {
				inAuthBlock = true
			}
			if inAuthBlock {
				if strings.HasPrefix(trimmed, "driver ") {
					exDriver = strings.TrimSpace(strings.TrimPrefix(trimmed, "driver"))
				}
				if strings.HasPrefix(trimmed, "dsn ") {
					d := strings.TrimSpace(strings.TrimPrefix(trimmed, "dsn"))
					d = strings.Trim(d, `"'`)
					if exDriver == "sqlite3" || exDriver == "sqlite" {
						if !filepath.IsAbs(d) {
							d = filepath.Join(stateDir, d)
						}
					}
					exchangerDSN = []string{d}
				}
				if trimmed == "}" {
					inAuthBlock = false
				}
			}
		}
	}

	if exDriver == "" {
		exDriver = "sqlite3"
	}

	e.logger.Msg("exchanger poller: opening DB", "driver", exDriver, "dsn", exchangerDSN[0])
	exGDB, err := mdb.New(exDriver, exchangerDSN, e.logger.Debug)
	if err != nil {
		e.logger.Error("failed to open exchanger GORM DB", err)
	} else {
		e.exchangerGORM = exGDB
		if err := exGDB.AutoMigrate(&mdb.Exchanger{}); err != nil {
			e.logger.Error("failed to migrate exchanger table", err)
		}
	}

	if e.enableContactSharing {
		driver := e.sharingDriver
		dsn := e.sharingDSN
		if dsn == nil && driver == "sqlite3" {
			dsn = []string{filepath.Join(stateDir, "sharing.db")}
		}

		gdb, err := mdb.New(driver, dsn, e.logger.Debug)
		if err != nil {
			return fmt.Errorf("%s: failed to open sharing GORM DB: %v", modName, err)
		}
		e.sharingGORM = gdb
		if err := gdb.AutoMigrate(&mdb.Contact{}); err != nil {
			return fmt.Errorf("%s: failed to migrate sharing table: %v", modName, err)
		}
	}

	// Start the exchanger poller in the background
	go e.runExchangerPoller()

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

	// Log any settings overridden by the database
	e.applyDBOverrides()
	e.logDBOverrides()

	e.mux = http.NewServeMux()
	// Priority 0: Well-known endpoints (DKIM key publishing for federation)
	e.mux.HandleFunc("/.well-known/_domainkey/", e.handleDKIMKey)

	// Priority 1: API endpoints
	e.mux.HandleFunc("/new", e.handleNewAccount)
	e.mux.HandleFunc("/qr", e.handleQRCode)
	e.mux.HandleFunc("/madmail", e.handleBinaryDownload)
	e.mux.HandleFunc("/mxdeliv", e.handleReceiveEmail)

	// Admin API: auto-generate token if not configured, respect "disabled"
	if e.adminToken == "disabled" {
		e.logger.Printf("admin API explicitly disabled via config")
	} else {
		if e.adminToken == "" {
			// Auto-generate or load from state dir
			var err error
			e.adminToken, err = e.ensureAdminToken()
			if err != nil {
				e.logger.Printf("WARNING: failed to initialize admin token: %v", err)
			}
		}
		if e.adminToken != "" {
			e.setupAdminAPI()
		}
	}

	// Admin Web UI: register routes if a path is configured.
	// The enabled/disabled check happens per-request inside serveAdminWeb,
	// so toggling via the Admin API takes effect immediately without restart.
	adminWebPath := e.adminWebPath
	if e.authDB != nil {
		if val, ok, err := e.authDB.GetSetting(resources.KeyAdminWebPath); err == nil && ok && val != "" {
			adminWebPath = val
		}
	}
	if adminWebPath != "" {
		// Ensure the path has proper format
		if !strings.HasPrefix(adminWebPath, "/") {
			adminWebPath = "/" + adminWebPath
		}
		adminWebPath = strings.TrimSuffix(adminWebPath, "/")
		e.mux.HandleFunc(adminWebPath+"/", e.serveAdminWeb(adminWebPath))
		// Also handle the exact path without trailing slash (redirect to with slash)
		e.mux.HandleFunc(adminWebPath, func(w http.ResponseWriter, r *http.Request) {
			http.Redirect(w, r, adminWebPath+"/", http.StatusMovedPermanently)
		})
		e.logger.Printf("admin web UI registered at %s/", adminWebPath)
	}

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
	// Priority 2: Documentation
	e.mux.HandleFunc("/docs", e.handleDocs)
	e.mux.HandleFunc("/docs/", e.handleDocs)

	// Priority 3: Static files and templates
	e.mux.HandleFunc("/", e.handleStaticFiles)
	e.serv.Handler = e.mux

	// Silence TLS handshake errors and other HTTP server noise unless debug is enabled
	if !e.logger.Debug {
		e.serv.ErrorLog = syslog.New(io.Discard, "", 0)
	} else {
		e.serv.ErrorLog = syslog.New(e.logger.DebugWriter(), "http: ", 0)
	}

	for _, a := range e.addrs {
		endp, err := config.ParseEndpoint(a)
		if err != nil {
			return fmt.Errorf("%s: malformed endpoint: %v", modName, err)
		}

		// Port access control: if HTTP or HTTPS is set to local-only,
		// rewrite 0.0.0.0 to 127.0.0.1.
		localOnlyKey := resources.KeyHTTPLocalOnly
		if endp.IsTLS() {
			localOnlyKey = resources.KeyHTTPSLocalOnly
		}
		isLocal := module.IsLocalOnly(localOnlyKey)
		if !isLocal {
			// Fallback: read directly from our authDB reference
			if val, ok, err := e.authDB.GetSetting(localOnlyKey); err == nil && ok && val == "true" {
				isLocal = true
			}
		}
		if isLocal {
			e.logger.Printf("local-only mode active for %s, binding to 127.0.0.1 only", endp)
			endp = endp.WithLocalHost()
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

	if e.ssPassword == "" {
		// Use a predictable but unique-ish default password if none provided
		// Or better, generate one if we can't find it.
		// For now, let's just ensure it's set to something if we want it DEFAULT.
		e.ssPassword = "chatmail-default-pass"
	}

	if e.ssAddr != "" && e.isShadowsocksEnabled() {
		go e.runShadowsocks()
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

// hydrateCache ensures that the settings cache is populated from the DB.
func (e *Endpoint) hydrateCache() {
	e.cache.Lock()
	defer e.cache.Unlock()

	if e.authDB != nil {
		if val, ok, err := e.authDB.GetSetting(resources.KeyLanguage); err == nil && ok {
			e.cache.language = val
		}
		if open, err := e.authDB.IsRegistrationOpen(); err == nil {
			e.cache.registrationOpen = open
		}
		if jit, err := e.authDB.IsJitRegistrationEnabled(); err == nil {
			e.cache.jitRegistrationEnabled = jit
		}
	}
	if e.storage != nil {
		e.cache.defaultQuota = e.storage.GetDefaultQuota()
	}
	e.cache.hydrated = true
	e.cache.lastChecked = time.Now()
}

// getLanguage returns the active UI language from cache (fallback to DB).
func (e *Endpoint) getLanguage() string {
	e.cache.RLock()
	// Re-hydrate if not hydrated or if cache is stale (TTL: 5 seconds for CLI sync)
	if !e.cache.hydrated || time.Since(e.cache.lastChecked) > 5*time.Second {
		e.cache.RUnlock()
		e.hydrateCache()
		e.cache.RLock()
	}
	lang := e.cache.language
	e.cache.RUnlock()

	if lang != "" {
		return lang
	}
	if e.language != "" {
		return e.language
	}
	return "en"
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

	open, err := e.authDB.IsRegistrationOpen()
	if err != nil {
		e.logger.Error("failed to check registration status", err)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}
	if !open {
		http.Error(w, "Registration is closed", http.StatusForbidden)
		return
	}

	// Retry loop with bounded attempts to avoid unbounded recursion
	const maxAttempts = 5
	for attempt := 0; attempt < maxAttempts; attempt++ {
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

		// Check blocklist before creating account
		if blocked, _ := e.storage.IsBlocked(email); blocked {
			continue // retry with new username
		}

		// Create user in authentication database
		if authHash, ok := e.authDB.(*pass_table.Auth); ok {
			// Use SHA256 for password hashing (fast, sufficient for server-generated passwords)
			err = authHash.CreateUserHash(email, password, pass_table.DefaultHash, pass_table.HashOpts{})
		} else {
			err = e.authDB.CreateUser(email, password)
		}

		if err != nil {
			// Check if user already exists and retry
			if strings.Contains(err.Error(), "already exist") {
				continue // retry with new username
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
		return // success
	}

	// All attempts exhausted — this should be extremely rare
	e.logger.Error("failed to create account after max retries", nil)
	http.Error(w, "Internal server error", http.StatusInternalServerError)
}

func (e *Endpoint) handleDocs(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	path := strings.TrimPrefix(r.URL.Path, "/docs")
	path = strings.TrimPrefix(path, "/")

	switch path {
	case "", "index", "index.html":
		e.serveTemplate(w, r, "docs_index.html", nil)
	case "admin":
		e.serveDocLang(w, r, "admin.html")
	case "api":
		e.serveTemplate(w, r, "admin_api_docs.html", nil)
	case "general":
		e.serveDocLang(w, r, "general.html")
	case "serve", "custom-html":
		e.serveDocLang(w, r, "serve.html")
	case "database":
		e.serveDocLang(w, r, "database.html")
	case "docker":
		e.serveDocLang(w, r, "docker.html")
	case "relay", "domain", "tls":
		e.serveDocLang(w, r, "relay.html")
	default:
		http.NotFound(w, r)
	}
}

// serveDocLang serves a doc template from docs/{lang}/ with fallback to docs/en/.
func (e *Endpoint) serveDocLang(w http.ResponseWriter, r *http.Request, name string) {
	lang := e.getLanguage()
	langPath := "docs/" + lang + "/" + name

	// Try language-specific file first
	if _, err := e.readFile(langPath); err == nil {
		e.serveTemplate(w, r, langPath, nil)
		return
	}

	// Fallback to English
	enPath := "docs/en/" + name
	if _, err := e.readFile(enPath); err == nil {
		e.serveTemplate(w, r, enPath, nil)
		return
	}

	// Final fallback: try legacy root-level name (e.g., general_docs.html)
	legacyName := strings.TrimSuffix(name, ".html") + "_docs.html"
	e.serveTemplate(w, r, legacyName, nil)
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

	// Try to read the file
	fileData, err := e.readFile(path)
	if err != nil {
		e.logger.Debugf("failed to read file: %s, error: %v", path, err)
		// If not a file, check if it's a contact slug
		if e.enableContactSharing && path != "index.html" {
			var contact mdb.Contact
			err := e.sharingGORM.Where("slug = ?", path).First(&contact).Error
			if err == nil {
				e.renderContactView(w, r, path, contact.URL, contact.Name)
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
			"safeHTML":    func(s string) template.HTML { return template.HTML(s) },
			"cleanDomain": func(s string) string { return strings.Trim(s, "[]") },
			"formatBytes": formatBytes,
		}).Parse(string(fileData))
		if err != nil {
			e.logger.Error("failed to parse template", err, "file", path)
			http.Error(w, "Internal server error", http.StatusInternalServerError)
			return
		}

		// Template data
		data := struct {
			MailDomain             string
			MXDomain               string
			WebDomain              string
			PublicIP               string
			TurnOffTLS             bool
			Version                string
			SSURL                  string
			SSGrpcURL              string
			SSWsURL                string
			V2rayNGConfigWS        string
			V2rayNGConfigGRPC      string
			STUNAddr               string
			DefaultQuota           int64
			MaxMessageSize         string
			RegistrationOpen       bool
			JitRegistrationEnabled bool
			TurnEnabled            bool
			Language               string
		}{
			MailDomain:             e.mailDomain,
			MXDomain:               e.mxDomain,
			WebDomain:              e.webDomain,
			PublicIP:               e.publicIP,
			TurnOffTLS:             e.turnOffTLS,
			Version:                config.Version,
			SSURL:                  e.getShadowsocksURL(r.Host),
			SSGrpcURL:              e.getShadowsocksGrpcURL(r.Host),
			SSWsURL:                e.getShadowsocksWsURL(r.Host),
			V2rayNGConfigWS:        e.getV2rayNGConfigWS(r.Host),
			V2rayNGConfigGRPC:      e.getV2rayNGConfigGRPC(r.Host),
			STUNAddr:               net.JoinHostPort(strings.Trim(e.webDomain, "[]"), "3478"),
			DefaultQuota:           e.storage.GetDefaultQuota(),
			MaxMessageSize:         e.maxMessageSize,
			RegistrationOpen:       func() bool { open, _ := e.authDB.IsRegistrationOpen(); return open }(),
			JitRegistrationEnabled: func() bool { enabled, _ := e.authDB.IsJitRegistrationEnabled(); return enabled }(),
			TurnEnabled:            func() bool { enabled, _ := e.authDB.IsTurnEnabled(); return enabled }(),
			Language:               e.getLanguage(),
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

// handleDKIMKey serves the DKIM public key record via HTTPS.
// This enables federation with chatmail relays by providing a fallback
// mechanism for DKIM key retrieval when DNS lookup is not available.
// URL format: /.well-known/_domainkey/<selector>
// Response: plain text DKIM DNS TXT record (e.g., "v=DKIM1; k=rsa; p=...")
//
// Security: This endpoint only serves public key material (the same data
// that would be published in DNS TXT records). No private keys or
// sensitive metadata are exposed.
func (e *Endpoint) handleDKIMKey(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Extract selector from URL path: /.well-known/_domainkey/<selector>
	selector := strings.TrimPrefix(r.URL.Path, "/.well-known/_domainkey/")
	selector = strings.TrimSuffix(selector, "/")

	if selector == "" {
		http.Error(w, "Missing selector", http.StatusBadRequest)
		return
	}

	// Defense-in-depth: canonicalize to bare filename component
	selector = filepath.Base(selector)

	// Strict whitelist: DKIM selectors per RFC 6376 are limited to
	// alphanumeric characters, hyphens, and underscores.
	// Reject anything else to prevent path injection or encoding tricks.
	for _, c := range selector {
		if !((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '-' || c == '_' || c == '.') {
			http.Error(w, "Invalid selector", http.StatusBadRequest)
			return
		}
	}

	// Length limit to prevent abuse
	if len(selector) > 64 {
		http.Error(w, "Invalid selector", http.StatusBadRequest)
		return
	}

	// Strip brackets from IP-literal domains for file lookup
	domain := strings.Trim(e.mailDomain, "[]")

	// Build path to the DKIM DNS record file
	// Convention: {state_dir}/dkim_keys/{domain}_{selector}.dns
	dnsPath := filepath.Join(config.StateDirectory, "dkim_keys", fmt.Sprintf("%s_%s.dns", domain, selector))

	dnsContent, err := os.ReadFile(dnsPath)
	if err != nil {
		if os.IsNotExist(err) {
			e.logger.Debugf("DKIM key not found for selector %q", selector)
			http.Error(w, "DKIM key not found", http.StatusNotFound)
			return
		}
		e.logger.Error("failed to read DKIM DNS record", err, "selector", selector)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.Header().Set("Cache-Control", "public, max-age=86400") // Cache for 24h
	_, _ = w.Write([]byte(strings.TrimSpace(string(dnsContent))))
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
	defer func() {
		if err := delivery.Abort(r.Context()); err != nil {
			// Ignore error if transaction was already committed or rolled back
			if !strings.Contains(err.Error(), "transaction has already been committed") {
				e.logger.Error("failed to abort delivery", err)
			}
		}
	}()

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

	module.IncrementReceivedMessages()

	scheme := "http"
	if r.TLS != nil {
		scheme = "https"
	}
	e.logger.Msg("[federation] received via "+scheme, "from", mailFrom, "to", mailTo)
	w.WriteHeader(http.StatusOK)
}

var shadowsocksOnce sync.Once

// isShadowsocksEnabled checks the __SS_ENABLED__ setting in the database.
// Defaults to true if not set.
func (e *Endpoint) isShadowsocksEnabled() bool {
	val, ok, err := e.authDB.GetSetting(resources.KeySsEnabled)
	if err != nil || !ok {
		return true // default enabled
	}
	return val != "false"
}

// isWsEnabled checks the __SS_WS_ENABLED__ setting in the database.
// Defaults to true if not set.
func (e *Endpoint) isWsEnabled() bool {
	val, ok, err := e.authDB.GetSetting(resources.KeySsWsEnabled)
	if err != nil || !ok {
		return true // default enabled
	}
	return val != "false"
}

// isGrpcEnabled checks the __SS_GRPC_ENABLED__ setting in the database.
// Defaults to true if not set.
func (e *Endpoint) isGrpcEnabled() bool {
	val, ok, err := e.authDB.GetSetting(resources.KeySsGrpcEnabled)
	if err != nil || !ok {
		return true // default enabled
	}
	return val != "false"
}

// resolveSSTlsPaths returns the TLS certificate and key paths for SS transports.
// Falls back from state_dir/certs to /etc/maddy/certs if the primary doesn't exist.
func (e *Endpoint) resolveSSTlsPaths() (certPath, keyPath string) {
	certPath = e.ssCertPath
	keyPath = e.ssKeyPath
	if certPath == "" {
		certPath = filepath.Join(config.StateDirectory, "certs", "fullchain.pem")
		// Fallback: try /etc/maddy/certs/ if stat_dir doesn't have the cert
		if _, err := os.Stat(certPath); os.IsNotExist(err) {
			alt := "/etc/maddy/certs/fullchain.pem"
			if _, err2 := os.Stat(alt); err2 == nil {
				certPath = alt
			}
		}
	}
	if keyPath == "" {
		keyPath = filepath.Join(config.StateDirectory, "certs", "privkey.pem")
		if _, err := os.Stat(keyPath); os.IsNotExist(err) {
			alt := "/etc/maddy/certs/privkey.pem"
			if _, err2 := os.Stat(alt); err2 == nil {
				keyPath = alt
			}
		}
	}
	return
}

func (e *Endpoint) runShadowsocks() {
	shadowsocksOnce.Do(func() {
		e.runShadowsocksInternal()
	})
}

func (e *Endpoint) runShadowsocksInternal() {
	ciph, err := core.PickCipher(e.ssCipher, nil, e.ssPassword)
	if err != nil {
		e.logger.Error("Shadowsocks: failed to pick cipher", err)
		return
	}

	// Raw TCP Shadowsocks on the configured public port (for Delta Chat
	// and other clients that don't use v2ray-plugin transports).
	publicListener, err := net.Listen("tcp", e.ssAddr)
	if err != nil {
		e.logger.Error("Shadowsocks: failed to listen on public port", err)
		return
	}

	e.logger.Printf("Shadowsocks: raw TCP on %s (cipher: %s)", e.ssAddr, e.ssCipher)

	// Accept loop for the raw TCP SS handler
	go func() {
		for {
			conn, err := publicListener.Accept()
			if err != nil {
				if !errors.Is(err, net.ErrClosed) {
					e.logger.Error("Shadowsocks: accept failed", err)
				}
				return
			}

			go func(conn net.Conn) {
				defer conn.Close()
				if !e.isShadowsocksEnabled() {
					return
				}
				cConn := ciph.StreamConn(conn)
				tgtAddr, err := socks.ReadAddr(cConn)
				if err != nil {
					e.logger.Error("Shadowsocks: failed to read target address", err)
					return
				}

				tgtHost, tgtPort, err := net.SplitHostPort(tgtAddr.String())
				if err != nil {
					e.logger.Error("Shadowsocks: failed to split host:port", err, "addr", tgtAddr.String())
					return
				}

				if !e.ssAllowedPorts[tgtPort] {
					e.logger.Msg("Shadowsocks: blocking unauthorized port", "port", tgtPort, "host", tgtHost)
					return
				}

				localAddr := net.JoinHostPort("127.0.0.1", tgtPort)
				e.logger.Msg("Shadowsocks: relaying", "from", conn.RemoteAddr(), "to", localAddr)

				remote, err := net.Dial("tcp", localAddr)
				if err != nil {
					e.logger.Error("Shadowsocks: failed to connect to local port", err, "addr", localAddr)
					return
				}
				defer remote.Close()

				go func() { _, _ = io.Copy(remote, cConn) }()
				_, _ = io.Copy(cConn, remote)
			}(conn)
		}
	}()

	// Additionally, start the embedded Xray-core transports:
	// - gRPC on SS port + 1 (e.g. 8389) for v2ray-plugin gRPC clients
	// - WebSocket on SS port + 2 (e.g. 8390) for v2ray-plugin WS clients
	// Both forward to the raw TCP SS handler, keeping it for Delta Chat.
	_, ssPortStr, err := net.SplitHostPort(e.ssAddr)
	if err == nil {
		ssPort, _ := strconv.ParseUint(ssPortStr, 10, 32)
		grpcPort := int(ssPort) + 1
		wsPort := int(ssPort) + 2
		if e.isGrpcEnabled() {
			go e.runXrayGRPC(grpcPort)
		} else {
			e.logger.Printf("Shadowsocks gRPC transport disabled via admin setting")
		}
		if e.isWsEnabled() {
			go e.runXrayWS(wsPort)
		} else {
			e.logger.Printf("Shadowsocks WebSocket transport disabled via admin setting")
		}
	}
}

// runXrayGRPC starts an embedded Xray-core instance that listens on the
// given port with gRPC transport (over TLS) and forwards to the raw TCP
// Shadowsocks server on e.ssAddr. This gives v2ray-plugin clients an
// obfuscated endpoint while the main SS port stays raw TCP for Delta Chat.
func (e *Endpoint) runXrayGRPC(grpcListenPort int) {
	// Resolve the listen host from the SS address
	ssHost, _, err := net.SplitHostPort(e.ssAddr)
	if err != nil {
		e.logger.Error("Shadowsocks gRPC: invalid ss_addr", err)
		return
	}
	if ssHost == "" {
		ssHost = "0.0.0.0"
	}

	// Read SS credentials
	password, cipher := e.ssPassword, e.ssCipher
	if e.authDB != nil {
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPassword); err == nil && ok && v != "" {
			password = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsCipher); err == nil && ok && v != "" {
			cipher = v
		}
	}

	var cipherType xss.CipherType
	switch strings.ToUpper(cipher) {
	case "AES-128-GCM":
		cipherType = xss.CipherType_AES_128_GCM
	case "AES-256-GCM":
		cipherType = xss.CipherType_AES_256_GCM
	case "CHACHA20-IETF-POLY1305":
		cipherType = xss.CipherType_CHACHA20_POLY1305
	default:
		e.logger.Error("Shadowsocks gRPC: unsupported cipher for Xray", nil, "cipher", cipher)
		return
	}

	ssAccount := &xss.Account{
		Password:   password,
		CipherType: cipherType,
	}

	// Resolve TLS cert/key paths.
	certPath, keyPath := e.resolveSSTlsPaths()

	certData, err := filesystem.ReadFile(certPath)
	if err != nil {
		e.logger.Error("Shadowsocks gRPC: failed to read TLS cert", err, "path", certPath)
		return
	}
	keyData, err := filesystem.ReadFile(keyPath)
	if err != nil {
		e.logger.Error("Shadowsocks gRPC: failed to read TLS key", err, "path", keyPath)
		return
	}

	serverName := strings.Trim(e.mailDomain, "[]")
	if e.publicIP != "" {
		serverName = e.publicIP
	}

	tlsConfig := &xtls.Config{
		ServerName: serverName,
		Certificate: []*xtls.Certificate{{
			Certificate: certData,
			Key:         keyData,
		}},
	}

	grpcConfig := &grpc.Config{
		ServiceName: "GunService",
	}

	streamConfig := &internet.StreamConfig{
		ProtocolName: "grpc",
		TransportSettings: []*internet.TransportConfig{{
			ProtocolName: "grpc",
			Settings:     serial.ToTypedMessage(grpcConfig),
		}},
		SecurityType:     serial.GetMessageType(tlsConfig),
		SecuritySettings: []*serial.TypedMessage{serial.ToTypedMessage(tlsConfig)},
	}

	xConfig := &xcore.Config{
		Inbound: []*xcore.InboundHandlerConfig{{
			ReceiverSettings: serial.ToTypedMessage(&proxyman.ReceiverConfig{
				PortList:       &xnet.PortList{Range: []*xnet.PortRange{xnet.SinglePortRange(xnet.Port(grpcListenPort))}},
				Listen:         xnet.NewIPOrDomain(xnet.ParseAddress(ssHost)),
				StreamSettings: streamConfig,
			}),
			ProxySettings: serial.ToTypedMessage(&xss.ServerConfig{
				Users: []*protocol.User{{
					Account: serial.ToTypedMessage(ssAccount),
				}},
				Network: []xnet.Network{xnet.Network_TCP},
			}),
		}},
		Outbound: []*xcore.OutboundHandlerConfig{
			{
				Tag: "allow",
				ProxySettings: serial.ToTypedMessage(&freedom.Config{
					DestinationOverride: &freedom.DestinationOverride{
						Server: &protocol.ServerEndpoint{
							Address: xnet.NewIPOrDomain(xnet.LocalHostIP),
						},
					},
				}),
			},
			{
				Tag:           "block",
				ProxySettings: serial.ToTypedMessage(&blackhole.Config{}),
			},
		},
		App: []*serial.TypedMessage{
			serial.ToTypedMessage(&dispatcher.Config{}),
			serial.ToTypedMessage(&proxyman.InboundConfig{}),
			serial.ToTypedMessage(&proxyman.OutboundConfig{}),
			serial.ToTypedMessage(e.buildSSPortRouting()),
			serial.ToTypedMessage(&xlog.Config{
				ErrorLogType:  xlog.LogType_None,
				AccessLogType: xlog.LogType_None,
			}),
		},
	}

	if e.logger.Debug {
		xConfig.App[4] = serial.ToTypedMessage(&xlog.Config{
			ErrorLogType:  xlog.LogType_Console,
			ErrorLogLevel: xclog.Severity_Debug,
			AccessLogType: xlog.LogType_Console,
		})
	}

	instance, err := xcore.New(xConfig)
	if err != nil {
		e.logger.Error("Shadowsocks gRPC: failed to create xray instance", err)
		return
	}

	if err := instance.Start(); err != nil {
		e.logger.Error("Shadowsocks gRPC: failed to start xray instance", err)
		return
	}

	e.logger.Printf("Shadowsocks gRPC: listening on %s:%d (xray ss+grpc+tls, cipher=%s, cert=%s)", ssHost, grpcListenPort, cipher, certPath)

	select {}
}

// runXrayWS starts an embedded Xray-core instance that listens on the
// given port with WebSocket transport (over TLS) and forwards to the raw TCP
// Shadowsocks server on e.ssAddr. This gives v2ray-plugin WS clients an
// obfuscated endpoint while the main SS port stays raw TCP for Delta Chat.
func (e *Endpoint) runXrayWS(wsListenPort int) {
	// Resolve the listen host from the SS address
	ssHost, _, err := net.SplitHostPort(e.ssAddr)
	if err != nil {
		e.logger.Error("Shadowsocks WS: invalid ss_addr", err)
		return
	}
	if ssHost == "" {
		ssHost = "0.0.0.0"
	}

	// Read SS credentials (same password/cipher as raw TCP handler)
	password, cipher := e.ssPassword, e.ssCipher
	if e.authDB != nil {
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPassword); err == nil && ok && v != "" {
			password = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsCipher); err == nil && ok && v != "" {
			cipher = v
		}
	}

	// Map cipher name to Xray enum
	var cipherType xss.CipherType
	switch strings.ToUpper(cipher) {
	case "AES-128-GCM":
		cipherType = xss.CipherType_AES_128_GCM
	case "AES-256-GCM":
		cipherType = xss.CipherType_AES_256_GCM
	case "CHACHA20-IETF-POLY1305":
		cipherType = xss.CipherType_CHACHA20_POLY1305
	default:
		e.logger.Error("Shadowsocks WS: unsupported cipher for Xray", nil, "cipher", cipher)
		return
	}

	ssAccount := &xss.Account{
		Password:   password,
		CipherType: cipherType,
	}

	// Plain WebSocket transport (no TLS) — SS encryption provides security.
	wsConfig := &websocket.Config{
		Path: "/ss",
	}

	streamConfig := &internet.StreamConfig{
		ProtocolName: "websocket",
		TransportSettings: []*internet.TransportConfig{{
			ProtocolName: "websocket",
			Settings:     serial.ToTypedMessage(wsConfig),
		}},
	}

	// Use Xray's native Shadowsocks inbound (NOT dokodemo).
	// This ensures compatibility with Xray/v2ray-based clients (v2rayNG etc)
	// which use Xray's own SS encryption that is NOT compatible with
	// go-shadowsocks2's AEAD implementation.
	xConfig := &xcore.Config{
		Inbound: []*xcore.InboundHandlerConfig{{
			ReceiverSettings: serial.ToTypedMessage(&proxyman.ReceiverConfig{
				PortList:       &xnet.PortList{Range: []*xnet.PortRange{xnet.SinglePortRange(xnet.Port(wsListenPort))}},
				Listen:         xnet.NewIPOrDomain(xnet.ParseAddress(ssHost)),
				StreamSettings: streamConfig,
			}),
			ProxySettings: serial.ToTypedMessage(&xss.ServerConfig{
				Users: []*protocol.User{{
					Account: serial.ToTypedMessage(ssAccount),
				}},
				Network: []xnet.Network{xnet.Network_TCP},
			}),
		}},
		// Outbound 'allow': forward to 127.0.0.1 (only for allowed ports)
		Outbound: []*xcore.OutboundHandlerConfig{
			{
				Tag: "allow",
				ProxySettings: serial.ToTypedMessage(&freedom.Config{
					DestinationOverride: &freedom.DestinationOverride{
						Server: &protocol.ServerEndpoint{
							Address: xnet.NewIPOrDomain(xnet.LocalHostIP),
						},
					},
				}),
			},
			{
				Tag:           "block",
				ProxySettings: serial.ToTypedMessage(&blackhole.Config{}),
			},
		},
		App: []*serial.TypedMessage{
			serial.ToTypedMessage(&dispatcher.Config{}),
			serial.ToTypedMessage(&proxyman.InboundConfig{}),
			serial.ToTypedMessage(&proxyman.OutboundConfig{}),
			serial.ToTypedMessage(e.buildSSPortRouting()),
			serial.ToTypedMessage(&xlog.Config{
				ErrorLogType:  xlog.LogType_None,
				AccessLogType: xlog.LogType_None,
			}),
		},
	}

	if e.logger.Debug {
		xConfig.App[4] = serial.ToTypedMessage(&xlog.Config{
			ErrorLogType:  xlog.LogType_Console,
			ErrorLogLevel: xclog.Severity_Debug,
			AccessLogType: xlog.LogType_Console,
		})
	}

	instance, err := xcore.New(xConfig)
	if err != nil {
		e.logger.Error("Shadowsocks WS: failed to create xray instance", err)
		return
	}

	if err := instance.Start(); err != nil {
		e.logger.Error("Shadowsocks WS: failed to start xray instance", err)
		return
	}

	e.logger.Printf("Shadowsocks WS: listening on %s:%d (xray ss+ws, cipher=%s)", ssHost, wsListenPort, cipher)

	select {}
}

func (e *Endpoint) getShadowsocksURL(hostHint string) string {
	if e.ssAddr == "" || !e.isShadowsocksEnabled() {
		return ""
	}

	password, cipher, port := e.ssPassword, e.ssCipher, ""
	_, port, _ = net.SplitHostPort(e.ssAddr)

	// Check for DB overrides to match what the admin panel shows
	if e.authDB != nil {
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPassword); err == nil && ok && v != "" {
			password = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsCipher); err == nil && ok && v != "" {
			cipher = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPort); err == nil && ok && v != "" {
			port = v
		}
	}

	// format: ss://BASE64(method:password)@host:port?plugin=...#tag
	userInfo := fmt.Sprintf("%s:%s", cipher, password)
	auth := base64.StdEncoding.EncodeToString([]byte(userInfo))
	auth = strings.TrimRight(auth, "=")

	host, _, _ := net.SplitHostPort(e.ssAddr)
	if host == "" || host == "0.0.0.0" {
		host = hostHint
		if host == "" {
			host = strings.Trim(e.mailDomain, "[]")
		} else {
			if h, _, err := net.SplitHostPort(host); err == nil {
				host = h
			}
		}
	}

	// Plain SS URL (raw TCP) — compatible with Delta Chat and all basic clients.
	// The gRPC transport is available on port+1 for v2ray-plugin clients.
	return fmt.Sprintf("ss://%s@%s:%s#%s", auth, host, port, url.QueryEscape(host))
}

// getShadowsocksGrpcURL returns the v2ray-plugin gRPC URL (port+1).
func (e *Endpoint) getShadowsocksGrpcURL(hostHint string) string {
	if e.ssAddr == "" || !e.isShadowsocksEnabled() || !e.isGrpcEnabled() {
		return ""
	}

	password, cipher, port := e.ssPassword, e.ssCipher, ""
	_, port, _ = net.SplitHostPort(e.ssAddr)

	if e.authDB != nil {
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPassword); err == nil && ok && v != "" {
			password = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsCipher); err == nil && ok && v != "" {
			cipher = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPort); err == nil && ok && v != "" {
			port = v
		}
	}

	// gRPC port = SS port + 1
	if p, err := strconv.Atoi(port); err == nil {
		port = strconv.Itoa(p + 1)
	}

	userInfo := fmt.Sprintf("%s:%s", cipher, password)
	auth := base64.StdEncoding.EncodeToString([]byte(userInfo))
	auth = strings.TrimRight(auth, "=")

	host, _, _ := net.SplitHostPort(e.ssAddr)
	if host == "" || host == "0.0.0.0" {
		host = hostHint
		if host == "" {
			host = strings.Trim(e.mailDomain, "[]")
		} else {
			if h, _, err := net.SplitHostPort(host); err == nil {
				host = h
			}
		}
	}

	pluginOpts := url.QueryEscape(fmt.Sprintf("v2ray-plugin;mode=grpc;host=%s", host))
	return fmt.Sprintf("ss://%s@%s:%s/?plugin=%s#%s", auth, host, port, pluginOpts, url.QueryEscape(host))
}

// getShadowsocksWsURL returns the v2ray-plugin WebSocket URL (port+2).
func (e *Endpoint) getShadowsocksWsURL(hostHint string) string {
	if e.ssAddr == "" || !e.isShadowsocksEnabled() || !e.isWsEnabled() {
		return ""
	}

	password, cipher, port := e.ssPassword, e.ssCipher, ""
	_, port, _ = net.SplitHostPort(e.ssAddr)

	if e.authDB != nil {
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPassword); err == nil && ok && v != "" {
			password = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsCipher); err == nil && ok && v != "" {
			cipher = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPort); err == nil && ok && v != "" {
			port = v
		}
	}

	// WS port = SS port + 2
	if p, err := strconv.Atoi(port); err == nil {
		port = strconv.Itoa(p + 2)
	}

	userInfo := fmt.Sprintf("%s:%s", cipher, password)
	auth := base64.StdEncoding.EncodeToString([]byte(userInfo))
	auth = strings.TrimRight(auth, "=")

	host, _, _ := net.SplitHostPort(e.ssAddr)
	if host == "" || host == "0.0.0.0" {
		host = hostHint
		if host == "" {
			host = strings.Trim(e.mailDomain, "[]")
		} else {
			if h, _, err := net.SplitHostPort(host); err == nil {
				host = h
			}
		}
	}

	pluginOpts := url.QueryEscape(fmt.Sprintf("v2ray-plugin;mode=websocket;host=%s;path=/ss", host))
	return fmt.Sprintf("ss://%s@%s:%s/?plugin=%s#%s", auth, host, port, pluginOpts, url.QueryEscape(host))
}

func (e *Endpoint) getShadowsocksActiveSettings() (password, cipher, port string) {
	_, port, _ = net.SplitHostPort(e.ssAddr)
	return e.ssPassword, e.ssCipher, port
}

// getAllowedPortsCSV returns a comma-separated list of allowed SS ports
// for use in v2rayNG routing rules (e.g. "25,143,465,587,993,3478,5349").
func (e *Endpoint) getAllowedPortsCSV() string {
	ports := make([]string, 0, len(e.ssAllowedPorts))
	for p := range e.ssAllowedPorts {
		ports = append(ports, p)
	}
	return strings.Join(ports, ",")
}

// getV2rayNGConfigWS returns a v2rayNG-compatible JSON config for SS+WS transport.
func (e *Endpoint) getV2rayNGConfigWS(hostHint string) string {
	if !e.isWsEnabled() {
		return ""
	}
	password, cipher, port := e.getShadowsocksActiveSettings()
	if e.authDB != nil {
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPassword); err == nil && ok && v != "" {
			password = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsCipher); err == nil && ok && v != "" {
			cipher = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPort); err == nil && ok && v != "" {
			port = v
		}
	}

	wsPort := 0
	if p, err := strconv.Atoi(port); err == nil {
		wsPort = p + 2
	}

	host := e.resolveSSHost(hostHint)
	allowedPorts := e.getAllowedPortsCSV()

	return fmt.Sprintf(`{
  "dns": {"servers": ["1.1.1.1", "8.8.8.8"]},
  "inbounds": [{"listen": "127.0.0.1", "port": 10808, "protocol": "socks", "settings": {"auth": "noauth", "udp": true}, "sniffing": {"destOverride": ["http", "tls"], "enabled": true}, "tag": "socks"}],
  "log": {"loglevel": "warning"},
  "outbounds": [
    {"protocol": "shadowsocks", "settings": {"servers": [{"address": "%s", "port": %d, "method": "%s", "password": "%s"}]}, "streamSettings": {"network": "ws", "wsSettings": {"path": "/ss", "headers": {"Host": "%s"}}}, "tag": "proxy"},
    {"protocol": "freedom", "tag": "direct"},
    {"protocol": "blackhole", "tag": "block"}
  ],
  "remarks": "%s (WS)",
  "routing": {"domainStrategy": "IPIfNonMatch", "rules": [
    {"outboundTag": "proxy", "port": "%s", "type": "field"},
    {"outboundTag": "block", "port": "0-65535", "type": "field"}
  ]}
}`, host, wsPort, cipher, password, host, host, allowedPorts)
}

// getV2rayNGConfigGRPC returns a v2rayNG-compatible JSON config for SS+gRPC+TLS transport.
func (e *Endpoint) getV2rayNGConfigGRPC(hostHint string) string {
	if !e.isGrpcEnabled() {
		return ""
	}
	password, cipher, port := e.getShadowsocksActiveSettings()
	if e.authDB != nil {
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPassword); err == nil && ok && v != "" {
			password = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsCipher); err == nil && ok && v != "" {
			cipher = v
		}
		if v, ok, err := e.authDB.GetSetting(resources.KeySsPort); err == nil && ok && v != "" {
			port = v
		}
	}

	grpcPort := 0
	if p, err := strconv.Atoi(port); err == nil {
		grpcPort = p + 1
	}

	host := e.resolveSSHost(hostHint)
	allowedPorts := e.getAllowedPortsCSV()

	return fmt.Sprintf(`{
  "dns": {"servers": ["1.1.1.1", "8.8.8.8"]},
  "inbounds": [{"listen": "127.0.0.1", "port": 10808, "protocol": "socks", "settings": {"auth": "noauth", "udp": true}, "sniffing": {"destOverride": ["http", "tls"], "enabled": true}, "tag": "socks"}],
  "log": {"loglevel": "warning"},
  "outbounds": [
    {"protocol": "shadowsocks", "settings": {"servers": [{"address": "%s", "port": %d, "method": "%s", "password": "%s"}]}, "streamSettings": {"network": "grpc", "security": "tls", "tlsSettings": {"allowInsecure": true, "serverName": "%s"}, "grpcSettings": {"serviceName": "GunService"}}, "tag": "proxy"},
    {"protocol": "freedom", "tag": "direct"},
    {"protocol": "blackhole", "tag": "block"}
  ],
  "remarks": "%s (gRPC)",
  "routing": {"domainStrategy": "IPIfNonMatch", "rules": [
    {"outboundTag": "proxy", "port": "%s", "type": "field"},
    {"outboundTag": "block", "port": "0-65535", "type": "field"}
  ]}
}`, host, grpcPort, cipher, password, host, host, allowedPorts)
}

// resolveSSHost determines the public host for SS URLs.
// buildSSPortRouting creates Xray routing config that restricts traffic
// to the same ports as e.ssAllowedPorts (used by the raw TCP SS handler).
// Allowed ports → "allow" outbound, everything else → "block" outbound.
func (e *Endpoint) buildSSPortRouting() *router.Config {
	// Build port ranges from ssAllowedPorts
	var portRanges []*xnet.PortRange
	for portStr := range e.ssAllowedPorts {
		if p, err := strconv.ParseUint(portStr, 10, 32); err == nil {
			portRanges = append(portRanges, xnet.SinglePortRange(xnet.Port(p)))
		}
	}

	return &router.Config{
		Rule: []*router.RoutingRule{
			{
				// Allow only specific ports
				PortList: &xnet.PortList{Range: portRanges},
				TargetTag: &router.RoutingRule_Tag{
					Tag: "allow",
				},
			},
			{
				// Block everything else (catch-all: all ports)
				PortList: &xnet.PortList{Range: []*xnet.PortRange{{
					From: 0,
					To:   65535,
				}}},
				TargetTag: &router.RoutingRule_Tag{
					Tag: "block",
				},
			},
		},
	}
}

func (e *Endpoint) resolveSSHost(hostHint string) string {
	host, _, _ := net.SplitHostPort(e.ssAddr)
	if host == "" || host == "0.0.0.0" {
		host = hostHint
		if host == "" {
			host = strings.Trim(e.mailDomain, "[]")
		} else {
			if h, _, err := net.SplitHostPort(host); err == nil {
				host = h
			}
		}
	}
	return host
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
				// Enforce local-only: if SMTP is restricted and connection is external, reject
				if e.isPortLocalOnly(resources.KeySMTPLocalOnly) && !isLoopback(conn.RemoteAddr()) {
					e.logger.Msg("ALPN: blocking external SMTP (local-only mode)", "remote", conn.RemoteAddr())
					conn.Close()
					return
				}
				e.logger.Msg("ALPN proxy: routing to internal smtp", "remote", conn.RemoteAddr())
				smtpConns <- tls.Server(bConn, e.tlsConfig)
				return
			}
		case "imap":
			if e.imapModule != nil {
				// Enforce local-only: if IMAP is restricted and connection is external, reject
				if e.isPortLocalOnly(resources.KeyIMAPLocalOnly) && !isLoopback(conn.RemoteAddr()) {
					e.logger.Msg("ALPN: blocking external IMAP (local-only mode)", "remote", conn.RemoteAddr())
					conn.Close()
					return
				}
				e.logger.Msg("ALPN proxy: routing to internal imap", "remote", conn.RemoteAddr())
				imapConns <- tls.Server(bConn, e.tlsConfig)
				return
			}
		}
	}
	// Enforce HTTPS local-only: block external HTTP(S) connections if restricted
	if e.isPortLocalOnly(resources.KeyHTTPSLocalOnly) && !isLoopback(conn.RemoteAddr()) {
		e.logger.Msg("ALPN: blocking external HTTPS (local-only mode)", "remote", conn.RemoteAddr())
		conn.Close()
		return
	}

	httpConns <- bConn
}

// isPortLocalOnly checks if a port is set to local-only mode in the settings DB.
func (e *Endpoint) isPortLocalOnly(key string) bool {
	val, ok, err := e.authDB.GetSetting(key)
	if err != nil || !ok {
		return false // default: public
	}
	return val == "true"
}

// isLoopback checks if a net.Addr is a loopback address (127.0.0.0/8 or ::1).
func isLoopback(addr net.Addr) bool {
	var ip net.IP
	switch a := addr.(type) {
	case *net.TCPAddr:
		ip = a.IP
	case *net.UDPAddr:
		ip = a.IP
	default:
		host, _, err := net.SplitHostPort(addr.String())
		if err != nil {
			return false
		}
		ip = net.ParseIP(host)
	}
	return ip != nil && ip.IsLoopback()
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
		var count int64
		e.sharingGORM.Model(&mdb.Contact{}).Where("slug = ?", slug).Count(&count)
		if count > 0 {
			http.Error(w, "این نام مسیر قبلاً انتخاب شده است", http.StatusBadRequest)
			return
		}
	}

	err := e.sharingGORM.Create(&mdb.Contact{Slug: slug, URL: rawURL, Name: name}).Error
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
		if err := json.NewEncoder(w).Encode(data); err != nil {
			e.logger.Error("failed to encode contact share response", err)
		}
		return
	}

	e.serveTemplate(w, r, "contact_share_success.html", data)
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
		"safeHTML":    func(s string) template.HTML { return template.HTML(s) },
		"cleanDomain": func(s string) string { return strings.Trim(s, "[]") },
		"formatBytes": formatBytes,
	}).Parse(string(fileData))
	if err != nil {
		e.logger.Error("failed to parse template", err, "file", name)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}

	// Composite data including default fields
	data := struct {
		MailDomain             string
		MXDomain               string
		WebDomain              string
		PublicIP               string
		TurnOffTLS             bool
		Version                string
		SSURL                  string
		DefaultQuota           int64
		RegistrationOpen       bool
		JitRegistrationEnabled bool
		Language               string
		Custom                 interface{}
	}{
		MailDomain:             e.mailDomain,
		MXDomain:               e.mxDomain,
		WebDomain:              e.webDomain,
		PublicIP:               e.publicIP,
		TurnOffTLS:             e.turnOffTLS,
		Version:                config.Version,
		SSURL:                  e.getShadowsocksURL(r.Host),
		DefaultQuota:           e.storage.GetDefaultQuota(),
		RegistrationOpen:       func() bool { open, _ := e.authDB.IsRegistrationOpen(); return open }(),
		JitRegistrationEnabled: func() bool { enabled, _ := e.authDB.IsJitRegistrationEnabled(); return enabled }(),
		Language:               e.getLanguage(),
		Custom:                 customData,
	}

	// Hot-path optimization: use cached values to avoid DB calls on every request
	// (Re-sync with DB every 5 seconds to catch CLI changes)
	e.cache.RLock()
	isStale := !e.cache.hydrated || time.Since(e.cache.lastChecked) > 5*time.Second
	e.cache.RUnlock()

	if isStale {
		e.hydrateCache()
	}

	e.cache.RLock()
	data.DefaultQuota = e.cache.defaultQuota
	data.RegistrationOpen = e.cache.registrationOpen
	data.JitRegistrationEnabled = e.cache.jitRegistrationEnabled
	data.Language = e.cache.language
	e.cache.RUnlock()

	// Fallback for language if cache is empty
	if data.Language == "" {
		data.Language = e.getLanguage()
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	if err := tmpl.Execute(w, data); err != nil {
		e.logger.Error("failed to execute template", err, "file", name)
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}
}

func formatBytes(b int64) string {
	const unit = 1024
	if b < unit {
		return fmt.Sprintf("%d B", b)
	}
	div, exp := int64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f %cB", float64(b)/float64(div), "KMGTPE"[exp])
}

// AdminTokenFileName is the filename in state_dir where the auto-generated admin token is stored.
const AdminTokenFileName = "admin_token"

// ensureAdminToken loads or generates a persistent admin token.
// The token is stored in {state_dir}/admin_token so it persists across restarts.
func (e *Endpoint) ensureAdminToken() (string, error) {
	tokenPath := filepath.Join(config.StateDirectory, AdminTokenFileName)

	// Try to read existing token
	data, err := os.ReadFile(tokenPath)
	if err == nil {
		token := strings.TrimSpace(string(data))
		if token != "" {
			e.logger.Printf("admin API token loaded from %s", tokenPath)
			return token, nil
		}
	}

	// Generate a new token
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return "", fmt.Errorf("failed to generate admin token: %v", err)
	}
	token := base64.RawURLEncoding.EncodeToString(b)

	// Persist it
	if err := os.WriteFile(tokenPath, []byte(token+"\n"), 0600); err != nil {
		return "", fmt.Errorf("failed to save admin token to %s: %v", tokenPath, err)
	}

	e.logger.Printf("admin API token generated and saved to %s", tokenPath)
	return token, nil
}

// setupAdminAPI creates and registers all admin API resource handlers.
func (e *Endpoint) setupAdminAPI() {
	handler := adminapi.NewHandler(e.adminToken, e.logger)

	// Build settings deps using the auth DB and GORM DB for generic settings
	var gormDB *gorm.DB
	if gp, ok := e.storage.(module.GORMProvider); ok {
		gormDB = gp.GetGORMDB()
	}

	settingsDeps := resources.SettingsToggleDeps{
		IsRegistrationOpen:           e.authDB.IsRegistrationOpen,
		SetRegistrationOpen:          e.authDB.SetRegistrationOpen,
		IsJitRegistrationEnabled:     e.authDB.IsJitRegistrationEnabled,
		SetJitRegistrationEnabled:    e.authDB.SetJitRegistrationEnabled,
		IsTurnEnabled:                e.authDB.IsTurnEnabled,
		SetTurnEnabled:               e.authDB.SetTurnEnabled,
		GetShadowsocksActiveSettings: e.getShadowsocksActiveSettings,
	}

	// Generic DB-backed settings using the auth pass_table's settings table
	settingsDeps.GetSetting = e.authDB.GetSetting
	settingsDeps.SetSetting = func(key, value string) error {
		err := e.authDB.SetSetting(key, value)
		if err == nil {
			e.cache.Lock()
			if key == resources.KeyLanguage {
				e.cache.language = value
			}
			e.cache.Unlock()
		}
		return err
	}
	settingsDeps.DeleteSetting = func(key string) error {
		err := e.authDB.DeleteSetting(key)
		if err == nil {
			e.cache.Lock()
			if key == resources.KeyLanguage {
				e.cache.language = ""
			}
			e.cache.Unlock()
		}
		return err
	}

	// Registration Control (override to update cache)
	settingsDeps.SetRegistrationOpen = func(open bool) error {
		err := e.authDB.SetRegistrationOpen(open)
		if err == nil {
			e.cache.Lock()
			e.cache.registrationOpen = open
			e.cache.Unlock()
		}
		return err
	}
	settingsDeps.SetJitRegistrationEnabled = func(enabled bool) error {
		err := e.authDB.SetJitRegistrationEnabled(enabled)
		if err == nil {
			e.cache.Lock()
			e.cache.jitRegistrationEnabled = enabled
			e.cache.Unlock()
		}
		return err
	}

	// Also ensure the GORM table_entries table exists for legacy compatibility
	if gormDB != nil {
		_ = gormDB.AutoMigrate(&mdb.TableEntry{})
	}

	// User count helper
	getUserCount := func() (int, error) {
		users, err := e.authDB.ListUsers()
		if err != nil {
			return 0, err
		}
		return len(users), nil
	}

	// Register resource handlers
	handler.Register("/admin/status", resources.StatusHandler(resources.StatusDeps{
		GetUserCount: getUserCount,
		GetSetting:   e.authDB.GetSetting,
	}))

	handler.Register("/admin/restart", resources.RestartHandler())

	handler.Register("/admin/storage", resources.StorageHandler(resources.StorageDeps{
		StateDir: config.StateDirectory,
	}))

	// Toggle settings
	handler.Register("/admin/registration", resources.RegistrationHandler(settingsDeps))
	handler.Register("/admin/registration/jit", resources.JitRegistrationHandler(settingsDeps))
	handler.Register("/admin/services/turn", resources.TurnHandler(settingsDeps))
	handler.Register("/admin/services/iroh", resources.IrohHandler(settingsDeps))
	handler.Register("/admin/services/shadowsocks", resources.ShadowsocksHandler(settingsDeps))
	handler.Register("/admin/services/ss_ws", resources.SsWsHandler(settingsDeps))
	handler.Register("/admin/services/ss_grpc", resources.SsGrpcHandler(settingsDeps))
	handler.Register("/admin/services/http_proxy", resources.HTTPProxyHandler(settingsDeps))
	handler.Register("/admin/services/log", resources.LogHandler(settingsDeps))

	// Bulk settings endpoint
	handler.Register("/admin/settings", resources.AllSettingsHandler(settingsDeps))

	// Port settings
	handler.Register("/admin/settings/smtp_port", resources.GenericSettingHandler(resources.KeySMTPPort, settingsDeps))
	handler.Register("/admin/settings/submission_port", resources.GenericSettingHandler(resources.KeySubmissionPort, settingsDeps))
	handler.Register("/admin/settings/imap_port", resources.GenericSettingHandler(resources.KeyIMAPPort, settingsDeps))
	handler.Register("/admin/settings/turn_port", resources.GenericSettingHandler(resources.KeyTurnPort, settingsDeps))
	handler.Register("/admin/settings/sasl_port", resources.GenericSettingHandler(resources.KeySaslPort, settingsDeps))
	handler.Register("/admin/settings/iroh_port", resources.GenericSettingHandler(resources.KeyIrohPort, settingsDeps))
	handler.Register("/admin/settings/ss_port", resources.GenericSettingHandler(resources.KeySsPort, settingsDeps))
	handler.Register("/admin/settings/ss_ws_port", resources.GenericSettingHandler(resources.KeySsWsPort, settingsDeps))
	handler.Register("/admin/settings/ss_grpc_port", resources.GenericSettingHandler(resources.KeySsGrpcPort, settingsDeps))

	// Per-port access control (local-only toggles)
	handler.Register("/admin/settings/smtp_local_only", resources.GenericSettingHandler(resources.KeySMTPLocalOnly, settingsDeps))
	handler.Register("/admin/settings/submission_local_only", resources.GenericSettingHandler(resources.KeySubmissionLocalOnly, settingsDeps))
	handler.Register("/admin/settings/imap_local_only", resources.GenericSettingHandler(resources.KeyIMAPLocalOnly, settingsDeps))
	handler.Register("/admin/settings/turn_local_only", resources.GenericSettingHandler(resources.KeyTurnLocalOnly, settingsDeps))
	handler.Register("/admin/settings/iroh_local_only", resources.GenericSettingHandler(resources.KeyIrohLocalOnly, settingsDeps))
	handler.Register("/admin/settings/http_local_only", resources.GenericSettingHandler(resources.KeyHTTPLocalOnly, settingsDeps))
	handler.Register("/admin/settings/https_local_only", resources.GenericSettingHandler(resources.KeyHTTPSLocalOnly, settingsDeps))

	// Configuration settings
	handler.Register("/admin/settings/smtp_hostname", resources.GenericSettingHandler(resources.KeySMTPHostname, settingsDeps))
	handler.Register("/admin/settings/turn_realm", resources.GenericSettingHandler(resources.KeyTurnRealm, settingsDeps))
	handler.Register("/admin/settings/turn_secret", resources.GenericSettingHandler(resources.KeyTurnSecret, settingsDeps))
	handler.Register("/admin/settings/turn_relay_ip", resources.GenericSettingHandler(resources.KeyTurnRelayIP, settingsDeps))
	handler.Register("/admin/settings/turn_ttl", resources.GenericSettingHandler(resources.KeyTurnTTL, settingsDeps))
	handler.Register("/admin/settings/iroh_relay_url", resources.GenericSettingHandler(resources.KeyIrohRelayURL, settingsDeps))
	handler.Register("/admin/settings/ss_cipher", resources.GenericSettingHandler(resources.KeySsCipher, settingsDeps))
	handler.Register("/admin/settings/ss_password", resources.GenericSettingHandler(resources.KeySsPassword, settingsDeps))
	handler.Register("/admin/settings/http_port", resources.GenericSettingHandler(resources.KeyHTTPPort, settingsDeps))
	handler.Register("/admin/settings/https_port", resources.GenericSettingHandler(resources.KeyHTTPSPort, settingsDeps))
	handler.Register("/admin/settings/http_proxy_port", resources.GenericSettingHandler(resources.KeyHTTPProxyPort, settingsDeps))
	handler.Register("/admin/settings/http_proxy_path", resources.GenericSettingHandler(resources.KeyHTTPProxyPath, settingsDeps))
	handler.Register("/admin/settings/http_proxy_username", resources.GenericSettingHandler(resources.KeyHTTPProxyUsername, settingsDeps))
	handler.Register("/admin/settings/http_proxy_password", resources.GenericSettingHandler(resources.KeyHTTPProxyPassword, settingsDeps))
	handler.Register("/admin/settings/admin_path", resources.GenericSettingHandler(resources.KeyAdminPath, settingsDeps))
	handler.Register("/admin/settings/admin_web_path", resources.GenericSettingHandler(resources.KeyAdminWebPath, settingsDeps))
	handler.Register("/admin/settings/language", resources.GenericSettingHandler(resources.KeyLanguage, settingsDeps))
	handler.Register("/admin/services/admin_web", resources.AdminWebHandler(settingsDeps))

	handler.Register("/admin/accounts", resources.AccountsHandler(resources.AccountsDeps{
		AuthDB:     e.authDB,
		Storage:    e.storage,
		MailDomain: e.mailDomain,
	}))

	handler.Register("/admin/notice", resources.NoticeHandler(resources.NoticeDeps{
		AuthDB:     e.authDB,
		Storage:    e.storage,
		MailDomain: e.mailDomain,
	}))

	handler.Register("/admin/quota", resources.QuotaHandler(resources.QuotaDeps{
		Storage: e.storage,
	}))

	handler.Register("/admin/queue", resources.QueueHandler(resources.QueueDeps{
		Storage: e.storage,
	}))

	handler.Register("/admin/blocklist", resources.BlocklistHandler(resources.BlocklistDeps{
		Storage: e.storage,
	}))

	// Contact shares (if enabled)
	if e.enableContactSharing && e.sharingGORM != nil {
		handler.Register("/admin/shares", resources.SharesHandler(resources.SharesDeps{
			DB: e.sharingGORM,
		}))
	}

	// Endpoint cache (if GORM DB available)
	if gormDB != nil {
		handler.Register("/admin/dns", resources.DNSCacheHandler(resources.DNSCacheDeps{
			DB: gormDB,
		}))
	}

	// Exchangers (if exchanger GORM DB available)
	if e.exchangerGORM != nil {
		handler.Register("/admin/exchangers", resources.ExchangerHandler(resources.ExchangerDeps{
			DB: e.exchangerGORM,
		}))
	}

	// Reload / restart endpoint — regenerates config from DB overrides and restarts
	handler.Register("/admin/reload", resources.ReloadHandler(resources.ReloadDeps{
		ReloadConfig: e.reloadConfig,
	}))

	// Determine the admin API path: DB override > config file
	apiPath := e.adminPath
	if e.authDB != nil {
		if val, ok, err := e.authDB.GetSetting(resources.KeyAdminPath); err == nil && ok && val != "" {
			apiPath = val
		}
	}
	e.mux.Handle(apiPath, handler)
	e.logger.Printf("admin API enabled at %s", apiPath)
}

func init() {
	module.RegisterEndpoint(modName, New)
}
