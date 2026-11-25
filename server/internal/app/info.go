package app

import "net/http"

// setupInfo is public configuration discovery, not a media/relay health probe.
// No host identifiers, URLs, public keys, credentials or ephemeral auth state
// are exposed or created. The ordinary HTTPS/storage/rate middleware applies.
func (s *Server) setupInfo(w http.ResponseWriter, r *http.Request) {
	if !method(w, r, "GET") {
		return
	}
	relay := map[string]bool{"configured": len(s.cfg.TURNURLs) > 0, "udp": false, "tcp": false, "tls": false}
	for _, value := range s.cfg.TURNURLs {
		_, transport, err := parseICEURL(value)
		if err == nil {
			relay[transport] = true
		}
	}
	automaticRegistrationVersion := 0
	if s.cfg.AutomaticHostRegistration {
		automaticRegistrationVersion = 1
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"service": "openremote", "protocolVersion": 1, "version": "0.8.0",
		"phoneSetupVersion": 1, "monitorSelection": true,
		"pairingCodeVersion": 1, "webController": true,
		"activationVersion": 1, "maxActivationConnections": activationCapacity,
		"automaticHostRegistrationVersion": automaticRegistrationVersion,
		"relay":                            relay, "relayOnly": s.cfg.ICERelayOnly,
	})
}
