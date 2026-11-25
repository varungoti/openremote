// Package webclient serves the first-party browser controller from the coordinator.
// Assets are compiled into the binary; no filesystem path or external script is served.
package webclient

import (
	"embed"
	"net/http"
)

//go:embed assets/index.html assets/app.js assets/core.mjs assets/style.css
var assets embed.FS

// Handler serves only the explicit controller routes. The coordinator's outer
// middleware additionally enforces HTTPS, storage health and request budgets.
func Handler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Cache-Control", "no-store")
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("X-Frame-Options", "DENY")
		w.Header().Set("Referrer-Policy", "no-referrer")
		w.Header().Set("Content-Security-Policy", "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; media-src 'self' blob:; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; object-src 'none'")
		w.Header().Set("Permissions-Policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=(), serial=(), clipboard-read=(), clipboard-write=()")
		w.Header().Set("Cross-Origin-Resource-Policy", "same-origin")
		if r.Method != http.MethodGet && r.Method != http.MethodHead {
			w.Header().Set("Allow", "GET, HEAD")
			http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
			return
		}
		name, contentType := "", ""
		switch r.URL.Path {
		case "/connect", "/connect/":
			name, contentType = "index.html", "text/html; charset=utf-8"
		case "/connect/app.js":
			name, contentType = "app.js", "text/javascript; charset=utf-8"
		case "/connect/core.mjs":
			name, contentType = "core.mjs", "text/javascript; charset=utf-8"
		case "/connect/style.css":
			name, contentType = "style.css", "text/css; charset=utf-8"
		default:
			http.NotFound(w, r)
			return
		}
		// Credentials in URLs are never a supported import mechanism.
		if r.URL.RawQuery != "" {
			http.Error(w, "This page does not accept URL parameters", http.StatusBadRequest)
			return
		}
		body, err := assets.ReadFile("assets/" + name)
		if err != nil {
			http.Error(w, "Controller unavailable", http.StatusServiceUnavailable)
			return
		}
		w.Header().Set("Content-Type", contentType)
		if r.Method == http.MethodGet {
			_, _ = w.Write(body)
		}
	})
}
