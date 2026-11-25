package main

import (
	"context"
	"errors"
	"log"
	"net/http"
	"openremote/controlplane/internal/app"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"
)

func listEnv(name string) []string {
	var out []string
	for _, v := range strings.Split(os.Getenv(name), ",") {
		if v = strings.TrimSpace(v); v != "" {
			out = append(out, v)
		}
	}
	return out
}
func main() {
	logger := log.New(os.Stdout, "controlplane ", log.LstdFlags|log.LUTC)
	cfg := app.DefaultConfig()
	cfg.EnrollmentToken = os.Getenv("OPENREMOTE_ENROLLMENT_TOKEN")
	cfg.GatewayToken = os.Getenv("OPENREMOTE_GATEWAY_TOKEN")
	cfg.StatePath = os.Getenv("OPENREMOTE_STATE_FILE")
	if cfg.StatePath == "" {
		cfg.StatePath = "./data/controlplane.json"
	}
	cfg.AllowInsecure = os.Getenv("OPENREMOTE_ALLOW_INSECURE") == "true"
	cfg.AutomaticHostRegistration = os.Getenv("OPENREMOTE_AUTOMATIC_HOST_REGISTRATION") == "true"
	cfg.PublicOrigin = os.Getenv("OPENREMOTE_PUBLIC_ORIGIN")
	cfg.TrustedProxyCIDRs = listEnv("OPENREMOTE_TRUSTED_PROXY_CIDRS")
	cfg.STUNURLs = listEnv("OPENREMOTE_STUN_URLS")
	cfg.TURNURLs = listEnv("OPENREMOTE_TURN_URLS")
	cfg.TURNSecret = os.Getenv("OPENREMOTE_TURN_SECRET")
	cfg.ICERelayOnly = os.Getenv("OPENREMOTE_RELAY_ONLY") == "true"
	if len(cfg.EnrollmentToken) < 32 {
		logger.Fatal("OPENREMOTE_ENROLLMENT_TOKEN is required (at least 32 random characters)")
	}
	cert, key := os.Getenv("OPENREMOTE_TLS_CERT"), os.Getenv("OPENREMOTE_TLS_KEY")
	if (cert == "") != (key == "") {
		logger.Fatal("TLS certificate and key must both be provided")
	}
	if cert == "" && !cfg.AllowInsecure && len(cfg.TrustedProxyCIDRs) == 0 {
		logger.Fatal("configure TLS or explicit trusted proxy CIDRs; insecure HTTP is development-only")
	}
	backend, err := app.NewWithConfig(logger, cfg)
	if err != nil {
		logger.Fatal(err)
	}
	defer backend.Close()
	addr := os.Getenv("OPENREMOTE_LISTEN")
	if addr == "" {
		addr = "127.0.0.1:8080"
	}
	srv := &http.Server{Addr: addr, Handler: backend.Handler(), ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, WriteTimeout: 30 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 16 * 1024}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	done := make(chan error, 1)
	go func() {
		logger.Printf("listening on %s", addr)
		if cert != "" {
			done <- srv.ListenAndServeTLS(cert, key)
		} else {
			done <- srv.ListenAndServe()
		}
	}()
	select {
	case err := <-done:
		if !errors.Is(err, http.ErrServerClosed) {
			logger.Fatal(err)
		}
	case <-ctx.Done():
		deadline, cancel := context.WithTimeout(context.Background(), 25*time.Second)
		defer cancel()
		if err := srv.Shutdown(deadline); err != nil {
			logger.Printf("shutdown: %v", err)
			_ = srv.Close()
		}
	}
}
