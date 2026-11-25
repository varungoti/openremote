package app

import (
	"crypto/hmac"
	"crypto/sha1" // coturn REST credentials require HMAC-SHA1, not a password hash.
	"encoding/base64"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

type iceServer struct {
	URLs       []string `json:"urls"`
	Username   string   `json:"username,omitempty"`
	Credential string   `json:"credential,omitempty"`
}

func validateICEConfig(c Config) error {
	if len(c.TURNURLs) > 0 && len(c.TURNSecret) < 32 {
		return errors.New("TURN secret must contain at least 32 characters")
	}
	if c.ICERelayOnly && len(c.TURNURLs) == 0 {
		return errors.New("relay-only mode requires a TURN server")
	}
	for _, v := range c.STUNURLs {
		scheme, _, err := parseICEURL(v)
		if err != nil {
			return err
		}
		if scheme != "stun" && scheme != "stuns" {
			return errors.New("STUN URLs must use stun or stuns")
		}
	}
	for _, v := range c.TURNURLs {
		scheme, _, err := parseICEURL(v)
		if err != nil {
			return err
		}
		if scheme != "turn" && scheme != "turns" {
			return errors.New("TURN URLs must use turn or turns")
		}
	}
	return nil
}

// Accept the explicitly supported RFC 7064/7065 provisioning subset: ASCII
// DNS/IP hosts, optional nonzero port, TURN UDP/TCP and TURN TLS-over-TCP.
// Queries are literal and unique; ambiguous encodings cannot make diagnostics
// describe one transport while a client parser selects another.
func parseICEURL(value string) (scheme, transport string, err error) {
	invalid := func() (string, string, error) { return "", "", errors.New("invalid or unsupported ICE URL") }
	if len(value) == 0 || len(value) > 1024 {
		return invalid()
	}
	for _, b := range []byte(value) {
		if b <= 32 || b >= 127 {
			return invalid()
		}
	}
	if strings.ContainsAny(value, "@#%\\/") {
		return invalid()
	}
	u, e := url.Parse(value)
	if e != nil || u.Opaque == "" || u.Host != "" || u.User != nil || u.Fragment != "" {
		return invalid()
	}
	scheme = u.Scheme
	// net/url lowercases the scheme, but clients consume the configured bytes.
	// Keep the wire form canonical instead of advertising a URI they reject.
	if !strings.HasPrefix(value, scheme+":") {
		return invalid()
	}
	if scheme != "stun" && scheme != "stuns" && scheme != "turn" && scheme != "turns" {
		return invalid()
	}
	authority, e := url.Parse("https://" + u.Opaque)
	if e != nil || authority.Host == "" || authority.User != nil || authority.Path != "" {
		return invalid()
	}
	host := authority.Hostname()
	if strings.Contains(host, ":") {
		if !strings.HasPrefix(u.Opaque, "[") || net.ParseIP(host) == nil {
			return invalid()
		}
	} else {
		if strings.ContainsAny(u.Opaque, "[]") {
			return invalid()
		}
		dns := strings.TrimSuffix(host, ".")
		if len(dns) == 0 || len(dns) > 253 {
			return invalid()
		}
		for _, label := range strings.Split(dns, ".") {
			if len(label) == 0 || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' {
				return invalid()
			}
			for _, b := range []byte(label) {
				if !(b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || b >= '0' && b <= '9' || b == '-') {
					return invalid()
				}
			}
		}
	}
	if strings.HasSuffix(u.Opaque, ":") {
		return invalid()
	}
	if port := authority.Port(); port != "" {
		n, e := strconv.Atoi(port)
		if e != nil || n < 1 || n > 65535 {
			return invalid()
		}
	}
	if (u.ForceQuery && u.RawQuery == "") || (strings.HasPrefix(scheme, "stun") && u.RawQuery != "") {
		return invalid()
	}
	switch u.RawQuery {
	case "":
		transport = "udp"
	case "transport=udp":
		if scheme == "turns" {
			return invalid()
		}
		transport = "udp"
	case "transport=tcp":
		transport = "tcp"
	default:
		return invalid()
	}
	if scheme == "turns" || scheme == "stuns" {
		transport = "tls"
	}
	return scheme, transport, nil
}
func (s *Server) iceLocked(w http.ResponseWriter, v *Session, role string) {
	servers := []iceServer{}
	if len(s.cfg.STUNURLs) > 0 {
		servers = append(servers, iceServer{URLs: s.cfg.STUNURLs})
	}
	expires := s.now().Add(10 * time.Minute)
	if v.ExpiresAt.Before(expires) {
		expires = v.ExpiresAt
	}
	if len(s.cfg.TURNURLs) > 0 {
		username := fmt.Sprintf("%d:%s:%s", expires.Unix(), v.ID, role)
		m := hmac.New(sha1.New, []byte(s.cfg.TURNSecret))
		_, _ = m.Write([]byte(username))
		servers = append(servers, iceServer{URLs: s.cfg.TURNURLs, Username: username, Credential: base64.StdEncoding.EncodeToString(m.Sum(nil))})
	}
	policy := "all"
	if s.cfg.ICERelayOnly {
		policy = "relay"
	}
	writeJSON(w, 200, map[string]any{"iceServers": servers, "iceTransportPolicy": policy, "expiresAt": expires})
}
