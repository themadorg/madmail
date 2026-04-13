package chatmail

import (
	"io/fs"
	"mime"
	"net/http"
	"path/filepath"
	"strings"

	adminweb "github.com/themadorg/madmail/internal/adminweb"
	"github.com/themadorg/madmail/internal/api/admin/resources"
)

// serveAdminWeb creates an HTTP handler that serves the embedded admin-web SPA
// under the given prefix path. It handles:
//   - Static assets (JS, CSS, images, fonts) with correct MIME types
//   - SPA fallback: any path that doesn't match a real file returns index.html
//   - Dynamic path rewriting in index.html so the SPA works under any prefix
//   - Runtime enable/disable check via the __ADMIN_WEB_ENABLED__ DB setting
func (e *Endpoint) serveAdminWeb(prefix string) http.HandlerFunc {
	// Build a sub-filesystem from the embedded admin-web build directory
	adminFS, err := fs.Sub(adminweb.Files, "build")
	if err != nil {
		e.logger.Error("failed to open embedded admin-web filesystem", err)
		return func(w http.ResponseWriter, r *http.Request) {
			http.Error(w, "Admin web UI not available", http.StatusInternalServerError)
		}
	}

	// Check if the admin-web build is actually available (not just the placeholder)
	if _, err := fs.ReadFile(adminFS, "index.html"); err != nil {
		e.logger.Printf("admin web UI not available (admin-web not built)")
		return func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.WriteHeader(http.StatusServiceUnavailable)
			_, _ = w.Write([]byte(`<!doctype html><html><body><h1>Admin Web UI Not Available</h1><p>The admin web dashboard was not included in this build. Build admin-web first, then rebuild the server.</p></body></html>`))
		}
	}

	// Pre-read and patch index.html once at startup.
	// We rewrite all absolute paths (/, /_app/, etc.) to be relative to the prefix.
	indexBytes, _ := fs.ReadFile(adminFS, "index.html")

	// Ensure prefix has trailing slash for path rewriting
	prefixWithSlash := prefix
	if !strings.HasSuffix(prefixWithSlash, "/") {
		prefixWithSlash += "/"
	}

	// Rewrite paths in index.html:
	// 1. /_app/ → /admin/_app/  (or whatever the prefix is)
	// 2. /manifest.json → /admin/manifest.json
	// 3. /icon-*.png → /admin/icon-*.png
	// 4. /sw.js → /admin/sw.js
	// 5. SvelteKit base path: base: "" → base: "/admin"
	patchedIndex := string(indexBytes)
	patchedIndex = strings.ReplaceAll(patchedIndex, `href="/`, `href="`+prefixWithSlash)
	patchedIndex = strings.ReplaceAll(patchedIndex, `src="/`, `src="`+prefixWithSlash)
	patchedIndex = strings.ReplaceAll(patchedIndex, `import("/`, `import("`+prefixWithSlash)
	// Set SvelteKit base path so client-side routing works
	cleanPrefix := strings.TrimSuffix(prefix, "/")
	patchedIndex = strings.ReplaceAll(patchedIndex, `base: ""`, `base: "`+cleanPrefix+`"`)
	patchedIndexBytes := []byte(patchedIndex)

	return func(w http.ResponseWriter, r *http.Request) {
		// CORS headers — needed when the SPA makes API calls cross-origin
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type")

		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}

		// Check if admin web is enabled (checked per-request so toggling is instant)
		if e.authDB != nil {
			if val, ok, err := e.authDB.GetSetting(resources.KeyAdminWebEnabled); err == nil && ok && val == "false" {
				http.NotFound(w, r)
				return
			}
		}

		if r.Method != http.MethodGet && r.Method != http.MethodHead {
			http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
			return
		}

		// Strip the prefix to get the file path within the SPA
		path := strings.TrimPrefix(r.URL.Path, prefix)
		path = strings.TrimPrefix(path, "/")

		// Empty path → serve patched index.html
		if path == "" {
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.Header().Set("Cache-Control", "no-cache")
			_, _ = w.Write(patchedIndexBytes)
			return
		}

		// Try to serve the file from the embedded FS
		f, err := adminFS.Open(path)
		if err != nil {
			// File not found → SPA fallback: serve patched index.html
			// This handles client-side routes like /admin/accounts, /admin/services, etc.
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.Header().Set("Cache-Control", "no-cache")
			_, _ = w.Write(patchedIndexBytes)
			return
		}
		f.Close()

		// Check if it's a directory (shouldn't serve directory listings)
		stat, err := fs.Stat(adminFS, path)
		if err != nil || stat.IsDir() {
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.Header().Set("Cache-Control", "no-cache")
			_, _ = w.Write(patchedIndexBytes)
			return
		}

		// Read the file content
		data, err := fs.ReadFile(adminFS, path)
		if err != nil {
			http.Error(w, "Internal server error", http.StatusInternalServerError)
			return
		}

		// Determine content type
		contentType := mime.TypeByExtension(filepath.Ext(path))
		if contentType == "" {
			contentType = "application/octet-stream"
		}
		w.Header().Set("Content-Type", contentType)

		// Immutable assets get aggressive caching (fingerprinted filenames)
		if strings.Contains(path, "/immutable/") {
			w.Header().Set("Cache-Control", "public, max-age=31536000, immutable")
		} else if path == "version.json" || path == "sw.js" {
			// version.json and sw.js must never be cached so updates are detected
			w.Header().Set("Cache-Control", "no-cache, no-store, must-revalidate")
		} else {
			w.Header().Set("Cache-Control", "public, max-age=3600")
		}

		_, _ = w.Write(data)
	}
}
