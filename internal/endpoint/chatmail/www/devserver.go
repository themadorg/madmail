//go:build ignore
// +build ignore

// devserver.go — standalone dev server for previewing chatmail HTML templates.
//
// Usage:
//
//	go run devserver.go                 (serves on :8080)
//	go run devserver.go -port 3000      (serves on :3000)
//	go run devserver.go -domain chat.example.org
//
// Every request re-reads the files from disk, so just edit & reload the browser.
package main

import (
	"flag"
	"fmt"
	"html/template"
	"log"
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
			http.Error(w, fmt.Sprintf("template parse error:\n%v", err), 500)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		if err := tmpl.Execute(w, data); err != nil {
			http.Error(w, fmt.Sprintf("template exec error:\n%v", err), 500)
		}
	default:
		w.Header().Set("Content-Type", "application/octet-stream")
		w.Write(raw)
	}
}

func main() {
	port := flag.Int("port", 8080, "listen port")
	domain := flag.String("domain", "chat.example.org", "mock domain for templates")
	ip := flag.String("ip", "203.0.113.42", "mock public IP")
	flag.Parse()

	// Resolve the www directory (same directory as this script)
	exe, _ := os.Getwd()
	wwwDir = exe
	// Also check if we're inside the www/ dir already
	if _, err := os.Stat(filepath.Join(wwwDir, "index.html")); err != nil {
		// Maybe we're one level up
		wwwDir = filepath.Join(exe, "www")
	}

	data = TemplateData{
		MailDomain:             *domain,
		MXDomain:               *domain,
		WebDomain:              *domain,
		PublicIP:               *ip,
		TurnOffTLS:             false,
		Version:                "dev-preview",
		SSURL:                  fmt.Sprintf("ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@%s:8388#%s", *ip, *domain),
		SSGrpcURL:              fmt.Sprintf("ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@%s:8389#%s-grpc", *ip, *domain),
		SSWsURL:                fmt.Sprintf("ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@%s:8390#%s-ws", *ip, *domain),
		V2rayNGConfigWS:        `{"remarks":"mock-ws","server":"203.0.113.42","server_port":8390}`,
		V2rayNGConfigGRPC:      `{"remarks":"mock-grpc","server":"203.0.113.42","server_port":8389}`,
		STUNAddr:               fmt.Sprintf("%s:3478", *domain),
		DefaultQuota:           100 * 1024 * 1024, // 100 MB
		MaxMessageSize:         "40M",
		RegistrationOpen:       true,
		JitRegistrationEnabled: true,
		TurnEnabled:            true,
		ImapPortTLS:            "993",
		ImapPortStartTLS:       "143",
		SmtpPortTLS:            "465",
		SmtpPortStartTLS:       "587",
		DcloginImapSecurity:    "ssl",
		DcloginSmtpSecurity:    "ssl",
		Custom: &CustomData{
			Name: "علی",
			URL:  "https://i.delta.chat/#C0FFEEBABE",
			Slug: "ali",
		},
	}

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
		w.Write([]byte(`{"email":"testuser@` + *domain + `","password":"mock-password-12345"}`))
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
