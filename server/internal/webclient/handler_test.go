package webclient

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestOnlyEmbeddedAssetsAndNoCredentialsInURLs(t *testing.T) {
	h := Handler()
	for _, path := range []string{"/connect", "/connect/", "/connect/app.js", "/connect/core.mjs", "/connect/style.css"} {
		r := httptest.NewRecorder()
		h.ServeHTTP(r, httptest.NewRequest("GET", path, nil))
		if r.Code != http.StatusOK || r.Body.Len() == 0 || r.Header().Get("Cache-Control") != "no-store" {
			t.Fatalf("asset %s: status=%d bytes=%d headers=%v", path, r.Code, r.Body.Len(), r.Header())
		}
		csp := r.Header().Get("Content-Security-Policy")
		if !strings.Contains(csp, "default-src 'none'") || !strings.Contains(csp, "frame-ancestors 'none'") || strings.Contains(csp, "unsafe-inline") {
			t.Fatalf("unsafe content policy: %s", csp)
		}
	}
	for _, path := range []string{"/connect?pin=000000", "/connect/app.js?token=secret", "/connect/missing.js", "/connect/../app.go", "/connect/assets/index.html"} {
		r := httptest.NewRecorder()
		h.ServeHTTP(r, httptest.NewRequest("GET", path, nil))
		if r.Code != http.StatusNotFound && r.Code != http.StatusBadRequest {
			t.Fatalf("unowned path %s exposed: %d", path, r.Code)
		}
	}
}

func TestControllerMethodsAndHead(t *testing.T) {
	h := Handler()
	r := httptest.NewRecorder()
	h.ServeHTTP(r, httptest.NewRequest("HEAD", "/connect", nil))
	if r.Code != http.StatusOK || r.Body.Len() != 0 {
		t.Fatal("HEAD must not return the page body")
	}
	r = httptest.NewRecorder()
	h.ServeHTTP(r, httptest.NewRequest("POST", "/connect", nil))
	if r.Code != http.StatusMethodNotAllowed || r.Header().Get("Allow") != "GET, HEAD" {
		t.Fatal("unsupported methods must fail")
	}
}
