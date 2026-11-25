package app

import (
	"context"
	"errors"
	"net/http"
	"net/netip"
)

// Only admitted middleware requests receive this context value. Reading a
// client-supplied header inside a later pairing/registration handler is unsafe.
type gatewayClientIPKey struct{}

func validateGatewayToken(value string) error {
	if value == "" {
		return nil
	}
	valid := len(value) >= 32 && len(value) <= 512
	for _, c := range []byte(value) {
		valid = valid && c >= 33 && c <= 126
	}
	if !valid {
		return errors.New("OPENREMOTE_GATEWAY_TOKEN must be empty or 32-512 printable non-space ASCII characters")
	}
	return nil
}

// The publisher-to-coordinator token proves the source of a single canonical
// client address. It grants no host/controller/PIN permissions and does not
// establish transport security or turn the claimed client into a trusted proxy.
func (s *Server) authenticateGateway(r *http.Request) (*http.Request, bool) {
	tokens := r.Header.Values("X-OpenRemote-Gateway-Token")
	ips := r.Header.Values("X-OpenRemote-Client-IP")
	if s.cfg.GatewayToken == "" || len(tokens) != 1 || len(ips) != 1 ||
		!sameToken(hashToken(tokens[0]), hashToken(s.cfg.GatewayToken)) {
		return r, false
	}
	ip, err := netip.ParseAddr(ips[0])
	if err != nil || ip.Zone() != "" || ip.Unmap().String() != ips[0] {
		return r, false
	}
	return r.WithContext(context.WithValue(r.Context(), gatewayClientIPKey{}, ips[0])), true
}
