package app

import (
	"bytes"
	"encoding/json"
	"net/http"
	"sort"
	"strings"
	"time"
)

func (p PermissionSet) subsetOf(q PermissionSet) bool {
	return (!p.ViewScreen || q.ViewScreen) && (!p.ControlInput || q.ControlInput) && (!p.Clipboard || q.Clipboard) && (!p.FileTransfer || q.FileTransfer) && (!p.Audio || q.Audio) && (!p.Power || q.Power)
}

// A newly started, single-instance host can inspect old process sessions before
// an explicitly authorized orphan cleanup. This route never returns bearer
// secrets or signal payloads and does not itself end or approve anything.
func (s *Server) hostSessionsLocked(w http.ResponseWriter, hostID string) {
	out := []Session{}
	for _, v := range s.sessions {
		if v.HostDeviceID != hostID || v.EndedAt != nil {
			continue
		}
		// Normal legacy/activation limits are at most three/four. Refuse an
		// unexpected oversized set instead of giving clients a partial cleanup.
		if len(out) >= 16 {
			fail(w, http.StatusConflict, "session_list_capacity")
			return
		}
		out = append(out, cloneSession(v))
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].CreatedAt.Equal(out[j].CreatedAt) {
			return out[i].ID < out[j].ID
		}
		return out[i].CreatedAt.Before(out[j].CreatedAt)
	})
	writeJSON(w, http.StatusOK, map[string]any{"sessions": out})
}
func (s *Server) createSession(w http.ResponseWriter, r *http.Request) {
	if !method(w, r, "POST") {
		return
	}
	var in struct {
		DisplayID           string          `json:"displayId"`
		MonitorID           json.RawMessage `json:"monitorId,omitempty"`
		ControllerName      string          `json:"controllerName"`
		ControllerPublicKey string          `json:"controllerPublicKey,omitempty"`
		Permissions         PermissionSet   `json:"permissions"`
	}
	if readJSON(w, r, &in) != nil {
		fail(w, 400, "invalid_json")
		return
	}
	display, ok := displayID(in.DisplayID)
	if !ok || !boundedName(in.ControllerName) || !in.Permissions.ViewScreen {
		fail(w, 400, "invalid_session_request")
		return
	}
	monitor, validMonitor := optionalMonitorID(in.MonitorID)
	if !validMonitor {
		fail(w, 400, "invalid_monitor_id")
		return
	}
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
	ctl, ok := s.controllerLocked(r)
	if !ok {
		fail(w, 401, "controller_auth_required")
		return
	}
	if in.ControllerPublicKey != "" && in.ControllerPublicKey != ctl.PublicKey {
		fail(w, 403, "controller_key_mismatch")
		return
	}
	h := s.byDisplay[display]
	if h == nil {
		fail(w, 404, "device_not_found")
		return
	}
	if status, code := s.sessionAdmissionLocked(h, ctl); status != 0 {
		fail(w, status, code)
		return
	}
	now := s.now()
	v := &Session{ID: id, HostDeviceID: h.ID, HostDisplayID: h.DisplayID, HostPublicKey: h.PublicKey, HostName: h.Name, MonitorID: monitor, ControllerName: in.ControllerName, ControllerPublicKey: ctl.PublicKey, ControllerTokenHash: hashToken(bearer), Status: "pending", Permissions: in.Permissions, CreatedAt: now, ExpiresAt: now.Add(s.cfg.PendingTTL)}
	s.sessions[id] = v
	payload := map[string]any{"controllerName": in.ControllerName, "controllerPublicKey": ctl.PublicKey, "controllerVerified": true, "permissions": in.Permissions, "expiresAt": v.ExpiresAt}
	if monitor != "" {
		payload["monitorId"] = monitor
	}
	s.eventLocked(h, "session.requested", id, payload)
	s.addAuditLocked(h.ID, id, "session.requested")
	if err = s.persistLocked(); err != nil {
		fail(w, 503, "storage_unavailable")
		return
	}
	writeJSON(w, 201, map[string]any{"session": cloneSession(v), "controllerToken": bearer})
}

// Both code/PIN modes and ordinary host approval share capacity, presence and
// controller-revocation gates. The caller holds s.mu.
func (s *Server) sessionBaseAdmissionLocked(h *Device, ctl controller) (int, string) {
	if h == nil {
		return http.StatusNotFound, "device_not_found"
	}
	if _, revoked := s.revoked[h.ID][ctl.PublicKey]; revoked {
		return http.StatusForbidden, "controller_revoked"
	}
	if !s.publicDeviceLocked(h).Online {
		return http.StatusConflict, "device_offline"
	}
	if len(s.sessions) >= s.cfg.MaxSessions {
		return http.StatusServiceUnavailable, "session_capacity"
	}
	return 0, ""
}

func (s *Server) sessionAdmissionLocked(h *Device, ctl controller) (int, string) {
	if status, code := s.sessionBaseAdmissionLocked(h, ctl); status != 0 {
		return status, code
	}
	if p := s.currentPairingLocked(h.ID); p != nil && p.Mode == activationMode {
		return http.StatusConflict, "host_busy"
	}
	hostPending, ctlPending := 0, 0
	for _, v := range s.sessions {
		if v.EndedAt != nil {
			continue
		}
		if v.HostDeviceID == h.ID {
			if v.Status != "pending" || v.Mode == activationMode {
				return http.StatusConflict, "host_busy"
			}
			hostPending++
		}
		if v.ControllerPublicKey == ctl.PublicKey {
			ctlPending++
		}
	}
	if hostPending >= 3 || ctlPending >= 5 {
		return http.StatusTooManyRequests, "too_many_sessions"
	}
	return 0, ""
}

// Only owner-created activation sessions can coexist. Pending sessions count
// against both the four-peer capacity and the one-input reservation, so racing
// redemptions cannot allocate multiple controllers before local approval.
func (s *Server) activationAdmissionLocked(h *Device, ctl controller, p *pairingGrant) (int, string, PermissionSet) {
	permissions := p.Permissions
	if status, code := s.sessionBaseAdmissionLocked(h, ctl); status != 0 {
		return status, code, permissions
	}
	hostCount, controllerCount := 0, 0
	for _, v := range s.sessions {
		if v.EndedAt != nil {
			continue
		}
		if v.HostDeviceID == h.ID {
			if v.Mode != activationMode || v.PairingID != p.ID {
				return http.StatusConflict, "host_busy", permissions
			}
			hostCount++
			if v.Permissions.ControlInput {
				permissions.ControlInput = false
			}
		}
		if v.ControllerPublicKey == ctl.PublicKey {
			controllerCount++
		}
	}
	if hostCount >= activationCapacity || controllerCount >= 5 {
		return http.StatusTooManyRequests, "too_many_sessions", permissions
	}
	return 0, "", permissions
}

// Monitor selectors are opaque host-issued values, not paths or authorization.
// RawMessage distinguishes an omitted field (primary monitor) from explicit
// null/empty values, which could otherwise silently select the wrong display.
func optionalMonitorID(raw json.RawMessage) (string, bool) {
	if len(raw) == 0 {
		return "", true
	}
	var value string
	if json.Unmarshal(raw, &value) != nil || len(value) != 64 {
		return "", false
	}
	for _, c := range value {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return "", false
		}
	}
	return value, true
}
func (s *Server) sessionRoutes(w http.ResponseWriter, r *http.Request) {
	parts := strings.Split(strings.TrimPrefix(r.URL.Path, "/v1/sessions/"), "/")
	if len(parts) > 2 || len(parts) < 1 || !safeID(parts[0]) {
		fail(w, 404, "not_found")
		return
	}
	sid := parts[0]
	action := ""
	if len(parts) == 2 {
		action = parts[1]
		if action == "" {
			fail(w, 404, "not_found")
			return
		}
	}
	if action == "signals" {
		s.pollSignals(w, r, sid)
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	v := s.sessions[sid]
	if v == nil {
		fail(w, 404, "session_not_found")
		return
	}
	role, ok := s.sessionRoleLocked(r, v)
	if !ok {
		fail(w, 401, "session_auth_required")
		return
	}
	switch action {
	case "":
		if method(w, r, "GET") {
			writeJSON(w, 200, map[string]any{"session": cloneSession(v)})
		}
	case "approve":
		s.approveLocked(w, r, v, role)
	case "reject":
		if !method(w, r, "POST") {
			return
		}
		if role != "host" {
			fail(w, 403, "host_auth_required")
			return
		}
		if v.Status != "pending" {
			fail(w, 409, "invalid_state")
			return
		}
		s.endLocked(v, "rejected", "session.rejected")
		if s.persistLocked() != nil {
			fail(w, 503, "storage_unavailable")
			return
		}
		writeJSON(w, 200, map[string]any{"session": cloneSession(v)})
	case "end":
		if !method(w, r, "POST") {
			return
		}
		if v.EndedAt == nil {
			s.endLocked(v, "ended", "session.ended")
			if s.persistLocked() != nil {
				fail(w, 503, "storage_unavailable")
				return
			}
		}
		writeJSON(w, 200, map[string]any{"session": cloneSession(v)})
	case "signal":
		s.signalLocked(w, r, v, role)
	case "ice":
		if !method(w, r, "GET") {
			return
		}
		if !live(v) {
			fail(w, 409, "session_not_approved")
			return
		}
		s.iceLocked(w, v, role)
	case "connected":
		if !method(w, r, "POST") {
			return
		}
		if !live(v) || v.awaitingAnswer || v.SignalSeq == 0 {
			fail(w, 409, "invalid_state")
			return
		}
		if v.Status != "active" {
			v.Status = "active"
			s.addAuditLocked(v.HostDeviceID, v.ID, "session.connected")
			if s.persistLocked() != nil {
				fail(w, 503, "storage_unavailable")
				return
			}
		}
		writeJSON(w, 200, map[string]any{"session": cloneSession(v)})
	default:
		fail(w, 404, "not_found")
	}
}
func live(v *Session) bool {
	return v.EndedAt == nil && (v.Status == "approved" || v.Status == "connecting" || v.Status == "active")
}
func (s *Server) approveLocked(w http.ResponseWriter, r *http.Request, v *Session, role string) {
	if !method(w, r, "POST") {
		return
	}
	if role != "host" {
		fail(w, 403, "host_auth_required")
		return
	}
	if v.Status != "pending" {
		fail(w, 409, "invalid_state")
		return
	}
	granted := v.Permissions
	if r.ContentLength != 0 {
		var in struct {
			Permissions *PermissionSet `json:"permissions"`
		}
		if readJSON(w, r, &in) != nil {
			fail(w, 400, "invalid_json")
			return
		}
		if in.Permissions != nil {
			granted = *in.Permissions
		}
	}
	if !granted.ViewScreen || !granted.subsetOf(v.Permissions) {
		fail(w, 403, "permission_escalation")
		return
	}
	var activation *pairingGrant
	if v.Mode == activationMode {
		activation = s.currentPairingLocked(v.HostDeviceID)
		if activation == nil || activation.Mode != activationMode || activation.ID != v.PairingID {
			fail(w, http.StatusConflict, "activation_inactive")
			return
		}
	}
	for _, other := range s.sessions {
		if other.ID == v.ID || other.HostDeviceID != v.HostDeviceID || other.EndedAt != nil {
			continue
		}
		if activation == nil && (live(other) || other.Mode == activationMode) || activation != nil && (other.Mode != activationMode || other.PairingID != v.PairingID) {
			fail(w, 409, "host_busy")
			return
		}
		if activation != nil && granted.ControlInput && other.Permissions.ControlInput {
			fail(w, http.StatusConflict, "input_busy")
			return
		}
	}
	now := s.now()
	v.Permissions = granted
	v.Status = "approved"
	v.ApprovedAt = &now
	v.ExpiresAt = now.Add(s.cfg.SessionTTL)
	if activation != nil && activation.ExpiresAt.Before(v.ExpiresAt) {
		v.ExpiresAt = activation.ExpiresAt
	}
	s.addAuditLocked(v.HostDeviceID, v.ID, "session.approved")
	if s.persistLocked() != nil {
		fail(w, 503, "storage_unavailable")
		return
	}
	writeJSON(w, 200, map[string]any{"session": cloneSession(v)})
}
func (s *Server) endLocked(v *Session, status, event string) {
	now := s.now()
	v.Status = status
	v.EndedAt = &now
	v.Signals = nil
	s.eventLocked(s.devices[v.HostDeviceID], "session.ended", v.ID, map[string]string{"reason": event})
	s.addAuditLocked(v.HostDeviceID, v.ID, event)
}
func validateSignal(kind string, raw json.RawMessage) bool {
	if len(raw) == 0 || len(raw) > 256*1024 {
		return false
	}
	d := json.NewDecoder(bytes.NewReader(raw))
	d.DisallowUnknownFields()
	switch kind {
	case "offer", "answer":
		var v struct {
			Type string `json:"type"`
			SDP  string `json:"sdp"`
		}
		if d.Decode(&v) != nil {
			return false
		}
		return v.Type == kind && strings.HasPrefix(v.SDP, "v=0") && len(v.SDP) >= 5 && len(v.SDP) <= 256*1024
	case "ice":
		var v struct {
			Candidate        string  `json:"candidate"`
			SDPMid           *string `json:"sdpMid"`
			SDPMLineIndex    *uint16 `json:"sdpMLineIndex"`
			UsernameFragment *string `json:"usernameFragment,omitempty"`
		}
		if d.Decode(&v) != nil || len(v.Candidate) > 4096 {
			return false
		}
		if v.SDPMid != nil && len(*v.SDPMid) > 64 {
			return false
		}
		if v.UsernameFragment != nil && len(*v.UsernameFragment) > 256 {
			return false
		}
		if v.SDPMLineIndex != nil && *v.SDPMLineIndex > 255 {
			return false
		}
		return v.Candidate == "" || strings.HasPrefix(v.Candidate, "candidate:")
	case "renegotiate":
		var v struct {
			ICERestart bool `json:"iceRestart"`
		}
		return d.Decode(&v) == nil && v.ICERestart
	}
	return false
}
func (s *Server) signalLocked(w http.ResponseWriter, r *http.Request, v *Session, role string) {
	if !method(w, r, "POST") {
		return
	}
	if !live(v) {
		fail(w, 409, "session_not_approved")
		return
	}
	var in struct {
		Kind    string          `json:"kind"`
		Payload json.RawMessage `json:"payload"`
	}
	if readJSON(w, r, &in) != nil {
		fail(w, 400, "invalid_json")
		return
	}
	if !validateSignal(in.Kind, in.Payload) {
		fail(w, 400, "invalid_signal")
		return
	}
	if in.Kind == "offer" && (v.awaitingAnswer || (v.SignalSeq == 0 && role != "controller")) {
		fail(w, 409, "invalid_negotiation_state")
		return
	}
	if in.Kind == "answer" && (!v.awaitingAnswer || v.lastOfferFrom == role) {
		fail(w, 409, "invalid_negotiation_state")
		return
	}
	countBytes, totalBytes := 0, 0
	for _, a := range v.Signals {
		countBytes += len(a.Payload)
	}
	for _, ss := range s.sessions {
		for _, a := range ss.Signals {
			totalBytes += len(a.Payload)
		}
	}
	if len(v.Signals) >= s.cfg.MaxQueue || countBytes+len(in.Payload) > 1024*1024 || totalBytes+len(in.Payload) > 32*1024*1024 {
		fail(w, 409, "signal_capacity")
		return
	}
	if in.Kind == "offer" {
		v.awaitingAnswer = true
		v.lastOfferFrom = role
		v.Status = "connecting"
	}
	if in.Kind == "answer" {
		v.awaitingAnswer = false
	}
	v.SignalSeq++
	v.Signals = append(v.Signals, Signal{Seq: v.SignalSeq, From: role, Kind: in.Kind, Payload: append(json.RawMessage(nil), in.Payload...), CreatedAt: s.now()})
	writeJSON(w, 202, map[string]int64{"seq": v.SignalSeq})
}
func (s *Server) pollSignals(w http.ResponseWriter, r *http.Request, sid string) {
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
		if s.storageErr != nil || s.closed {
			s.mu.Unlock()
			fail(w, 503, "storage_unavailable")
			return
		}
		v := s.sessions[sid]
		if v == nil {
			s.mu.Unlock()
			fail(w, 404, "session_not_found")
			return
		}
		role, ok := s.sessionRoleLocked(r, v)
		if !ok {
			s.mu.Unlock()
			fail(w, 401, "session_auth_required")
			return
		}
		if !s.now().Before(v.ExpiresAt) || !live(v) {
			s.mu.Unlock()
			fail(w, 409, "session_not_approved")
			return
		}
		if after > v.SignalSeq {
			s.mu.Unlock()
			fail(w, 400, "invalid_cursor")
			return
		}
		out := []Signal{}
		for _, sig := range v.Signals {
			if sig.Seq > after && sig.From != role {
				out = append(out, sig)
			}
		}
		seq := v.SignalSeq
		s.mu.Unlock()
		if len(out) > 0 {
			writeJSON(w, 200, map[string]any{"signals": out, "cursor": seq})
			return
		}
		select {
		case <-r.Context().Done():
			return
		case <-deadline.C:
			writeJSON(w, 200, map[string]any{"signals": out, "cursor": seq})
			return
		case <-tick.C:
		}
	}
}
