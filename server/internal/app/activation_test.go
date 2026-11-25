package app

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func activationTestConfig() Config {
	cfg := testConfig()
	// KDF work is real, including under -race. Freeze unrelated presence and
	// pending deadlines so concurrency exercises admission rather than wall time.
	now := time.Date(2026, 11, 20, 12, 0, 0, 0, time.UTC)
	cfg.now = func() time.Time { return now }
	return cfg
}

func createActivationTest(t *testing.T, h http.Handler, host enrolled) pairingPublic {
	t.Helper()
	r := call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true, ControlInput: true}, "mode": "activation"})
	wantCode(t, r, http.StatusCreated)
	p := decode[pairingPublic](t, r)
	if p.Mode != "activation" || p.MaxConnections != 4 {
		t.Fatalf("unexpected activation metadata: %s", r.Body.String())
	}
	return p
}

func activationSessionTest(t *testing.T, h http.Handler, host enrolled, auth string) sessionResult {
	t.Helper()
	r := redeemPairingTest(t, h, auth, host.Device.DisplayID, testPairingPIN)
	wantCode(t, r, http.StatusCreated)
	return decode[sessionResult](t, r)
}

func TestActivationContractIsAdditiveAndRequiresExplicitMode(t *testing.T) {
	cfg := testConfig()
	now := time.Date(2026, 11, 20, 12, 0, 0, 0, time.UTC)
	cfg.now = func() time.Time { return now }
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	path := "/v1/hosts/" + host.Device.ID + "/pairing"
	r := call(t, h, "POST", path, host.Token, map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true, ControlInput: true}, "mode": "activation"})
	wantCode(t, r, http.StatusCreated)
	p := decode[map[string]any](t, r)
	if p["mode"] != "activation" || p["maxConnections"] != float64(4) || len(p) != 6 {
		t.Fatalf("activation capability missing or ambiguous: %s", r.Body.String())
	}
	expires, err := time.Parse(time.RFC3339Nano, p["expiresAt"].(string))
	if err != nil || !expires.Equal(now.Add(24*time.Hour)) {
		t.Fatalf("activation must have a 24-hour safety expiry: %s", r.Body.String())
	}
	info := decode[map[string]any](t, call(t, h, "GET", "/v1/info", "", nil))
	if info["activationVersion"] != float64(1) || info["maxActivationConnections"] != float64(4) || info["version"] != "0.8.0" {
		t.Fatalf("activation discovery mismatch: %v", info)
	}
	for _, mode := range []any{"", "pairing", "Activation", nil, 1, true, map[string]string{"mode": "activation"}} {
		wantCode(t, call(t, h, "POST", path, host.Token, map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true}, "mode": mode}), http.StatusBadRequest)
	}
	wantCode(t, call(t, h, "POST", path, host.Token, map[string]any{"pin": "1234567", "permissions": PermissionSet{ViewScreen: true}, "mode": "activation"}), http.StatusBadRequest)
	legacy := call(t, h, "POST", path, host.Token, map[string]any{"pin": "123456789012", "permissions": PermissionSet{ViewScreen: true}})
	wantCode(t, legacy, http.StatusCreated)
	if value := decode[map[string]any](t, legacy); len(value) != 4 || value["mode"] != nil || value["maxConnections"] != nil {
		t.Fatalf("legacy response changed: %s", legacy.Body.String())
	}
}

func TestActivationConcurrentCapacityAndInputReservation(t *testing.T) {
	s := newTestServer(t, activationTestConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	p := createActivationTest(t, h, host)
	var auths []string
	for i := 0; i < 8; i++ {
		auth, _ := loginTest(t, h)
		auths = append(auths, auth)
	}
	sessions := []sessionResult{activationSessionTest(t, h, host, auths[0]), activationSessionTest(t, h, host, auths[1])}
	start := make(chan struct{})
	results := make(chan *httptest.ResponseRecorder, 6)
	var wg sync.WaitGroup
	for _, auth := range auths[2:] {
		wg.Add(1)
		go func(auth string) {
			defer wg.Done()
			<-start
			results <- redeemPairingTest(t, h, auth, host.Device.DisplayID, testPairingPIN)
		}(auth)
	}
	close(start)
	wg.Wait()
	close(results)
	for r := range results {
		switch r.Code {
		case http.StatusCreated:
			sessions = append(sessions, decode[sessionResult](t, r))
		case http.StatusForbidden:
			expectPairingUnavailable(t, r)
		case http.StatusTooManyRequests:
			if r.Header().Get("Retry-After") != "60" {
				t.Fatal("worker saturation lost its bounded retry response")
			}
		default:
			t.Fatalf("unexpected concurrent redemption: %d %s", r.Code, r.Body.String())
		}
	}
	if len(sessions) != 4 || len(s.sessions) != 4 || len(s.pairingWorkers) != 0 {
		t.Fatalf("four total peers must include pending reservations: %d", len(sessions))
	}
	controlCount := 0
	var controller sessionResult
	var spectators []sessionResult
	for _, ss := range sessions {
		if ss.Session.Mode != "activation" || ss.Session.PairingID != p.ID || ss.Session.Status != "pending" || !ss.Session.Permissions.ViewScreen {
			t.Fatalf("incorrect activation session: %+v", ss.Session)
		}
		if ss.Session.Permissions.ControlInput {
			controlCount++
			controller = ss
		} else {
			spectators = append(spectators, ss)
		}
	}
	if controlCount != 1 {
		t.Fatalf("concurrent admissions reserved %d input controllers", controlCount)
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auths[0], host.Device.DisplayID, testPairingPIN))
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+spectators[0].Session.ID+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true, ControlInput: true}}), http.StatusForbidden)
	// Approving spectators before the reserved controller must not reorder the
	// input lease. Later promotion attempts still fail the existing subset rule.
	for _, ss := range append(spectators, controller) {
		wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, nil), http.StatusOK)
	}
	status := call(t, h, "GET", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, nil)
	if got := decode[struct{ Pairing *pairingPublic }](t, status).Pairing; got == nil || *got != p {
		t.Fatal("successful redemptions consumed or rotated activation")
	}
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+controller.Session.ID+"/end", controller.Token, nil), http.StatusOK)
	for _, ss := range spectators {
		got := decode[sessionResult](t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil)).Session
		if got.Permissions.ControlInput || got.EndedAt != nil {
			t.Fatal("ending the controller promoted or ended an existing spectator")
		}
	}
	replacement := activationSessionTest(t, h, host, auths[0])
	if !replacement.Session.Permissions.ControlInput {
		t.Fatal("a new session could not reserve input after the previous controller ended")
	}
	for _, event := range s.devices[host.Device.ID].Events {
		if event.Type != "session.requested" {
			continue
		}
		var payload map[string]any
		if json.Unmarshal(event.Payload, &payload) != nil || payload["mode"] != "activation" || payload["pairingId"] != p.ID || payload["controllerVerified"] != true {
			t.Fatalf("request event lost activation or authenticated-key metadata: %s", event.Payload)
		}
	}
}

func TestActivationStopEndsPendingApprovedAndConnectedSessions(t *testing.T) {
	s := newTestServer(t, activationTestConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	p := createActivationTest(t, h, host)
	var sessions []sessionResult
	for i := 0; i < 4; i++ {
		sessions = append(sessions, activationSessionTest(t, h, host, auth))
	}
	for _, ss := range sessions[:2] {
		wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, nil), http.StatusOK)
	}
	active := sessions[0]
	path := "/v1/sessions/" + active.Session.ID
	wantCode(t, call(t, h, "POST", path+"/signal", active.Token, offer()), http.StatusAccepted)
	wantCode(t, call(t, h, "POST", path+"/signal", host.Token, answer()), http.StatusAccepted)
	wantCode(t, call(t, h, "POST", path+"/connected", host.Token, nil), http.StatusOK)
	ownerPath := "/v1/hosts/" + host.Device.ID + "/pairing"
	wantCode(t, call(t, h, "DELETE", ownerPath, auth, nil), http.StatusUnauthorized)
	wantCode(t, call(t, h, "DELETE", ownerPath, host.Token, nil), http.StatusOK)
	wantCode(t, call(t, h, "DELETE", ownerPath, host.Token, nil), http.StatusOK)
	for _, ss := range sessions {
		got := decode[sessionResult](t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil)).Session
		if got.Status != "ended" || got.EndedAt == nil || len(s.sessions[ss.Session.ID].Signals) != 0 {
			t.Fatalf("Stop left session resources authorized: %+v", got)
		}
		wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/signal", ss.Token, offer()), http.StatusConflict)
		wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, nil), http.StatusConflict)
	}
	if got := decode[struct{ Pairing *pairingPublic }](t, call(t, h, "GET", ownerPath, host.Token, nil)).Pairing; got != nil {
		t.Fatal("Stop left an accepting grant")
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, host.Device.DisplayID, testPairingPIN))
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p.Code, testPairingPIN))
}

func TestActivationStablePCIDAndPINLockoutDoNotEnumerateHosts(t *testing.T) {
	s := newTestServer(t, activationTestConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	unknown := "000000000"
	if s.byDisplay[unknown] != nil {
		t.Fatal("test unknown display ID collided")
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, unknown, testPairingPIN))
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, host.Device.DisplayID, testPairingPIN))
	legacy := createPairingTest(t, h, host, testPairingPIN)
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, host.Device.DisplayID, testPairingPIN))
	if s.pairings[legacy.Code].Failures != 0 {
		t.Fatal("stable ID incorrectly selected a legacy one-use verifier")
	}
	p := createActivationTest(t, h, host)
	ss := activationSessionTest(t, h, host, auth)
	for i := 0; i < 5; i++ {
		expectPairingUnavailable(t, redeemPairingTest(t, h, auth, host.Device.DisplayID, "654321"))
	}
	other, _ := loginTest(t, h)
	expectPairingUnavailable(t, redeemPairingTest(t, h, other, host.Device.DisplayID, testPairingPIN))
	expectPairingUnavailable(t, redeemPairingTest(t, h, other, p.Code, testPairingPIN))
	ownerPath := "/v1/hosts/" + host.Device.ID + "/pairing"
	if got := decode[struct{ Pairing *pairingPublic }](t, call(t, h, "GET", ownerPath, host.Token, nil)).Pairing; got != nil {
		t.Fatal("host cannot detect PIN lockout")
	}
	if got := decode[sessionResult](t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil)).Session; got.EndedAt == nil {
		t.Fatal("locked activation retained a pending approval")
	}
	wantCode(t, call(t, h, "DELETE", ownerPath, host.Token, nil), http.StatusOK)
}

func TestActivationIsolationPreservesLegacyOneUseAndSingleSession(t *testing.T) {
	s := newTestServer(t, activationTestConfig())
	h := s.Handler()
	host := enrollTest(t, h)
	auth, key := loginTest(t, h)
	legacy := sessionTest(t, h, host, auth, key)
	ownerPath := "/v1/hosts/" + host.Device.ID + "/pairing"
	activationBody := map[string]any{"pin": testPairingPIN, "permissions": PermissionSet{ViewScreen: true, ControlInput: true}, "mode": "activation"}
	wantCode(t, call(t, h, "POST", ownerPath, host.Token, activationBody), http.StatusConflict)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+legacy.Session.ID+"/approve", host.Token, nil), http.StatusOK)
	wantCode(t, call(t, h, "POST", ownerPath, host.Token, activationBody), http.StatusConflict)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+legacy.Session.ID+"/end", legacy.Token, nil), http.StatusOK)
	createActivationTest(t, h, host)
	request := map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Legacy controller", "permissions": PermissionSet{ViewScreen: true, ControlInput: true}}
	wantCode(t, call(t, h, "POST", "/v1/sessions", auth, request), http.StatusConflict)
	active := activationSessionTest(t, h, host, auth)
	wantCode(t, call(t, h, "POST", "/v1/sessions", auth, request), http.StatusConflict)
	wantCode(t, call(t, h, "DELETE", ownerPath, host.Token, nil), http.StatusOK)
	if s.sessions[active.Session.ID].EndedAt == nil {
		t.Fatal("mode transition left an activation session")
	}
	p := createPairingTest(t, h, host, "123456789012")
	r := redeemPairingTest(t, h, auth, p.Code, "123456789012")
	wantCode(t, r, http.StatusCreated)
	legacy = decode[sessionResult](t, r)
	if legacy.Session.Mode != "" || bytes.Contains(r.Body.Bytes(), []byte(`"mode"`)) {
		t.Fatal("legacy session wire format acquired activation metadata")
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, p.Code, "123456789012"))
	second := sessionTest(t, h, host, auth, key)
	wantCode(t, call(t, h, "DELETE", ownerPath, host.Token, nil), http.StatusOK)
	if s.sessions[legacy.Session.ID].EndedAt != nil {
		t.Fatal("legacy code revocation changed its historical session semantics")
	}
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+legacy.Session.ID+"/approve", host.Token, nil), http.StatusOK)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+second.Session.ID+"/approve", host.Token, nil), http.StatusConflict)
}

func TestActivationReplacementExpiryAndRestartRetainOnlyStablePCIdentity(t *testing.T) {
	cfg := activationTestConfig()
	now := cfg.now()
	cfg.now = func() time.Time { return now }
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	cfg.SessionTTL = 48 * time.Hour
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	first := createActivationTest(t, h, host)
	old := activationSessionTest(t, h, host, auth)
	second := createActivationTest(t, h, host)
	if first.ID == second.ID || first.Code == second.Code || s.sessions[old.Session.ID].EndedAt == nil {
		t.Fatal("replacement reused a generation or retained its pending session")
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, first.Code, testPairingPIN))
	current := activationSessionTest(t, h, host, auth)
	r := call(t, h, "POST", "/v1/sessions/"+current.Session.ID+"/approve", host.Token, nil)
	wantCode(t, r, http.StatusOK)
	if !decode[sessionResult](t, r).Session.ExpiresAt.Equal(second.ExpiresAt) {
		t.Fatal("session authorization exceeded the activation safety expiry")
	}
	now = now.Add(24 * time.Hour)
	ownerPath := "/v1/hosts/" + host.Device.ID + "/pairing"
	if got := decode[struct{ Pairing *pairingPublic }](t, call(t, h, "GET", ownerPath, host.Token, nil)).Pairing; got != nil {
		t.Fatal("expired activation remains accepting")
	}
	if s.sessions[current.Session.ID].EndedAt == nil {
		t.Fatal("activation expiry left an approved child")
	}
	wantCode(t, call(t, h, "DELETE", ownerPath, host.Token, nil), http.StatusOK)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), http.StatusOK)
	third := createActivationTest(t, h, host)
	snapshot, err := os.ReadFile(cfg.StatePath)
	if err != nil {
		t.Fatal(err)
	}
	for _, secret := range []string{`"` + testPairingPIN + `"`, `"` + third.Code + `"`, third.ID, "PINHash", "pairings", "Salt"} {
		if bytes.Contains(snapshot, []byte(secret)) {
			t.Fatalf("activation verifier or code was persisted: %q", secret)
		}
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	restarted := newTestServer(t, cfg)
	if len(restarted.sessions) != 0 || len(restarted.pairings) != 0 || restarted.devices[host.Device.ID].DisplayID != host.Device.DisplayID {
		t.Fatal("restart restored authorization or lost the permanent PC ID")
	}
	h = restarted.Handler()
	auth, _ = loginTest(t, h)
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, host.Device.DisplayID, testPairingPIN))
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, third.Code, testPairingPIN))
}

func TestActivationHostRecoveryEndsPreviousProcessGeneration(t *testing.T) {
	cfg := automaticRegistrationConfig()
	cfg.now = activationTestConfig().now
	s := newTestServer(t, cfg)
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	auth, _ := loginTest(t, h)
	createActivationTest(t, h, host)
	ss := activationSessionTest(t, h, host, auth)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, nil), http.StatusOK)
	challenge := hostChallengeTest(t, h, key, "Restarted PC")
	r := call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, challenge))
	wantCode(t, r, http.StatusOK)
	recovered := decode[enrolled](t, r)
	if recovered.Device.DisplayID != host.Device.DisplayID || recovered.Token == host.Token || s.sessions[ss.Session.ID].EndedAt == nil || len(s.pairings) != 0 {
		t.Fatal("host recovery lost permanent identity or retained process authorization")
	}
	wantCode(t, call(t, h, "GET", "/v1/hosts/"+host.Device.ID+"/pairing", host.Token, nil), http.StatusUnauthorized)
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, recovered.Device.DisplayID, testPairingPIN))
}

func TestHostSessionInspectionIsOwnerOnlyAndContainsCurrentMetadata(t *testing.T) {
	s := newTestServer(t, activationTestConfig())
	h := s.Handler()
	host, other := enrollTest(t, h), enrollTest(t, h)
	auth, key := loginTest(t, h)
	ended := sessionTest(t, h, host, auth, key)
	approved := sessionTest(t, h, host, auth, key)
	pending := sessionTest(t, h, host, auth, key)
	foreign := sessionTest(t, h, other, auth, key)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+ended.Session.ID+"/end", ended.Token, nil), http.StatusOK)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+approved.Session.ID+"/approve", host.Token, nil), http.StatusOK)
	path := "/v1/hosts/" + host.Device.ID + "/sessions"
	for _, bearer := range []string{"", auth, other.Token, approved.Token} {
		wantCode(t, call(t, h, "GET", path, bearer, nil), http.StatusUnauthorized)
	}
	wantCode(t, call(t, h, "POST", path, host.Token, nil), http.StatusMethodNotAllowed)
	r := call(t, h, "GET", path, host.Token, nil)
	wantCode(t, r, http.StatusOK)
	got := decode[struct{ Sessions []Session }](t, r).Sessions
	if len(got) != 2 || got[0].ID >= got[1].ID {
		t.Fatalf("current session list is incomplete or unstable: %s", r.Body.String())
	}
	for _, v := range got {
		if v.HostDeviceID != host.Device.ID || v.EndedAt != nil || (v.ID != approved.Session.ID && v.ID != pending.Session.ID) || v.ControllerPublicKey != key {
			t.Fatalf("wrong session exposed to owner: %+v", v)
		}
	}
	for _, secret := range []string{ended.Session.ID, foreign.Session.ID, approved.Token, pending.Token, "controllerToken", "signals", "PINHash", "Salt"} {
		if bytes.Contains(r.Body.Bytes(), []byte(secret)) {
			t.Fatalf("inspection exposed excluded session or secret field: %q", secret)
		}
	}
	for _, ss := range []sessionResult{approved, pending} {
		wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/end", host.Token, nil), http.StatusOK)
	}
	if got := decode[struct{ Sessions []Session }](t, call(t, h, "GET", path, host.Token, nil)).Sessions; len(got) != 0 {
		t.Fatal("ended orphans remain in owner inspection")
	}
	createActivationTest(t, h, host)
}
