/*
Maddy Mail Server - Composable all-in-one email server.
Copyright © 2024 Madmail contributors

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

package turn

import (
	"crypto/hmac"
	"crypto/sha1"
	"crypto/tls"
	"encoding/base64"
	"fmt"
	"net"
	"strconv"
	"sync"

	"github.com/pion/logging"
	"github.com/pion/stun/v3"
	"github.com/pion/turn/v4"
	"github.com/themadorg/madmail/framework/config"
	tls2 "github.com/themadorg/madmail/framework/config/tls"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
)

type Endpoint struct {
	name      string
	addrs     []string
	listeners []net.Listener
	conns     []net.PacketConn
	server    *turn.Server

	realm   string
	secret  string
	relayIP net.IP

	tlsConfig *tls.Config

	listenersWg sync.WaitGroup
	Log         log.Logger
}

func (endp *Endpoint) Name() string {
	return endp.name
}

func (endp *Endpoint) InstanceName() string {
	return endp.name
}

func New(modName string, addrs []string) (module.Module, error) {
	return &Endpoint{
		name:  modName,
		addrs: addrs,
		Log:   log.Logger{Name: modName},
	}, nil
}

type pionLogger struct {
	log log.Logger
}

func (p *pionLogger) NewLogger(scope string) logging.LeveledLogger {
	return &pionLeveledLogger{log: p.log, scope: scope}
}

type pionLeveledLogger struct {
	log   log.Logger
	scope string
}

func (p *pionLeveledLogger) Trace(msg string) { p.log.Debugf("[%s] %s", p.scope, msg) }
func (p *pionLeveledLogger) Tracef(format string, args ...interface{}) {
	p.log.Debugf("[%s] "+format, append([]interface{}{p.scope}, args...)...)
}
func (p *pionLeveledLogger) Debug(msg string) { p.log.Debugf("[%s] %s", p.scope, msg) }
func (p *pionLeveledLogger) Debugf(format string, args ...interface{}) {
	p.log.Debugf("[%s] "+format, append([]interface{}{p.scope}, args...)...)
}
func (p *pionLeveledLogger) Info(msg string) { p.log.Printf("[%s] %s", p.scope, msg) }
func (p *pionLeveledLogger) Infof(format string, args ...interface{}) {
	p.log.Printf("[%s] "+format, append([]interface{}{p.scope}, args...)...)
}
func (p *pionLeveledLogger) Warn(msg string) { p.log.Printf("[%s] WARN: %s", p.scope, msg) }
func (p *pionLeveledLogger) Warnf(format string, args ...interface{}) {
	p.log.Printf("[%s] WARN: "+format, append([]interface{}{p.scope}, args...)...)
}
func (p *pionLeveledLogger) Error(msg string) { p.log.Printf("[%s] ERROR: %s", p.scope, msg) }
func (p *pionLeveledLogger) Errorf(format string, args ...interface{}) {
	p.log.Printf("[%s] ERROR: "+format, append([]interface{}{p.scope}, args...)...)
}

type MinimalRelayGenerator struct {
	RelayAddress net.IP
	Log          log.Logger
}

func (m *MinimalRelayGenerator) Validate() error {
	return nil
}

func (m *MinimalRelayGenerator) AllocatePacketConn(network string, requestedPort int) (net.PacketConn, net.Addr, error) {
	m.Log.Debugf("TURN: Allocating PacketConn (network=%s, port=%d)", network, requestedPort)
	conn, err := net.ListenPacket(network, net.JoinHostPort("0.0.0.0", strconv.Itoa(requestedPort)))
	if err != nil {
		m.Log.Debugf("TURN: Allocation FAILED: %v", err)
		return nil, nil, err
	}

	actualAddr := conn.LocalAddr().(*net.UDPAddr)
	relayAddr := &net.UDPAddr{
		IP:   m.RelayAddress,
		Port: actualAddr.Port,
	}
	m.Log.Debugf("TURN: Allocation SUCCESS: relay addr is %v", relayAddr)
	return conn, relayAddr, nil
}

func (m *MinimalRelayGenerator) AllocateConn(network string, requestedPort int) (net.Conn, net.Addr, error) {
	return nil, nil, fmt.Errorf("TCP relay not supported by minimal generator")
}

type stunLoggingPacketConn struct {
	net.PacketConn
	log log.Logger
}

func (s *stunLoggingPacketConn) ReadFrom(p []byte) (n int, addr net.Addr, err error) {
	n, addr, err = s.PacketConn.ReadFrom(p)
	if err == nil && stun.IsMessage(p[:n]) {
		m := &stun.Message{}
		if unmarshalErr := m.UnmarshalBinary(p[:n]); unmarshalErr == nil {
			if m.Type.Class == stun.ClassRequest && m.Type.Method == stun.MethodBinding {
				s.log.Debugf("STUN: Binding Request from %v", addr)
			}
		}
	}
	return n, addr, err
}

func (endp *Endpoint) Init(cfg *config.Map) error {
	var relayIP string
	cfg.String("realm", true, true, "", &endp.realm)
	cfg.String("secret", true, true, "", &endp.secret)
	cfg.String("relay_ip", false, false, "", &relayIP)
	cfg.Custom("tls", false, false, nil, tls2.TLSDirective, &endp.tlsConfig)
	cfg.Bool("debug", true, false, &endp.Log.Debug)
	endp.Log.Debugf("TURN: Init called")
	if _, err := cfg.Process(); err != nil {
		return err
	}

	if relayIP != "" {
		endp.relayIP = net.ParseIP(relayIP)
		if endp.relayIP == nil {
			return fmt.Errorf("turn: invalid relay_ip: %s", relayIP)
		}
	}

	if endp.realm == "" {
		return fmt.Errorf("turn: realm is required")
	}
	if endp.secret == "" {
		return fmt.Errorf("turn: secret is required")
	}

	pl := &pionLogger{log: endp.Log}
	authHandler := turn.AuthHandler(func(username, realm string, srcAddr net.Addr) ([]byte, bool) {
		endp.Log.Debugf("TURN: Auth attempt from %v (username=%s, realm=%s)", srcAddr, username, realm)
		if realm != endp.realm {
			endp.Log.Debugf("TURN: Auth FAILED - realm mismatch (expected %s)", endp.realm)
			return nil, false
		}
		mac := hmac.New(sha1.New, []byte(endp.secret))
		mac.Write([]byte(username))
		passwordStr := base64.StdEncoding.EncodeToString(mac.Sum(nil))

		key := turn.GenerateAuthKey(username, realm, passwordStr)
		endp.Log.Debugf("TURN: Auth SUCCESS for %s", username)
		return key, true
	})

	packetConnConfigs := []turn.PacketConnConfig{}
	listenerConfigs := []turn.ListenerConfig{}

	addresses := make([]config.Endpoint, 0, len(endp.addrs))
	for _, addr := range endp.addrs {
		saddr, err := config.ParseEndpoint(addr)
		if err != nil {
			return fmt.Errorf("%s: invalid address", addr)
		}
		addresses = append(addresses, saddr)
	}

	for _, addr := range addresses {
		relayIP := endp.relayIP
		if relayIP == nil || relayIP.IsUnspecified() {
			relayIP = net.ParseIP(addr.Host)
		}
		if relayIP == nil || relayIP.IsUnspecified() {
			endp.Log.Printf("WARN: TURN relay IP is unspecified (0.0.0.0), relaying might not work correctly")
			relayIP = net.IPv4zero
		}

		gen := &MinimalRelayGenerator{
			RelayAddress: relayIP,
			Log:          endp.Log,
		}

		if addr.Network() == "tcp" {
			l, err := net.Listen("tcp", addr.Address())
			if err != nil {
				return err
			}

			if addr.IsTLS() {
				if endp.tlsConfig == nil {
					return fmt.Errorf("turn: TLS configured for %s but no tls directive found", addr)
				}
				l = tls.NewListener(l, endp.tlsConfig)
				endp.Log.Printf("listening on %v (TURNS/TLS)", addr)
			} else {
				endp.Log.Printf("listening on %v (TCP)", addr)
			}

			endp.listeners = append(endp.listeners, l)
			listenerConfigs = append(listenerConfigs, turn.ListenerConfig{
				Listener:              l,
				RelayAddressGenerator: gen,
			})
		} else if addr.Network() == "udp" {
			packetConn, err := net.ListenPacket("udp", addr.Address())
			if err != nil {
				return err
			}
			endp.conns = append(endp.conns, packetConn)

			wrappedConn := &stunLoggingPacketConn{
				PacketConn: packetConn,
				log:        endp.Log,
			}

			packetConnConfigs = append(packetConnConfigs, turn.PacketConnConfig{
				PacketConn:            wrappedConn,
				RelayAddressGenerator: gen,
			})
			endp.Log.Printf("listening on %v (UDP)", addr)
		}
	}

	s, err := turn.NewServer(turn.ServerConfig{
		Realm:             endp.realm,
		AuthHandler:       authHandler,
		PacketConnConfigs: packetConnConfigs,
		ListenerConfigs:   listenerConfigs,
		LoggerFactory:     pl,
	})
	if err != nil {
		return err
	}
	endp.server = s

	return nil
}

func (endp *Endpoint) Close() error {
	if endp.server != nil {
		return endp.server.Close()
	}
	return nil
}

func init() {
	module.RegisterEndpoint("turn", New)
}
