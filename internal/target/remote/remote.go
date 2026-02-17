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

// Package remote implements module which does outgoing
// message delivery using servers discovered using DNS MX records.
//
// Implemented interfaces:
// - module.DeliveryTarget
package remote

import (
	"context"
	"crypto/tls"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"runtime/trace"
	"strings"
	"sync"
	"time"

	"github.com/emersion/go-message/textproto"
	"github.com/emersion/go-smtp"
	"github.com/themadorg/madmail/framework/address"
	"github.com/themadorg/madmail/framework/buffer"
	"github.com/themadorg/madmail/framework/config"
	modconfig "github.com/themadorg/madmail/framework/config/module"
	tls2 "github.com/themadorg/madmail/framework/config/tls"
	"github.com/themadorg/madmail/framework/dns"
	"github.com/themadorg/madmail/framework/exterrors"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/dns_cache"
	"github.com/themadorg/madmail/internal/limits"
	"github.com/themadorg/madmail/internal/smtpconn/pool"
	"github.com/themadorg/madmail/internal/target"
	"golang.org/x/net/idna"
)

var smtpPort = "25"

func moduleError(err error) error {
	return exterrors.WithFields(err, map[string]interface{}{
		"target": "remote",
	})
}

type Target struct {
	name      string
	hostname  string
	localIP   string
	ipv4      bool
	tlsConfig *tls.Config

	resolver    dns.Resolver
	dialer      func(ctx context.Context, network, addr string) (net.Conn, error)
	extResolver *dns.ExtResolver

	// dnsCache provides database-backed DNS overrides.
	// When set, MX lookups and host resolution check the local DB first
	// before falling back to the standard OS resolver.
	dnsCache *dns_cache.Cache

	policies          []module.MXAuthPolicy
	limits            *limits.Group
	allowSecOverride  bool
	relaxedREQUIRETLS bool

	pool           *pool.P
	connReuseLimit int

	Log log.Logger

	connectTimeout    time.Duration
	commandTimeout    time.Duration
	submissionTimeout time.Duration
}

var _ module.DeliveryTarget = &Target{}

func New(_, instName string, _, inlineArgs []string) (module.Module, error) {
	if len(inlineArgs) != 0 {
		return nil, errors.New("remote: inline arguments are not used")
	}
	// Keep this synchronized with testTarget.
	return &Target{
		name:     instName,
		resolver: dns.DefaultResolver(),
		dialer:   (&net.Dialer{}).DialContext,
		Log:      log.Logger{Name: "remote"},
	}, nil
}

func (rt *Target) Init(cfg *config.Map) error {
	var err error
	rt.extResolver, err = dns.NewExtResolver()
	if err != nil {
		rt.Log.Error("cannot initialize DNSSEC-aware resolver, DNSSEC and DANE are not available", err)
	}

	cfg.String("hostname", true, true, "", &rt.hostname)
	cfg.String("local_ip", false, false, "", &rt.localIP)
	cfg.Bool("force_ipv4", false, false, &rt.ipv4)
	cfg.Bool("debug", true, false, &rt.Log.Debug)
	cfg.Custom("tls_client", true, false, func() (interface{}, error) {
		return &tls.Config{}, nil
	}, tls2.TLSClientBlock, &rt.tlsConfig)
	cfg.Custom("mx_auth", false, false, func() (interface{}, error) {
		// Default is "no policies" to follow the principles of explicit
		// configuration (if it is not requested - it is not done).
		return nil, nil
	}, func(cfg *config.Map, n config.Node) (interface{}, error) {
		// Module instance is &PolicyGroup.
		var p *PolicyGroup
		if err := modconfig.GroupFromNode("mx_auth", n.Args, n, cfg.Globals, &p); err != nil {
			return nil, err
		}
		return p.L, nil
	}, &rt.policies)
	cfg.Custom("limits", false, false, func() (interface{}, error) {
		return &limits.Group{}, nil
	}, func(cfg *config.Map, n config.Node) (interface{}, error) {
		var g *limits.Group
		if err := modconfig.GroupFromNode("limits", n.Args, n, cfg.Globals, &g); err != nil {
			return nil, err
		}
		return g, nil
	}, &rt.limits)
	cfg.Bool("requiretls_override", false, true, &rt.allowSecOverride)
	cfg.Bool("relaxed_requiretls", false, true, &rt.relaxedREQUIRETLS)
	cfg.Int("conn_reuse_limit", false, false, 10, &rt.connReuseLimit)
	cfg.Duration("connect_timeout", false, false, 5*time.Minute, &rt.connectTimeout)
	cfg.Duration("command_timeout", false, false, 5*time.Minute, &rt.commandTimeout)
	cfg.Duration("submission_timeout", false, false, 5*time.Minute, &rt.submissionTimeout)

	// Optional reference to the storage module for shared GORM database access.
	// When set, the DNS cache table is stored in the same database as the
	// rest of the application (quotas, etc.) instead of a separate file.
	var storageName string
	cfg.String("storage", false, false, "", &storageName)

	poolCfg := pool.Config{
		MaxKeys:             5000,
		MaxConnsPerKey:      5,      // basically, max. amount of idle connections in cache
		MaxConnLifetimeSec:  150,    // 2.5 mins, half of recommended idle time from RFC 5321
		StaleKeyLifetimeSec: 60 * 5, // should be bigger than MaxConnLifetimeSec
	}
	cfg.Int("conn_max_idle_count", false, false, 5, &poolCfg.MaxConnsPerKey)
	cfg.Int64("conn_max_idle_time", false, false, 150, &poolCfg.MaxConnLifetimeSec)

	if _, err := cfg.Process(); err != nil {
		return err
	}
	rt.pool = pool.New(poolCfg)

	// INTERNATIONALIZATION: See RFC 6531 Section 3.7.1.
	rt.hostname, err = idna.ToASCII(rt.hostname)
	if err != nil {
		return fmt.Errorf("remote: cannot represent the hostname as an A-label name: %w", err)
	}

	if rt.localIP != "" {
		addr, err := net.ResolveTCPAddr("tcp", rt.localIP+":0")
		if err != nil {
			return fmt.Errorf("remote: failed to parse local IP: %w", err)
		}
		rt.dialer = (&net.Dialer{
			LocalAddr: addr,
		}).DialContext
	}
	if rt.ipv4 {
		dial := rt.dialer
		rt.dialer = func(ctx context.Context, network, addr string) (net.Conn, error) {
			if network == "tcp" {
				network = "tcp4"
			}
			return dial(ctx, network, addr)
		}
	}

	// Initialize DNS cache using the shared storage database.
	if storageName != "" {
		storageInst, err := module.GetInstance(storageName)
		if err != nil {
			rt.Log.Error("failed to get storage instance for DNS cache", err)
		} else if gormProvider, ok := storageInst.(module.GORMProvider); !ok {
			rt.Log.Error("storage does not implement GORMProvider, DNS cache overrides will not be available", nil)
		} else {
			cache, cacheErr := dns_cache.New(gormProvider.GetGORMDB(), rt.Log)
			if cacheErr != nil {
				rt.Log.Error("failed to initialize DNS cache", cacheErr)
			} else {
				rt.dnsCache = cache
				rt.Log.Debugf("DNS cache initialized from shared storage")
			}
		}
	}

	return nil
}

func (rt *Target) Close() error {
	rt.pool.Close()

	return nil
}

func (rt *Target) Name() string {
	return "remote"
}

func (rt *Target) InstanceName() string {
	return rt.name
}

// SetDNSCache sets the database-backed DNS override cache.
// When set, MX lookups will check the local DB before falling back
// to the OS DNS resolver.
func (rt *Target) SetDNSCache(cache *dns_cache.Cache) {
	rt.dnsCache = cache
}

type remoteDelivery struct {
	rt       *Target
	mailFrom string
	msgMeta  *module.MsgMetadata
	Log      log.Logger

	recipients    []string
	rcptsByDomain map[string][]string
	connections   map[string]*mxConn
	connMu        sync.Mutex

	policies []module.DeliveryMXAuthPolicy
}

func (rt *Target) Start(ctx context.Context, msgMeta *module.MsgMetadata, mailFrom string) (module.Delivery, error) {
	policies := make([]module.DeliveryMXAuthPolicy, 0, len(rt.policies))
	if !(msgMeta.TLSRequireOverride && rt.allowSecOverride) {
		for _, p := range rt.policies {
			policies = append(policies, p.Start(msgMeta))
		}
	}

	var (
		ratelimitDomain string
		err             error
	)
	// This will leave ratelimitDomain = "" for null return path which is fine
	// for purposes of ratelimiting.
	if mailFrom != "" {
		_, ratelimitDomain, err = address.Split(mailFrom)
		if err != nil {
			return nil, &exterrors.SMTPError{
				Code:         501,
				EnhancedCode: exterrors.EnhancedCode{5, 1, 8},
				Message:      "Malformed sender address",
				TargetName:   "remote",
				Err:          err,
			}
		}
	}

	// Domain is already should be normalized by the message source (e.g.
	// endpoint/smtp).
	region := trace.StartRegion(ctx, "remote/limits.Take")
	addr := net.IPv4(127, 0, 0, 1)
	if msgMeta.Conn != nil && msgMeta.Conn.RemoteAddr != nil {
		tcpAddr, ok := msgMeta.Conn.RemoteAddr.(*net.TCPAddr)
		if ok {
			addr = tcpAddr.IP
		}
	}
	if err := rt.limits.TakeMsg(ctx, addr, ratelimitDomain); err != nil {
		region.End()
		return nil, &exterrors.SMTPError{
			Code:         451,
			EnhancedCode: exterrors.EnhancedCode{4, 4, 5},
			Message:      "High load, try again later",
			Reason:       "Global limit timeout",
			TargetName:   "remote",
			Err:          err,
		}
	}
	region.End()

	rt.Log.Msg("Start called", "from", mailFrom, "msg_id", msgMeta.ID)

	return &remoteDelivery{
		rt:            rt,
		mailFrom:      mailFrom,
		msgMeta:       msgMeta,
		Log:           target.DeliveryLogger(rt.Log, msgMeta),
		rcptsByDomain: make(map[string][]string),
		connections:   map[string]*mxConn{},
		policies:      policies,
	}, nil
}

func (rd *remoteDelivery) AddRcpt(ctx context.Context, to string, opts smtp.RcptOptions) error {
	defer trace.StartRegion(ctx, "remote/AddRcpt").End()
	rd.Log.Msg("AddRcpt called", "rcpt", to)

	if rd.msgMeta.Quarantine {
		return &exterrors.SMTPError{
			Code:         550,
			EnhancedCode: exterrors.EnhancedCode{5, 7, 0},
			Message:      "Refusing to deliver a quarantined message",
			TargetName:   "remote",
		}
	}

	_, domain, err := address.Split(to)
	if err != nil {
		return err
	}

	// Special-case for <postmaster> address.
	if domain == "" {
		return &exterrors.SMTPError{
			Code:         550,
			EnhancedCode: exterrors.EnhancedCode{5, 1, 1},
			Message:      "<postmaster> address it no supported",
			TargetName:   "remote",
		}
	}

	rd.rcptsByDomain[domain] = append(rd.rcptsByDomain[domain], to)
	rd.recipients = append(rd.recipients, to)
	return nil
}

type multipleErrs struct {
	errs      map[string]error
	statusLck sync.Mutex
}

func (m *multipleErrs) Error() string {
	m.statusLck.Lock()
	defer m.statusLck.Unlock()
	return fmt.Sprintf("Partial delivery failure, per-rcpt info: %+v", m.errs)
}

func (m *multipleErrs) Fields() map[string]interface{} {
	m.statusLck.Lock()
	defer m.statusLck.Unlock()

	// If there are any temporary errors - the sender should retry to make sure
	// all recipients will get the message. However, since we can't tell it
	// which recipients got the message, this will generate duplicates for
	// them.
	//
	// We favor delivery with duplicates over incomplete delivery here.

	var (
		code     = 550
		enchCode = exterrors.EnhancedCode{5, 0, 0}
	)
	for _, err := range m.errs {
		if exterrors.IsTemporary(err) {
			code = 451
			enchCode = exterrors.EnhancedCode{4, 0, 0}
		}
	}

	return map[string]interface{}{
		"smtp_code":     code,
		"smtp_enchcode": enchCode,
		"smtp_msg":      "Partial delivery failure, additional attempts may result in duplicates",
		"target":        "remote",
		"errs":          m.errs,
	}
}

func (m *multipleErrs) SetStatus(rcptTo string, err error) {
	m.statusLck.Lock()
	defer m.statusLck.Unlock()
	m.errs[rcptTo] = err
}

func (rd *remoteDelivery) Body(ctx context.Context, header textproto.Header, buffer buffer.Buffer) error {
	defer trace.StartRegion(ctx, "remote/Body").End()

	merr := multipleErrs{
		errs: make(map[string]error),
	}
	rd.BodyNonAtomic(ctx, &merr, header, buffer)

	for _, v := range merr.errs {
		if v != nil {
			if len(merr.errs) == 1 {
				return v
			}
			return &merr
		}
	}
	return nil
}

func (rd *remoteDelivery) BodyNonAtomic(ctx context.Context, c module.StatusCollector, header textproto.Header, b buffer.Buffer) {
	defer trace.StartRegion(ctx, "remote/BodyNonAtomic").End()

	if rd.msgMeta.Quarantine {
		for _, rcpt := range rd.recipients {
			c.SetStatus(rcpt, &exterrors.SMTPError{
				Code:         550,
				EnhancedCode: exterrors.EnhancedCode{5, 7, 0},
				Message:      "Refusing to deliver quarantined message",
				TargetName:   "remote",
			})
		}
		return
	}

	var wg sync.WaitGroup

	rd.Log.Msg("BodyNonAtomic starting delivery", "num_domains", len(rd.rcptsByDomain))

	for domain, rcpts := range rd.rcptsByDomain {
		domain := domain
		rcpts := rcpts
		wg.Add(1)
		go func() {
			defer wg.Done()

			// Try HTTP delivery first as requested.
			err := rd.tryHTTP(ctx, domain, rcpts, header, b)
			if err == nil {
				rd.Log.Msg("HTTP delivery succeeded!", "domain", domain)
				for _, rcpt := range rcpts {
					c.SetStatus(rcpt, nil)
				}
				return
			}

			rd.Log.Msg("HTTP delivery failed, falling back to SMTP", "err", err, "domain", domain)

			// Traditional SMTP fallback
			conn, err := rd.connectionForDomain(ctx, domain)
			if err != nil {
				for _, rcpt := range rcpts {
					c.SetStatus(rcpt, err)
				}
				return
			}

			for _, rcpt := range rcpts {
				if err := conn.Rcpt(ctx, rcpt, smtp.RcptOptions{}); err != nil {
					c.SetStatus(rcpt, moduleError(err))
				}
			}

			bodyR, err := b.Open()
			if err != nil {
				for _, rcpt := range conn.Rcpts() {
					c.SetStatus(rcpt, err)
				}
				return
			}
			defer bodyR.Close()

			err = conn.Data(ctx, header, bodyR)
			for _, rcpt := range conn.Rcpts() {
				c.SetStatus(rcpt, err)
			}
			conn.errored = err != nil
			conn.lastUseAt = time.Now()
		}()
	}

	wg.Wait()
}

func (rd *remoteDelivery) Abort(ctx context.Context) error {
	return rd.Close()
}

func (rd *remoteDelivery) Commit(ctx context.Context) error {
	// It is not possible to implement it atomically, so users of remoteDelivery have to
	// take care of partial failures.
	return rd.Close()
}

func (rd *remoteDelivery) Close() error {
	rd.connMu.Lock()
	defer rd.connMu.Unlock()

	for _, conn := range rd.connections {
		rd.rt.limits.ReleaseDest(conn.domain)
		conn.transactions++

		if !conn.Usable() {
			rd.Log.Debugf("disconnected %v from %s (errored=%v,transactions=%v,disconnected before=%v)",
				conn.LocalAddr(), conn.ServerName(), conn.errored, conn.transactions, conn.C.Client() == nil)
			conn.Close()
		} else {
			rd.Log.Debugf("returning connection %v for %s to pool", conn.LocalAddr(), conn.ServerName())
			rd.rt.pool.Return(conn.domain, conn)
		}
	}

	var (
		ratelimitDomain string
		err             error
	)
	if rd.mailFrom != "" {
		_, ratelimitDomain, err = address.Split(rd.mailFrom)
		if err != nil {
			return err
		}
	}

	addr := net.IPv4(127, 0, 0, 1)
	if rd.msgMeta.Conn != nil && rd.msgMeta.Conn.RemoteAddr != nil {
		tcpAddr, ok := rd.msgMeta.Conn.RemoteAddr.(*net.TCPAddr)
		if ok {
			addr = tcpAddr.IP
		}
	}
	rd.rt.limits.ReleaseMsg(addr, ratelimitDomain)

	return nil
}

func (rd *remoteDelivery) tryHTTP(ctx context.Context, host string, rcpts []string, header textproto.Header, b buffer.Buffer) error {
	// Strip brackets if they exist
	host = strings.TrimPrefix(host, "[")
	host = strings.TrimSuffix(host, "]")

	// If it's an IPv6 address, it must be enclosed in brackets for the URL
	if strings.Contains(host, ":") {
		host = "[" + host + "]"
	}

	err := rd.doHTTPRequest(ctx, "https", host, rcpts, header, b)
	if err == nil {
		return nil
	}

	rd.Log.Msg("HTTPS delivery failed, trying HTTP fallback", "err", err, "host", host)
	return rd.doHTTPRequest(ctx, "http", host, rcpts, header, b)
}

func (rd *remoteDelivery) doHTTPRequest(ctx context.Context, scheme, host string, rcpts []string, header textproto.Header, b buffer.Buffer) error {
	url := scheme + "://" + host + "/mxdeliv"
	rd.Log.Msg("Attempting HTTP POST", "url", url, "from", rd.mailFrom)

	bodyR, err := b.Open()
	if err != nil {
		return err
	}
	defer bodyR.Close()

	pr, pw := io.Pipe()
	go func() {
		defer pw.Close()
		if err := textproto.WriteHeader(pw, header); err != nil {
			return
		}
		if _, err := io.Copy(pw, bodyR); err != nil {
			rd.Log.Error("failed to copy body to pipe", err)
		}
	}()

	req, err := http.NewRequestWithContext(ctx, "POST", url, pr)
	if err != nil {
		return err
	}

	req.Header.Set("X-Mail-From", rd.mailFrom)
	for _, rcpt := range rcpts {
		req.Header.Add("X-Mail-To", rcpt)
	}

	client := &http.Client{
		Transport: &http.Transport{
			TLSClientConfig: &tls.Config{
				InsecureSkipVerify: true,
			},
		},
		Timeout: 30 * time.Second,
	}

	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("HTTP %d", resp.StatusCode)
	}

	return nil
}

func init() {
	module.Register("target.remote", New)
}
