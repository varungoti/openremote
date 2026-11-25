package app

import (
	"encoding/base64"
	"net/http"
	"strings"
	"time"
)

func (s *Server) enroll(w http.ResponseWriter, r *http.Request) {
	if !method(w, r, "POST") {
		return
	}
	if len(s.cfg.EnrollmentToken) < 32 || !sameToken(hashToken(authHeader(r)), hashToken(s.cfg.EnrollmentToken)) {
		fail(w, 401, "enrollment_auth_required")
		return
	}
	var in struct {
		Name      string `json:"name"`
		Platform  string `json:"platform"`
		PublicKey string `json:"publicKey"`
	}
	if readJSON(w, r, &in) != nil {
		fail(w, 400, "invalid_json")
		return
	}
	key, err := base64.StdEncoding.Strict().DecodeString(in.PublicKey)
	if !boundedName(in.Name) || in.Platform != "windows" || err != nil || len(key) != 32 {
		fail(w, 400, "invalid_device")
		return
	}
	in.PublicKey = base64.StdEncoding.EncodeToString(key)
	id, err := token(18)
	if err != nil {
		fail(w, 500, "entropy_unavailable")
		return
	}
	bearer, err := token(32)
	if err != nil {
		fail(w, 500, "entropy_unavailable")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.devices) >= s.cfg.MaxDevices {
		fail(w, 503, "device_capacity")
		return
	}
	if s.byPublicKey[in.PublicKey] != nil {
		fail(w, 409, "device_already_enrolled")
		return
	}
	var display string
	for i := 0; i < 20; i++ {
		display, err = randomDisplayID()
		if err != nil {
			fail(w, 500, "entropy_unavailable")
			return
		}
		if s.byDisplay[display] == nil {
			break
		}
		display = ""
	}
	if display == "" {
		fail(w, 503, "id_capacity")
		return
	}
	now := s.now()
	d := &Device{ID: id, DisplayID: display, Name: in.Name, Platform: in.Platform, PublicKey: in.PublicKey, TokenHash: hashToken(bearer), Online: true, LastSeen: now, CreatedAt: now}
	s.devices[id] = d
	s.byDisplay[display] = d
	s.byPublicKey[d.PublicKey] = d
	s.addAuditLocked(id, "", "device.enrolled")
	if err := s.persistLocked(); err != nil {
		fail(w, 503, "storage_unavailable")
		return
	}
	writeJSON(w, 201, map[string]any{"device": s.publicDeviceLocked(d), "deviceToken": bearer})
}
func (s *Server) deviceLookup(w http.ResponseWriter, r *http.Request) {
	if !method(w, r, "GET") {
		return
	}
	display, ok := displayID(strings.TrimPrefix(r.URL.Path, "/v1/devices/"))
	if !ok {
		fail(w, 400, "invalid_display_id")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, ok = s.controllerLocked(r); !ok {
		fail(w, 401, "controller_auth_required")
		return
	}
	d := s.byDisplay[display]
	if d == nil {
		fail(w, 404, "device_not_found")
		return
	}
	v := s.publicDeviceLocked(d)
	writeJSON(w, 200, map[string]any{"displayId": v.DisplayID, "name": v.Name, "platform": v.Platform, "online": v.Online, "lastSeen": v.LastSeen, "publicKey": v.PublicKey})
}
func (s *Server) hostRoutes(w http.ResponseWriter, r *http.Request) {
	parts := strings.Split(strings.TrimPrefix(r.URL.Path, "/v1/hosts/"), "/")
	if len(parts) != 2 || !safeID(parts[0]) {
		fail(w, 404, "not_found")
		return
	}
	id, action := parts[0], parts[1]
	if action == "events" {
		s.hostEvents(w, r, id)
		return
	}
	if action == "pairing" {
		s.hostPairingRoutes(w, r, id)
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	d, ok := s.hostLocked(r, id)
	if !ok {
		fail(w, 401, "host_auth_required")
		return
	}
	switch action {
	case "heartbeat":
		if !method(w, r, "POST") {
			return
		}
		d.LastSeen = s.now()
		d.Online = true
		writeJSON(w, 200, map[string]bool{"ok": true})
	case "sessions":
		if !method(w, r, "GET") {
			return
		}
		s.hostSessionsLocked(w, id)
	case "audit":
		if !method(w, r, "GET") {
			return
		}
		out := []AuditEvent{}
		for i := len(s.audit) - 1; i >= 0 && len(out) < 200; i-- {
			if s.audit[i].DeviceID == id {
				out = append(out, s.audit[i])
			}
		}
		writeJSON(w, 200, map[string]any{"events": out})
	case "trusted":
		s.trustedControllersLocked(w, r, d)
	default:
		fail(w, 404, "not_found")
	}
}
func (s *Server) hostEvents(w http.ResponseWriter, r *http.Request, id string) {
	if !method(w, r, "GET") {
		return
	}
	after, err := cursor(r)
	if err != nil {
		fail(w, 400, "invalid_cursor")
		return
	}
	deadline := time.NewTimer(s.cfg.PollTimeout)
	defer deadline.Stop()
	tick := time.NewTicker(100 * time.Millisecond)
	defer tick.Stop()
	for {
		s.mu.Lock()
		// A poll can outlive its middleware check. Never expose an event whose
		// snapshot failed, since its sequence could be reused after restart.
		if s.storageErr != nil || s.closed {
			s.mu.Unlock()
			fail(w, 503, "storage_unavailable")
			return
		}
		d, ok := s.hostLocked(r, id)
		if !ok {
			s.mu.Unlock()
			fail(w, 401, "host_auth_required")
			return
		}
		if after > d.EventSeq {
			s.mu.Unlock()
			writeJSON(w, 409, map[string]any{"error": "cursor_reset_required", "resetAfter": int64(0)})
			return
		}
		if len(d.Events) > 0 && after < d.Events[0].Seq-1 {
			resetAfter := d.Events[0].Seq - 1
			s.mu.Unlock()
			writeJSON(w, 409, map[string]any{"error": "cursor_expired", "resetAfter": resetAfter})
			return
		}
		out := []HostEvent{}
		for _, e := range d.Events {
			if e.Seq > after {
				out = append(out, e)
			}
		}
		last := d.EventSeq
		s.mu.Unlock()
		if len(out) > 0 {
			writeJSON(w, 200, map[string]any{"events": out, "cursor": last})
			return
		}
		select {
		case <-r.Context().Done():
			return
		case <-deadline.C:
			writeJSON(w, 200, map[string]any{"events": out, "cursor": last})
			return
		case <-tick.C:
		}
	}
}
func (s *Server) trustedControllersLocked(w http.ResponseWriter, r *http.Request, d *Device) {
	switch r.Method {
	case "GET":
		out := []TrustedController{}
		for _, c := range s.trusted[d.ID] {
			out = append(out, c)
		}
		writeJSON(w, 200, map[string]any{"controllers": out})
	case "PUT":
		var in struct {
			PublicKey   string        `json:"publicKey"`
			Label       string        `json:"label"`
			Permissions PermissionSet `json:"permissions"`
		}
		if readJSON(w, r, &in) != nil {
			fail(w, 400, "invalid_json")
			return
		}
		_, canonical, err := parseControllerKey(in.PublicKey)
		if err != nil || !boundedName(in.Label) || !in.Permissions.ViewScreen {
			fail(w, 400, "invalid_controller")
			return
		}
		if s.trusted[d.ID] == nil {
			s.trusted[d.ID] = map[string]TrustedController{}
		}
		if len(s.trusted[d.ID]) >= 50 {
			if _, ok := s.trusted[d.ID][canonical]; !ok {
				fail(w, 409, "trust_capacity")
				return
			}
		}
		v := TrustedController{PublicKey: canonical, Label: in.Label, Permissions: in.Permissions, CreatedAt: s.now()}
		s.trusted[d.ID][canonical] = v
		delete(s.revoked[d.ID], canonical)
		s.addAuditLocked(d.ID, "", "controller.trusted")
		if err = s.persistLocked(); err != nil {
			fail(w, 503, "storage_unavailable")
			return
		}
		writeJSON(w, 200, v)
	case "DELETE":
		var in struct {
			PublicKey string `json:"publicKey"`
		}
		if readJSON(w, r, &in) != nil {
			fail(w, 400, "invalid_json")
			return
		}
		_, canonical, err := parseControllerKey(in.PublicKey)
		if err != nil {
			fail(w, 400, "invalid_public_key")
			return
		}
		if s.revoked[d.ID] == nil {
			s.revoked[d.ID] = map[string]time.Time{}
		}
		if len(s.revoked[d.ID]) >= 1000 {
			if _, exists := s.revoked[d.ID][canonical]; !exists {
				fail(w, 409, "revocation_capacity")
				return
			}
		}
		s.revoked[d.ID][canonical] = s.now()
		delete(s.trusted[d.ID], canonical)
		s.addAuditLocked(d.ID, "", "controller.revoked")
		for _, ss := range s.sessions {
			if ss.HostDeviceID == d.ID && ss.ControllerPublicKey == canonical && ss.EndedAt == nil {
				s.endLocked(ss, "ended", "controller.revoked")
			}
		}
		if err = s.persistLocked(); err != nil {
			fail(w, 503, "storage_unavailable")
			return
		}
		writeJSON(w, 200, map[string]bool{"ok": true})
	default:
		w.Header().Set("Allow", "GET, PUT, DELETE")
		fail(w, 405, "method_not_allowed")
	}
}
