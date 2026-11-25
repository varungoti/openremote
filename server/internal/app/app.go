// Package app implements the authenticated OpenRemote control plane.
// The coordinator is deliberately single-node: identity/audit snapshots are durable,
// while sessions and challenges are ephemeral and invalidated on restart.
package app

import (
	"bytes"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"log"
	"math/big"
	"mime"
	"net"
	"net/http"
	"openremote/controlplane/internal/webclient"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"
)

type PermissionSet struct {
	ViewScreen   bool `json:"viewScreen"`
	ControlInput bool `json:"controlInput"`
	Clipboard    bool `json:"clipboard"`
	FileTransfer bool `json:"fileTransfer"`
	Audio        bool `json:"audio"`
	Power        bool `json:"power"`
}

type Device struct {
	ID                    string      `json:"id"`
	DisplayID             string      `json:"displayId"`
	Name                  string      `json:"name"`
	Platform              string      `json:"platform"`
	PublicKey             string      `json:"publicKey"`
	TokenHash             [32]byte    `json:"-"`
	Online                bool        `json:"online"`
	LastSeen              time.Time   `json:"lastSeen"`
	CreatedAt             time.Time   `json:"createdAt"`
	EventSeq              int64       `json:"-"`
	Events                []HostEvent `json:"-"`
	AutomaticRegistration bool        `json:"-"`
	RegistrationRevokedAt *time.Time  `json:"-"`
}
type HostEvent struct {
	Seq       int64           `json:"seq"`
	Type      string          `json:"type"`
	SessionID string          `json:"sessionId,omitempty"`
	Payload   json.RawMessage `json:"payload,omitempty"`
	CreatedAt time.Time       `json:"createdAt"`
}
type Signal struct {
	Seq       int64           `json:"seq"`
	From      string          `json:"from"`
	Kind      string          `json:"kind"`
	Payload   json.RawMessage `json:"payload"`
	CreatedAt time.Time       `json:"createdAt"`
}
type Session struct {
	ID                  string        `json:"id"`
	HostDeviceID        string        `json:"hostDeviceId"`
	HostDisplayID       string        `json:"hostDisplayId"`
	HostPublicKey       string        `json:"hostPublicKey"`
	HostName            string        `json:"hostName"`
	PairingID           string        `json:"pairingId,omitempty"`
	Mode                string        `json:"mode,omitempty"`
	MonitorID           string        `json:"monitorId,omitempty"`
	ControllerName      string        `json:"controllerName"`
	ControllerPublicKey string        `json:"controllerPublicKey"`
	ControllerTokenHash [32]byte      `json:"-"`
	Status              string        `json:"status"`
	Permissions         PermissionSet `json:"permissions"`
	CreatedAt           time.Time     `json:"createdAt"`
	ExpiresAt           time.Time     `json:"expiresAt"`
	ApprovedAt          *time.Time    `json:"approvedAt,omitempty"`
	EndedAt             *time.Time    `json:"endedAt,omitempty"`
	SignalSeq           int64         `json:"-"`
	Signals             []Signal      `json:"-"`
	lastOfferFrom       string
	awaitingAnswer      bool
}
type TrustedController struct {
	PublicKey   string        `json:"publicKey"`
	Label       string        `json:"label"`
	Permissions PermissionSet `json:"permissions"`
	CreatedAt   time.Time     `json:"createdAt"`
}
type AuditEvent struct {
	ID        int64     `json:"id"`
	DeviceID  string    `json:"deviceId"`
	SessionID string    `json:"sessionId,omitempty"`
	Type      string    `json:"type"`
	CreatedAt time.Time `json:"createdAt"`
}
type Config struct {
	EnrollmentToken                     string
	GatewayToken                        string
	StatePath                           string
	AllowInsecure                       bool
	TrustedProxyCIDRs                   []string
	STUNURLs                            []string
	TURNURLs                            []string
	TURNSecret                          string
	ICERelayOnly                        bool
	SessionTTL                          time.Duration
	PendingTTL                          time.Duration
	AuthTTL                             time.Duration
	ChallengeTTL                        time.Duration
	HeartbeatTTL                        time.Duration
	PollTimeout                         time.Duration
	RatePerMinute                       int
	SensitiveRatePerMinute              int
	MaxDevices                          int
	MaxSessions                         int
	MaxChallenges                       int
	MaxAuthTokens                       int
	MaxQueue                            int
	MaxAudit                            int
	MaxLimiterEntries                   int
	PairingTTL                          time.Duration
	PairingIPRate                       int
	PairingControllerRate               int
	PairingGlobalRate                   int
	PairingHostCreateRate               int
	PairingGlobalCreateRate             int
	AutomaticHostRegistration           bool
	PublicOrigin                        string
	HostRegistrationChallengeTTL        time.Duration
	HostRegistrationChallengeIPRate     int
	HostRegistrationChallengeGlobalRate int
	HostRegistrationProofIPRate         int
	HostRegistrationProofGlobalRate     int
	HostRegistrationRecoveryRate        int
	MaxHostRegistrationChallenges       int
	now                                 func() time.Time
}

func DefaultConfig() Config {
	return Config{SessionTTL: 2 * time.Hour, PendingTTL: 90 * time.Second, AuthTTL: 10 * time.Minute, ChallengeTTL: 60 * time.Second, HeartbeatTTL: 45 * time.Second, PollTimeout: 20 * time.Second, RatePerMinute: 300, SensitiveRatePerMinute: 30, MaxDevices: 1000, MaxSessions: 2000, MaxChallenges: 2000, MaxAuthTokens: 2000, MaxQueue: 512, MaxAudit: 20000, MaxLimiterEntries: 10000, PairingTTL: 10 * time.Minute, PairingIPRate: 20, PairingControllerRate: 10, PairingGlobalRate: 120, PairingHostCreateRate: 6, PairingGlobalCreateRate: 60, HostRegistrationChallengeTTL: time.Minute, HostRegistrationChallengeIPRate: 10, HostRegistrationChallengeGlobalRate: 120, HostRegistrationProofIPRate: 10, HostRegistrationProofGlobalRate: 60, HostRegistrationRecoveryRate: 6, MaxHostRegistrationChallenges: 1000, now: time.Now}
}
func normalizeConfig(c Config) Config {
	d := DefaultConfig()
	if c.SessionTTL <= 0 {
		c.SessionTTL = d.SessionTTL
	}
	if c.PendingTTL <= 0 {
		c.PendingTTL = d.PendingTTL
	}
	if c.AuthTTL <= 0 {
		c.AuthTTL = d.AuthTTL
	}
	if c.ChallengeTTL <= 0 {
		c.ChallengeTTL = d.ChallengeTTL
	}
	if c.HeartbeatTTL <= 0 {
		c.HeartbeatTTL = d.HeartbeatTTL
	}
	if c.PollTimeout <= 0 {
		c.PollTimeout = d.PollTimeout
	}
	if c.RatePerMinute <= 0 {
		c.RatePerMinute = d.RatePerMinute
	}
	if c.SensitiveRatePerMinute <= 0 {
		c.SensitiveRatePerMinute = d.SensitiveRatePerMinute
	}
	if c.MaxDevices <= 0 {
		c.MaxDevices = d.MaxDevices
	}
	if c.MaxSessions <= 0 {
		c.MaxSessions = d.MaxSessions
	}
	if c.MaxChallenges <= 0 {
		c.MaxChallenges = d.MaxChallenges
	}
	if c.MaxAuthTokens <= 0 {
		c.MaxAuthTokens = d.MaxAuthTokens
	}
	if c.MaxQueue <= 0 {
		c.MaxQueue = d.MaxQueue
	}
	if c.MaxAudit <= 0 {
		c.MaxAudit = d.MaxAudit
	}
	if c.MaxLimiterEntries <= 0 {
		c.MaxLimiterEntries = d.MaxLimiterEntries
	}
	if c.PairingTTL <= 0 || c.PairingTTL > d.PairingTTL {
		c.PairingTTL = d.PairingTTL
	}
	if c.PairingIPRate <= 0 {
		c.PairingIPRate = d.PairingIPRate
	}
	if c.PairingControllerRate <= 0 {
		c.PairingControllerRate = d.PairingControllerRate
	}
	if c.PairingGlobalRate <= 0 {
		c.PairingGlobalRate = d.PairingGlobalRate
	}
	if c.PairingHostCreateRate <= 0 {
		c.PairingHostCreateRate = d.PairingHostCreateRate
	}
	if c.PairingGlobalCreateRate <= 0 {
		c.PairingGlobalCreateRate = d.PairingGlobalCreateRate
	}
	if c.HostRegistrationChallengeTTL <= 0 || c.HostRegistrationChallengeTTL > time.Minute {
		c.HostRegistrationChallengeTTL = d.HostRegistrationChallengeTTL
	}
	if c.HostRegistrationChallengeIPRate <= 0 {
		c.HostRegistrationChallengeIPRate = d.HostRegistrationChallengeIPRate
	}
	if c.HostRegistrationChallengeGlobalRate <= 0 {
		c.HostRegistrationChallengeGlobalRate = d.HostRegistrationChallengeGlobalRate
	}
	if c.HostRegistrationProofIPRate <= 0 {
		c.HostRegistrationProofIPRate = d.HostRegistrationProofIPRate
	}
	if c.HostRegistrationProofGlobalRate <= 0 {
		c.HostRegistrationProofGlobalRate = d.HostRegistrationProofGlobalRate
	}
	if c.HostRegistrationRecoveryRate <= 0 {
		c.HostRegistrationRecoveryRate = d.HostRegistrationRecoveryRate
	}
	if c.MaxHostRegistrationChallenges <= 0 {
		c.MaxHostRegistrationChallenges = d.MaxHostRegistrationChallenges
	}
	if c.now == nil {
		c.now = time.Now
	}
	return c
}

type bucket struct {
	used  int
	reset time.Time
}
type Server struct {
	mu                         sync.Mutex
	cfg                        Config
	devices                    map[string]*Device
	byDisplay                  map[string]*Device
	byPublicKey                map[string]*Device
	sessions                   map[string]*Session
	challenges                 map[string]challenge
	controllers                map[[32]byte]controller
	trusted                    map[string]map[string]TrustedController
	revoked                    map[string]map[string]time.Time
	audit                      []AuditEvent
	auditSeq                   int64
	limiters                   map[string]bucket
	proxies                    []*net.IPNet
	log                        *log.Logger
	storageErr                 error
	lockFile                   *os.File
	closed                     bool
	pairings                   map[string]*pairingGrant
	hostPairing                map[string]string
	pairingGeneration          map[string]uint64
	pairingWorkers             chan struct{}
	dummyPairingSalt           [32]byte
	dummyPairingHash           [32]byte
	hostRegistrationChallenges map[string]hostRegistrationChallenge
}

func New(logger *log.Logger) *Server {
	// Tests/development only. Enrollment stays closed until configured explicitly.
	s, err := NewWithConfig(logger, DefaultConfig())
	if err != nil {
		panic(err)
	}
	return s
}
func NewWithConfig(logger *log.Logger, cfg Config) (*Server, error) {
	cfg = normalizeConfig(cfg)
	if err := validateGatewayToken(cfg.GatewayToken); err != nil {
		return nil, err
	}
	if logger == nil {
		logger = log.New(io.Discard, "", 0)
	}
	if cfg.AutomaticHostRegistration {
		if err := validateRegistrationOrigin(cfg.PublicOrigin, cfg.AllowInsecure); err != nil {
			return nil, err
		}
	}
	s := &Server{cfg: cfg, devices: map[string]*Device{}, byDisplay: map[string]*Device{}, byPublicKey: map[string]*Device{}, sessions: map[string]*Session{}, challenges: map[string]challenge{}, controllers: map[[32]byte]controller{}, trusted: map[string]map[string]TrustedController{}, revoked: map[string]map[string]time.Time{}, audit: []AuditEvent{}, limiters: map[string]bucket{}, log: logger, pairings: map[string]*pairingGrant{}, hostPairing: map[string]string{}, pairingGeneration: map[string]uint64{}, pairingWorkers: make(chan struct{}, 2), hostRegistrationChallenges: map[string]hostRegistrationChallenge{}}
	if _, err := rand.Read(s.dummyPairingSalt[:]); err != nil {
		return nil, err
	}
	if _, err := rand.Read(s.dummyPairingHash[:]); err != nil {
		return nil, err
	}
	for _, cidr := range cfg.TrustedProxyCIDRs {
		_, n, err := net.ParseCIDR(cidr)
		if err != nil {
			return nil, errors.New("invalid trusted proxy CIDR")
		}
		s.proxies = append(s.proxies, n)
	}
	if err := validateICEConfig(cfg); err != nil {
		return nil, err
	}
	if cfg.StatePath != "" {
		if err := os.MkdirAll(filepath.Dir(cfg.StatePath), 0700); err != nil {
			return nil, err
		}
		var err error
		s.lockFile, err = acquireStateLock(cfg.StatePath + ".lock")
		if err != nil {
			return nil, err
		}
	}
	if err := s.load(); err != nil {
		if s.lockFile != nil {
			_ = s.lockFile.Close()
		}
		return nil, err
	}
	return s, nil
}
func (s *Server) now() time.Time { return s.cfg.now().UTC() }
func token(n int) (string, error) {
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(b), nil
}
func hashToken(t string) [32]byte  { return sha256.Sum256([]byte(t)) }
func sameToken(a, b [32]byte) bool { return subtle.ConstantTimeCompare(a[:], b[:]) == 1 }
func authHeader(r *http.Request) string {
	v := r.Header.Get("Authorization")
	if !strings.HasPrefix(v, "Bearer ") {
		return ""
	}
	v = strings.TrimPrefix(v, "Bearer ")
	if v == "" || strings.ContainsAny(v, " \r\n\t") {
		return ""
	}
	return v
}
func randomDisplayID() (string, error) {
	n, err := rand.Int(rand.Reader, big.NewInt(900000000))
	if err != nil {
		return "", err
	}
	return strconv.FormatInt(n.Int64()+100000000, 10), nil
}
func displayID(v string) (string, bool) {
	v = strings.NewReplacer(" ", "", "-", "").Replace(v)
	if len(v) != 9 {
		return "", false
	}
	for _, c := range v {
		if c < '0' || c > '9' {
			return "", false
		}
	}
	return v, true
}
func safeID(v string) bool {
	if len(v) < 16 || len(v) > 64 {
		return false
	}
	for _, c := range v {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_') {
			return false
		}
	}
	return true
}
func boundedName(s string) bool {
	if len(s) < 1 || len(s) > 80 || strings.TrimSpace(s) != s {
		return false
	}
	for _, c := range s {
		if c < 32 || c == 127 {
			return false
		}
	}
	return true
}
func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}
func fail(w http.ResponseWriter, status int, code string) {
	writeJSON(w, status, map[string]string{"error": code})
}
func method(w http.ResponseWriter, r *http.Request, want string) bool {
	if r.Method != want {
		w.Header().Set("Allow", want)
		fail(w, 405, "method_not_allowed")
		return false
	}
	return true
}
func readJSON(w http.ResponseWriter, r *http.Request, dst any) error {
	if ct := r.Header.Get("Content-Type"); ct != "" {
		mediaType, _, err := mime.ParseMediaType(ct)
		if err != nil || mediaType != "application/json" {
			return errors.New("content type")
		}
	}
	r.Body = http.MaxBytesReader(w, r.Body, 300*1024)
	raw, err := io.ReadAll(r.Body)
	if err != nil {
		return err
	}
	// encoding/json normally accepts duplicate keys. Reject them at every depth
	// so authorization flags cannot be interpreted differently across clients.
	scanner := json.NewDecoder(bytes.NewReader(raw))
	if err = uniqueJSON(scanner, 0); err != nil {
		return err
	}
	if _, err = scanner.Token(); err != io.EOF {
		return errors.New("trailing JSON")
	}
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	return dec.Decode(dst)
}
func uniqueJSON(d *json.Decoder, depth int) error {
	if depth > 32 {
		return errors.New("JSON nesting limit")
	}
	t, err := d.Token()
	if err != nil {
		return err
	}
	if delim, ok := t.(json.Delim); ok {
		switch delim {
		case '{':
			seen := map[string]bool{}
			for d.More() {
				k, err := d.Token()
				if err != nil {
					return err
				}
				key, ok := k.(string)
				if !ok || seen[key] {
					return errors.New("duplicate JSON key")
				}
				seen[key] = true
				if err = uniqueJSON(d, depth+1); err != nil {
					return err
				}
			}
			t, err = d.Token()
			if err != nil || t != json.Delim('}') {
				return errors.New("invalid JSON object")
			}
		case '[':
			for d.More() {
				if err = uniqueJSON(d, depth+1); err != nil {
					return err
				}
			}
			t, err = d.Token()
			if err != nil || t != json.Delim(']') {
				return errors.New("invalid JSON array")
			}
		default:
			return errors.New("invalid delimiter")
		}
	}
	return nil
}

func cursor(r *http.Request) (int64, error) {
	v := r.URL.Query().Get("after")
	if v == "" {
		return 0, nil
	}
	n, err := strconv.ParseInt(v, 10, 64)
	if err != nil || n < 0 {
		return 0, errors.New("invalid cursor")
	}
	return n, nil
}
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		if method(w, r, "GET") {
			writeJSON(w, 200, map[string]any{"ok": true, "time": s.now()})
		}
	})
	mux.HandleFunc("/v1/info", s.setupInfo)
	mux.HandleFunc("/v1/auth/challenge", s.authChallenge)
	mux.HandleFunc("/v1/auth/verify", s.authVerify)
	mux.HandleFunc("/v1/devices/enroll", s.enroll)
	mux.HandleFunc("/v1/devices/", s.deviceLookup)
	mux.HandleFunc("/v1/hosts/", s.hostRoutes)
	mux.HandleFunc("/v1/hosts/register/challenge", s.hostRegistrationChallenge)
	mux.HandleFunc("/v1/hosts/register", s.registerHost)
	mux.HandleFunc("/v1/admin/hosts/", s.adminHostRoutes)
	mux.HandleFunc("/v1/pairings/redeem", s.redeemPairing)
	mux.HandleFunc("/v1/sessions", s.createSession)
	mux.HandleFunc("/v1/sessions/", s.sessionRoutes)
	web := webclient.Handler()
	mux.Handle("/connect", web)
	mux.Handle("/connect/", web)
	return s.middleware(mux)
}
func (s *Server) middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("X-Frame-Options", "DENY")
		w.Header().Set("Referrer-Policy", "no-referrer")
		w.Header().Set("Cache-Control", "no-store")
		secure := r.TLS != nil
		ip, trusted := s.requestClient(r)
		if trusted {
			secure = secure || r.Header.Get("X-Forwarded-Proto") == "https"
		}
		if !secure && !s.cfg.AllowInsecure {
			fail(w, 426, "https_required")
			return
		}
		if secure {
			w.Header().Set("Strict-Transport-Security", "max-age=31536000")
		}
		if s.cfg.GatewayToken != "" && strings.HasPrefix(r.URL.Path, "/v1/") {
			var authenticated bool
			r, authenticated = s.authenticateGateway(r)
			if !authenticated {
				fail(w, http.StatusUnauthorized, "gateway_auth_required")
				return
			}
			// Authentication changes rate identity only. HTTPS was independently
			// established above from TLS or the actual immediate proxy peer.
			ip, _ = s.requestClient(r)
		}
		s.mu.Lock()
		s.expireLocked()
		now := s.now()
		key := "all:" + ip
		allowed := s.limitLocked(key, s.cfg.RatePerMinute, now)
		if allowed && (strings.HasPrefix(r.URL.Path, "/v1/auth/") || r.URL.Path == "/v1/sessions" || strings.HasPrefix(r.URL.Path, "/v1/devices/") || r.URL.Path == "/v1/pairings/redeem" || hostRegistrationRequest(r) || strings.HasPrefix(r.URL.Path, "/v1/admin/")) {
			allowed = s.limitLocked("sensitive:"+ip, s.cfg.SensitiveRatePerMinute, now)
		}
		storageErr := s.storageErr
		closed := s.closed
		s.mu.Unlock()
		if storageErr != nil || closed {
			fail(w, 503, "storage_unavailable")
			return
		}
		if !allowed {
			w.Header().Set("Retry-After", "60")
			code := "rate_limited"
			if pairingRequest(r) {
				code = "pairing_rate_limited"
			}
			if hostRegistrationRequest(r) {
				code = "host_registration_rate_limited"
			}
			fail(w, 429, code)
			return
		}
		// Network reads/writes must not hold the state mutex. Responses are
		// bounded by queue limits, so buffer them until the handler releases locks.
		if r.Body != nil && r.ContentLength != 0 {
			raw, err := io.ReadAll(http.MaxBytesReader(w, r.Body, 300*1024))
			if err != nil {
				fail(w, 413, "body_too_large_or_unreadable")
				return
			}
			r.Body = io.NopCloser(bytes.NewReader(raw))
		}
		reply := &bufferedReply{header: make(http.Header), status: 200}
		next.ServeHTTP(reply, r)
		for k, values := range reply.header {
			w.Header()[k] = values
		}
		w.WriteHeader(reply.status)
		_, _ = w.Write(reply.body.Bytes())
	})
}
func (s *Server) limitLocked(key string, max int, now time.Time) bool {
	b, ok := s.limiters[key]
	if !ok && len(s.limiters) >= s.cfg.MaxLimiterEntries {
		return false
	}
	if !ok || !now.Before(b.reset) {
		b = bucket{reset: now.Add(time.Minute)}
	}
	b.used++
	s.limiters[key] = b
	return b.used <= max
}
func (s *Server) expireLocked() {
	now := s.now()
	for k, v := range s.challenges {
		if !now.Before(v.ExpiresAt) {
			delete(s.challenges, k)
		}
	}
	for k, v := range s.hostRegistrationChallenges {
		if !now.Before(v.ExpiresAt) {
			delete(s.hostRegistrationChallenges, k)
		}
	}
	for k, v := range s.controllers {
		if !now.Before(v.ExpiresAt) {
			delete(s.controllers, k)
		}
	}
	for k, v := range s.limiters {
		if !now.Before(v.reset) {
			delete(s.limiters, k)
		}
	}
	for _, p := range s.pairings {
		if !now.Before(p.ExpiresAt) {
			s.removePairingLocked(p)
		}
	}
	changed := false
	for k, v := range s.sessions {
		if v.EndedAt != nil {
			if now.Sub(*v.EndedAt) > 5*time.Minute {
				delete(s.sessions, k)
			}
			continue
		}
		if !now.Before(v.ExpiresAt) {
			v.Status = "expired"
			t := now
			v.EndedAt = &t
			v.Signals = nil
			s.addAuditLocked(v.HostDeviceID, v.ID, "session.expired")
			s.eventLocked(s.devices[v.HostDeviceID], "session.ended", v.ID, nil)
			changed = true
		}
	}
	if changed {
		if err := s.persistLocked(); err != nil {
			s.log.Printf("audit persistence failed during expiry: %v", err)
		}
	}
}
func (s *Server) publicDeviceLocked(d *Device) Device {
	c := *d
	c.Events = nil
	c.Online = d.RegistrationRevokedAt == nil && d.Online && !d.LastSeen.IsZero() && s.now().Sub(d.LastSeen) <= s.cfg.HeartbeatTTL
	return c
}
func cloneSession(v *Session) Session {
	c := *v
	c.Signals = nil
	if v.ApprovedAt != nil {
		t := *v.ApprovedAt
		c.ApprovedAt = &t
	}
	if v.EndedAt != nil {
		t := *v.EndedAt
		c.EndedAt = &t
	}
	return c
}
func (s *Server) eventLocked(d *Device, typ, sid string, payload any) {
	if d == nil {
		return
	}
	var b json.RawMessage
	if payload != nil {
		b, _ = json.Marshal(payload)
	}
	d.EventSeq++
	d.Events = append(d.Events, HostEvent{Seq: d.EventSeq, Type: typ, SessionID: sid, Payload: b, CreatedAt: s.now()})
	if len(d.Events) > s.cfg.MaxQueue {
		d.Events = append([]HostEvent(nil), d.Events[len(d.Events)-s.cfg.MaxQueue:]...)
	}
}
func (s *Server) addAuditLocked(did, sid, typ string) {
	s.auditSeq++
	s.audit = append(s.audit, AuditEvent{ID: s.auditSeq, DeviceID: did, SessionID: sid, Type: typ, CreatedAt: s.now()})
	if len(s.audit) > s.cfg.MaxAudit {
		s.audit = append([]AuditEvent(nil), s.audit[len(s.audit)-s.cfg.MaxAudit:]...)
	}
}

// Close releases the single-writer lease after HTTP handlers have drained.
func (s *Server) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.closed = true
	clear(s.pairings)
	clear(s.hostPairing)
	clear(s.hostRegistrationChallenges)
	if s.lockFile != nil {
		err := s.lockFile.Close()
		s.lockFile = nil
		return err
	}
	return nil
}

type bufferedReply struct {
	header      http.Header
	status      int
	wroteHeader bool
	body        bytes.Buffer
}

func (b *bufferedReply) Header() http.Header { return b.header }
func (b *bufferedReply) WriteHeader(code int) {
	if !b.wroteHeader {
		b.status = code
		b.wroteHeader = true
	}
}
func (b *bufferedReply) Write(p []byte) (int, error) {
	if !b.wroteHeader {
		b.WriteHeader(200)
	}
	return b.body.Write(p)
}
