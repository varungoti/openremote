package app

import (
	"crypto/pbkdf2"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/json"
	"fmt"
	"math/big"
	"net"
	"net/http"
	"strings"
	"time"
)

const (
	pairingPINIterations = 600_000
	pairingAttemptLimit  = 5
	activationMode       = "activation"
	activationTTL        = 24 * time.Hour
	activationCapacity   = 4
)

// A grant never enters a snapshot. The local host separately retains the
// opaque ID: a server record alone cannot trigger local automatic approval.
type pairingGrant struct {
	ID          string
	Code        string
	HostID      string
	Mode        string
	ExpiresAt   time.Time
	Permissions PermissionSet
	Salt        [32]byte
	PINHash     [32]byte
	Failures    int
	InFlight    int
}

type pairingPublic struct {
	ID             string        `json:"pairingId"`
	Code           string        `json:"code"`
	ExpiresAt      time.Time     `json:"expiresAt"`
	Permissions    PermissionSet `json:"permissions"`
	Mode           string        `json:"mode,omitempty"`
	MaxConnections int           `json:"maxConnections,omitempty"`
}

func (p *pairingGrant) public() pairingPublic {
	v := pairingPublic{ID: p.ID, Code: p.Code, ExpiresAt: p.ExpiresAt, Permissions: p.Permissions, Mode: p.Mode}
	if p.Mode == activationMode {
		v.MaxConnections = activationCapacity
	}
	return v
}

// Absence preserves the legacy one-use protocol. Explicit empty/null/unknown
// values must not silently downgrade an owner's reusable activation request.
func optionalPairingMode(raw json.RawMessage) (string, bool) {
	if len(raw) == 0 {
		return "", true
	}
	var mode string
	if json.Unmarshal(raw, &mode) != nil || mode != activationMode {
		return "", false
	}
	return mode, true
}

// An activation owns its pending and live sessions. Passing an empty pairingID
// is used by owner Stop so cleanup still works after a code expired or locked.
func (s *Server) endActivationSessionsLocked(hostID, pairingID, event string) bool {
	changed := false
	for _, v := range s.sessions {
		if v.HostDeviceID == hostID && v.Mode == activationMode && v.EndedAt == nil && (pairingID == "" || v.PairingID == pairingID) {
			s.endLocked(v, "ended", event)
			changed = true
		}
	}
	return changed
}

func (s *Server) hasLegacySessionsLocked(hostID string) bool {
	for _, v := range s.sessions {
		if v.HostDeviceID == hostID && v.EndedAt == nil && v.Mode != activationMode {
			return true
		}
	}
	return false
}

func numeric(value string, min, max int) bool {
	if len(value) < min || len(value) > max {
		return false
	}
	for _, c := range value {
		if c < '0' || c > '9' {
			return false
		}
	}
	return true
}

func pairingRequest(r *http.Request) bool {
	return r.URL.Path == "/v1/pairings/redeem" || strings.HasPrefix(r.URL.Path, "/v1/hosts/") && strings.HasSuffix(r.URL.Path, "/pairing")
}

// A gateway override exists only after middleware authentication. Its client
// address changes rate identity, never the immediate peer's HTTPS trust. The
// controller's separate identity bucket remains tied to its proved public key.
func (s *Server) requestClient(r *http.Request) (string, bool) {
	ip, trusted := s.requestPeer(r)
	if authenticated, ok := r.Context().Value(gatewayClientIPKey{}).(string); ok {
		return authenticated, trusted
	}
	return ip, trusted
}

// Only an explicitly trusted immediate peer may supply its rightmost XFF hop.
func (s *Server) requestPeer(r *http.Request) (string, bool) {
	ip, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		ip = r.RemoteAddr
	}
	peer := net.ParseIP(ip)
	if peer != nil {
		ip = peer.String()
	}
	for _, proxy := range s.proxies {
		if proxy.Contains(peer) {
			hops := strings.Split(r.Header.Get("X-Forwarded-For"), ",")
			candidate := net.ParseIP(strings.TrimSpace(hops[len(hops)-1]))
			if candidate != nil {
				return candidate.String(), true
			}
			return ip, true
		}
	}
	return ip, false
}

func (s *Server) pairingClientIP(r *http.Request) string {
	ip, _ := s.requestClient(r)
	return ip
}

func pairingLimited(w http.ResponseWriter) {
	w.Header().Set("Retry-After", "60")
	fail(w, http.StatusTooManyRequests, "pairing_rate_limited")
}

func (s *Server) pairingHealthyLocked(w http.ResponseWriter) bool {
	if s.closed || s.storageErr != nil {
		fail(w, http.StatusServiceUnavailable, "storage_unavailable")
		return false
	}
	return true
}

func (s *Server) reservePairingWorkerLocked() bool {
	select {
	case s.pairingWorkers <- struct{}{}:
		return true
	default:
		return false
	}
}

func derivePairingPIN(pin string, salt [32]byte) ([32]byte, error) {
	var out [32]byte
	derived, err := pbkdf2.Key(sha256.New, pin, salt[:], pairingPINIterations, len(out))
	if err != nil {
		return out, err
	}
	copy(out[:], derived)
	clear(derived)
	return out, nil
}

func (s *Server) removePairingLocked(p *pairingGrant) {
	if p == nil || s.pairings[p.Code] != p {
		return
	}
	delete(s.pairings, p.Code)
	if s.hostPairing[p.HostID] == p.Code {
		delete(s.hostPairing, p.HostID)
	}
}

func (s *Server) currentPairingLocked(hostID string) *pairingGrant {
	p := s.pairings[s.hostPairing[hostID]]
	if p != nil && (!s.now().Before(p.ExpiresAt) || p.Failures >= pairingAttemptLimit) {
		s.removePairingLocked(p)
		return nil
	}
	return p
}

func (s *Server) hostPairingRoutes(w http.ResponseWriter, r *http.Request, hostID string) {
	if r.URL.RawQuery != "" {
		fail(w, http.StatusBadRequest, "pairing_query_not_allowed")
		return
	}
	if r.Method != "GET" && r.Method != "POST" && r.Method != "DELETE" {
		w.Header().Set("Allow", "GET, POST, DELETE")
		fail(w, http.StatusMethodNotAllowed, "method_not_allowed")
		return
	}
	s.mu.Lock()
	if !s.pairingHealthyLocked(w) {
		s.mu.Unlock()
		return
	}
	host, ok := s.hostLocked(r, hostID)
	if !ok {
		s.mu.Unlock()
		fail(w, http.StatusUnauthorized, "host_auth_required")
		return
	}
	switch r.Method {
	case "GET":
		var value *pairingPublic
		if p := s.currentPairingLocked(hostID); p != nil {
			v := p.public()
			value = &v
		}
		s.mu.Unlock()
		writeJSON(w, http.StatusOK, map[string]any{"pairing": value})
		return
	case "DELETE":
		// Invalidate a creation currently hashing outside the mutex as well as
		// the displayed code. An old response must never resurrect revocation.
		s.pairingGeneration[hostID]++
		changed := s.endActivationSessionsLocked(hostID, "", "activation.deactivated")
		if p := s.currentPairingLocked(hostID); p != nil {
			s.removePairingLocked(p)
			s.addAuditLocked(hostID, "", "pairing.revoked")
			changed = true
		}
		if changed && s.persistLocked() != nil {
			s.mu.Unlock()
			fail(w, http.StatusServiceUnavailable, "storage_unavailable")
			return
		}
		s.mu.Unlock()
		writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
		return
	}
	var in struct {
		PIN         string          `json:"pin"`
		Permissions PermissionSet   `json:"permissions"`
		Mode        json.RawMessage `json:"mode,omitempty"`
	}
	if readJSON(w, r, &in) != nil {
		s.mu.Unlock()
		fail(w, http.StatusBadRequest, "invalid_json")
		return
	}
	mode, validMode := optionalPairingMode(in.Mode)
	maxPIN := 12
	if mode == activationMode {
		maxPIN = 6
	}
	if !validMode || !numeric(in.PIN, 6, maxPIN) || !in.Permissions.ViewScreen || !in.Permissions.subsetOf(PermissionSet{ViewScreen: true, ControlInput: true}) {
		s.mu.Unlock()
		fail(w, http.StatusBadRequest, "invalid_pairing_request")
		return
	}
	now := s.now()
	if !s.limitLocked("pairing-create-host:"+hostID, s.cfg.PairingHostCreateRate, now) ||
		!s.limitLocked("pairing-create-ip:"+s.pairingClientIP(r), s.cfg.PairingIPRate, now) ||
		!s.limitLocked("pairing-create-global", s.cfg.PairingGlobalCreateRate, now) ||
		!s.reservePairingWorkerLocked() {
		s.mu.Unlock()
		pairingLimited(w)
		return
	}
	if !s.publicDeviceLocked(host).Online {
		<-s.pairingWorkers
		s.mu.Unlock()
		fail(w, http.StatusConflict, "device_offline")
		return
	}
	s.pairingGeneration[hostID]++
	generation := s.pairingGeneration[hostID]
	s.mu.Unlock()
	defer func() { <-s.pairingWorkers }()

	var salt [32]byte
	if _, err := rand.Read(salt[:]); err != nil {
		fail(w, http.StatusInternalServerError, "entropy_unavailable")
		return
	}
	id, err := token(24)
	if err != nil {
		fail(w, http.StatusInternalServerError, "entropy_unavailable")
		return
	}
	pinHash, err := derivePairingPIN(in.PIN, salt)
	in.PIN = ""
	if err != nil {
		fail(w, http.StatusInternalServerError, "pin_verifier_unavailable")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.pairingHealthyLocked(w) {
		return
	}
	if r.Context().Err() != nil {
		return
	}
	if s.pairingGeneration[hostID] != generation {
		fail(w, http.StatusConflict, "pairing_request_superseded")
		return
	}
	if mode == activationMode && s.hasLegacySessionsLocked(hostID) {
		fail(w, http.StatusConflict, "host_busy")
		return
	}
	// The nonce and verifier are ready before the old code is removed, so an
	// entropy or KDF error leaves the previously displayed code intact.
	var code string
	for attempt := 0; attempt < 32; attempt++ {
		n, err := rand.Int(rand.Reader, big.NewInt(1_000_000))
		if err != nil {
			fail(w, http.StatusInternalServerError, "entropy_unavailable")
			return
		}
		candidate := fmt.Sprintf("%06d", n.Int64())
		if s.pairings[candidate] == nil {
			code = candidate
			break
		}
	}
	if code == "" {
		fail(w, http.StatusServiceUnavailable, "pairing_capacity")
		return
	}
	s.endActivationSessionsLocked(hostID, "", "activation.replaced")
	s.removePairingLocked(s.currentPairingLocked(hostID))
	ttl := s.cfg.PairingTTL
	if mode == activationMode {
		ttl = activationTTL
	}
	p := &pairingGrant{ID: id, Code: code, HostID: hostID, Mode: mode, ExpiresAt: s.now().Add(ttl), Permissions: in.Permissions, Salt: salt, PINHash: pinHash}
	s.pairings[code] = p
	s.hostPairing[hostID] = code
	s.addAuditLocked(hostID, "", "pairing.created")
	if s.persistLocked() != nil {
		s.removePairingLocked(p)
		fail(w, http.StatusServiceUnavailable, "storage_unavailable")
		return
	}
	writeJSON(w, http.StatusCreated, p.public())
}

func (s *Server) redeemPairing(w http.ResponseWriter, r *http.Request) {
	if !method(w, r, "POST") {
		return
	}
	if r.URL.RawQuery != "" {
		fail(w, http.StatusBadRequest, "pairing_query_not_allowed")
		return
	}
	var in struct {
		Code           string `json:"code"`
		PIN            string `json:"pin"`
		ControllerName string `json:"controllerName"`
	}
	if readJSON(w, r, &in) != nil {
		fail(w, http.StatusBadRequest, "invalid_json")
		return
	}
	if !(numeric(in.Code, 6, 6) || numeric(in.Code, 9, 9)) || !numeric(in.PIN, 6, 12) || !boundedName(in.ControllerName) {
		fail(w, http.StatusBadRequest, "invalid_pairing_request")
		return
	}
	s.mu.Lock()
	if !s.pairingHealthyLocked(w) {
		s.mu.Unlock()
		return
	}
	ctl, ok := s.controllerLocked(r)
	if !ok {
		s.mu.Unlock()
		fail(w, http.StatusUnauthorized, "controller_auth_required")
		return
	}
	now := s.now()
	if !s.limitLocked("pairing-redeem-controller:"+ctl.PublicKey, s.cfg.PairingControllerRate, now) ||
		!s.limitLocked("pairing-redeem-ip:"+s.pairingClientIP(r), s.cfg.PairingIPRate, now) ||
		!s.limitLocked("pairing-redeem-global", s.cfg.PairingGlobalRate, now) ||
		!s.reservePairingWorkerLocked() {
		s.mu.Unlock()
		pairingLimited(w)
		return
	}
	p := s.pairings[in.Code]
	if len(in.Code) == 9 {
		// A stable public PC ID selects only the currently owner-enabled
		// activation. Unknown, stopped and legacy-only hosts take the same
		// dummy-verifier path and generic response as an unknown short code.
		if host := s.byDisplay[in.Code]; host != nil {
			candidate := s.pairings[s.hostPairing[host.ID]]
			if candidate != nil && candidate.Mode == activationMode {
				p = candidate
			}
		}
	}
	salt, expected := s.dummyPairingSalt, s.dummyPairingHash
	var reserved *pairingGrant
	if p != nil && now.Before(p.ExpiresAt) && p.Failures+p.InFlight < pairingAttemptLimit {
		p.InFlight++
		reserved = p
		salt, expected = p.Salt, p.PINHash
	}
	s.mu.Unlock()
	defer func() { <-s.pairingWorkers }()
	// Unknown, expired and already-consumed codes incur the same KDF work as
	// an eligible known code. Neither status nor response details enumerate it.
	actual, err := derivePairingPIN(in.PIN, salt)
	in.PIN = ""
	matched := subtle.ConstantTimeCompare(actual[:], expected[:]) == 1
	clear(actual[:])
	s.mu.Lock()
	defer s.mu.Unlock()
	if reserved != nil {
		reserved.InFlight--
	}
	if !s.pairingHealthyLocked(w) {
		return
	}
	if err != nil {
		fail(w, http.StatusInternalServerError, "pin_verifier_unavailable")
		return
	}
	if reserved == nil || s.pairings[reserved.Code] != reserved || !s.now().Before(reserved.ExpiresAt) {
		fail(w, http.StatusForbidden, "pairing_unavailable")
		return
	}
	if !matched {
		reserved.Failures++
		if reserved.Failures >= pairingAttemptLimit {
			s.removePairingLocked(reserved)
			if reserved.Mode == activationMode {
				s.endActivationSessionsLocked(reserved.HostID, reserved.ID, "activation.locked")
				s.addAuditLocked(reserved.HostID, "", "activation.locked")
				if s.persistLocked() != nil {
					fail(w, http.StatusServiceUnavailable, "storage_unavailable")
					return
				}
			}
		}
		fail(w, http.StatusForbidden, "pairing_unavailable")
		return
	}
	// Re-check authentication, storage and host/session availability after the
	// expensive verifier. No mutex is held while hashing; revoke/replace wins.
	current, authenticated := s.controllerLocked(r)
	if !authenticated || current.PublicKey != ctl.PublicKey {
		fail(w, http.StatusUnauthorized, "controller_auth_required")
		return
	}
	if r.Context().Err() != nil {
		return
	}
	host := s.devices[reserved.HostID]
	permissions := reserved.Permissions
	var status int
	var code string
	if reserved.Mode == activationMode {
		status, code, permissions = s.activationAdmissionLocked(host, ctl, reserved)
	} else {
		status, code = s.sessionAdmissionLocked(host, ctl)
	}
	if status != 0 {
		if status == http.StatusServiceUnavailable {
			fail(w, status, code)
		} else {
			fail(w, http.StatusForbidden, "pairing_unavailable")
		}
		return
	}
	id, err := token(18)
	if err != nil {
		fail(w, http.StatusInternalServerError, "entropy_unavailable")
		return
	}
	bearer, err := token(32)
	if err != nil {
		fail(w, http.StatusInternalServerError, "entropy_unavailable")
		return
	}
	now = s.now()
	expires := now.Add(s.cfg.PendingTTL)
	if reserved.Mode == activationMode && reserved.ExpiresAt.Before(expires) {
		expires = reserved.ExpiresAt
	}
	v := &Session{ID: id, HostDeviceID: host.ID, HostDisplayID: host.DisplayID, HostPublicKey: host.PublicKey, HostName: host.Name, PairingID: reserved.ID, Mode: reserved.Mode, ControllerName: in.ControllerName, ControllerPublicKey: ctl.PublicKey, ControllerTokenHash: hashToken(bearer), Status: "pending", Permissions: permissions, CreatedAt: now, ExpiresAt: expires}
	// One state lock covers the input reservation, optional one-use consumption,
	// session insertion, event and snapshot. Spectators never gain input later.
	if reserved.Mode != activationMode {
		s.removePairingLocked(reserved)
	}
	s.sessions[id] = v
	payload := map[string]any{"controllerName": in.ControllerName, "controllerPublicKey": ctl.PublicKey, "controllerVerified": true, "permissions": v.Permissions, "expiresAt": v.ExpiresAt, "pairingId": reserved.ID}
	if reserved.Mode == activationMode {
		payload["mode"] = activationMode
	}
	s.eventLocked(host, "session.requested", id, payload)
	s.addAuditLocked(host.ID, id, "session.requested")
	if s.persistLocked() != nil {
		fail(w, http.StatusServiceUnavailable, "storage_unavailable")
		return
	}
	writeJSON(w, http.StatusCreated, map[string]any{"session": cloneSession(v), "controllerToken": bearer})
}
