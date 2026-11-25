package app

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/tls"
	"encoding/base64"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func gatewayTestToken(t *testing.T) string {
	t.Helper()
	value, err := token(32)
	if err != nil {
		t.Fatal(err)
	}
	return value
}

// Every synthetic caller has the same immediate proxy address. Only its
// authenticated gateway assertion may separate the client rate buckets.
func gatewayTestCaller(h http.Handler, secret, ip string) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		r.RemoteAddr = "192.0.2.8:44000"
		r.Header.Set("X-OpenRemote-Gateway-Token", secret)
		r.Header.Set("X-OpenRemote-Client-IP", ip)
		// This is deliberately unrelated to the authenticated address. Even a
		// trusted TLS terminator's forwarding hop must not replace that address.
		r.Header.Set("X-Forwarded-For", "198.51.100.80")
		h.ServeHTTP(w, r)
	})
}

func TestGatewaySeparatesClientsBehindOneEgress(t *testing.T) {
	for _, sensitive := range []bool{false, true} {
		t.Run(map[bool]string{false: "ordinary", true: "sensitive"}[sensitive], func(t *testing.T) {
			cfg := testConfig()
			cfg.GatewayToken = gatewayTestToken(t)
			cfg.TrustedProxyCIDRs = []string{"192.0.2.8/32"}
			path, firstStatus := "/v1/info", 200
			if sensitive {
				cfg.SensitiveRatePerMinute = 1
				path, firstStatus = "/v1/devices/123456789", 401
			} else {
				cfg.RatePerMinute = 1
			}
			h := newTestServer(t, cfg).Handler()
			first := gatewayTestCaller(h, cfg.GatewayToken, "203.0.113.10")
			second := gatewayTestCaller(h, cfg.GatewayToken, "2001:db8::20")
			wantCode(t, call(t, first, "GET", path, "", nil), firstStatus)
			wantCode(t, call(t, second, "GET", path, "", nil), firstStatus)
			wantCode(t, call(t, first, "GET", path, "", nil), 429)
		})
	}
}

func TestGatewayRequiresExactTokenAndOneCanonicalClientIP(t *testing.T) {
	cfg := testConfig()
	cfg.GatewayToken = gatewayTestToken(t)
	s := newTestServer(t, cfg)
	h := s.Handler()
	invalid := []struct {
		name   string
		tokens []string
		ips    []string
	}{
		{"missing token", nil, []string{"203.0.113.10"}},
		{"wrong token", []string{gatewayTestToken(t)}, []string{"203.0.113.10"}},
		{"duplicate token", []string{cfg.GatewayToken, cfg.GatewayToken}, []string{"203.0.113.10"}},
		{"missing IP", []string{cfg.GatewayToken}, nil},
		{"duplicate IP", []string{cfg.GatewayToken}, []string{"203.0.113.10", "203.0.113.10"}},
	}
	for _, ip := range []string{"", "203.0.113.10, 203.0.113.11", "203.0.113.010", " 203.0.113.10", "203.0.113.10 ", "2001:DB8::1", "2001:0db8::1", "::ffff:203.0.113.10", "fe80::1%eth0", "pc.example", "203.0.113.10:443", "[2001:db8::1]"} {
		invalid = append(invalid, struct {
			name   string
			tokens []string
			ips    []string
		}{"invalid IP " + ip, []string{cfg.GatewayToken}, []string{ip}})
	}
	for _, tc := range invalid {
		t.Run(tc.name, func(t *testing.T) {
			r := httptest.NewRequest("GET", "/v1/info", nil)
			for _, value := range tc.tokens {
				r.Header.Add("X-OpenRemote-Gateway-Token", value)
			}
			for _, value := range tc.ips {
				r.Header.Add("X-OpenRemote-Client-IP", value)
			}
			r.Header.Set("X-Forwarded-For", "203.0.113.99")
			r.Header.Set("X-Forwarded-Proto", "https")
			rr := httptest.NewRecorder()
			h.ServeHTTP(rr, r)
			wantCode(t, rr, 401)
			if rr.Body.String() != "{\"error\":\"gateway_auth_required\"}\n" {
				t.Fatal("gateway rejection exposed credential or address details")
			}
		})
	}
	if len(s.limiters) != 0 {
		t.Fatal("unverified gateway requests consumed client rate buckets")
	}
	for _, ip := range []string{"203.0.113.10", "2001:db8::20"} {
		wantCode(t, call(t, gatewayTestCaller(h, cfg.GatewayToken, ip), "GET", "/v1/info", "", nil), 200)
	}
	// Existing local infrastructure health checks are not API authorization.
	wantCode(t, call(t, h, "GET", "/healthz", "", nil), 200)
}

func TestGatewayTokenValidation(t *testing.T) {
	for _, invalid := range []string{strings.Repeat("x", 31), strings.Repeat("x", 513), strings.Repeat("x", 31) + " ", strings.Repeat("x", 31) + "\n", strings.Repeat("x", 31) + "\x7f", strings.Repeat("x", 31) + "é"} {
		cfg := testConfig()
		cfg.GatewayToken = invalid
		if s, err := NewWithConfig(nil, cfg); err == nil {
			_ = s.Close()
			t.Fatal("invalid gateway token accepted")
		} else if strings.Contains(err.Error(), invalid) {
			t.Fatal("configuration error exposed token")
		}
	}
	for _, length := range []int{0, 32, 512} {
		cfg := testConfig()
		cfg.GatewayToken = strings.Repeat("x", length)
		newTestServer(t, cfg)
	}
}

func TestGatewayProofDoesNotReplaceTLSOrClientAuthorization(t *testing.T) {
	cfg := testConfig()
	cfg.GatewayToken = gatewayTestToken(t)
	cfg.AllowInsecure = false
	cfg.TrustedProxyCIDRs = []string{"192.0.2.8/32"}
	h := newTestServer(t, cfg).Handler()
	for _, tc := range []struct {
		name, peer, proto string
		tls               bool
		status            int
	}{
		{"untrusted peer cannot assert HTTPS", "198.51.100.1:44000", "https", false, 426},
		{"client IP in proxy CIDR is not peer trust", "198.51.100.1:44000", "https", false, 426},
		{"trusted peer needs HTTPS assertion", "192.0.2.8:44000", "", false, 426},
		{"actual TLS", "198.51.100.1:44000", "", true, 200},
		{"trusted TLS terminator", "192.0.2.8:44000", "https", false, 200},
	} {
		t.Run(tc.name, func(t *testing.T) {
			r := httptest.NewRequest("GET", "/v1/info", nil)
			r.RemoteAddr = tc.peer
			if tc.tls {
				r.TLS = &tls.ConnectionState{}
			}
			r.Header.Set("X-OpenRemote-Gateway-Token", cfg.GatewayToken)
			clientIP := "203.0.113.10"
			if tc.name == "client IP in proxy CIDR is not peer trust" {
				clientIP = "192.0.2.8"
			}
			r.Header.Set("X-OpenRemote-Client-IP", clientIP)
			r.Header.Set("X-Forwarded-Proto", tc.proto)
			rr := httptest.NewRecorder()
			h.ServeHTTP(rr, r)
			wantCode(t, rr, tc.status)
		})
	}
	// TLS is real here, but the transport token grants no host/controller access.
	authorizedTransport := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		r.TLS = &tls.ConnectionState{}
		gatewayTestCaller(h, cfg.GatewayToken, "203.0.113.10").ServeHTTP(w, r)
	})
	for _, path := range []string{"/v1/devices/123456789", "/v1/hosts/abcdefghijklmnop/heartbeat", "/v1/hosts/abcdefghijklmnop/pairing"} {
		method := "GET"
		if strings.HasSuffix(path, "/heartbeat") {
			method = "POST"
		}
		wantCode(t, call(t, authorizedTransport, method, path, "", nil), 401)
	}
	wantCode(t, call(t, authorizedTransport, "POST", "/v1/pairings/redeem", "", map[string]string{"code": "123456789", "pin": testPairingPIN, "controllerName": "Phone"}), 401)
}

func TestGatewayPairingAndRegistrationUseAuthenticatedClientBuckets(t *testing.T) {
	t.Run("pairing", func(t *testing.T) {
		cfg := testConfig()
		cfg.GatewayToken = gatewayTestToken(t)
		cfg.PairingIPRate = 1
		h := newTestServer(t, cfg).Handler()
		first := gatewayTestCaller(h, cfg.GatewayToken, "203.0.113.10")
		second := gatewayTestCaller(h, cfg.GatewayToken, "2001:db8::20")
		auth, _ := loginTest(t, first)
		expectPairingUnavailable(t, redeemPairingTest(t, first, auth, "000001", testPairingPIN))
		expectPairingUnavailable(t, redeemPairingTest(t, second, auth, "000001", testPairingPIN))
		wantCode(t, redeemPairingTest(t, first, auth, "000001", testPairingPIN), 429)
	})
	for _, proof := range []bool{false, true} {
		t.Run(map[bool]string{false: "registration challenge", true: "registration proof"}[proof], func(t *testing.T) {
			cfg := automaticRegistrationConfig()
			cfg.GatewayToken = gatewayTestToken(t)
			cfg.HostRegistrationChallengeIPRate = 1
			cfg.HostRegistrationProofIPRate = 1
			h := newTestServer(t, cfg).Handler()
			first := gatewayTestCaller(h, cfg.GatewayToken, "203.0.113.10")
			second := gatewayTestCaller(h, cfg.GatewayToken, "2001:db8::20")
			pub, _, err := ed25519.GenerateKey(rand.Reader)
			if err != nil {
				t.Fatal(err)
			}
			path, status := "/v1/hosts/register/challenge", 201
			body := map[string]string{"publicKey": base64.StdEncoding.EncodeToString(pub), "name": "Gateway test PC", "platform": "windows"}
			if proof {
				path, status = "/v1/hosts/register", 401
				body = map[string]string{"challengeId": strings.Repeat("a", 24), "signature": ""}
			}
			wantCode(t, call(t, first, "POST", path, "", body), status)
			wantCode(t, call(t, second, "POST", path, "", body), status)
			wantCode(t, call(t, first, "POST", path, "", body), 429)
		})
	}
}

func TestGatewayEmptyConfigurationPreservesDirectClients(t *testing.T) {
	cfg := testConfig()
	cfg.RatePerMinute = 1
	h := newTestServer(t, cfg).Handler()
	first := gatewayTestCaller(h, gatewayTestToken(t), "203.0.113.10")
	second := gatewayTestCaller(h, gatewayTestToken(t), "2001:db8::20")
	wantCode(t, call(t, first, "GET", "/v1/info", "", nil), 200)
	wantCode(t, call(t, second, "GET", "/v1/info", "", nil), 429)
}
