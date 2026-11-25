// This executable is only a loopback integration fixture. It has no desktop
// access, production state, public listener or credential supplied by the user.
package main

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"net"
	"net/http"
	"openremote/controlplane/internal/app"
	"os"
	"os/signal"
	"syscall"
	"time"
)

func main() {
	secret := make([]byte, 48)
	if _, err := rand.Read(secret); err != nil {
		panic("fixture entropy unavailable")
	}
	cfg := app.DefaultConfig()
	cfg.EnrollmentToken = base64.RawURLEncoding.EncodeToString(secret)
	cfg.AllowInsecure = true // loopback test only; production defaults remain HTTPS.
	cfg.PollTimeout = 2 * time.Second
	backend, err := app.NewWithConfig(nil, cfg)
	if err != nil {
		panic(err)
	}
	defer backend.Close()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	srv := &http.Server{Handler: backend.Handler(), ReadHeaderTimeout: 5 * time.Second}
	// The parent consumes this pipe in memory. Its evidence never includes tokens.
	if err := json.NewEncoder(os.Stdout).Encode(map[string]string{"origin": "http://" + listener.Addr().String(), "enrollmentToken": cfg.EnrollmentToken}); err != nil {
		panic(err)
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	go func() { <-ctx.Done(); _ = srv.Close() }()
	if err = srv.Serve(listener); err != nil && err != http.ErrServerClosed {
		panic(err)
	}
}
