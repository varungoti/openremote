package app

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

const testPairingPIN = "001234"

func createPairingTest(t *testing.T, h http.Handler, host enrolled, pin string) pairingPublic {
	t.Helper()
	r := call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, map[string]any{"pin": pin, "permissions": PermissionSet{ViewScreen: true, ControlInput: true}})
	wantCode(t, r, 201)
	if strings.Contains(r.Body.String(), `"pin"`) || strings.Contains(r.Body.String(), "PINHash") || strings.Contains(r.Body.String(), "Salt") {
		t.Fatal("creation response leaked verifier material")
	}
	return decode[pairingPublic](t, r)
}

func redeemPairingTest(t *testing.T, h http.Handler, token, code, pin string) *httptest.ResponseRecorder {
	t.Helper()
	return call(t, h, "POST", "/v1/pairings/redeem", token, map[string]string{"code": code, "pin": pin, "controllerName": "Test phone"})
}

func expectPairingUnavailable(t *testing.T, r *httptest.ResponseRecorder) {
	t.Helper()
	wantCode(t, r, 403)
	if r.Body.String() != "{\"error\":\"pairing_unavailable\"}\n" {
		t.Fatalf("credential state exposed: %s", r.Body.String())
	}
}

func TestPairingCreatesOnePendingSessionAndConsumesCode(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, key := loginTest(t, h)
	p := createPairingTest(t, h, host, testPairingPIN)
	if !numeric(p.Code, 6, 6) || !safeID(p.ID) || p.ExpiresAt.Before(s.now().Add(9*time.Minute)) || p.ExpiresAt.After(s.now().Add(10*time.Minute)) {
		t.Fatalf("invalid generated code metadata: %+v", p)
	}
	path := "/v1/hosts/" + host.Device.ID + "/pairing"
	status := call(t, h, "GET", path, host.Token, nil)
	wantCode(t, status, 200)
	if got := decode[struct{ Pairing *pairingPublic }](t, status).Pairing; got == nil || *got != p {
		t.Fatal("host cannot read its current pairing status")
	}
	created := redeemPairingTest(t, h, auth, p.Code, testPairingPIN)
	wantCode(t, created, 201)
	ss := decode[sessionResult](t, created)
	if ss.Token == "" || ss.Session.Status != "pending" || ss.Session.PairingID != p.ID || ss.Session.HostName != host.Device.Name || ss.Session.HostDisplayID != host.Device.DisplayID || ss.Session.HostPublicKey != host.Device.PublicKey || ss.Session.ControllerPublicKey != key || ss.Session.Permissions != p.Permissions || ss.Session.MonitorID != "" {
		t.Fatalf("pairing did not bind the intended pending session: %+v", ss.Session)
	}
	if ss.Session.ApprovedAt != nil {
		t.Fatal("coordinator bypassed the local host grant")
	}
	if got := decode[struct{ Pairing *pairingPublic }](t, call(t, h, "GET", path, host.Token, nil)).Pairing; got != nil {
		t.Fatal("consumed code remains visible")
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p.Code, testPairingPIN))
	events := decode[struct{ Events []HostEvent }](t, call(t, h, "GET", "/v1/hosts/"+host.Device.ID+"/events", host.Token, nil)).Events
	if len(events) != 1 || events[0].Type != "session.requested" || events[0].SessionID != ss.Session.ID {
		t.Fatalf("wrong host events: %+v", events)
	}
	var payload map[string]any
	if json.Unmarshal(events[0].Payload, &payload) != nil || payload["pairingId"] != p.ID || payload["controllerVerified"] != true {
		t.Fatal("host event lacks matching grant")
	}
	wantCode(t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID+"/ice", ss.Token, nil), 409)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true, ControlInput: true, Clipboard: true}}), 403)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true}}), 200)
	got := decode[sessionResult](t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil)).Session
	if got.HostName != host.Device.Name || got.PairingID != p.ID || got.Permissions.ControlInput {
		t.Fatal("session snapshot or permission narrowing changed")
	}
}

func TestPairingValidationAuthorizationAndNoEscalation(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host, other := enrollTest(t, h), enrollTest(t, h)
	auth, _ := loginTest(t, h)
	path := "/v1/hosts/" + host.Device.ID + "/pairing"
	for _, bearer := range []string{"", other.Token, auth} {
		for _, method := range []string{"GET", "POST", "DELETE"} {
			wantCode(t, call(t, h, method, path, bearer, nil), 401)
		}
	}
	for _, pin := range []string{"", "12345", "1234567890123", "１２３４５６", "12345x", "123 456", "123456\n", "-123456"} {
		wantCode(t, call(t, h, "POST", path, host.Token, map[string]any{"pin": pin, "permissions": PermissionSet{ViewScreen: true}}), 400)
	}
	for _, permissions := range []PermissionSet{{}, {ControlInput: true}, {ViewScreen: true, Clipboard: true}, {ViewScreen: true, FileTransfer: true}, {ViewScreen: true, Audio: true}, {ViewScreen: true, Power: true}} {
		wantCode(t, call(t, h, "POST", path, host.Token, map[string]any{"pin": testPairingPIN, "permissions": permissions}), 400)
	}
	wantCode(t, call(t, h, "PUT", path, host.Token, nil), 405)
	for _, method := range []string{"GET", "POST", "DELETE"} {
		wantCode(t, call(t, h, method, path+"?pin=123456", host.Token, nil), 400)
	}
	wantCode(t, call(t, h, "GET", "/v1/pairings/redeem", auth, nil), 405)
	wantCode(t, call(t, h, "POST", "/v1/pairings/redeem?code=123456", auth, nil), 400)
	p := createPairingTest(t, h, host, testPairingPIN)
	for _, bearer := range []string{"", host.Token} {
		wantCode(t, redeemPairingTest(t, h, bearer, p.Code, testPairingPIN), 401)
	}
	for _, code := range []string{"12345", "1234567", "123-456", " 123456", "12345x", "１２３４５６"} {
		wantCode(t, redeemPairingTest(t, h, auth, code, testPairingPIN), 400)
	}
	for _, field := range []string{"permissions", "monitorId", "pairingId", "controllerPublicKey"} {
		wantCode(t, call(t, h, "POST", "/v1/pairings/redeem", auth, map[string]any{"code": p.Code, "pin": testPairingPIN, "controllerName": "Phone", field: "injected"}), 400)
	}
	wantCode(t, call(t, h, "POST", "/v1/sessions", auth, map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Phone", "permissions": PermissionSet{ViewScreen: true}, "pairingId": p.ID}), 400)
	if len(s.sessions) != 0 || s.pairings[p.Code].Failures != 0 {
		t.Fatal("malformed/unauthorized requests mutated a grant")
	}
}

func TestPairingRejectsDuplicateJSONFields(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	request := httptest.NewRequest("POST", "/v1/hosts/"+host.Device.ID+"/pairing", strings.NewReader(`{"pin":"123456","pin":"654321","permissions":{"viewScreen":true}}`))
	request.Header.Set("Authorization", "Bearer "+host.Token)
	request.Header.Set("Content-Type", "application/json")
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, request)
	wantCode(t, rr, 400)
	if len(s.pairings) != 0 {
		t.Fatal("duplicate JSON created a grant")
	}
}

func TestPairingReplacementRevocationExpiryAndRestart(t *testing.T) {
	cfg := testConfig()
	now := time.Date(2026, 11, 19, 12, 0, 0, 0, time.UTC)
	cfg.now = func() time.Time { return now }
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	path := "/v1/hosts/" + host.Device.ID + "/pairing"
	p1 := createPairingTest(t, h, host, testPairingPIN)
	p2 := createPairingTest(t, h, host, "987654321012")
	if p1.ID == p2.ID || p1.Code == p2.Code || len(s.pairings) != 1 {
		t.Fatal("replacement did not allocate a new unique grant")
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p1.Code, testPairingPIN))
	wantCode(t, call(t, h, "DELETE", path, host.Token, nil), 200)
	wantCode(t, call(t, h, "DELETE", path, host.Token, nil), 200)
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p2.Code, "987654321012"))
	p3 := createPairingTest(t, h, host, testPairingPIN)
	now = now.Add(10 * time.Minute)
	// Fresh authentication isolates expiry from the controller's own TTL.
	auth, _ = loginTest(t, h)
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p3.Code, testPairingPIN))
	if len(s.pairings) != 0 || len(s.hostPairing) != 0 {
		t.Fatal("expired code not pruned")
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
	p4 := createPairingTest(t, h, host, testPairingPIN)
	snapshot, err := os.ReadFile(cfg.StatePath)
	if err != nil {
		t.Fatal(err)
	}
	for _, secret := range []string{testPairingPIN, p4.Code, p4.ID, "PINHash", "pairings", "Salt"} {
		if bytes.Contains(snapshot, []byte(secret)) {
			t.Fatalf("pairing secret persisted: %q", secret)
		}
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	restarted := newTestServer(t, cfg)
	if len(restarted.pairings) != 0 || len(restarted.hostPairing) != 0 {
		t.Fatal("ephemeral pairing was restored")
	}
	h = restarted.Handler()
	auth, _ = loginTest(t, h)
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p4.Code, testPairingPIN))
}

func TestPairingFiveWrongPINsLockCodeForEveryController(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	p := createPairingTest(t, h, host, testPairingPIN)
	for n := 0; n < 5; n++ {
		expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p.Code, "654321"))
	}
	other, _ := loginTest(t, h)
	expectPairingUnavailable(t, redeemPairingTest(t, h, other, p.Code, testPairingPIN))
	if len(s.pairings) != 0 || len(s.sessions) != 0 {
		t.Fatal("locked code remains redeemable or created a session")
	}
}

func TestPairingCorrectPINBeforeLockoutAndViewOnlyGrant(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	r := call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true}})
	wantCode(t, r, 201)
	p := decode[pairingPublic](t, r)
	for n := 0; n < 4; n++ {
		expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p.Code, "654321"))
	}
	r = redeemPairingTest(t, h, auth, p.Code, testPairingPIN)
	wantCode(t, r, 201)
	if decode[sessionResult](t, r).Session.Permissions != (PermissionSet{ViewScreen: true}) {
		t.Fatal("redemption escalated view-only grant")
	}
}

func TestPairingConcurrentRedemptionIsAtMostOnce(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	p := createPairingTest(t, h, host, testPairingPIN)
	var wg sync.WaitGroup
	results := make(chan int, 8)
	start := make(chan struct{})
	for i := 0; i < cap(results); i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start
			r := redeemPairingTest(t, h, auth, p.Code, testPairingPIN)
			results <- r.Code
		}()
	}
	close(start)
	wg.Wait()
	close(results)
	created := 0
	for status := range results {
		if status == 201 {
			created++
		} else if status != 403 && status != 429 {
			t.Fatalf("unexpected concurrent result %d", status)
		}
	}
	if created != 1 || len(s.sessions) != 1 || len(s.devices[host.Device.ID].Events) != 1 || len(s.pairings) != 0 || len(s.pairingWorkers) != 0 {
		t.Fatalf("one-use/concurrency violation: created=%d sessions=%d", created, len(s.sessions))
	}
}

func TestPairingUniqueActiveCodesAndLeadingZeros(t *testing.T) {
	cfg := testConfig()
	// KDF work is intentionally real and can take longer than a host's
	// heartbeat TTL under the race detector. Presence is tested separately;
	// keep this uniqueness/leading-zero fixture's logical clock stable.
	fixedNow := time.Date(2026, 11, 19, 12, 0, 0, 0, time.UTC)
	cfg.now = func() time.Time { return fixedNow }
	cfg.PairingIPRate = 100
	s := newTestServer(t, cfg)
	h := s.Handler()
	seen := make(map[string]bool)
	var first pairingPublic
	for i := 0; i < 12; i++ {
		p := createPairingTest(t, h, enrollTest(t, h), testPairingPIN)
		if !numeric(p.Code, 6, 6) || seen[p.Code] {
			t.Fatal("duplicate or malformed active code")
		}
		seen[p.Code] = true
		if i == 0 {
			first = p
		}
	}
	// A fixed fixture forces leading-zero handling instead of relying on a
	// probabilistic code generator to hit that case during this test run.
	s.mu.Lock()
	p := s.pairings[first.Code]
	delete(s.pairings, first.Code)
	leading := ""
	for n := 0; n < 100; n++ {
		candidate := fmt.Sprintf("%06d", n)
		if s.pairings[candidate] == nil {
			leading = candidate
			break
		}
	}
	p.Code = leading
	s.pairings[leading], s.hostPairing[p.HostID] = p, leading
	s.mu.Unlock()
	auth, _ := loginTest(t, h)
	r := redeemPairingTest(t, h, auth, leading, testPairingPIN)
	wantCode(t, r, 201)
	if decode[sessionResult](t, r).Session.PairingID != first.ID {
		t.Fatal("leading-zero code lost its identity")
	}
}

func TestPairingOfflineRevokedAndBusyReturnGenericFailure(t *testing.T) {
	for _, scenario := range []string{"offline", "revoked", "busy", "pending_limit"} {
		t.Run(scenario, func(t *testing.T) {
			s := newTestServer(t, testConfig())
			h := s.Handler()
			host := enrollTest(t, h)
			auth, key := loginTest(t, h)
			p := createPairingTest(t, h, host, testPairingPIN)
			switch scenario {
			case "offline":
				s.mu.Lock()
				s.devices[host.Device.ID].LastSeen = s.now().Add(-time.Hour)
				s.mu.Unlock()
			case "revoked":
				wantCode(t, call(t, h, "DELETE", "/v1/hosts/"+host.Device.ID+"/trusted", host.Token, map[string]string{"publicKey": key}), 200)
			case "busy":
				ss := sessionTest(t, h, host, auth, key)
				wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, nil), 200)
			case "pending_limit":
				for i := 0; i < 3; i++ {
					sessionTest(t, h, host, auth, key)
				}
			}
			count := len(s.sessions)
			expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p.Code, testPairingPIN))
			if len(s.sessions) != count || len(s.pairings) != 1 {
				t.Fatal("failed admission consumed a valid grant or created session")
			}
		})
	}
}

func TestPairingBudgetsAndWorkerCapacity(t *testing.T) {
	for _, scenario := range []string{"host_generation", "global_generation", "controller", "ip", "global", "workers", "limiter_capacity"} {
		t.Run(scenario, func(t *testing.T) {
			cfg := testConfig()
			if scenario == "host_generation" {
				cfg.PairingHostCreateRate = 1
			}
			if scenario == "global_generation" {
				cfg.PairingGlobalCreateRate = 1
			}
			if scenario == "controller" {
				cfg.PairingControllerRate = 1
			}
			if scenario == "ip" {
				cfg.PairingIPRate = 1
			}
			if scenario == "global" {
				cfg.PairingGlobalRate = 1
			}
			s := newTestServer(t, cfg)
			h := s.Handler()
			host := enrollTest(t, h)
			auth, _ := loginTest(t, h)
			p := createPairingTest(t, h, host, testPairingPIN)
			var limited *httptest.ResponseRecorder
			if scenario == "host_generation" || scenario == "global_generation" {
				if scenario == "global_generation" {
					host = enrollTest(t, h)
				}
				limited = call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true}})
			} else {
				if scenario == "workers" {
					s.pairingWorkers <- struct{}{}
					s.pairingWorkers <- struct{}{}
					defer func() { <-s.pairingWorkers; <-s.pairingWorkers }()
				} else if scenario == "limiter_capacity" {
					s.mu.Lock()
					s.cfg.MaxLimiterEntries = len(s.limiters)
					s.mu.Unlock()
				} else {
					expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p.Code, "654321"))
					if scenario == "ip" || scenario == "global" {
						auth, _ = loginTest(t, h)
					}
				}
				limited = redeemPairingTest(t, h, auth, p.Code, testPairingPIN)
			}
			wantCode(t, limited, 429)
			if limited.Header().Get("Retry-After") != "60" || limited.Body.String() != "{\"error\":\"pairing_rate_limited\"}\n" {
				t.Fatalf("unbounded or duplicate rate response: %s", limited.Body.String())
			}
			if len(s.sessions) != 0 {
				t.Fatal("rate limit admitted session")
			}
		})
	}
}

func TestPairingStorageFailureFailsClosed(t *testing.T) {
	for _, at := range []string{"create", "redeem"} {
		t.Run(at, func(t *testing.T) {
			cfg := testConfig()
			cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
			s := newTestServer(t, cfg)
			h := s.Handler()
			host := enrollTest(t, h)
			auth, _ := loginTest(t, h)
			var p pairingPublic
			if at == "redeem" {
				p = createPairingTest(t, h, host, testPairingPIN)
			}
			if err := os.Remove(cfg.StatePath); err != nil {
				t.Fatal(err)
			}
			if err := os.Mkdir(cfg.StatePath, 0700); err != nil {
				t.Fatal(err)
			}
			var rr *httptest.ResponseRecorder
			if at == "create" {
				rr = call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true}})
			} else {
				rr = redeemPairingTest(t, h, auth, p.Code, testPairingPIN)
			}
			wantCode(t, rr, 503)
			if s.storageErr == nil || len(s.pairings) != 0 {
				t.Fatal("failed persistence left usable grant")
			}
			wantCode(t, call(t, h, "GET", "/v1/hosts/"+host.Device.ID+"/events", host.Token, nil), 503)
			wantCode(t, call(t, h, "GET", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, nil), 503)
		})
	}
}

func TestPairingHTTPSCancellationAndHealthLatch(t *testing.T) {
	cfg := testConfig()
	cfg.AllowInsecure = false
	wantCode(t, call(t, newTestServer(t, cfg).Handler(), "POST", "/v1/pairings/redeem", "", nil), 426)
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	p := createPairingTest(t, h, host, testPairingPIN)
	body, _ := json.Marshal(map[string]string{"code": p.Code, "pin": testPairingPIN, "controllerName": "Phone"})
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	req := httptest.NewRequest("POST", "/v1/pairings/redeem", bytes.NewReader(body)).WithContext(ctx)
	req.Header.Set("Authorization", "Bearer "+auth)
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	if len(s.sessions) != 0 || s.pairings[p.Code] == nil || len(s.pairingWorkers) != 0 {
		t.Fatal("canceled redemption consumed code or leaked KDF worker")
	}
	s.mu.Lock()
	s.storageErr = errors.New("test storage unavailable")
	s.mu.Unlock()
	wantCode(t, redeemPairingTest(t, h, auth, p.Code, testPairingPIN), 503)
}

func TestPairingPINVerifierMatchesIndependentKnownAnswer(t *testing.T) {
	// Computed with Python hashlib.pbkdf2_hmac('sha256', b'001234',
	// bytes(range(32)), 600000, 32), independent of Go's implementation.
	var salt [32]byte
	for i := range salt {
		salt[i] = byte(i)
	}
	got, err := derivePairingPIN(testPairingPIN, salt)
	if err != nil {
		t.Fatal(err)
	}
	const expected = "01de156f5c6ab203969a853a23b3873896f3a951116176929e2b41d5f0d3c1a1"
	if hex.EncodeToString(got[:]) != expected {
		t.Fatalf("PIN KDF mismatch: %x", got)
	}
}

func waitPairingWork(t *testing.T, s *Server, condition func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		s.mu.Lock()
		ready := condition()
		s.mu.Unlock()
		if ready {
			return
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatal("pairing KDF reservation was not observed")
}

func TestPairingRevocationWinsOverInFlightGeneration(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	path := "/v1/hosts/" + host.Device.ID + "/pairing"
	done := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		done <- call(t, h, "POST", path, host.Token, map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true}})
	}()
	waitPairingWork(t, s, func() bool { return s.pairingGeneration[host.Device.ID] == 1 && len(s.pairingWorkers) == 1 })
	wantCode(t, call(t, h, "DELETE", path, host.Token, nil), 200)
	wantCode(t, <-done, 409)
	if len(s.pairings) != 0 || len(s.pairingWorkers) != 0 {
		t.Fatal("completed generation resurrected revoked authorization")
	}
}

func TestPairingRevocationWinsOverInFlightRedemption(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	p := createPairingTest(t, h, host, testPairingPIN)
	done := make(chan *httptest.ResponseRecorder, 1)
	go func() { done <- redeemPairingTest(t, h, auth, p.Code, testPairingPIN) }()
	waitPairingWork(t, s, func() bool { return s.pairings[p.Code] != nil && s.pairings[p.Code].InFlight == 1 })
	wantCode(t, call(t, h, "DELETE", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, nil), 200)
	expectPairingUnavailable(t, <-done)
	if len(s.sessions) != 0 || len(s.pairingWorkers) != 0 {
		t.Fatal("revoked in-flight request created session or leaked capacity")
	}
}

func TestPairingCloseWinsOverInFlightRedemption(t *testing.T) {
	s := newTestServer(t, testConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	p := createPairingTest(t, h, host, testPairingPIN)
	done := make(chan *httptest.ResponseRecorder, 1)
	go func() { done <- redeemPairingTest(t, h, auth, p.Code, testPairingPIN) }()
	waitPairingWork(t, s, func() bool { return s.pairings[p.Code] != nil && s.pairings[p.Code].InFlight == 1 })
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	wantCode(t, <-done, 503)
	if len(s.sessions) != 0 || len(s.pairings) != 0 || len(s.pairingWorkers) != 0 {
		t.Fatal("closed server retained authorization or admitted a session")
	}
}

func TestPairingRateIdentityUsesProvedKeyAndTrustedProxy(t *testing.T) {
	t.Run("new bearer cannot reset controller budget", func(t *testing.T) {
		cfg := testConfig()
		cfg.PairingControllerRate = 1
		s := newTestServer(t, cfg)
		h := s.Handler()
		k, key := keyPair(t)
		login := func() string {
			challenge := challengeTest(t, h, key)
			rr := call(t, h, "POST", "/v1/auth/verify", "", proof(t, k, challenge))
			wantCode(t, rr, 200)
			return decode[authResult](t, rr).Token
		}
		expectPairingUnavailable(t, redeemPairingTest(t, h, login(), "000001", testPairingPIN))
		wantCode(t, redeemPairingTest(t, h, login(), "000001", testPairingPIN), 429)
	})
	for _, trusted := range []bool{false, true} {
		t.Run(map[bool]string{false: "untrusted forwarding is ignored", true: "trusted rightmost IP is canonical"}[trusted], func(t *testing.T) {
			cfg := testConfig()
			cfg.PairingIPRate = 1
			if trusted {
				cfg.TrustedProxyCIDRs = []string{"192.0.2.0/24"}
			}
			s := newTestServer(t, cfg)
			h := s.Handler()
			auth1, _ := loginTest(t, h)
			auth2, _ := loginTest(t, h)
			request := func(bearer, forwarded string) *httptest.ResponseRecorder {
				body, _ := json.Marshal(map[string]string{"code": "000001", "pin": testPairingPIN, "controllerName": "Phone"})
				r := httptest.NewRequest("POST", "/v1/pairings/redeem", bytes.NewReader(body))
				r.RemoteAddr = "192.0.2.9:45000"
				r.Header.Set("Authorization", "Bearer "+bearer)
				r.Header.Set("X-Forwarded-For", forwarded)
				rr := httptest.NewRecorder()
				h.ServeHTTP(rr, r)
				return rr
			}
			expectPairingUnavailable(t, request(auth1, "203.0.113.1, 2001:db8::1"))
			wantCode(t, request(auth2, "203.0.113.99, 2001:0db8:0000:0000:0000:0000:0000:0001"), 429)
		})
	}
}

func TestPairingSessionCapacityPreservesUnusedCode(t *testing.T) {
	cfg := testConfig()
	cfg.MaxSessions = 1
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	other := enrollTest(t, h)
	auth, key := loginTest(t, h)
	p := createPairingTest(t, h, host, testPairingPIN)
	sessionTest(t, h, other, auth, key)
	wantCode(t, redeemPairingTest(t, h, auth, p.Code, testPairingPIN), 503)
	if len(s.pairings) != 1 || len(s.sessions) != 1 {
		t.Fatal("capacity failure consumed code or exceeded session limit")
	}
}
