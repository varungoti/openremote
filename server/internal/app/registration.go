package app

import (
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"errors"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
	"unicode"
)

const hostRegistrationPurpose = "OpenRemote.HostRegistration.v1"

type hostRegistrationChallenge struct {
	PublicKey         string
	Name              string
	Message           []byte
	ExpiresAt         time.Time
	ExistingHostID    string
	ExistingTokenHash [32]byte
}

// An explicitly configured origin is part of the signing transcript. Request
// Host/X-Forwarded headers cannot select which service the host authorizes.
func validateRegistrationOrigin(value string, allowInsecure bool) error {
	invalid := func() error {
		return errors.New("OPENREMOTE_PUBLIC_ORIGIN must be one canonical HTTPS origin when automatic host registration is enabled")
	}
	if len(value) == 0 || len(value) > 2048 || strings.ContainsAny(value, "\\%+@?#") {
		return invalid()
	}
	for _, b := range []byte(value) {
		if b <= 32 || b >= 127 {
			return invalid()
		}
	}
	u, err := url.Parse(value)
	if err != nil || u.Host == "" || u.User != nil || u.Opaque != "" || u.Path != "" || u.RawPath != "" || u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || u.RawFragment != "" {
		return invalid()
	}
	host := u.Hostname()
	if host == "" || host != strings.ToLower(host) || !strings.HasPrefix(value, u.Scheme+"://") || strings.HasSuffix(u.Host, ":") {
		return invalid()
	}
	if u.Scheme != "https" && !(allowInsecure && u.Scheme == "http" && (host == "localhost" || host == "127.0.0.1" || host == "::1")) {
		return invalid()
	}
	ip := net.ParseIP(host)
	if ip != nil {
		if ip.String() != host || strings.Contains(host, ":") != strings.HasPrefix(u.Host, "[") {
			return invalid()
		}
	} else {
		if strings.ContainsAny(u.Host, "[]") {
			return invalid()
		}
		dns := strings.TrimSuffix(host, ".")
		if len(dns) == 0 || len(dns) > 253 {
			return invalid()
		}
		numericOnly := true
		for _, label := range strings.Split(dns, ".") {
			if len(label) == 0 || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' {
				return invalid()
			}
			hexAddressPart := len(label) > 2 && strings.HasPrefix(label, "0x")
			if hexAddressPart {
				for _, b := range []byte(label[2:]) {
					if !(b >= '0' && b <= '9' || b >= 'a' && b <= 'f') {
						hexAddressPart = false
					}
				}
			}
			if hexAddressPart {
				return invalid()
			}
			for _, b := range []byte(label) {
				if !(b >= 'a' && b <= 'z' || b >= '0' && b <= '9' || b == '-') {
					return invalid()
				}
				if b < '0' || b > '9' {
					numericOnly = false
				}
			}
		}
		if numericOnly {
			return invalid()
		}
	}
	if port := u.Port(); port != "" {
		n, err := strconv.Atoi(port)
		if err != nil || n < 1 || n > 65535 || strconv.Itoa(n) != port || u.Scheme == "https" && n == 443 || u.Scheme == "http" && n == 80 {
			return invalid()
		}
	}
	if u.String() != value {
		return invalid()
	}
	return nil
}

func hostRegistrationRequest(r *http.Request) bool {
	return r.URL.Path == "/v1/hosts/register" || r.URL.Path == "/v1/hosts/register/challenge"
}

func registrationLimited(w http.ResponseWriter) {
	w.Header().Set("Retry-After", "60")
	fail(w, http.StatusTooManyRequests, "host_registration_rate_limited")
}

func (s *Server) registrationEnabledLocked(w http.ResponseWriter) bool {
	if s.storageErr != nil || s.closed {
		fail(w, http.StatusServiceUnavailable, "storage_unavailable")
		return false
	}
	if !s.cfg.AutomaticHostRegistration {
		fail(w, http.StatusForbidden, "automatic_host_registration_disabled")
		return false
	}
	return true
}

func registrationJSON(w http.ResponseWriter, r *http.Request, dst any) bool {
	if !method(w, r, "POST") {
		return false
	}
	if r.URL.RawQuery != "" {
		fail(w, http.StatusBadRequest, "host_registration_query_not_allowed")
		return false
	}
	if readJSON(w, r, dst) != nil {
		fail(w, http.StatusBadRequest, "invalid_json")
		return false
	}
	return true
}

func (s *Server) hostRegistrationChallenge(w http.ResponseWriter, r *http.Request) {
	var in struct {
		PublicKey string `json:"publicKey"`
		Name      string `json:"name"`
		Platform  string `json:"platform"`
	}
	if !registrationJSON(w, r, &in) {
		return
	}
	key, err := base64.StdEncoding.Strict().DecodeString(in.PublicKey)
	if len(in.PublicKey) != 44 || err != nil || len(key) != ed25519.PublicKeySize || base64.StdEncoding.EncodeToString(key) != in.PublicKey || !boundedName(in.Name) || strings.IndexFunc(in.Name, unicode.IsControl) >= 0 || in.Platform != "windows" {
		fail(w, http.StatusBadRequest, "invalid_host_registration_request")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.registrationEnabledLocked(w) {
		return
	}
	now := s.now()
	if !s.limitLocked("host-register-challenge-ip:"+s.pairingClientIP(r), s.cfg.HostRegistrationChallengeIPRate, now) ||
		!s.limitLocked("host-register-challenge-global", s.cfg.HostRegistrationChallengeGlobalRate, now) {
		registrationLimited(w)
		return
	}
	if len(s.hostRegistrationChallenges) >= s.cfg.MaxHostRegistrationChallenges {
		fail(w, http.StatusServiceUnavailable, "host_registration_challenge_capacity")
		return
	}
	var id string
	for attempt := 0; attempt < 8; attempt++ {
		id, err = token(18)
		if err != nil {
			fail(w, http.StatusInternalServerError, "entropy_unavailable")
			return
		}
		if _, exists := s.hostRegistrationChallenges[id]; !exists {
			break
		}
		id = ""
	}
	if id == "" {
		fail(w, http.StatusServiceUnavailable, "host_registration_challenge_capacity")
		return
	}
	nonce := make([]byte, 32)
	if _, err = rand.Read(nonce); err != nil {
		fail(w, http.StatusInternalServerError, "entropy_unavailable")
		return
	}
	expiration := now.Add(s.cfg.HostRegistrationChallengeTTL)
	expires := expiration.Format(time.RFC3339Nano)
	message := []byte(strings.Join([]string{hostRegistrationPurpose, s.cfg.PublicOrigin, id, base64.StdEncoding.EncodeToString(nonce), in.PublicKey, in.Name, "windows", expires, ""}, "\n"))
	challenge := hostRegistrationChallenge{PublicKey: in.PublicKey, Name: in.Name, Message: message, ExpiresAt: expiration}
	if d := s.byPublicKey[in.PublicKey]; d != nil {
		challenge.ExistingHostID = d.ID
		challenge.ExistingTokenHash = d.TokenHash
	}
	s.hostRegistrationChallenges[id] = challenge
	writeJSON(w, http.StatusCreated, map[string]any{"challengeId": id, "challenge": base64.StdEncoding.EncodeToString(message), "expiresAt": expires, "apiOrigin": s.cfg.PublicOrigin})
}

func (s *Server) registerHost(w http.ResponseWriter, r *http.Request) {
	var in struct {
		ChallengeID string `json:"challengeId"`
		Signature   string `json:"signature"`
	}
	if !registrationJSON(w, r, &in) {
		return
	}
	if len(in.ChallengeID) != 24 || !safeID(in.ChallengeID) {
		fail(w, http.StatusBadRequest, "invalid_host_registration_request")
		return
	}
	s.mu.Lock()
	if !s.registrationEnabledLocked(w) {
		s.mu.Unlock()
		return
	}
	now := s.now()
	if !s.limitLocked("host-register-proof-ip:"+s.pairingClientIP(r), s.cfg.HostRegistrationProofIPRate, now) ||
		!s.limitLocked("host-register-proof-global", s.cfg.HostRegistrationProofGlobalRate, now) {
		s.mu.Unlock()
		registrationLimited(w)
		return
	}
	challenge, exists := s.hostRegistrationChallenges[in.ChallengeID]
	delete(s.hostRegistrationChallenges, in.ChallengeID)
	s.mu.Unlock()
	if !exists || !now.Before(challenge.ExpiresAt) {
		fail(w, http.StatusUnauthorized, "invalid_host_registration_proof")
		return
	}
	key, keyErr := base64.StdEncoding.Strict().DecodeString(challenge.PublicKey)
	signature, signatureErr := base64.StdEncoding.Strict().DecodeString(in.Signature)
	if keyErr != nil || len(key) != ed25519.PublicKeySize || len(in.Signature) != 88 || signatureErr != nil || len(signature) != ed25519.SignatureSize || base64.StdEncoding.EncodeToString(signature) != in.Signature || !ed25519.Verify(ed25519.PublicKey(key), challenge.Message, signature) {
		fail(w, http.StatusUnauthorized, "invalid_host_registration_proof")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.registrationEnabledLocked(w) {
		return
	}
	if !s.now().Before(challenge.ExpiresAt) {
		fail(w, http.StatusUnauthorized, "invalid_host_registration_proof")
		return
	}
	if r.Context().Err() != nil {
		return
	}
	d := s.byPublicKey[challenge.PublicKey]
	if d != nil && (!d.AutomaticRegistration || d.RegistrationRevokedAt != nil) {
		fail(w, http.StatusForbidden, "host_registration_unavailable")
		return
	}
	// A delayed earlier proof must not replace a bearer issued by a later
	// successful registration/recovery. Fetching a new challenge is required.
	if (d == nil && challenge.ExistingHostID != "") || (d != nil && (challenge.ExistingHostID != d.ID || !sameToken(challenge.ExistingTokenHash, d.TokenHash))) {
		fail(w, http.StatusUnauthorized, "invalid_host_registration_proof")
		return
	}
	if !s.limitLocked("host-register-owner:"+challenge.PublicKey, s.cfg.HostRegistrationRecoveryRate, s.now()) {
		registrationLimited(w)
		return
	}
	if d == nil && len(s.devices) >= s.cfg.MaxDevices {
		fail(w, http.StatusServiceUnavailable, "device_capacity")
		return
	}
	bearer, err := token(32)
	if err != nil {
		fail(w, http.StatusInternalServerError, "entropy_unavailable")
		return
	}
	status := http.StatusOK
	if d == nil {
		id, err := token(18)
		if err != nil {
			fail(w, http.StatusInternalServerError, "entropy_unavailable")
			return
		}
		var display string
		for attempt := 0; attempt < 20; attempt++ {
			display, err = randomDisplayID()
			if err != nil {
				fail(w, http.StatusInternalServerError, "entropy_unavailable")
				return
			}
			if s.byDisplay[display] == nil {
				break
			}
			display = ""
		}
		if display == "" {
			fail(w, http.StatusServiceUnavailable, "id_capacity")
			return
		}
		d = &Device{ID: id, DisplayID: display, PublicKey: challenge.PublicKey, Platform: "windows", CreatedAt: s.now(), AutomaticRegistration: true}
		s.devices[id], s.byDisplay[display], s.byPublicKey[d.PublicKey] = d, d, d
		s.addAuditLocked(id, "", "device.automatically_registered")
		status = http.StatusCreated
	} else {
		s.invalidateHostAccessLocked(d, "host.credentials_rotated")
		s.addAuditLocked(d.ID, "", "device.credentials_recovered")
	}
	d.Name = challenge.Name
	d.TokenHash = hashToken(bearer)
	d.Online, d.LastSeen = true, s.now()
	if s.persistLocked() != nil {
		fail(w, http.StatusServiceUnavailable, "storage_unavailable")
		return
	}
	writeJSON(w, status, map[string]any{"device": s.publicDeviceLocked(d), "deviceToken": bearer})
}

func (s *Server) invalidateHostAccessLocked(d *Device, event string) {
	s.pairingGeneration[d.ID]++
	s.removePairingLocked(s.currentPairingLocked(d.ID))
	for _, session := range s.sessions {
		if session.HostDeviceID == d.ID && session.EndedAt == nil {
			s.endLocked(session, "ended", event)
		}
	}
	// Cursor numbers remain monotonic in the snapshot, but a replacement host
	// process must not act on the previous process's pending request payloads.
	d.Events = nil
}

func (s *Server) adminHostRoutes(w http.ResponseWriter, r *http.Request) {
	parts := strings.Split(strings.TrimPrefix(r.URL.Path, "/v1/admin/hosts/"), "/")
	if len(parts) != 2 || !safeID(parts[0]) || parts[1] != "revoke" {
		fail(w, http.StatusNotFound, "not_found")
		return
	}
	if len(s.cfg.EnrollmentToken) < 32 || !sameToken(hashToken(authHeader(r)), hashToken(s.cfg.EnrollmentToken)) {
		fail(w, http.StatusUnauthorized, "admin_auth_required")
		return
	}
	var in struct{}
	if !registrationJSON(w, r, &in) {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.storageErr != nil || s.closed {
		fail(w, http.StatusServiceUnavailable, "storage_unavailable")
		return
	}
	d := s.devices[parts[0]]
	if d == nil {
		fail(w, http.StatusNotFound, "device_not_found")
		return
	}
	if d.RegistrationRevokedAt == nil {
		now := s.now()
		d.RegistrationRevokedAt = &now
		d.Online = false
		s.invalidateHostAccessLocked(d, "host.registration_revoked")
		s.addAuditLocked(d.ID, "", "device.registration_revoked")
		if s.persistLocked() != nil {
			fail(w, http.StatusServiceUnavailable, "storage_unavailable")
			return
		}
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}
