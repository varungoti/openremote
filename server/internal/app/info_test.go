package app

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

func TestPublicSetupInfoIsBoundedAndContainsNoCredentials(t *testing.T) {
	cfg := testConfig()
	cfg.TURNURLs = []string{"turn:private-relay.example:3478", "turn:private-relay.example:3478?transport=tcp", "turns:private-relay.example:443?transport=tcp"}
	cfg.TURNSecret = "not-for-the-phone-public-info-secret-32"
	cfg.ICERelayOnly = true
	s := newTestServer(t, cfg)
	h := s.Handler()
	r := call(t, h, "GET", "/v1/info", "", nil)
	wantCode(t, r, 200)
	want := `{"service":"openremote","protocolVersion":1,"version":"0.8.0","automaticHostRegistrationVersion":0,"pairingCodeVersion":1,"webController":true,"activationVersion":1,"maxActivationConnections":4,"phoneSetupVersion":1,"monitorSelection":true,"relay":{"configured":true,"udp":true,"tcp":true,"tls":true},"relayOnly":true}`
	var got, expected any
	if err := json.Unmarshal(r.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal([]byte(want), &expected); err != nil {
		t.Fatal(err)
	}
	gotJSON, _ := json.Marshal(got)
	wantJSON, _ := json.Marshal(expected)
	if string(gotJSON) != string(wantJSON) {
		t.Fatalf("info %s; want %s", gotJSON, wantJSON)
	}
	for _, secret := range []string{cfg.TURNSecret, cfg.EnrollmentToken, "private-relay.example"} {
		if strings.Contains(r.Body.String(), secret) {
			t.Fatal("public info exposed private configuration")
		}
	}
	if r.Body.Len() > 1024 || r.Header().Get("Cache-Control") != "no-store" {
		t.Fatal("info must be small and uncached")
	}
	if len(s.controllers) != 0 || len(s.challenges) != 0 || len(s.sessions) != 0 {
		t.Fatal("info must not create identities or sessions")
	}
	badMethod := call(t, h, "POST", "/v1/info", "", nil)
	wantCode(t, badMethod, 405)
	if badMethod.Header().Get("Allow") != "GET" {
		t.Fatal("method allow header missing")
	}
}

func TestSetupInfoReportsOnlyConfiguredTransports(t *testing.T) {
	cases := []struct {
		name  string
		urls  []string
		relay map[string]bool
	}{
		{"none", nil, map[string]bool{"configured": false, "udp": false, "tcp": false, "tls": false}},
		{"udp", []string{"turn:relay.example"}, map[string]bool{"configured": true, "udp": true, "tcp": false, "tls": false}},
		{"tcp", []string{"turn:relay.example?transport=tcp"}, map[string]bool{"configured": true, "udp": false, "tcp": true, "tls": false}},
		{"tls", []string{"turns:relay.example"}, map[string]bool{"configured": true, "udp": false, "tcp": false, "tls": true}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			cfg := testConfig()
			cfg.TURNURLs = tc.urls
			cfg.TURNSecret = strings.Repeat("s", 32)
			r := call(t, newTestServer(t, cfg).Handler(), "GET", "/v1/info", "", nil)
			wantCode(t, r, 200)
			value := decode[struct {
				Relay map[string]bool `json:"relay"`
			}](t, r)
			if len(value.Relay) != len(tc.relay) {
				t.Fatal("unexpected relay fields")
			}
			for key, want := range tc.relay {
				if value.Relay[key] != want {
					t.Errorf("%s=%v, want%v", key, value.Relay[key], want)
				}
			}
		})
	}
}

func TestSetupInfoObeysHTTPSStorageAndRateLimits(t *testing.T) {
	t.Run("HTTPS", func(t *testing.T) {
		cfg := testConfig()
		cfg.AllowInsecure = false
		wantCode(t, call(t, newTestServer(t, cfg).Handler(), "GET", "/v1/info", "", nil), 426)
	})
	t.Run("storage", func(t *testing.T) {
		s := newTestServer(t, testConfig())
		s.mu.Lock()
		s.storageErr = errors.New("disk failure")
		s.mu.Unlock()
		wantCode(t, call(t, s.Handler(), "GET", "/v1/info", "", nil), 503)
	})
	t.Run("closed", func(t *testing.T) {
		s := newTestServer(t, testConfig())
		if err := s.Close(); err != nil {
			t.Fatal(err)
		}
		wantCode(t, call(t, s.Handler(), "GET", "/v1/info", "", nil), 503)
	})
	t.Run("rate", func(t *testing.T) {
		cfg := testConfig()
		cfg.RatePerMinute = 1
		h := newTestServer(t, cfg).Handler()
		wantCode(t, call(t, h, "GET", "/v1/info", "", nil), 200)
		wantCode(t, call(t, h, "GET", "/v1/info", "", nil), 429)
	})
}

func TestICEURLConfigurationRejectsAmbiguity(t *testing.T) {
	invalid := []string{"TURN:relay.example", "Turns:relay.example", "turn:", "turn://relay.example:3478", "turn:relay.example/path", "turn:relay.example?transport=banana", "turn:relay.example?transport=udp&transport=tcp", "turn:relay.example?foo=bar", "turn:relay.example?transport=", "turn:relay.example?transport=%74cp", "turns:relay.example?transport=udp", "turn:relay.example:0", "turn:relay.example:65536", "turn:relay.example:abc", "turn:relay.example:", "turn:bad%20host", "turn:relay.example?", "turn:relay.example\\evil"}
	for _, value := range invalid {
		t.Run(value, func(t *testing.T) {
			cfg := testConfig()
			cfg.TURNURLs = []string{value}
			cfg.TURNSecret = strings.Repeat("s", 32)
			if err := validateICEConfig(cfg); err == nil {
				t.Fatalf("accepted invalid TURN URL %q", value)
			}
		})
	}
	for _, value := range []string{"turn:relay.example", "turn:relay.example:3478?transport=udp", "turn:192.0.2.1:3478?transport=tcp", "turns:relay.example:443", "turns:[2001:db8::1]:443?transport=tcp"} {
		t.Run(value, func(t *testing.T) {
			cfg := testConfig()
			cfg.TURNURLs = []string{value}
			cfg.TURNSecret = strings.Repeat("s", 32)
			if err := validateICEConfig(cfg); err != nil {
				t.Fatal(err)
			}
		})
	}
	for _, value := range []string{"STUN:relay.example", "stun:relay.example?transport=udp", "stuns:relay.example?anything=1", "stun://relay.example"} {
		cfg := testConfig()
		cfg.STUNURLs = []string{value}
		if err := validateICEConfig(cfg); err == nil {
			t.Fatalf("accepted ambiguous STUN URL %q", value)
		}
	}
}
