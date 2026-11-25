package app

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/ed25519"
	"crypto/elliptic"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

const enrollmentSecret = "test-enrollment-secret-with-32-bytes-or-more"

func testConfig() Config {
	c := DefaultConfig()
	c.AllowInsecure = true
	c.EnrollmentToken = enrollmentSecret
	c.RatePerMinute = 10000
	c.SensitiveRatePerMinute = 10000
	c.PollTimeout = 10 * time.Millisecond
	return c
}
func newTestServer(t *testing.T, c Config) *Server {
	t.Helper()
	s, err := NewWithConfig(log.New(io.Discard, "", 0), c)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = s.Close() })
	return s
}
func call(t *testing.T, h http.Handler, method, path, bearer string, body any) *httptest.ResponseRecorder {
	t.Helper()
	var r io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			t.Fatal(err)
		}
		r = bytes.NewReader(b)
	}
	req := httptest.NewRequest(method, path, r)
	req.Header.Set("Content-Type", "application/json")
	if bearer != "" {
		req.Header.Set("Authorization", "Bearer "+bearer)
	}
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	return rr
}
func wantCode(t *testing.T, r *httptest.ResponseRecorder, n int) {
	t.Helper()
	if r.Code != n {
		t.Fatalf("status=%d want=%d body=%s", r.Code, n, r.Body.String())
	}
}
func decode[T any](t *testing.T, r *httptest.ResponseRecorder) T {
	t.Helper()
	var x T
	if err := json.Unmarshal(r.Body.Bytes(), &x); err != nil {
		t.Fatal(err)
	}
	return x
}

type enrolled struct {
	Device Device `json:"device"`
	Token  string `json:"deviceToken"`
}
type sessionResult struct {
	Session Session `json:"session"`
	Token   string  `json:"controllerToken"`
}
type authResult struct {
	Token string `json:"accessToken"`
}
type challengeResult struct {
	ID        string `json:"challengeId"`
	Challenge string `json:"challenge"`
}

func enrollTest(t *testing.T, h http.Handler) enrolled {
	t.Helper()
	pub, _, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	r := call(t, h, "POST", "/v1/devices/enroll", enrollmentSecret, map[string]any{"name": "Test PC", "platform": "windows", "publicKey": base64.StdEncoding.EncodeToString(pub)})
	wantCode(t, r, 201)
	return decode[enrolled](t, r)
}
func keyPair(t *testing.T) (*ecdsa.PrivateKey, string) {
	t.Helper()
	k, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	der, err := x509.MarshalPKIXPublicKey(&k.PublicKey)
	if err != nil {
		t.Fatal(err)
	}
	return k, base64.StdEncoding.EncodeToString(der)
}
func challengeTest(t *testing.T, h http.Handler, key string) challengeResult {
	t.Helper()
	r := call(t, h, "POST", "/v1/auth/challenge", "", map[string]string{"publicKey": key, "algorithm": "p256"})
	wantCode(t, r, 201)
	return decode[challengeResult](t, r)
}
func proof(t *testing.T, k *ecdsa.PrivateKey, c challengeResult) map[string]string {
	t.Helper()
	raw, err := base64.StdEncoding.DecodeString(c.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	d := sha256.Sum256(raw)
	sig, err := ecdsa.SignASN1(rand.Reader, k, d[:])
	if err != nil {
		t.Fatal(err)
	}
	return map[string]string{"challengeId": c.ID, "signature": base64.StdEncoding.EncodeToString(sig)}
}
func loginTest(t *testing.T, h http.Handler) (string, string) {
	t.Helper()
	k, key := keyPair(t)
	c := challengeTest(t, h, key)
	r := call(t, h, "POST", "/v1/auth/verify", "", proof(t, k, c))
	wantCode(t, r, 200)
	return decode[authResult](t, r).Token, key
}
func sessionTest(t *testing.T, h http.Handler, host enrolled, auth, key string) sessionResult {
	t.Helper()
	r := call(t, h, "POST", "/v1/sessions", auth, map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Test Phone", "controllerPublicKey": key, "permissions": PermissionSet{ViewScreen: true, ControlInput: true, Audio: true}})
	wantCode(t, r, 201)
	return decode[sessionResult](t, r)
}
func offer() map[string]any {
	return map[string]any{"kind": "offer", "payload": map[string]string{"type": "offer", "sdp": "v=0\r\ns=-\r\n"}}
}
func answer() map[string]any {
	return map[string]any{"kind": "answer", "payload": map[string]string{"type": "answer", "sdp": "v=0\r\ns=-\r\n"}}
}
func ice() map[string]any {
	return map[string]any{"kind": "ice", "payload": map[string]any{"candidate": "candidate:1 1 udp 2122260223 192.0.2.1 5000 typ host", "sdpMid": "0", "sdpMLineIndex": 0}}
}

func TestProofOfPossessionAndReplay(t *testing.T) {
	h := newTestServer(t, testConfig()).Handler()
	host := enrollTest(t, h)
	wantCode(t, call(t, h, "GET", "/v1/devices/"+host.Device.DisplayID, "", nil), 401)
	k, key := keyPair(t)
	ch := challengeTest(t, h, key)
	wrong, _ := keyPair(t)
	wantCode(t, call(t, h, "POST", "/v1/auth/verify", "", proof(t, wrong, ch)), 401)
	wantCode(t, call(t, h, "POST", "/v1/auth/verify", "", proof(t, k, ch)), 401)
	ch = challengeTest(t, h, key)
	p := proof(t, k, ch)
	r := call(t, h, "POST", "/v1/auth/verify", "", p)
	wantCode(t, r, 200)
	auth := decode[authResult](t, r).Token
	wantCode(t, call(t, h, "POST", "/v1/auth/verify", "", p), 401)
	wantCode(t, call(t, h, "GET", "/v1/devices/"+host.Device.DisplayID, auth, nil), 200)
	_, otherKey := keyPair(t)
	wantCode(t, call(t, h, "POST", "/v1/sessions", auth, map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Impersonator", "controllerPublicKey": otherKey, "permissions": PermissionSet{ViewScreen: true}}), 403)
	wantCode(t, call(t, h, "POST", "/v1/auth/challenge", "", map[string]string{"algorithm": "p256", "publicKey": base64.StdEncoding.EncodeToString(make([]byte, 32))}), 400)
}
func TestSessionLifecycleAuthorizationAndPermissions(t *testing.T) {
	h := newTestServer(t, testConfig()).Handler()
	host := enrollTest(t, h)
	other := enrollTest(t, h)
	auth, key := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, key)
	path := "/v1/sessions/" + ss.Session.ID
	wantCode(t, call(t, h, "POST", path+"/signal", ss.Token, offer()), 409)
	wantCode(t, call(t, h, "GET", path+"/ice", ss.Token, nil), 409)
	wantCode(t, call(t, h, "POST", path+"/approve", ss.Token, nil), 403)
	wantCode(t, call(t, h, "POST", path+"/approve", other.Token, nil), 401)
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true, Power: true}}), 403)
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ControlInput: true}}), 403)
	approved := call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true, ControlInput: true}})
	wantCode(t, approved, 200)
	if decode[sessionResult](t, approved).Session.Permissions.Audio {
		t.Fatal("host narrowing was not enforced")
	}
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, nil), 409)
	wantCode(t, call(t, h, "POST", path+"/signal", ss.Token, answer()), 409)
	wantCode(t, call(t, h, "POST", path+"/signal", host.Token, offer()), 409)
	wantCode(t, call(t, h, "POST", path+"/signal", ss.Token, offer()), 202)
	wantCode(t, call(t, h, "POST", path+"/connected", ss.Token, nil), 409)
	wantCode(t, call(t, h, "POST", path+"/signal", ss.Token, answer()), 409)
	got := call(t, h, "GET", path+"/signals?after=0", host.Token, nil)
	wantCode(t, got, 200)
	if !bytes.Contains(got.Body.Bytes(), []byte(`"kind":"offer"`)) {
		t.Fatal(got.Body.String())
	}
	wantCode(t, call(t, h, "POST", path+"/signal", host.Token, answer()), 202)
	status := decode[sessionResult](t, call(t, h, "GET", path, ss.Token, nil))
	if status.Session.Status == "active" {
		t.Fatal("signaling falsely marked media connected")
	}
	wantCode(t, call(t, h, "POST", path+"/connected", ss.Token, nil), 200)
	wantCode(t, call(t, h, "POST", path+"/end", ss.Token, nil), 200)
	wantCode(t, call(t, h, "POST", path+"/end", ss.Token, nil), 200)
	wantCode(t, call(t, h, "POST", path+"/signal", host.Token, ice()), 409)
	wantCode(t, call(t, h, "GET", path+"/signals", host.Token, nil), 409)
	wantCode(t, call(t, h, "POST", path+"/reject", host.Token, nil), 409)
	wantCode(t, call(t, h, "GET", path+"/extra/path", host.Token, nil), 404)
}

func TestClipboardAndFilePermissionApproval(t *testing.T) {
	cases := []struct {
		name      string
		requested PermissionSet
		granted   PermissionSet
		allowed   bool
	}{
		{"clipboard_escalation", PermissionSet{ViewScreen: true, FileTransfer: true}, PermissionSet{ViewScreen: true, Clipboard: true}, false},
		{"file_escalation", PermissionSet{ViewScreen: true, Clipboard: true}, PermissionSet{ViewScreen: true, FileTransfer: true}, false},
		{"both_explicitly_granted", PermissionSet{ViewScreen: true, Clipboard: true, FileTransfer: true}, PermissionSet{ViewScreen: true, Clipboard: true, FileTransfer: true}, true},
		{"file_permission_narrowed", PermissionSet{ViewScreen: true, Clipboard: true, FileTransfer: true}, PermissionSet{ViewScreen: true, Clipboard: true}, true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			h := newTestServer(t, testConfig()).Handler()
			host := enrollTest(t, h)
			auth, _ := loginTest(t, h)
			r := call(t, h, "POST", "/v1/sessions", auth, map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Tools phone", "permissions": tc.requested})
			wantCode(t, r, 201)
			ss := decode[sessionResult](t, r)
			path := "/v1/sessions/" + ss.Session.ID
			r = call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": tc.granted})
			if !tc.allowed {
				wantCode(t, r, 403)
				status := decode[sessionResult](t, call(t, h, "GET", path, ss.Token, nil)).Session
				if status.Status != "pending" || status.Permissions != tc.requested {
					t.Fatal("denied escalation changed request")
				}
				return
			}
			wantCode(t, r, 200)
			for _, token := range []string{host.Token, ss.Token} {
				r = call(t, h, "GET", path, token, nil)
				wantCode(t, r, 200)
				status := decode[sessionResult](t, r).Session
				if status.Status != "approved" || status.Permissions != tc.granted {
					t.Fatalf("effective tool permissions did not survive approval: %+v", status)
				}
			}
			// Grants cannot be silently replaced after the approval boundary.
			wantCode(t, call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": tc.requested}), 409)
		})
	}
}
func TestExpiryAndPresenceUseClock(t *testing.T) {
	now := time.Now()
	cfg := testConfig()
	cfg.now = func() time.Time { return now }
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	auth, key := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, key)
	now = now.Add(46 * time.Second)
	r := call(t, h, "GET", "/v1/devices/"+host.Device.DisplayID, auth, nil)
	wantCode(t, r, 200)
	var d struct{ Online bool }
	_ = json.Unmarshal(r.Body.Bytes(), &d)
	if d.Online {
		t.Fatal("stale host reported online")
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
	now = now.Add(50 * time.Second)
	r = call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil)
	wantCode(t, r, 200)
	if decode[sessionResult](t, r).Session.Status != "expired" {
		t.Fatal("pending session did not expire")
	}
	k, pub := keyPair(t)
	ch := challengeTest(t, h, pub)
	now = now.Add(61 * time.Second)
	wantCode(t, call(t, h, "POST", "/v1/auth/verify", "", proof(t, k, ch)), 401)
	now = now.Add(10 * time.Minute)
	wantCode(t, call(t, h, "GET", "/v1/devices/"+host.Device.DisplayID, auth, nil), 401)
}
func TestPersistenceAndRestartInvalidatesSessions(t *testing.T) {
	cfg := testConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	auth, key := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, key)
	trust := map[string]any{"publicKey": key, "label": "My phone", "permissions": PermissionSet{ViewScreen: true}}
	wantCode(t, call(t, h, "PUT", "/v1/hosts/"+host.Device.ID+"/trusted", host.Token, trust), 200)
	raw, err := os.ReadFile(cfg.StatePath)
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(raw, []byte(host.Token)) || bytes.Contains(raw, []byte(ss.Token)) || bytes.Contains(raw, []byte(auth)) {
		t.Fatal("plaintext token persisted")
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	restarted := newTestServer(t, cfg).Handler()
	wantCode(t, call(t, restarted, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
	fresh, _ := loginTest(t, restarted)
	wantCode(t, call(t, restarted, "GET", "/v1/devices/"+host.Device.DisplayID, fresh, nil), 200)
	wantCode(t, call(t, restarted, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil), 404)
	wantCode(t, call(t, restarted, "GET", "/v1/devices/"+host.Device.DisplayID, auth, nil), 401)
	r := call(t, restarted, "GET", "/v1/hosts/"+host.Device.ID+"/trusted", host.Token, nil)
	wantCode(t, r, 200)
	if !bytes.Contains(r.Body.Bytes(), []byte(key)) {
		t.Fatal("trust lost")
	}
	r = call(t, restarted, "GET", "/v1/hosts/"+host.Device.ID+"/audit", host.Token, nil)
	wantCode(t, r, 200)
	if !bytes.Contains(r.Body.Bytes(), []byte("device.enrolled")) {
		t.Fatal("audit lost")
	}
	st, _ := os.Stat(cfg.StatePath)
	if st.Mode().Perm() != 0600 {
		t.Fatal("state file is not private")
	}
}
func TestTURNShortLivedCredentials(t *testing.T) {
	cfg := testConfig()
	cfg.TURNURLs = []string{"turn:relay.example:3478?transport=udp", "turns:relay.example:443?transport=tcp"}
	cfg.STUNURLs = []string{"stun:relay.example:3478"}
	cfg.TURNSecret = "a-long-random-turn-secret-with-32-bytes"
	cfg.ICERelayOnly = true
	h := newTestServer(t, cfg).Handler()
	host := enrollTest(t, h)
	auth, key := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, key)
	path := "/v1/sessions/" + ss.Session.ID
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, nil), 200)
	r := call(t, h, "GET", path+"/ice", ss.Token, nil)
	wantCode(t, r, 200)
	var x struct {
		Servers []iceServer `json:"iceServers"`
		Policy  string      `json:"iceTransportPolicy"`
		Expires time.Time   `json:"expiresAt"`
	}
	x = decode[struct {
		Servers []iceServer `json:"iceServers"`
		Policy  string      `json:"iceTransportPolicy"`
		Expires time.Time   `json:"expiresAt"`
	}](t, r)
	if x.Policy != "relay" || len(x.Servers) != 2 || time.Until(x.Expires) > 10*time.Minute || time.Until(x.Expires) < 9*time.Minute {
		t.Fatal("invalid ICE policy/expiry")
	}
	srv := x.Servers[1]
	m := hmac.New(sha1.New, []byte(cfg.TURNSecret))
	_, _ = m.Write([]byte(srv.Username))
	if srv.Credential != base64.StdEncoding.EncodeToString(m.Sum(nil)) {
		t.Fatal("TURN credential not coturn compatible")
	}
	if bytes.Contains(r.Body.Bytes(), []byte(cfg.TURNSecret)) {
		t.Fatal("shared TURN secret leaked")
	}
}
func TestQueueCapacityAndInputValidation(t *testing.T) {
	cfg := testConfig()
	cfg.MaxQueue = 2
	h := newTestServer(t, cfg).Handler()
	host := enrollTest(t, h)
	auth, key := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, key)
	path := "/v1/sessions/" + ss.Session.ID
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, nil), 200)
	wantCode(t, call(t, h, "POST", path+"/signal", ss.Token, map[string]any{"kind": "offer", "payload": "fake answer"}), 400)
	wantCode(t, call(t, h, "POST", path+"/signal", ss.Token, map[string]any{"kind": "ice", "payload": map[string]any{"candidate": "evil", "sdpMLineIndex": -1}}), 400)
	wantCode(t, call(t, h, "POST", path+"/signal", ss.Token, offer()), 202)
	wantCode(t, call(t, h, "POST", path+"/signal", host.Token, answer()), 202)
	wantCode(t, call(t, h, "POST", path+"/signal", host.Token, ice()), 409)
	wantCode(t, call(t, h, "GET", path+"/signals?after=-1", host.Token, nil), 400)
	for _, body := range []string{`{"publicKey":"x","algorithm":"p256"} {}`, `{"publicKey":"x","algorithm":"p256","extra":1}`, `{"publicKey":"x","publicKey":"y","algorithm":"p256"}`} {
		req := httptest.NewRequest("POST", "/v1/auth/challenge", strings.NewReader(body))
		rr := httptest.NewRecorder()
		h.ServeHTTP(rr, req)
		wantCode(t, rr, 400)
	}
}
func TestTLSRequirementAndProxyTrust(t *testing.T) {
	cfg := testConfig()
	cfg.AllowInsecure = false
	h := newTestServer(t, cfg).Handler()
	wantCode(t, call(t, h, "GET", "/healthz", "", nil), 426)
	req := httptest.NewRequest("GET", "/healthz", nil)
	req.Header.Set("X-Forwarded-Proto", "https")
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	wantCode(t, rr, 426)
	tlsServer := httptest.NewTLSServer(h)
	defer tlsServer.Close()
	res, err := tlsServer.Client().Get(tlsServer.URL + "/healthz")
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 || res.Header.Get("Strict-Transport-Security") == "" {
		t.Fatal("TLS health failed")
	}
	cfg.TrustedProxyCIDRs = []string{"192.0.2.0/24"}
	h = newTestServer(t, cfg).Handler()
	req = httptest.NewRequest("GET", "/healthz", nil)
	req.RemoteAddr = "192.0.2.2:1234"
	req.Header.Set("X-Forwarded-Proto", "https")
	rr = httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	wantCode(t, rr, 200)
}
func TestRateLimitingIgnoresUntrustedForwardingHeaders(t *testing.T) {
	cfg := testConfig()
	cfg.RatePerMinute = 2
	h := newTestServer(t, cfg).Handler()
	for i := 0; i < 3; i++ {
		req := httptest.NewRequest("GET", "/healthz", nil)
		req.Header.Set("X-Forwarded-For", "203.0.113."+string(rune('1'+i)))
		rr := httptest.NewRecorder()
		h.ServeHTTP(rr, req)
		want := 200
		if i == 2 {
			want = 429
		}
		wantCode(t, rr, want)
	}
}
func TestConcurrentHeartbeatAndSessionReads(t *testing.T) {
	h := newTestServer(t, testConfig()).Handler()
	host := enrollTest(t, h)
	auth, key := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, key)
	var wg sync.WaitGroup
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
			wantCode(t, call(t, h, "GET", "/v1/devices/"+host.Device.DisplayID, auth, nil), 200)
			wantCode(t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil), 200)
		}()
	}
	wg.Wait()
}
func TestCanceledPollReturnsPromptly(t *testing.T) {
	cfg := testConfig()
	cfg.PollTimeout = time.Minute
	h := newTestServer(t, cfg).Handler()
	host := enrollTest(t, h)
	req := httptest.NewRequest("GET", "/v1/hosts/"+host.Device.ID+"/events", nil)
	req.Header.Set("Authorization", "Bearer "+host.Token)
	ctx, cancel := context.WithCancel(req.Context())
	req = req.WithContext(ctx)
	done := make(chan struct{})
	go func() { h.ServeHTTP(httptest.NewRecorder(), req); close(done) }()
	cancel()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("canceled poll leaked")
	}
}
func TestStorageFailureFailsClosed(t *testing.T) {
	cfg := testConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	h := newTestServer(t, cfg).Handler()
	if err := os.Mkdir(cfg.StatePath, 0700); err != nil {
		t.Fatal(err)
	}
	pub, _, _ := ed25519.GenerateKey(rand.Reader)
	r := call(t, h, "POST", "/v1/devices/enroll", enrollmentSecret, map[string]string{"name": "test", "platform": "windows", "publicKey": base64.StdEncoding.EncodeToString(pub)})
	wantCode(t, r, 503)
	wantCode(t, call(t, h, "GET", "/healthz", "", nil), 503)
}

func TestAdmittedHostPollDoesNotPublishUncommittedEvents(t *testing.T) {
	cfg := testConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	if err := os.Remove(cfg.StatePath); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(cfg.StatePath, 0700); err != nil {
		t.Fatal(err)
	}
	r := call(t, h, "POST", "/v1/sessions", auth, map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Test phone", "permissions": PermissionSet{ViewScreen: true}})
	wantCode(t, r, 503)
	// A long poll may have passed middleware before another request's commit
	// failed. Exercise that admitted handler directly: it must not publish a
	// sequence that was never durable and could be reused after restart.
	req := httptest.NewRequest("GET", "/v1/hosts/"+host.Device.ID+"/events?after=0", nil)
	req.Header.Set("Authorization", "Bearer "+host.Token)
	rr := httptest.NewRecorder()
	s.hostEvents(rr, req, host.Device.ID)
	wantCode(t, rr, 503)
}

func TestDurableStateAllowsOnlyOneWriter(t *testing.T) {
	cfg := testConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	other, err := NewWithConfig(log.New(io.Discard, "", 0), cfg)
	if err == nil {
		_ = other.Close()
		t.Fatal("multiple writers acquired same state")
	}
	if err = s.Close(); err != nil {
		t.Fatal(err)
	}
	_ = newTestServer(t, cfg)
}

func TestHostEventCursorRecovery(t *testing.T) {
	cfg := testConfig()
	cfg.MaxQueue = 2
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	s.mu.Lock()
	for i := 0; i < 5; i++ {
		s.eventLocked(s.devices[host.Device.ID], "session.ended", "", nil)
	}
	s.mu.Unlock()
	path := "/v1/hosts/" + host.Device.ID + "/events"
	r := call(t, h, "GET", path+"?after=0", host.Token, nil)
	wantCode(t, r, 409)
	x := decode[struct {
		Reset int64 `json:"resetAfter"`
	}](t, r)
	if x.Reset != 3 {
		t.Fatalf("reset=%d", x.Reset)
	}
	wantCode(t, call(t, h, "GET", path+"?after=3", host.Token, nil), 200)
	r = call(t, h, "GET", path+"?after=100", host.Token, nil)
	wantCode(t, r, 409)
	x = decode[struct {
		Reset int64 `json:"resetAfter"`
	}](t, r)
	if x.Reset != 0 {
		t.Fatal("restart cursor must be zero")
	}
}

func TestHostCursorSurvivesRestartBeforeFirstPoll(t *testing.T) {
	for _, legacy := range []bool{false, true} {
		name := "current_snapshot"
		if legacy {
			name = "legacy_snapshot_without_event_sequence"
		}
		t.Run(name, func(t *testing.T) {
			cfg := testConfig()
			cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
			s := newTestServer(t, cfg)
			h := s.Handler()
			host := enrollTest(t, h)
			auth, key := loginTest(t, h)
			_ = sessionTest(t, h, host, auth, key)
			path := "/v1/hosts/" + host.Device.ID + "/events"
			type inbox struct {
				Events []HostEvent `json:"events"`
			}
			initial := call(t, h, "GET", path+"?after=0", host.Token, nil)
			wantCode(t, initial, 200)
			seen := decode[inbox](t, initial).Events
			if len(seen) != 1 {
				t.Fatalf("initial inbox=%v", seen)
			}
			after := seen[0].Seq
			if err := s.Close(); err != nil {
				t.Fatal(err)
			}
			if legacy {
				// Existing snapshots predate the separate durable sequence field.
				raw, err := os.ReadFile(cfg.StatePath)
				if err != nil {
					t.Fatal(err)
				}
				var state map[string]any
				if err = json.Unmarshal(raw, &state); err != nil {
					t.Fatal(err)
				}
				for _, row := range state["devices"].([]any) {
					delete(row.(map[string]any), "eventSeq")
				}
				raw, err = json.Marshal(state)
				if err != nil {
					t.Fatal(err)
				}
				if err = os.WriteFile(cfg.StatePath, raw, 0600); err != nil {
					t.Fatal(err)
				}
			}
			h = newTestServer(t, cfg).Handler()
			wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
			auth, key = loginTest(t, h)
			fresh := sessionTest(t, h, host, auth, key)
			// A new request is queued before the still-running host resumes polling.
			// Reusing sequence 1 would make its retained after=1 silently skip it.
			r := call(t, h, "GET", path+"?after="+strconv.FormatInt(after, 10), host.Token, nil)
			if legacy && r.Code == 409 {
				reset := decode[struct {
					Error string `json:"error"`
					After int64  `json:"resetAfter"`
				}](t, r)
				if reset.Error != "cursor_expired" || reset.After < after {
					t.Fatalf("invalid legacy cursor recovery: %+v", reset)
				}
				r = call(t, h, "GET", path+"?after="+strconv.FormatInt(reset.After, 10), host.Token, nil)
			}
			wantCode(t, r, 200)
			received := decode[inbox](t, r).Events
			if len(received) != 1 || received[0].SessionID != fresh.Session.ID || received[0].Seq <= after {
				t.Fatalf("new request was skipped or sequence reused after restart: old=%d inbox=%+v", after, received)
			}
		})
	}
}

func TestRevocationBlocksLocallyTrustedKeyAcrossRestart(t *testing.T) {
	cfg := testConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	key, pub := keyPair(t)
	login := func(handler http.Handler) string {
		ch := challengeTest(t, handler, pub)
		r := call(t, handler, "POST", "/v1/auth/verify", "", proof(t, key, ch))
		wantCode(t, r, 200)
		return decode[authResult](t, r).Token
	}
	auth := login(h)
	ss := sessionTest(t, h, host, auth, pub)
	path := "/v1/hosts/" + host.Device.ID + "/trusted"
	body := map[string]any{"publicKey": pub, "label": "Phone", "permissions": PermissionSet{ViewScreen: true, ControlInput: true}}
	wantCode(t, call(t, h, "PUT", path, host.Token, body), 200)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, nil), 200)
	wantCode(t, call(t, h, "DELETE", path, host.Token, map[string]string{"publicKey": pub}), 200)
	status := decode[sessionResult](t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil))
	if status.Session.Status != "ended" {
		t.Fatal("revocation failed to end session")
	}
	request := map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Old locally trusted phone", "permissions": PermissionSet{ViewScreen: true}}
	wantCode(t, call(t, h, "POST", "/v1/sessions", auth, request), 403)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	h = newTestServer(t, cfg).Handler()
	auth = login(h)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
	wantCode(t, call(t, h, "POST", "/v1/sessions", auth, request), 403)
	wantCode(t, call(t, h, "PUT", path, host.Token, body), 200)
	wantCode(t, call(t, h, "POST", "/v1/sessions", auth, request), 201)
}
