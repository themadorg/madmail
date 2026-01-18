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

package imap

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha1"
	"crypto/tls"
	"encoding/base64"
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/emersion/go-imap"
	compress "github.com/emersion/go-imap-compress"
	sortthread "github.com/emersion/go-imap-sortthread"
	imapbackend "github.com/emersion/go-imap/backend"
	imapserver "github.com/emersion/go-imap/server"
	"github.com/emersion/go-message"
	_ "github.com/emersion/go-message/charset"
	"github.com/emersion/go-sasl"
	i18nlevel "github.com/foxcpp/go-imap-i18nlevel"
	namespace "github.com/foxcpp/go-imap-namespace"
	"github.com/themadorg/madmail/framework/config"
	modconfig "github.com/themadorg/madmail/framework/config/module"
	tls2 "github.com/themadorg/madmail/framework/config/tls"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/auth"
	"github.com/themadorg/madmail/internal/authz"
	"github.com/themadorg/madmail/internal/pgp_verify"
	"github.com/themadorg/madmail/internal/proxy_protocol"
	"github.com/themadorg/madmail/internal/updatepipe"
)

type Endpoint struct {
	addrs         []string
	serv          *imapserver.Server
	listeners     []net.Listener
	proxyProtocol *proxy_protocol.ProxyProtocol
	Store         module.Storage

	tlsConfig   *tls.Config
	listenersWg sync.WaitGroup

	saslAuth auth.SASLAuth

	storageNormalize authz.NormalizeFunc
	storageMap       module.Table

	enableTURN    bool
	turnServer    string
	turnPort      int
	turnSecret    string
	turnTTL       int
	turnPreferTLS bool

	Log log.Logger
}

func New(modName string, addrs []string) (module.Module, error) {
	endp := &Endpoint{
		addrs: addrs,
		Log:   log.Logger{Name: modName},
		saslAuth: auth.SASLAuth{
			Log: log.Logger{Name: modName + "/sasl"},
		},
	}

	return endp, nil
}

func (endp *Endpoint) Init(cfg *config.Map) error {
	var (
		insecureAuth bool
		ioDebug      bool
		ioErrors     bool
	)

	cfg.Callback("auth", func(m *config.Map, node config.Node) error {
		return endp.saslAuth.AddProvider(m, node)
	})
	cfg.Bool("sasl_login", false, false, &endp.saslAuth.EnableLogin)
	cfg.Custom("storage", false, true, nil, modconfig.StorageDirective, &endp.Store)
	cfg.Custom("tls", true, true, nil, tls2.TLSDirective, &endp.tlsConfig)
	cfg.Custom("proxy_protocol", false, false, nil, proxy_protocol.ProxyProtocolDirective, &endp.proxyProtocol)
	cfg.Bool("insecure_auth", false, false, &insecureAuth)
	cfg.Bool("io_debug", false, false, &ioDebug)
	cfg.Bool("io_errors", false, false, &ioErrors)
	cfg.Bool("debug", true, false, &endp.Log.Debug)
	config.EnumMapped(cfg, "storage_map_normalize", false, false, authz.NormalizeFuncs, authz.NormalizeAuto,
		&endp.storageNormalize)
	modconfig.Table(cfg, "storage_map", false, false, nil, &endp.storageMap)
	config.EnumMapped(cfg, "auth_map_normalize", true, false, authz.NormalizeFuncs, authz.NormalizeAuto,
		&endp.saslAuth.AuthNormalize)
	modconfig.Table(cfg, "auth_map", true, false, nil, &endp.saslAuth.AuthMap)

	cfg.Bool("turn_enable", false, false, &endp.enableTURN)
	cfg.String("turn_server", false, false, "", &endp.turnServer)
	cfg.Int("turn_port", false, false, 3478, &endp.turnPort)
	cfg.String("turn_secret", false, false, "", &endp.turnSecret)
	cfg.Int("turn_ttl", false, false, 86400, &endp.turnTTL)
	cfg.Bool("turn_prefer_tls", true, true, &endp.turnPreferTLS)

	if _, err := cfg.Process(); err != nil {
		return err
	}

	if updBe, ok := endp.Store.(updatepipe.Backend); ok {
		if err := updBe.EnableUpdatePipe(updatepipe.ModeReplicate); err != nil {
			endp.Log.Error("failed to initialize updates pipe", err)
		}
	}

	endp.saslAuth.Log.Debug = endp.Log.Debug

	addresses := make([]config.Endpoint, 0, len(endp.addrs))
	for _, addr := range endp.addrs {
		saddr, err := config.ParseEndpoint(addr)
		if err != nil {
			return fmt.Errorf("imap: invalid address: %s", addr)
		}
		addresses = append(addresses, saddr)
	}

	endp.serv = imapserver.New(endp)
	endp.serv.AllowInsecureAuth = insecureAuth
	endp.serv.TLSConfig = endp.tlsConfig
	if ioErrors {
		endp.serv.ErrorLog = &endp.Log
	} else {
		endp.serv.ErrorLog = log.Logger{Out: log.NopOutput{}}
	}
	if ioDebug {
		endp.serv.Debug = endp.Log.DebugWriter()
		endp.Log.Println("I/O debugging is on! It may leak passwords in logs, be careful!")
	}

	if err := endp.enableExtensions(); err != nil {
		return err
	}

	for _, mech := range endp.saslAuth.SASLMechanisms() {
		endp.serv.EnableAuth(mech, func(c imapserver.Conn) sasl.Server {
			return endp.saslAuth.CreateSASL(mech, c.Info().RemoteAddr, func(identity string, data auth.ContextData) error {
				return endp.openAccount(c, identity)
			})
		})
	}

	return endp.setupListeners(addresses)
}

func (endp *Endpoint) setupListeners(addresses []config.Endpoint) error {
	for _, addr := range addresses {
		var l net.Listener
		var err error
		l, err = net.Listen(addr.Network(), addr.Address())
		if err != nil {
			return fmt.Errorf("imap: %v", err)
		}
		endp.Log.Printf("listening on %v", addr)

		if addr.IsTLS() {
			if endp.tlsConfig == nil {
				return errors.New("imap: can't bind on IMAPS endpoint without TLS configuration")
			}
			l = tls.NewListener(l, endp.tlsConfig)
		}

		if endp.proxyProtocol != nil {
			l = proxy_protocol.NewListener(l, endp.proxyProtocol, endp.Log)
		}

		endp.listeners = append(endp.listeners, l)

		endp.listenersWg.Add(1)
		go func() {
			if err := endp.serv.Serve(l); err != nil && !strings.HasSuffix(err.Error(), "use of closed network connection") {
				endp.Log.Printf("imap: failed to serve %s: %s", addr, err)
			}
			endp.listenersWg.Done()
		}()
	}

	if endp.serv.AllowInsecureAuth {
		endp.Log.Println("authentication over unencrypted connections is allowed, this is insecure configuration and should be used only for testing!")
	}
	if endp.serv.TLSConfig == nil {
		endp.Log.Println("TLS is disabled, this is insecure configuration and should be used only for testing!")
		endp.serv.AllowInsecureAuth = true
	}

	return nil
}

func (endp *Endpoint) Name() string {
	return "imap"
}

func (endp *Endpoint) InstanceName() string {
	return "imap"
}

func (endp *Endpoint) Close() error {
	for _, l := range endp.listeners {
		l.Close()
	}
	if err := endp.serv.Close(); err != nil {
		return err
	}
	endp.listenersWg.Wait()
	return nil
}

func (endp *Endpoint) Serve(l net.Listener) error {
	return endp.serv.Serve(l)
}

func (endp *Endpoint) usernameForStorage(ctx context.Context, saslUsername string) (string, error) {
	saslUsername, err := endp.storageNormalize(saslUsername)
	if err != nil {
		return "", err
	}

	if endp.storageMap == nil {
		return saslUsername, nil
	}

	mapped, ok, err := endp.storageMap.Lookup(ctx, saslUsername)
	if err != nil {
		return "", err
	}
	if !ok {
		return "", imapbackend.ErrInvalidCredentials
	}

	if saslUsername != mapped {
		endp.Log.DebugMsg("using mapped username for storage", "username", saslUsername, "mapped_username", mapped)
	}

	return mapped, nil
}

func (endp *Endpoint) openAccount(c imapserver.Conn, identity string) error {
	username, err := endp.usernameForStorage(context.TODO(), identity)
	if err != nil {
		if errors.Is(err, imapbackend.ErrInvalidCredentials) {
			return err
		}
		endp.Log.Error("failed to determine storage account name", err, "username", username)
		return fmt.Errorf("internal server error")
	}

	u, err := endp.Store.GetOrCreateIMAPAcct(username)
	if err != nil {
		return err
	}
	ctx := c.Context()
	ctx.State = imap.AuthenticatedState
	ctx.User = &encryptionWrapperUser{u}
	return nil
}

func (endp *Endpoint) Login(connInfo *imap.ConnInfo, username, password string) (imapbackend.User, error) {
	// saslAuth handles AuthMap calling.
	err := endp.saslAuth.AuthPlain(username, password)
	if err != nil {
		endp.Log.Error("authentication failed", err, "username", username, "src_ip", connInfo.RemoteAddr)
		return nil, imapbackend.ErrInvalidCredentials
	}

	storageUsername, err := endp.usernameForStorage(context.TODO(), username)
	if err != nil {
		if errors.Is(err, imapbackend.ErrInvalidCredentials) {
			return nil, err
		}
		endp.Log.Error("authentication failed due to an internal error", err, "username", username, "src_ip", connInfo.RemoteAddr)
		return nil, fmt.Errorf("internal server error")
	}

	u, err := endp.Store.GetOrCreateIMAPAcct(storageUsername)
	if err != nil {
		return nil, err
	}
	return &encryptionWrapperUser{u}, nil
}

func (endp *Endpoint) I18NLevel() int {
	be, ok := endp.Store.(i18nlevel.Backend)
	if !ok {
		return 0
	}
	return be.I18NLevel()
}

func (endp *Endpoint) enableExtensions() error {
	exts := endp.Store.IMAPExtensions()
	hasQuota := false
	for _, ext := range exts {
		switch ext {
		case "I18NLEVEL=1", "I18NLEVEL=2":
			endp.serv.Enable(i18nlevel.NewExtension())
		case "SORT":
			endp.serv.Enable(sortthread.NewSortExtension())
		case "QUOTA":
			hasQuota = true
		}
		if strings.HasPrefix(ext, "THREAD") {
			endp.serv.Enable(sortthread.NewThreadExtension())
		}
	}

	if hasQuota {
		endp.serv.Enable(&quotaExtension{endp: endp})
	}

	endp.serv.Enable(compress.NewExtension())
	endp.serv.Enable(namespace.NewExtension())

	if endp.enableTURN {
		endp.serv.Enable(&metadataExtension{endp: endp})
	}

	return nil
}

type quotaExtension struct {
	endp *Endpoint
}

func (ext *quotaExtension) Capabilities(c imapserver.Conn) []string {
	if c.Context().State&imap.AuthenticatedState != 0 {
		return []string{"QUOTA"}
	}
	return nil
}

func (ext *quotaExtension) Command(name string) imapserver.HandlerFactory {
	switch strings.ToUpper(name) {
	case "GETQUOTA":
		return func() imapserver.Handler {
			return &getQuotaHandler{endp: ext.endp}
		}
	case "GETQUOTAROOT":
		return func() imapserver.Handler {
			return &getQuotaRootHandler{endp: ext.endp}
		}
	case "SETQUOTA":
		return func() imapserver.Handler {
			return &setQuotaHandler{endp: ext.endp}
		}
	}
	return nil
}

type getQuotaHandler struct {
	endp *Endpoint
	root string
}

func (h *getQuotaHandler) Parse(fields []interface{}) error {
	if len(fields) < 1 {
		return errors.New("GETQUOTA requires a quota root")
	}
	root, ok := fields[0].(string)
	if !ok {
		return errors.New("Quota root must be a string")
	}
	h.root = root
	return nil
}

type quotaStore interface {
	GetQuota(username string) (used, max int64, isDefault bool, err error)
}

func (h *getQuotaHandler) Handle(conn imapserver.Conn) error {
	user := conn.Context().User
	if user == nil {
		return errors.New("Not authenticated")
	}

	qs, ok := h.endp.Store.(quotaStore)
	if !ok {
		return errors.New("Storage does not support quotas")
	}

	used, max, _, err := qs.GetQuota(user.Username())
	if err != nil {
		return err
	}

	usedKB := used / 1024
	maxKB := max / 1024

	// RFC 2087: * QUOTA "ROOT" (STORAGE 10 512)
	if err := conn.WriteResp(&imap.DataResp{
		Fields: []interface{}{
			imap.RawString("QUOTA"),
			"ROOT",
			[]interface{}{
				imap.RawString("STORAGE"),
				uint32(usedKB),
				uint32(maxKB),
			},
		},
	}); err != nil {
		return err
	}

	return nil
}

type getQuotaRootHandler struct {
	endp    *Endpoint
	mailbox string
}

func (h *getQuotaRootHandler) Parse(fields []interface{}) error {
	if len(fields) < 1 {
		return errors.New("GETQUOTAROOT requires a mailbox name")
	}
	mailbox, ok := fields[0].(string)
	if !ok {
		return errors.New("Mailbox name must be a string")
	}
	h.mailbox = mailbox
	return nil
}

func (h *getQuotaRootHandler) Handle(conn imapserver.Conn) error {
	user := conn.Context().User
	if user == nil {
		return errors.New("Not authenticated")
	}

	qs, ok := h.endp.Store.(quotaStore)
	if !ok {
		return errors.New("Storage does not support quotas")
	}

	used, max, _, err := qs.GetQuota(user.Username())
	if err != nil {
		return err
	}

	// For simplicity, we only have one quota root which is "ROOT"
	if err := conn.WriteResp(&imap.DataResp{
		Fields: []interface{}{
			imap.RawString("QUOTAROOT"),
			h.mailbox,
			"ROOT",
		},
	}); err != nil {
		return err
	}

	usedKB := used / 1024
	maxKB := max / 1024
	if err := conn.WriteResp(&imap.DataResp{
		Fields: []interface{}{
			imap.RawString("QUOTA"),
			"ROOT",
			[]interface{}{
				imap.RawString("STORAGE"),
				uint32(usedKB),
				uint32(maxKB),
			},
		},
	}); err != nil {
		return err
	}

	return nil
}

type setQuotaHandler struct {
	endp *Endpoint
}

func (h *setQuotaHandler) Parse(fields []interface{}) error {
	return errors.New("SETQUOTA is not allowed via IMAP")
}

func (h *setQuotaHandler) Handle(conn imapserver.Conn) error {
	return errors.New("SETQUOTA is not allowed via IMAP")
}

func (endp *Endpoint) SupportedThreadAlgorithms() []sortthread.ThreadAlgorithm {
	be, ok := endp.Store.(sortthread.ThreadBackend)
	if !ok {
		return nil
	}

	return be.SupportedThreadAlgorithms()
}

type metadataExtension struct {
	endp *Endpoint
}

func (ext *metadataExtension) Capabilities(c imapserver.Conn) []string {
	turnEnabled := ext.endp.enableTURN && ext.endp.saslAuth.IsTurnEnabled()
	ext.endp.Log.Debugf("IMAP: Capabilities check (state=%v, turnEnabled=%v)", c.Context().State, turnEnabled)
	if !turnEnabled {
		return nil
	}
	return []string{"METADATA"}
}

func (ext *metadataExtension) Command(name string) imapserver.HandlerFactory {
	ext.endp.Log.Debugf("IMAP: Command received: %s", name)
	if strings.ToUpper(name) != "GETMETADATA" {
		return nil
	}
	return func() imapserver.Handler {
		return &getMetadataHandler{endp: ext.endp}
	}
}

type getMetadataHandler struct {
	endp    *Endpoint
	mailbox string
	keys    []string
}

func (h *getMetadataHandler) Parse(fields []interface{}) error {
	h.endp.Log.Debugf("GETMETADATA: parsing fields: %v", fields)
	if len(fields) < 2 {
		return errors.New("GETMETADATA requires mailbox and keys")
	}

	fIdx := 0
	// RFC 5464: GETMETADATA ["mailbox"] [options] (keys)
	// Some clients might send options first, or none.
	for fIdx < len(fields) {
		field, ok := fields[fIdx].(string)
		if !ok {
			// If not a string, maybe it's the keys list?
			if _, ok := fields[fIdx].([]interface{}); ok {
				break
			}
			return fmt.Errorf("unexpected field type at index %d: %T", fIdx, fields[fIdx])
		}

		if strings.ToUpper(field) == "MAXSIZE" || strings.ToUpper(field) == "DEPTH" {
			h.endp.Log.Debugf("GETMETADATA: skipping option %s", field)
			fIdx += 2 // Skip option and value
			continue
		}

		// If it's the last or second to last field and it's a string, it might be the mailbox
		if fIdx == len(fields)-2 || (fIdx == len(fields)-1 && !strings.Contains(field, "/")) {
			h.mailbox = field
			fIdx++
			h.endp.Log.Debugf("GETMETADATA: parsed mailbox: %q", h.mailbox)
			break
		}

		// If it's a single key (string) at the end
		if fIdx == len(fields)-1 {
			h.keys = []string{field}
			h.endp.Log.Debugf("GETMETADATA: parsed single key: %q", field)
			return nil
		}

		fIdx++
	}

	if fIdx >= len(fields) {
		return errors.New("GETMETADATA: missing keys")
	}

	switch keys := fields[fIdx].(type) {
	case string:
		h.keys = []string{keys}
		h.endp.Log.Debugf("GETMETADATA: parsed trailing key: %q", keys)
	case []interface{}:
		for _, k := range keys {
			s, ok := k.(string)
			if !ok {
				return errors.New("Keys must be strings")
			}
			h.keys = append(h.keys, s)
		}
		h.endp.Log.Debugf("GETMETADATA: parsed keys list: %v", h.keys)
	default:
		return fmt.Errorf("GETMETADATA: unexpected keys type: %T", fields[fIdx])
	}

	return nil
}

func (h *getMetadataHandler) Handle(conn imapserver.Conn) error {
	state := conn.Context().State
	h.endp.Log.Debugf("GETMETADATA: handling request (keys=%v, mailbox=%q, state=%v)", h.keys, h.mailbox, state)
	if state&imap.AuthenticatedState == 0 {
		h.endp.Log.Debugf("GETMETADATA: FAILED - connection NOT authenticated")
		return errors.New("Not authenticated")
	}

	h.endp.Log.DebugMsg("handling GETMETADATA", "mailbox", h.mailbox, "keys", h.keys)
	turnEnabled := h.endp.enableTURN && h.endp.saslAuth.IsTurnEnabled()
	h.endp.Log.Debugf("GETMETADATA: TURN is GLOBALLY %v", turnEnabled)

	for _, key := range h.keys {
		h.endp.Log.Debugf("GETMETADATA: checking key %q", key)
		if h.mailbox == "" && (key == "/shared/vendor/deltachat/turn" || key == "/shared/vendor/deltachat/turns") {
			if !turnEnabled {
				continue
			}

			username := strconv.FormatInt(time.Now().Unix()+int64(h.endp.turnTTL), 10)
			mac := hmac.New(sha1.New, []byte(h.endp.turnSecret))
			mac.Write([]byte(username))
			password := base64.StdEncoding.EncodeToString(mac.Sum(nil))

			value := fmt.Sprintf("%s:%d:%s:%s", h.endp.turnServer, h.endp.turnPort, username, password)
			h.endp.Log.Debugf("GETMETADATA: sending %s info: %s", key, value)

			if err := conn.WriteResp(&imap.DataResp{
				Fields: []interface{}{
					imap.RawString("METADATA"),
					h.mailbox,
					[]interface{}{imap.RawString(key), value},
				},
			}); err != nil {
				return err
			}
		}
	}

	return nil
}

type encryptionWrapperUser struct {
	imapbackend.User
}

func (u *encryptionWrapperUser) CreateMessage(mbox string, flags []string, date time.Time, body imap.Literal, mboxObj imapbackend.Mailbox) error {
	res, err := io.ReadAll(body)
	if err != nil {
		return fmt.Errorf("failed to read message for PGP verification: %w", err)
	}

	msg, err := message.Read(bytes.NewReader(res))
	if err != nil {
		return fmt.Errorf("failed to parse message for PGP verification: %w", err)
	}

	isAccepted, err := pgp_verify.IsAcceptedMessage(msg.Header.Header, msg.Body)
	if err != nil {
		return fmt.Errorf("PGP verification failed: %w", err)
	}

	if !isAccepted {
		return errors.New("Encryption Needed: Invalid Unencrypted Mail")
	}

	return u.User.CreateMessage(mbox, flags, date, bytes.NewReader(res), mboxObj)
}

func (u *encryptionWrapperUser) GetMailbox(name string, subscribed bool, conn imapbackend.Conn) (*imap.MailboxStatus, imapbackend.Mailbox, error) {
	status, m, err := u.User.GetMailbox(name, subscribed, conn)
	if err != nil {
		return nil, nil, err
	}
	return status, &encryptionWrapperMailbox{m}, nil
}

type encryptionWrapperMailbox struct {
	imapbackend.Mailbox
}

func init() {
	module.RegisterEndpoint("imap", New)

	imap.CharsetReader = message.CharsetReader
}
