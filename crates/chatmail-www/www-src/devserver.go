// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//go:build ignore
// +build ignore

// devserver.go — standalone dev server for previewing chatmail HTML templates.
//
// Usage:
//
//	go run devserver.go                                    (serves on :3000, reads CONFIG or ../../data/chatmail.toml)
//	go run devserver.go -port 3000 -config ./data/chatmail.toml
//	make web-dev                                           (passes CONFIG from the Makefile)
//
// Every request re-reads the files from disk, so just edit & reload the browser.
package main

import (
	"flag"
	"fmt"
	"html/template"
	"log"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
)

// ─── Mock template data (superset of both handleStaticFiles and serveTemplate) ───

type CustomData struct {
	Name string
	URL  string
	Slug string
}

type TemplateData struct {
	MailDomain             string
	MXDomain               string
	WebDomain              string
	PublicIP               string
	Language               string
	ClientHost             string
	MessageRetentionLine   string
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
	ImapPortTLS            string
	ImapPortStartTLS       string
	SmtpPortTLS            string
	SmtpPortStartTLS       string
	DcloginImapSecurity    string
	DcloginSmtpSecurity    string
	Custom                 *CustomData
}

// ─── Template helpers (same as the real server) ───

var funcMap = template.FuncMap{
	"upper":       strings.ToUpper,
	"safeURL":     func(s string) template.URL { return template.URL(s) },
	"safeHTML":    func(s string) template.HTML { return template.HTML(s) },
	"cleanDomain": func(s string) string { return strings.Trim(s, "[]") },
	"formatBytes": func(b int64) string {
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
	},
}

// ─── Server ───

var (
	wwwDir string // resolved at startup
	data   TemplateData
)

func serveFile(w http.ResponseWriter, r *http.Request, name string) {
	raw, err := os.ReadFile(filepath.Join(wwwDir, name))
	if err != nil {
		http.NotFound(w, r)
		return
	}

	switch {
	case strings.HasSuffix(name, ".css"):
		w.Header().Set("Content-Type", "text/css")
		w.Write(raw)
	case strings.HasSuffix(name, ".js"):
		w.Header().Set("Content-Type", "application/javascript")
		w.Write(raw)
	case strings.HasSuffix(name, ".svg"):
		w.Header().Set("Content-Type", "image/svg+xml")
		w.Write(raw)
	case strings.HasSuffix(name, ".png"):
		w.Header().Set("Content-Type", "image/png")
		w.Write(raw)
	case strings.HasSuffix(name, ".html"):
		tmpl, err := template.New(name).Funcs(funcMap).Parse(string(raw))
		if err != nil {
			http.Error(w, fmt.Sprintf("template parse error:\n%v", err), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		if err := tmpl.Execute(w, data); err != nil {
			log.Printf("template exec error for %s: %v", name, err)
		}
	default:
		w.Header().Set("Content-Type", "application/octet-stream")
		w.Write(raw)
	}
}

func parseTomlConfig(path string) (map[string]string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	cfg := make(map[string]string)
	for _, line := range strings.Split(string(raw), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.SplitN(line, "=", 2)
		if len(parts) != 2 {
			continue
		}
		key := strings.TrimSpace(parts[0])
		val := strings.TrimSpace(parts[1])
		if strings.HasPrefix(val, `"`) && strings.HasSuffix(val, `"`) && len(val) >= 2 {
			val = val[1 : len(val)-1]
		}
		cfg[key] = val
	}
	return cfg, nil
}

func portFromListen(listen, fallback string) string {
	if listen == "" {
		return fallback
	}
	if host, port, err := net.SplitHostPort(listen); err == nil && port != "" {
		_ = host
		return port
	}
	if idx := strings.LastIndex(listen, ":"); idx >= 0 {
		return listen[idx+1:]
	}
	return listen
}

func isIPLiteral(s string) bool {
	return net.ParseIP(strings.Trim(s, "[]")) != nil
}

func wrapIPDomain(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return s
	}
	if strings.HasPrefix(s, "[") && strings.HasSuffix(s, "]") {
		return s
	}
	if isIPLiteral(s) {
		return "[" + s + "]"
	}
	return s
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if strings.TrimSpace(v) != "" {
			return v
		}
	}
	return ""
}

func cleanHost(s string) string {
	return strings.Trim(strings.TrimSpace(s), "[]")
}

func clientConnectHost(primary, hostname, mxDomain, publicIP, mailDomain string) string {
	if primary = strings.TrimSpace(primary); primary != "" {
		return cleanHost(primary)
	}
	if publicIP = strings.TrimSpace(publicIP); publicIP != "" {
		return cleanHost(publicIP)
	}
	for _, candidate := range []string{mxDomain, hostname, mailDomain} {
		if h := cleanHost(candidate); h != "" {
			return h
		}
	}
	return "127.0.0.1"
}

func buildTemplateData(cfg map[string]string) TemplateData {
	hostname := cfg["hostname"]
	primary := cfg["primary_domain"]
	mailDomain := firstNonEmpty(primary, hostname, "127.0.0.1")
	mailDomain = wrapIPDomain(mailDomain)

	webDomain := firstNonEmpty(hostname, primary, mailDomain)
	mxDomain := firstNonEmpty(cfg["mx_domain"], hostname, webDomain)

	publicIP := cfg["public_ip"]
	if publicIP == "" {
		publicIP = strings.Trim(mailDomain, "[]")
	}

	imapTLS := portFromListen(cfg["imap_tls_listen"], "993")
	imapPlain := portFromListen(cfg["imap_listen"], "143")
	smtpTLS := portFromListen(cfg["submission_tls_listen"], "465")
	smtpPlain := portFromListen(cfg["submission_listen"], "587")

	dcloginImap := "plain"
	dcloginSmtp := "plain"
	if imapTLS == "993" && imapPlain == "143" {
		dcloginImap = "starttls"
	}
	if smtpTLS == "465" && smtpPlain == "587" {
		dcloginSmtp = "starttls"
	}

	turnEnabled := cfg["turn_enable"] == "true"
	jitEnabled := cfg["jit_domain"] != "" || cfg["auth_auto_create"] == "true"

	language := firstNonEmpty(cfg["language"], "en")
	clientHost := clientConnectHost(primary, hostname, mxDomain, publicIP, mailDomain)

	hostHint := strings.Trim(webDomain, "[]")
	return TemplateData{
		MailDomain:             mailDomain,
		MXDomain:               mxDomain,
		WebDomain:              webDomain,
		PublicIP:               publicIP,
		Language:               language,
		ClientHost:             clientHost,
		MessageRetentionLine:   "",
		TurnOffTLS:             false,
		Version:                "dev-preview",
		SSURL:                  fmt.Sprintf("ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@%s:8388#%s", publicIP, hostHint),
		SSGrpcURL:              fmt.Sprintf("ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@%s:8389#%s-grpc", publicIP, hostHint),
		SSWsURL:                fmt.Sprintf("ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@%s:8390#%s-ws", publicIP, hostHint),
		V2rayNGConfigWS:        fmt.Sprintf(`{"remarks":"mock-ws","server":"%s","server_port":8390}`, publicIP),
		V2rayNGConfigGRPC:      fmt.Sprintf(`{"remarks":"mock-grpc","server":"%s","server_port":8389}`, publicIP),
		STUNAddr:               fmt.Sprintf("%s:3478", hostHint),
		DefaultQuota:           100 * 1024 * 1024,
		MaxMessageSize:         "40M",
		RegistrationOpen:       true,
		JitRegistrationEnabled: jitEnabled,
		TurnEnabled:            turnEnabled,
		ImapPortTLS:            imapTLS,
		ImapPortStartTLS:       imapPlain,
		SmtpPortTLS:            smtpTLS,
		SmtpPortStartTLS:       smtpPlain,
		DcloginImapSecurity:    dcloginImap,
		DcloginSmtpSecurity:    dcloginSmtp,
		Custom: &CustomData{
			Name: "علی",
			URL:  "https://i.delta.chat/#C0FFEEBABE",
			Slug: "ali",
		},
	}
}

func resolveConfigPath(flagValue string) string {
	if flagValue != "" {
		return flagValue
	}
	if v := os.Getenv("CONFIG"); v != "" {
		return v
	}
	return "../../data/chatmail.toml"
}

func main() {
	port := flag.Int("port", 3000, "listen port")
	configPath := flag.String("config", "", "chatmail.toml (default: CONFIG env or ../../data/chatmail.toml)")
	flag.Parse()

	cfgFile := resolveConfigPath(*configPath)
	cfg, err := parseTomlConfig(cfgFile)
	if err != nil {
		log.Fatalf("read config %s: %v", cfgFile, err)
	}

	// Resolve the www directory (same directory as this script)
	exe, _ := os.Getwd()
	wwwDir = exe
	if _, err := os.Stat(filepath.Join(wwwDir, "index.html")); err != nil {
		wwwDir = filepath.Join(exe, "www")
	}

	data = buildTemplateData(cfg)

	mux := http.NewServeMux()

	// ── Docs routing (mirrors real server) ──
	mux.HandleFunc("/docs", func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "/docs/", http.StatusMovedPermanently)
	})
	mux.HandleFunc("/docs/", func(w http.ResponseWriter, r *http.Request) {
		sub := strings.TrimPrefix(r.URL.Path, "/docs")
		sub = strings.TrimPrefix(sub, "/")
		switch sub {
		case "", "index", "index.html":
			serveFile(w, r, "docs_index.html")
		case "admin":
			serveFile(w, r, "admin_docs.html")
		case "api":
			serveFile(w, r, "admin_api_docs.html")
		case "general":
			serveFile(w, r, "general_docs.html")
		case "serve", "custom-html":
			serveFile(w, r, "docs_serve.html")
		case "database":
			serveFile(w, r, "database_docs.html")
		case "docker":
			serveFile(w, r, "docker_docs.html")
		case "relay", "domain", "tls":
			serveFile(w, r, "relay_docs.html")
		default:
			http.NotFound(w, r)
		}
	})

	// ── Contact share pages ──
	mux.HandleFunc("/share", func(w http.ResponseWriter, r *http.Request) {
		serveFile(w, r, "contact_share.html")
	})
	mux.HandleFunc("/share/success", func(w http.ResponseWriter, r *http.Request) {
		serveFile(w, r, "contact_share_success.html")
	})

	// ── Mock contact view (the slug route) ──
	mux.HandleFunc("/ali", func(w http.ResponseWriter, r *http.Request) {
		serveFile(w, r, "contact_view.html")
	})

	// ── Mock /new API (returns fake JSON) ──
	mux.HandleFunc("/new", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		domain := strings.Trim(data.MailDomain, "[]")
		w.Write([]byte(`{"email":"testuser@` + domain + `","password":"mock-password-12345"}`))
	})

	// ── Catch-all: static files and index ──
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		path := strings.TrimPrefix(r.URL.Path, "/")
		if path == "" {
			path = "index.html"
		}
		serveFile(w, r, path)
	})

	addr := fmt.Sprintf(":%d", *port)
	fmt.Printf("\n")
	fmt.Printf("  ┌──────────────────────────────────────────────┐\n")
	fmt.Printf("  │   Chatmail Dev Server                        │\n")
	fmt.Printf("  │   http://localhost%-5s                       │\n", fmt.Sprintf(":%d", *port))
	fmt.Printf("  │   config: %-34s │\n", truncate(cfgFile, 34))
	fmt.Printf("  │   domain:  %-34s │\n", truncate(data.MailDomain, 34))
	fmt.Printf("  │                                              │\n")
	fmt.Printf("  │   Pages:                                     │\n")
	fmt.Printf("  │     /              index                     │\n")
	fmt.Printf("  │     /info.html     info                      │\n")
	fmt.Printf("  │     /security.html security                  │\n")
	fmt.Printf("  │     /deploy.html   deploy                    │\n")
	fmt.Printf("  │     /share         contact share             │\n")
	fmt.Printf("  │     /share/success share success             │\n")
	fmt.Printf("  │     /ali           contact view              │\n")
	fmt.Printf("  │     /docs/         docs index                │\n")
	fmt.Printf("  │     /docs/admin    admin docs                │\n")
	fmt.Printf("  │     /docs/api      admin API docs            │\n")
	fmt.Printf("  │     /docs/general  general docs              │\n")
	fmt.Printf("  │     /docs/serve    serve docs                │\n")
	fmt.Printf("  │     /docs/database database docs             │\n")
	fmt.Printf("  │     /docs/docker   docker docs               │\n")
	fmt.Printf("  │     /docs/relay    relay docs                │\n")
	fmt.Printf("  │                                              │\n")
	fmt.Printf("  │   Edit HTML/CSS/JS and refresh the browser!  │\n")
	fmt.Printf("  └──────────────────────────────────────────────┘\n")
	fmt.Printf("\n")

	log.Fatal(http.ListenAndServe(addr, mux))
}

func truncate(s string, max int) string {
	if len(s) <= max {
		return s
	}
	if max <= 3 {
		return s[:max]
	}
	return s[:max-3] + "..."
}
