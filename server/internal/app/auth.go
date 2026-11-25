package app

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"errors"
	"net/http"
	"time"
)

type challenge struct {
	PublicKey string
	Challenge []byte
	ExpiresAt time.Time
}
type controller struct {
	PublicKey string
	ExpiresAt time.Time
}

func parseControllerKey(encoded string) (*ecdsa.PublicKey, string, error) {
	if len(encoded) > 256 {
		return nil, "", errors.New("public key too large")
	}
	der, err := base64.StdEncoding.Strict().DecodeString(encoded)
	if err != nil {
		return nil, "", err
	}
	key, err := x509.ParsePKIXPublicKey(der)
	if err != nil {
		return nil, "", err
	}
	ec, ok := key.(*ecdsa.PublicKey)
	if !ok || ec.Curve != elliptic.P256() || !ec.Curve.IsOnCurve(ec.X, ec.Y) {
		return nil, "", errors.New("expected P-256 public key")
	}
	canonical, err := x509.MarshalPKIXPublicKey(ec)
	if err != nil {
		return nil, "", err
	}
	return ec, base64.StdEncoding.EncodeToString(canonical), nil
}
func (s *Server) authChallenge(w http.ResponseWriter, r *http.Request) {
	if !method(w, r, "POST") {
		return
	}
	var in struct {
		PublicKey string `json:"publicKey"`
		Algorithm string `json:"algorithm"`
	}
	if readJSON(w, r, &in) != nil {
		fail(w, 400, "invalid_json")
		return
	}
	if in.Algorithm != "p256" {
		fail(w, 400, "unsupported_algorithm")
		return
	}
	_, canonical, err := parseControllerKey(in.PublicKey)
	if err != nil {
		fail(w, 400, "invalid_public_key")
		return
	}
	id, err := token(18)
	if err != nil {
		fail(w, 500, "entropy_unavailable")
		return
	}
	b := make([]byte, 32)
	if _, err = rand.Read(b); err != nil {
		fail(w, 500, "entropy_unavailable")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.challenges) >= s.cfg.MaxChallenges {
		fail(w, 503, "challenge_capacity")
		return
	}
	exp := s.now().Add(s.cfg.ChallengeTTL)
	s.challenges[id] = challenge{PublicKey: canonical, Challenge: b, ExpiresAt: exp}
	writeJSON(w, 201, map[string]any{"challengeId": id, "challenge": base64.StdEncoding.EncodeToString(b), "expiresAt": exp})
}
func (s *Server) authVerify(w http.ResponseWriter, r *http.Request) {
	if !method(w, r, "POST") {
		return
	}
	var in struct {
		ChallengeID string `json:"challengeId"`
		Signature   string `json:"signature"`
	}
	if readJSON(w, r, &in) != nil {
		fail(w, 400, "invalid_json")
		return
	}
	if !safeID(in.ChallengeID) || len(in.Signature) > 192 {
		fail(w, 400, "invalid_proof")
		return
	}
	s.mu.Lock()
	ch, ok := s.challenges[in.ChallengeID]
	delete(s.challenges, in.ChallengeID)
	s.mu.Unlock()
	if !ok || !s.now().Before(ch.ExpiresAt) {
		fail(w, 401, "invalid_or_expired_challenge")
		return
	}
	sig, err := base64.StdEncoding.Strict().DecodeString(in.Signature)
	pub, _, keyErr := parseControllerKey(ch.PublicKey)
	digest := sha256.Sum256(ch.Challenge)
	if err != nil || keyErr != nil || !ecdsa.VerifyASN1(pub, digest[:], sig) {
		fail(w, 401, "invalid_proof")
		return
	}
	bearer, err := token(32)
	if err != nil {
		fail(w, 500, "entropy_unavailable")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.controllers) >= s.cfg.MaxAuthTokens {
		fail(w, 503, "auth_capacity")
		return
	}
	exp := s.now().Add(s.cfg.AuthTTL)
	s.controllers[hashToken(bearer)] = controller{PublicKey: ch.PublicKey, ExpiresAt: exp}
	writeJSON(w, 200, map[string]any{"accessToken": bearer, "expiresAt": exp, "publicKey": ch.PublicKey})
}
func (s *Server) controllerLocked(r *http.Request) (controller, bool) {
	t := authHeader(r)
	if t == "" {
		return controller{}, false
	}
	c, ok := s.controllers[hashToken(t)]
	return c, ok && s.now().Before(c.ExpiresAt)
}
func (s *Server) hostLocked(r *http.Request, id string) (*Device, bool) {
	d := s.devices[id]
	t := authHeader(r)
	return d, d != nil && d.RegistrationRevokedAt == nil && t != "" && sameToken(hashToken(t), d.TokenHash)
}
func (s *Server) sessionRoleLocked(r *http.Request, v *Session) (string, bool) {
	t := authHeader(r)
	if t == "" {
		return "", false
	}
	h := hashToken(t)
	if sameToken(h, v.ControllerTokenHash) {
		return "controller", true
	}
	d := s.devices[v.HostDeviceID]
	if d != nil && d.RegistrationRevokedAt == nil && sameToken(h, d.TokenHash) {
		return "host", true
	}
	return "", false
}
