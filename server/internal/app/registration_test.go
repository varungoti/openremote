package app

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha512"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

type hostRegistrationReply struct {
	ID        string `json:"challengeId"`
	Challenge string `json:"challenge"`
	ExpiresAt string `json:"expiresAt"`
	Origin    string `json:"apiOrigin"`
}

func automaticRegistrationConfig() Config {
	cfg := testConfig()
	cfg.AutomaticHostRegistration = true
	cfg.PublicOrigin = "https://remote.example.com"
	cfg.HostRegistrationChallengeIPRate = 10000
	cfg.HostRegistrationChallengeGlobalRate = 10000
	cfg.HostRegistrationProofIPRate = 10000
	cfg.HostRegistrationProofGlobalRate = 10000
	cfg.HostRegistrationRecoveryRate = 10000
	return cfg
}

func hostIdentityTest(t *testing.T) (ed25519.PrivateKey, string) {
	t.Helper()
	pub, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return private, base64.StdEncoding.EncodeToString(pub)
}

func hostChallengeTest(t *testing.T, h http.Handler, key, name string) hostRegistrationReply {
	t.Helper()
	r := call(t, h, "POST", "/v1/hosts/register/challenge", "", map[string]string{"publicKey": key, "name": name, "platform": "windows"})
	wantCode(t, r, 201)
	return decode[hostRegistrationReply](t, r)
}

func hostProofTest(t *testing.T, private ed25519.PrivateKey, challenge hostRegistrationReply) map[string]string {
	t.Helper()
	message, err := base64.StdEncoding.Strict().DecodeString(challenge.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	return map[string]string{"challengeId": challenge.ID, "signature": base64.StdEncoding.EncodeToString(ed25519.Sign(private, message))}
}

func registerHostTest(t *testing.T, h http.Handler, private ed25519.PrivateKey, key string) enrolled {
	t.Helper()
	challenge := hostChallengeTest(t, h, key, "Automatic PC")
	r := call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, challenge))
	wantCode(t, r, 201)
	return decode[enrolled](t, r)
}

func registrationCallIP(t *testing.T, h http.Handler, path, ip string, body any) *httptest.ResponseRecorder {
	t.Helper()
	b, err := json.Marshal(body)
	if err != nil {
		t.Fatal(err)
	}
	r := httptest.NewRequest("POST", path, bytes.NewReader(b))
	r.RemoteAddr = ip + ":40000"
	r.Header.Set("Content-Type", "application/json")
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, r)
	return rr
}

func TestAutomaticRegistrationTranscriptAndOwnership(t *testing.T) {
	cfg := automaticRegistrationConfig()
	now := time.Date(2026, 11, 19, 12, 0, 0, 123456789, time.UTC)
	cfg.now = func() time.Time { return now }
	s := newTestServer(t, cfg)
	h := s.Handler()
	private, key := hostIdentityTest(t)
	name := "Office తెలుగు PC"
	ch := hostChallengeTest(t, h, key, name)
	message, err := base64.StdEncoding.Strict().DecodeString(ch.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(string(message), "\n")
	if len(lines) != 9 || lines[0] != "OpenRemote.HostRegistration.v1" || lines[1] != cfg.PublicOrigin || lines[2] != ch.ID || lines[4] != key || lines[5] != name || lines[6] != "windows" || lines[7] != ch.ExpiresAt || lines[8] != "" || ch.Origin != cfg.PublicOrigin || len(ch.ID) != 24 || !safeID(ch.ID) || len(message) > 1024 {
		t.Fatalf("invalid transcript: %q", message)
	}
	nonce, err := base64.StdEncoding.Strict().DecodeString(lines[3])
	if err != nil || len(nonce) != 32 || base64.StdEncoding.EncodeToString(nonce) != lines[3] {
		t.Fatal("invalid nonce")
	}
	expires, err := time.Parse(time.RFC3339Nano, ch.ExpiresAt)
	if err != nil || !expires.Equal(now.Add(time.Minute)) {
		t.Fatal("wrong challenge lifetime")
	}
	if len(s.devices) != 0 || len(s.challenges) != 0 || len(s.hostRegistrationChallenges) != 1 {
		t.Fatal("challenge allocated host or controller state")
	}
	proof := hostProofTest(t, private, ch)
	r := call(t, h, "POST", "/v1/hosts/register", "", proof)
	wantCode(t, r, 201)
	registered := decode[enrolled](t, r)
	if registered.Token == "" || !safeID(registered.Device.ID) || registered.Device.PublicKey != key || registered.Device.Name != name || registered.Device.Platform != "windows" || !registered.Device.Online {
		t.Fatalf("incorrect registration: %+v", registered.Device)
	}
	if _, ok := displayID(registered.Device.DisplayID); !ok {
		t.Fatal("invalid stable display ID")
	}
	if !s.devices[registered.Device.ID].AutomaticRegistration || s.byPublicKey[key] != s.devices[registered.Device.ID] || len(s.sessions) != 0 || len(s.pairings) != 0 || len(s.hostRegistrationChallenges) != 0 {
		t.Fatal("registration did not establish only host ownership")
	}
	for _, secret := range []string{enrollmentSecret, "automaticRegistration", "registrationRevokedAt", ch.Challenge, proof["signature"]} {
		if strings.Contains(r.Body.String(), secret) {
			t.Fatal("response leaked server-only state or proof")
		}
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+registered.Device.ID+"/heartbeat", registered.Token, nil), 200)
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", proof), 401)
}

func TestAutomaticRegistrationDisabledByDefaultAndDiscovery(t *testing.T) {
	private, key := hostIdentityTest(t)
	s := newTestServer(t, testConfig())
	h := s.Handler()
	r := call(t, h, "GET", "/v1/info", "", nil)
	wantCode(t, r, 200)
	if decode[map[string]any](t, r)["automaticHostRegistrationVersion"] != float64(0) {
		t.Fatal("legacy deployment opted into public registration")
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/register/challenge", "", map[string]string{"publicKey": key, "name": "PC", "platform": "windows"}), 403)
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", map[string]string{"challengeId": strings.Repeat("a", 24), "signature": ""}), 403)
	if len(s.devices) != 0 {
		t.Fatal("disabled registration allocated host")
	}
	s = newTestServer(t, automaticRegistrationConfig())
	h = s.Handler()
	r = call(t, h, "GET", "/v1/info", "", nil)
	info := decode[map[string]any](t, r)
	if info["automaticHostRegistrationVersion"] != float64(1) || info["version"] != "0.8.0" {
		t.Fatal("automatic registration discovery mismatch")
	}
	registerHostTest(t, h, private, key)
}

func TestAutomaticRegistrationRejectsCopiedKeyAndBadProofs(t *testing.T) {
	cases := []string{"copied-public-key", "nonce-only", "prehashed", "wrong-purpose", "wrong-origin", "wrong-name", "noncanonical-signature", "short-signature"}
	for _, scenario := range cases {
		t.Run(scenario, func(t *testing.T) {
			s := newTestServer(t, automaticRegistrationConfig())
			h := s.Handler()
			private, key := hostIdentityTest(t)
			ch := hostChallengeTest(t, h, key, "Victim PC")
			message, _ := base64.StdEncoding.DecodeString(ch.Challenge)
			proof := hostProofTest(t, private, ch)
			switch scenario {
			case "copied-public-key":
				other, _ := hostIdentityTest(t)
				proof = hostProofTest(t, other, ch)
			case "nonce-only":
				lines := strings.Split(string(message), "\n")
				nonce, _ := base64.StdEncoding.DecodeString(lines[3])
				proof["signature"] = base64.StdEncoding.EncodeToString(ed25519.Sign(private, nonce))
			case "prehashed":
				digest := sha512.Sum512(message)
				proof["signature"] = base64.StdEncoding.EncodeToString(ed25519.Sign(private, digest[:]))
			case "wrong-purpose":
				proof["signature"] = base64.StdEncoding.EncodeToString(ed25519.Sign(private, bytes.Replace(message, []byte("OpenRemote.HostRegistration.v1"), []byte("OpenRemote.ControllerAuthentication.v1"), 1)))
			case "wrong-origin":
				proof["signature"] = base64.StdEncoding.EncodeToString(ed25519.Sign(private, bytes.Replace(message, []byte("https://remote.example.com"), []byte("https://other.example.com"), 1)))
			case "wrong-name":
				proof["signature"] = base64.StdEncoding.EncodeToString(ed25519.Sign(private, bytes.Replace(message, []byte("Victim PC"), []byte("Other PC"), 1)))
			case "noncanonical-signature":
				proof["signature"] += "\n"
			case "short-signature":
				proof["signature"] = base64.StdEncoding.EncodeToString(make([]byte, 32))
			}
			wrong := call(t, h, "POST", "/v1/hosts/register", "", proof)
			wantCode(t, wrong, 401)
			if wrong.Body.String() != "{\"error\":\"invalid_host_registration_proof\"}\n" {
				t.Fatal("proof failure leaked state")
			}
			wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch)), 401)
			if len(s.devices) != 0 || len(s.hostRegistrationChallenges) != 0 {
				t.Fatal("bad proof allocated a host or remained replayable")
			}
		})
	}
}

func TestAutomaticRegistrationExpiryAndCrossPurposeIsolation(t *testing.T) {
	cfg := automaticRegistrationConfig()
	now := time.Date(2026, 11, 19, 12, 0, 0, 0, time.UTC)
	cfg.now = func() time.Time { return now }
	s := newTestServer(t, cfg)
	h := s.Handler()
	private, key := hostIdentityTest(t)
	ch := hostChallengeTest(t, h, key, "PC")
	now = now.Add(time.Minute)
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch)), 401)
	controllerKey, controllerPublic := keyPair(t)
	controllerChallenge := challengeTest(t, h, controllerPublic)
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", map[string]string{"challengeId": controllerChallenge.ID, "signature": base64.StdEncoding.EncodeToString(make([]byte, 64))}), 401)
	wantCode(t, call(t, h, "POST", "/v1/auth/verify", "", proof(t, controllerKey, controllerChallenge)), 200)
	ch = hostChallengeTest(t, h, key, "PC")
	wantCode(t, call(t, h, "POST", "/v1/auth/verify", "", hostProofTest(t, private, ch)), 401)
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch)), 201)
}

func TestAutomaticRegistrationRecoversSameIdentityAndRevokesOldAccess(t *testing.T) {
	s := newTestServer(t, automaticRegistrationConfig())
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	auth, controllerKey := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, controllerKey)
	wantCode(t, call(t, h, "POST", "/v1/sessions/"+ss.Session.ID+"/approve", host.Token, nil), 200)
	pair := createPairingTest(t, h, host, testPairingPIN)
	ch := hostChallengeTest(t, h, key, "Renamed PC")
	r := call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch))
	wantCode(t, r, 200)
	recovered := decode[enrolled](t, r)
	if recovered.Device.ID != host.Device.ID || recovered.Device.DisplayID != host.Device.DisplayID || recovered.Device.CreatedAt != host.Device.CreatedAt || recovered.Device.Name != "Renamed PC" || recovered.Token == host.Token || len(s.devices) != 1 {
		t.Fatal("recovery changed identity or reused bearer")
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 401)
	wantCode(t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, host.Token, nil), 401)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", recovered.Token, nil), 200)
	status := decode[sessionResult](t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil)).Session
	if status.Status != "ended" || status.EndedAt == nil {
		t.Fatal("recovery left old session live")
	}
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, pair.Code, testPairingPIN))
	if len(s.devices[host.Device.ID].Events) != 0 || len(s.pairings) != 0 {
		t.Fatal("recovery retained stale pending payload or local code")
	}
}

func TestAutomaticRegistrationCopiedKeyCannotRecoverExistingHost(t *testing.T) {
	s := newTestServer(t, automaticRegistrationConfig())
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	attacker, _ := hostIdentityTest(t)
	ch := hostChallengeTest(t, h, key, "Attacker PC")
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, attacker, ch)), 401)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
	if len(s.devices) != 1 || s.devices[host.Device.ID].Name != host.Device.Name {
		t.Fatal("copied public key changed owner record")
	}
}

func TestAutomaticRegistrationLegacyHostCannotBeRecovered(t *testing.T) {
	s := newTestServer(t, automaticRegistrationConfig())
	h := s.Handler()
	private, key := hostIdentityTest(t)
	r := call(t, h, "POST", "/v1/devices/enroll", enrollmentSecret, map[string]string{"name": "Legacy PC", "publicKey": key, "platform": "windows"})
	wantCode(t, r, 201)
	legacy := decode[enrolled](t, r)
	ch := hostChallengeTest(t, h, key, "New name")
	r = call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch))
	wantCode(t, r, 403)
	if r.Body.String() != "{\"error\":\"host_registration_unavailable\"}\n" {
		t.Fatal("wrong legacy recovery failure")
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+legacy.Device.ID+"/heartbeat", legacy.Token, nil), 200)
	if s.devices[legacy.Device.ID].AutomaticRegistration || s.devices[legacy.Device.ID].Name != "Legacy PC" {
		t.Fatal("legacy record converted to public recovery")
	}
	// The old admin API must not admit the same raw key through alternate base64 spelling.
	wantCode(t, call(t, h, "POST", "/v1/devices/enroll", enrollmentSecret, map[string]string{"name": "Duplicate", "publicKey": key + "\n", "platform": "windows"}), 409)
}

func TestAutomaticRegistrationConcurrentProofHasOneWinner(t *testing.T) {
	for _, sameChallenge := range []bool{true, false} {
		t.Run(map[bool]string{true: "same challenge", false: "distinct stale challenges"}[sameChallenge], func(t *testing.T) {
			s := newTestServer(t, automaticRegistrationConfig())
			h := s.Handler()
			private, key := hostIdentityTest(t)
			ch := hostChallengeTest(t, h, key, "PC")
			proofs := make([]map[string]string, 8)
			for i := range proofs {
				if !sameChallenge {
					ch = hostChallengeTest(t, h, key, "PC")
				}
				proofs[i] = hostProofTest(t, private, ch)
			}
			start := make(chan struct{})
			out := make(chan *httptest.ResponseRecorder, len(proofs))
			var wg sync.WaitGroup
			for _, proof := range proofs {
				wg.Add(1)
				go func(p map[string]string) {
					defer wg.Done()
					<-start
					out <- call(t, h, "POST", "/v1/hosts/register", "", p)
				}(proof)
			}
			close(start)
			wg.Wait()
			close(out)
			wins := 0
			var winner enrolled
			for r := range out {
				if r.Code == 201 {
					wins++
					winner = decode[enrolled](t, r)
				} else {
					wantCode(t, r, 401)
				}
			}
			if wins != 1 || len(s.devices) != 1 {
				t.Fatal("concurrent proofs allocated or rotated multiple identities")
			}
			wantCode(t, call(t, h, "POST", "/v1/hosts/"+winner.Device.ID+"/heartbeat", winner.Token, nil), 200)
		})
	}
}

func TestAutomaticRegistrationStaleProofCannotUndoRecovery(t *testing.T) {
	s := newTestServer(t, automaticRegistrationConfig())
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	old := hostChallengeTest(t, h, key, "Old request")
	fresh := hostChallengeTest(t, h, key, "Fresh request")
	r := call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, fresh))
	wantCode(t, r, 200)
	recovered := decode[enrolled](t, r)
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, old)), 401)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", recovered.Token, nil), 200)
	if s.devices[host.Device.ID].Name != "Fresh request" {
		t.Fatal("stale proof overwrote recovered host")
	}
}

func TestAutomaticRegistrationRevocationIsDurableAndAdminOnly(t *testing.T) {
	cfg := automaticRegistrationConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	auth, controllerKey := loginTest(t, h)
	ss := sessionTest(t, h, host, auth, controllerKey)
	pair := createPairingTest(t, h, host, testPairingPIN)
	path := "/v1/admin/hosts/" + host.Device.ID + "/revoke"
	for _, bearer := range []string{"", host.Token, auth} {
		wantCode(t, call(t, h, "POST", path, bearer, map[string]any{}), 401)
	}
	before := hostChallengeTest(t, h, key, "Before revoke")
	wantCode(t, call(t, h, "POST", path, enrollmentSecret, map[string]any{}), 200)
	wantCode(t, call(t, h, "POST", path, enrollmentSecret, map[string]any{}), 200)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 401)
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, before)), 403)
	expectPairingUnavailable(t, redeemPairingTest(t, h, auth, pair.Code, testPairingPIN))
	if decode[sessionResult](t, call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, ss.Token, nil)).Session.Status != "ended" {
		t.Fatal("revocation left session pending")
	}
	if decode[Device](t, call(t, h, "GET", "/v1/devices/"+host.Device.DisplayID, auth, nil)).Online {
		t.Fatal("revoked host reported online")
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	s = newTestServer(t, cfg)
	h = s.Handler()
	if s.devices[host.Device.ID].RegistrationRevokedAt == nil || !s.devices[host.Device.ID].AutomaticRegistration {
		t.Fatal("revocation/origin not durable")
	}
	ch := hostChallengeTest(t, h, key, "After restart")
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch)), 403)
	wantCode(t, call(t, h, "POST", "/v1/devices/enroll", enrollmentSecret, map[string]string{"name": "New PC", "platform": "windows", "publicKey": key}), 409)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 401)
}

func TestAutomaticRegistrationRecoverySurvivesRestartWithoutPersistedProof(t *testing.T) {
	cfg := automaticRegistrationConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	ch := hostChallengeTest(t, h, key, "Pending proof")
	proof := hostProofTest(t, private, ch)
	raw, err := os.ReadFile(cfg.StatePath)
	if err != nil {
		t.Fatal(err)
	}
	for _, secret := range []string{host.Token, ch.ID, ch.Challenge, proof["signature"], enrollmentSecret} {
		if bytes.Contains(raw, []byte(secret)) {
			t.Fatal("registration persisted bearer/proof/challenge")
		}
	}
	var disk snapshot
	if json.Unmarshal(raw, &disk) != nil || disk.Version != 2 || len(disk.Devices) != 1 || !disk.Devices[0].AutomaticRegistration {
		t.Fatal("missing versioned durable ownership flag")
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	s = newTestServer(t, cfg)
	h = s.Handler()
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", proof), 401)
	ch = hostChallengeTest(t, h, key, "Recovered PC")
	r := call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch))
	wantCode(t, r, 200)
	recovered := decode[enrolled](t, r)
	if recovered.Device.ID != host.Device.ID || recovered.Device.DisplayID != host.Device.DisplayID || len(s.devices) != 1 {
		t.Fatal("restart recovery allocated a second identity")
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 401)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", recovered.Token, nil), 200)
}

func TestAutomaticRegistrationLegacySnapshotMigrationCannotGrantRecovery(t *testing.T) {
	cfg := automaticRegistrationConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	private, key := hostIdentityTest(t)
	r := call(t, h, "POST", "/v1/devices/enroll", enrollmentSecret, map[string]string{"name": "Legacy PC", "platform": "windows", "publicKey": key})
	wantCode(t, r, 201)
	host := decode[enrolled](t, r)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	raw, _ := os.ReadFile(cfg.StatePath)
	var disk snapshot
	if err := json.Unmarshal(raw, &disk); err != nil {
		t.Fatal(err)
	}
	disk.Version = 1
	raw, _ = json.Marshal(disk)
	if err := os.WriteFile(cfg.StatePath, raw, 0600); err != nil {
		t.Fatal(err)
	}
	s = newTestServer(t, cfg)
	h = s.Handler()
	ch := hostChallengeTest(t, h, key, "Public claim")
	wantCode(t, call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch)), 403)
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
	enrollTest(t, h)
	raw, _ = os.ReadFile(cfg.StatePath)
	if err := json.Unmarshal(raw, &disk); err != nil || disk.Version != 2 {
		t.Fatal("old snapshot was not migrated to explicit version2")
	}
}

func TestAutomaticRegistrationRejectsAmbiguousDurableIdentity(t *testing.T) {
	for _, scenario := range []string{"legacy-auto-flag", "duplicate-public-key"} {
		t.Run(scenario, func(t *testing.T) {
			cfg := automaticRegistrationConfig()
			cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
			s := newTestServer(t, cfg)
			h := s.Handler()
			enrollTest(t, h)
			enrollTest(t, h)
			if err := s.Close(); err != nil {
				t.Fatal(err)
			}
			raw, _ := os.ReadFile(cfg.StatePath)
			var disk snapshot
			if err := json.Unmarshal(raw, &disk); err != nil {
				t.Fatal(err)
			}
			if scenario == "legacy-auto-flag" {
				disk.Version = 1
				disk.Devices[0].AutomaticRegistration = true
			} else {
				disk.Devices[1].Device.PublicKey = disk.Devices[0].Device.PublicKey + "\n"
			}
			raw, _ = json.Marshal(disk)
			if err := os.WriteFile(cfg.StatePath, raw, 0600); err != nil {
				t.Fatal(err)
			}
			if restored, err := NewWithConfig(nil, cfg); err == nil {
				_ = restored.Close()
				t.Fatal("ambiguous identity snapshot accepted")
			}
		})
	}
}

func TestAutomaticRegistrationCapacityAndBudgets(t *testing.T) {
	for _, scenario := range []string{"challenge-ip", "challenge-global", "challenge-capacity", "proof-ip", "proof-global", "owner", "device-capacity"} {
		t.Run(scenario, func(t *testing.T) {
			cfg := automaticRegistrationConfig()
			switch scenario {
			case "challenge-ip":
				cfg.HostRegistrationChallengeIPRate = 1
			case "challenge-global":
				cfg.HostRegistrationChallengeGlobalRate = 1
			case "challenge-capacity":
				cfg.MaxHostRegistrationChallenges = 1
			case "proof-ip":
				cfg.HostRegistrationProofIPRate = 1
			case "proof-global":
				cfg.HostRegistrationProofGlobalRate = 1
			case "owner":
				cfg.HostRegistrationRecoveryRate = 1
			case "device-capacity":
				cfg.MaxDevices = 1
			}
			s := newTestServer(t, cfg)
			h := s.Handler()
			private, key := hostIdentityTest(t)
			ch := hostChallengeTest(t, h, key, "PC")
			if strings.HasPrefix(scenario, "challenge-") {
				ip := "192.0.2.1"
				if scenario == "challenge-global" {
					ip = "203.0.113.1"
				}
				r := registrationCallIP(t, h, "/v1/hosts/register/challenge", ip, map[string]string{"publicKey": key, "name": "PC", "platform": "windows"})
				code := 429
				if scenario == "challenge-capacity" {
					code = 503
				}
				wantCode(t, r, code)
				return
			}
			first := call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch))
			wantCode(t, first, 201)
			host := decode[enrolled](t, first)
			if scenario != "owner" {
				private, key = hostIdentityTest(t)
			}
			ch = hostChallengeTest(t, h, key, "Second PC")
			ip := "192.0.2.1"
			if scenario == "proof-global" {
				ip = "203.0.113.9"
			}
			r := registrationCallIP(t, h, "/v1/hosts/register", ip, hostProofTest(t, private, ch))
			code := 429
			if scenario == "device-capacity" {
				code = 503
			}
			wantCode(t, r, code)
			if code == 429 && (r.Header().Get("Retry-After") != "60" || r.Body.String() != "{\"error\":\"host_registration_rate_limited\"}\n") {
				t.Fatal("wrong rate failure")
			}
			wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
			if len(s.devices) != 1 {
				t.Fatal("budget allocated another identity")
			}
		})
	}
}

func TestAutomaticRegistrationAtCapacityStillRecoversOwner(t *testing.T) {
	cfg := automaticRegistrationConfig()
	cfg.MaxDevices = 1
	s := newTestServer(t, cfg)
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	ch := hostChallengeTest(t, h, key, "Recovered at capacity")
	r := call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch))
	wantCode(t, r, 200)
	if decode[enrolled](t, r).Device.ID != host.Device.ID {
		t.Fatal("capacity recovery changed identity")
	}
}

func TestAutomaticRegistrationStorageFailureFailsClosed(t *testing.T) {
	for _, scenario := range []string{"new", "recovery", "revoke"} {
		t.Run(scenario, func(t *testing.T) {
			cfg := automaticRegistrationConfig()
			cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
			s := newTestServer(t, cfg)
			h := s.Handler()
			private, key := hostIdentityTest(t)
			var host enrolled
			if scenario != "new" {
				host = registerHostTest(t, h, private, key)
			}
			ch := hostChallengeTest(t, h, key, "PC")
			if err := os.Remove(cfg.StatePath); err != nil && !os.IsNotExist(err) {
				t.Fatal(err)
			}
			if err := os.Mkdir(cfg.StatePath, 0700); err != nil {
				t.Fatal(err)
			}
			var r *httptest.ResponseRecorder
			if scenario == "revoke" {
				r = call(t, h, "POST", "/v1/admin/hosts/"+host.Device.ID+"/revoke", enrollmentSecret, map[string]any{})
			} else {
				r = call(t, h, "POST", "/v1/hosts/register", "", hostProofTest(t, private, ch))
			}
			wantCode(t, r, 503)
			if strings.Contains(r.Body.String(), "deviceToken") || s.storageErr == nil {
				t.Fatal("failed durable transition returned credential or failed to latch")
			}
			wantCode(t, call(t, h, "GET", "/v1/info", "", nil), 503)
		})
	}
}

func TestAutomaticRegistrationCanceledProofDoesNotRotate(t *testing.T) {
	s := newTestServer(t, automaticRegistrationConfig())
	h := s.Handler()
	private, key := hostIdentityTest(t)
	host := registerHostTest(t, h, private, key)
	ch := hostChallengeTest(t, h, key, "Canceled")
	body, _ := json.Marshal(hostProofTest(t, private, ch))
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	r := httptest.NewRequest("POST", "/v1/hosts/register", bytes.NewReader(body)).WithContext(ctx)
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, r)
	if len(s.devices) != 1 || s.devices[host.Device.ID].Name != host.Device.Name {
		t.Fatal("canceled proof changed host")
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/"+host.Device.ID+"/heartbeat", host.Token, nil), 200)
}

func TestAutomaticRegistrationValidatesOriginAndIgnoresHeaders(t *testing.T) {
	invalid := []string{"", "http://remote.example.com", "https://remote.example.com/", "https://REMOTE.example.com", "HTTPS://remote.example.com", "https://user@remote.example.com", "https://remote.example.com/path", "https://remote.example.com?", "https://remote.example.com#fragment", "https://remote.example.com:443", "https://remote.example.com:0444", "https://remote.example.com:0", "https://remote.example.com:65536", "https://127.1", "https://2130706433", "https://0x7f.0.0.1", "https://remote%2eexample.com", "https://remote.example.com\\evil", "https://[fe80::1%25eth0]", "https://remote.example.com\n"}
	for _, value := range invalid {
		if err := validateRegistrationOrigin(value, false); err == nil {
			t.Fatalf("accepted invalid configured origin %q", value)
		}
	}
	for _, value := range []string{"https://remote.example.com", "https://remote.example.com:8443", "https://0xray.example.com", "https://192.0.2.9", "https://[2001:db8::1]:8443"} {
		if err := validateRegistrationOrigin(value, false); err != nil {
			t.Fatalf("rejected valid origin %q: %v", value, err)
		}
	}
	for _, value := range []string{"http://127.0.0.1:8080", "http://localhost:8080", "http://[::1]:8080"} {
		if err := validateRegistrationOrigin(value, true); err != nil {
			t.Fatal(err)
		}
	}
	if validateRegistrationOrigin("http://remote.example.com", true) == nil {
		t.Fatal("development mode allowed non-loopback origin")
	}
	cfg := automaticRegistrationConfig()
	cfg.PublicOrigin = ""
	if server, err := NewWithConfig(nil, cfg); err == nil {
		_ = server.Close()
		t.Fatal("automatic registration started without configured origin")
	}
	cfg = automaticRegistrationConfig()
	cfg.TrustedProxyCIDRs = []string{"192.0.2.0/24"}
	s := newTestServer(t, cfg)
	_, key := hostIdentityTest(t)
	body, _ := json.Marshal(map[string]string{"publicKey": key, "name": "PC", "platform": "windows"})
	r := httptest.NewRequest("POST", "https://attacker.example/v1/hosts/register/challenge", bytes.NewReader(body))
	r.Header.Set("X-Forwarded-Host", "other-attacker.example")
	r.Header.Set("Forwarded", "host=attacker.example;proto=https")
	rr := httptest.NewRecorder()
	s.Handler().ServeHTTP(rr, r)
	wantCode(t, rr, 201)
	ch := decode[hostRegistrationReply](t, rr)
	if ch.Origin != cfg.PublicOrigin {
		t.Fatal("request headers chose signed origin")
	}
	decoded, _ := base64.StdEncoding.DecodeString(ch.Challenge)
	if strings.Contains(string(decoded), "attacker.example") {
		t.Fatal("attacker host entered signed transcript")
	}
}

func TestAutomaticRegistrationRejectsMalformedRequestsAndUsesSensitiveLimit(t *testing.T) {
	s := newTestServer(t, automaticRegistrationConfig())
	h := s.Handler()
	_, key := hostIdentityTest(t)
	for _, name := range []string{"", " PC", "PC ", "line\nbreak", strings.Repeat("a", 81), "PC\u0085"} {
		wantCode(t, call(t, h, "POST", "/v1/hosts/register/challenge", "", map[string]string{"publicKey": key, "name": name, "platform": "windows"}), 400)
	}
	for _, public := range []string{"", key + "\n", strings.TrimRight(key, "="), base64.StdEncoding.EncodeToString(make([]byte, 31))} {
		wantCode(t, call(t, h, "POST", "/v1/hosts/register/challenge", "", map[string]string{"publicKey": public, "name": "PC", "platform": "windows"}), 400)
	}
	wantCode(t, call(t, h, "POST", "/v1/hosts/register/challenge", "", map[string]string{"publicKey": key, "name": "PC", "platform": "linux"}), 400)
	for _, path := range []string{"/v1/hosts/register/challenge", "/v1/hosts/register"} {
		wantCode(t, call(t, h, "GET", path, "", nil), 405)
		wantCode(t, call(t, h, "POST", path+"?token=secret", "", map[string]any{}), 400)
	}
	request := httptest.NewRequest("POST", "/v1/hosts/register/challenge", strings.NewReader(`{"publicKey":"`+key+`","publicKey":"`+key+`","name":"PC","platform":"windows"}`))
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, request)
	wantCode(t, rr, 400)
	for _, path := range []string{"/v1/hosts/register/challenge", "/v1/hosts/register", "/v1/admin/hosts/" + strings.Repeat("a", 24) + "/revoke"} {
		cfg := automaticRegistrationConfig()
		cfg.SensitiveRatePerMinute = 1
		limited := newTestServer(t, cfg).Handler()
		call(t, limited, "POST", path, "", map[string]any{})
		wantCode(t, call(t, limited, "POST", path, "", map[string]any{}), 429)
	}
	secure := automaticRegistrationConfig()
	secure.AllowInsecure = false
	wantCode(t, call(t, newTestServer(t, secure).Handler(), "POST", "/v1/hosts/register/challenge", "", map[string]string{"publicKey": key, "name": "PC", "platform": "windows"}), 426)
}

func TestAutomaticRegistrationExpiredChallengesFreeCapacity(t *testing.T) {
	cfg := automaticRegistrationConfig()
	cfg.MaxHostRegistrationChallenges = 1
	now := time.Date(2026, 11, 19, 12, 0, 0, 0, time.UTC)
	cfg.now = func() time.Time { return now }
	s := newTestServer(t, cfg)
	h := s.Handler()
	_, key := hostIdentityTest(t)
	hostChallengeTest(t, h, key, "PC")
	now = now.Add(time.Minute)
	hostChallengeTest(t, h, key, "PC")
	if len(s.hostRegistrationChallenges) != 1 {
		t.Fatal("expired challenge retained capacity")
	}
}

func TestAutomaticRegistrationDurablePublicKeysCanonicalizeLegacySpelling(t *testing.T) {
	cfg := automaticRegistrationConfig()
	cfg.StatePath = filepath.Join(t.TempDir(), "state.json")
	s := newTestServer(t, cfg)
	h := s.Handler()
	host := enrollTest(t, h)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	raw, _ := os.ReadFile(cfg.StatePath)
	var disk snapshot
	if err := json.Unmarshal(raw, &disk); err != nil {
		t.Fatal(err)
	}
	disk.Version = 1
	disk.Devices[0].Device.PublicKey += "\n"
	raw, _ = json.Marshal(disk)
	if err := os.WriteFile(cfg.StatePath, raw, 0600); err != nil {
		t.Fatal(err)
	}
	s = newTestServer(t, cfg)
	if s.byPublicKey[host.Device.PublicKey] == nil || s.devices[host.Device.ID].PublicKey != host.Device.PublicKey || s.devices[host.Device.ID].AutomaticRegistration {
		t.Fatal("legacy key normalization changed registration ownership")
	}
}
