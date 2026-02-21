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

package tls

import (
	"crypto/tls"
	"fmt"
	"net/http"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"golang.org/x/crypto/acme/autocert"
)

// AutocertLoader implements module.TLSLoader using golang.org/x/crypto/acme/autocert.
// It obtains and automatically renews Let's Encrypt certificates using the HTTP-01 challenge.
// This requires port 80 to be available for the ACME challenge handler.
type AutocertLoader struct {
	instName string
	manager  *autocert.Manager
	log      log.Logger
}

func NewAutocertLoader(_, instName string, _, inlineArgs []string) (module.Module, error) {
	return &AutocertLoader{
		instName: instName,
		log:      log.Logger{Name: "tls.loader.autocert"},
	}, nil
}

func (l *AutocertLoader) Init(cfg *config.Map) error {
	var (
		hostname   string
		extraNames []string
		cacheDir   string
		email      string
		agreed     bool
		listenAddr string
	)

	cfg.Bool("debug", true, false, &l.log.Debug)
	cfg.String("hostname", true, true, "", &hostname)
	cfg.StringList("extra_names", false, false, nil, &extraNames)
	cfg.String("cache_dir", false, false, "/var/lib/maddy/autocert", &cacheDir)
	cfg.String("email", false, false, "", &email)
	cfg.Bool("agreed", false, false, &agreed)
	cfg.String("listen", false, false, ":80", &listenAddr)

	if _, err := cfg.Process(); err != nil {
		return err
	}

	if !agreed {
		return fmt.Errorf("tls.loader.autocert: you must set 'agreed' to accept Let's Encrypt ToS")
	}

	// Build the host whitelist
	hosts := append([]string{hostname}, extraNames...)

	l.manager = &autocert.Manager{
		Prompt:     autocert.AcceptTOS,
		HostPolicy: autocert.HostWhitelist(hosts...),
		Cache:      autocert.DirCache(cacheDir),
		Email:      email,
	}

	if module.NoRun {
		return nil
	}

	// Start the HTTP-01 challenge server on port 80
	go func() {
		l.log.Printf("starting ACME HTTP-01 challenge server on %s", listenAddr)
		srv := &http.Server{
			Addr:    listenAddr,
			Handler: l.manager.HTTPHandler(nil),
		}
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			l.log.Error("ACME HTTP-01 challenge server failed", err)
		}
	}()

	l.log.Printf("autocert configured for %v (cache: %s)", hosts, cacheDir)

	return nil
}

func (l *AutocertLoader) ConfigureTLS(c *tls.Config) error {
	c.GetCertificate = l.manager.GetCertificate
	return nil
}

func (l *AutocertLoader) Name() string {
	return "tls.loader.autocert"
}

func (l *AutocertLoader) InstanceName() string {
	return l.instName
}

func init() {
	var _ module.TLSLoader = &AutocertLoader{}
	module.Register("tls.loader.autocert", NewAutocertLoader)
}
