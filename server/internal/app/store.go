package app

import (
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"
)

type diskDevice struct {
	Device                Device     `json:"device"`
	TokenHash             [32]byte   `json:"tokenHash"`
	EventSeq              *int64     `json:"eventSeq,omitempty"`
	AutomaticRegistration bool       `json:"automaticRegistration,omitempty"`
	RegistrationRevokedAt *time.Time `json:"registrationRevokedAt,omitempty"`
}
type snapshot struct {
	Version  int                                     `json:"version"`
	Devices  []diskDevice                            `json:"devices"`
	Trusted  map[string]map[string]TrustedController `json:"trusted"`
	Revoked  map[string]map[string]time.Time         `json:"revoked,omitempty"`
	Audit    []AuditEvent                            `json:"audit"`
	AuditSeq int64                                   `json:"auditSeq"`
}

func (s *Server) load() error {
	if s.cfg.StatePath == "" {
		return nil
	}
	st, err := os.Lstat(s.cfg.StatePath)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("stat state: %w", err)
	}
	if !st.Mode().IsRegular() || st.Mode().Perm()&0077 != 0 {
		return errors.New("state file must be a regular private file (0600)")
	}
	f, err := os.Open(s.cfg.StatePath)
	if err != nil {
		return err
	}
	defer f.Close()
	var v snapshot
	dec := json.NewDecoder(io.LimitReader(f, 64*1024*1024+1))
	dec.DisallowUnknownFields()
	if err = dec.Decode(&v); err != nil {
		return fmt.Errorf("decode state: %w", err)
	}
	var extra any
	if dec.Decode(&extra) != io.EOF {
		return errors.New("trailing state content")
	}
	if (v.Version != 1 && v.Version != 2) || len(v.Devices) > s.cfg.MaxDevices || len(v.Audit) > s.cfg.MaxAudit || v.AuditSeq < 0 {
		return errors.New("invalid state version or capacity")
	}
	for _, row := range v.Devices {
		d := row.Device
		canonicalID, ok := displayID(d.DisplayID)
		key, keyErr := base64.StdEncoding.Strict().DecodeString(d.PublicKey)
		if !ok || canonicalID != d.DisplayID || !safeID(d.ID) || s.devices[d.ID] != nil || s.byDisplay[d.DisplayID] != nil || !boundedName(d.Name) || d.Platform != "windows" || keyErr != nil || len(key) != 32 || row.TokenHash == [32]byte{} {
			return errors.New("invalid or duplicate persisted device")
		}
		d.PublicKey = base64.StdEncoding.EncodeToString(key)
		if s.byPublicKey[d.PublicKey] != nil || (v.Version == 1 && (row.AutomaticRegistration || row.RegistrationRevokedAt != nil)) || (row.RegistrationRevokedAt != nil && row.RegistrationRevokedAt.IsZero()) {
			return errors.New("invalid persisted host identity or registration flags")
		}
		if row.EventSeq != nil && (*row.EventSeq < 0 || *row.EventSeq > v.AuditSeq) {
			return errors.New("invalid persisted host event sequence")
		}
		d.TokenHash = row.TokenHash
		d.AutomaticRegistration = row.AutomaticRegistration
		d.RegistrationRevokedAt = row.RegistrationRevokedAt
		d.Online = false
		d.Events = nil
		// Payloads and sessions are ephemeral, but sequence numbers must never
		// be reused: a host may retain its polling cursor across server restart.
		// Legacy snapshots omitted this field. Every emitted host event also
		// appends an audit event before the same snapshot is committed, so the
		// durable global audit sequence is a safe upper bound for that cursor.
		d.EventSeq = v.AuditSeq
		if row.EventSeq != nil {
			d.EventSeq = *row.EventSeq
		}
		s.devices[d.ID] = &d
		s.byDisplay[d.DisplayID] = &d
		s.byPublicKey[d.PublicKey] = &d
	}
	for id, entries := range v.Trusted {
		if s.devices[id] == nil || len(entries) > 50 {
			return errors.New("invalid persisted trust registry")
		}
		for key, c := range entries {
			_, canonical, err := parseControllerKey(key)
			if err != nil || canonical != key || c.PublicKey != key {
				return errors.New("invalid persisted controller key")
			}
		}
	}
	for id, entries := range v.Revoked {
		if s.devices[id] == nil || len(entries) > 1000 {
			return errors.New("invalid revoked controller registry")
		}
		for key := range entries {
			_, canonical, err := parseControllerKey(key)
			if err != nil || key != canonical {
				return errors.New("invalid revoked key")
			}
			if _, exists := v.Trusted[id][key]; exists {
				return errors.New("controller both trusted and revoked")
			}
		}
	}
	s.revoked = v.Revoked
	if s.revoked == nil {
		s.revoked = map[string]map[string]time.Time{}
	}
	s.trusted = v.Trusted
	if s.trusted == nil {
		s.trusted = map[string]map[string]TrustedController{}
	}
	s.audit = v.Audit
	s.auditSeq = v.AuditSeq
	return nil
}
func (s *Server) persistLocked() (err error) {
	if s.cfg.StatePath == "" {
		return nil
	}
	defer func() {
		if err != nil {
			s.storageErr = err
			s.log.Printf("state storage unavailable: %v", err)
		}
	}()
	v := snapshot{Version: 2, Trusted: s.trusted, Revoked: s.revoked, Audit: s.audit, AuditSeq: s.auditSeq, Devices: []diskDevice{}}
	for _, d := range s.devices {
		seq := d.EventSeq
		v.Devices = append(v.Devices, diskDevice{Device: *d, TokenHash: d.TokenHash, EventSeq: &seq, AutomaticRegistration: d.AutomaticRegistration, RegistrationRevokedAt: d.RegistrationRevokedAt})
	}
	dir := filepath.Dir(s.cfg.StatePath)
	if err = os.MkdirAll(dir, 0700); err != nil {
		return err
	}
	if st, e := os.Lstat(s.cfg.StatePath); e == nil && !st.Mode().IsRegular() {
		return errors.New("state path is not a regular file")
	} else if e != nil && !os.IsNotExist(e) {
		return e
	}
	f, err := os.CreateTemp(dir, ".controlplane-*.tmp")
	if err != nil {
		return err
	}
	tmp := f.Name()
	defer os.Remove(tmp)
	if err = f.Chmod(0600); err == nil {
		err = json.NewEncoder(f).Encode(v)
	}
	if err == nil {
		err = f.Sync()
	}
	closeErr := f.Close()
	if err == nil {
		err = closeErr
	}
	if err != nil {
		return err
	}
	if err = os.Rename(tmp, s.cfg.StatePath); err != nil {
		return err
	}
	d, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer d.Close()
	return d.Sync()
}
