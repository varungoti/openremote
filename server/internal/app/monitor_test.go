package app

import (
	"encoding/json"
	"strings"
	"testing"
)

// Exercise the authenticated HTTP contract, not the host's selector algorithm.
func TestMonitorSelectorSessionContract(t *testing.T) {
	h := newTestServer(t, testConfig()).Handler()
	host := enrollTest(t, h)
	auth, _ := loginTest(t, h)
	monitor := strings.Repeat("0123456789abcdef", 4)
	body := map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Display test phone", "monitorId": monitor, "permissions": PermissionSet{ViewScreen: true}}
	wantCode(t, call(t, h, "POST", "/v1/sessions", "", body), 401)
	r := call(t, h, "POST", "/v1/sessions", auth, body)
	wantCode(t, r, 201)
	ss := decode[sessionResult](t, r)
	path := "/v1/sessions/" + ss.Session.ID
	assertMonitor := func(body []byte) {
		t.Helper()
		var got struct {
			Session struct {
				MonitorID   string        `json:"monitorId"`
				Permissions PermissionSet `json:"permissions"`
			} `json:"session"`
		}
		if err := json.Unmarshal(body, &got); err != nil {
			t.Fatal(err)
		}
		if got.Session.MonitorID != monitor || got.Session.Permissions != (PermissionSet{ViewScreen: true}) {
			t.Fatalf("monitor binding or view-only permission changed: %s", body)
		}
	}
	assertMonitor(r.Body.Bytes())
	for _, bearer := range []string{host.Token, ss.Token} {
		r := call(t, h, "GET", path, bearer, nil)
		wantCode(t, r, 200)
		assertMonitor(r.Body.Bytes())
	}
	wantCode(t, call(t, h, "GET", path, auth, nil), 401)
	events := call(t, h, "GET", "/v1/hosts/"+host.Device.ID+"/events?after=0", host.Token, nil)
	wantCode(t, events, 200)
	var eventBody struct {
		Events []HostEvent `json:"events"`
	}
	if err := json.Unmarshal(events.Body.Bytes(), &eventBody); err != nil {
		t.Fatal(err)
	}
	found := false
	for _, event := range eventBody.Events {
		if event.Type != "session.requested" || event.SessionID != ss.Session.ID {
			continue
		}
		var payload struct {
			MonitorID string `json:"monitorId"`
		}
		if err := json.Unmarshal(event.Payload, &payload); err != nil {
			t.Fatal(err)
		}
		found = payload.MonitorID == monitor
	}
	if !found {
		t.Fatal("authenticated host event lost the monitor selector")
	}
	// The selector is fixed at creation; even the approving host cannot replace it.
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true}, "monitorId": strings.Repeat("a", 64)}), 400)
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true, ControlInput: true}}), 403)
	wantCode(t, call(t, h, "POST", path+"/approve", host.Token, map[string]any{"permissions": PermissionSet{ViewScreen: true}}), 200)
	assertMonitor(call(t, h, "GET", path, ss.Token, nil).Body.Bytes())
	wantCode(t, call(t, h, "POST", path+"/end", ss.Token, nil), 200)
}

func TestMonitorSelectorValidationAndLegacy(t *testing.T) {
	cases := []struct {
		name  string
		value any
	}{
		{"empty", ""}, {"null", nil}, {"uppercase", strings.Repeat("A", 64)},
		{"short", strings.Repeat("a", 63)}, {"long", strings.Repeat("a", 65)},
		{"non_hex", strings.Repeat("g", 64)}, {"integer", 123}, {"boolean", true},
		{"object", map[string]any{"id": strings.Repeat("a", 64)}}, {"array", []string{strings.Repeat("a", 64)}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			h := newTestServer(t, testConfig()).Handler()
			host := enrollTest(t, h)
			auth, _ := loginTest(t, h)
			wantCode(t, call(t, h, "POST", "/v1/sessions", auth, map[string]any{"displayId": host.Device.DisplayID, "controllerName": "Phone", "permissions": PermissionSet{ViewScreen: true}, "monitorId": tc.value}), 400)
		})
	}
	t.Run("omitted_primary", func(t *testing.T) {
		h := newTestServer(t, testConfig()).Handler()
		host := enrollTest(t, h)
		auth, key := loginTest(t, h)
		ss := sessionTest(t, h, host, auth, key)
		r := call(t, h, "GET", "/v1/sessions/"+ss.Session.ID, host.Token, nil)
		wantCode(t, r, 200)
		var got map[string]json.RawMessage
		var envelope struct {
			Session json.RawMessage `json:"session"`
		}
		if err := json.Unmarshal(r.Body.Bytes(), &envelope); err != nil {
			t.Fatal(err)
		}
		if err := json.Unmarshal(envelope.Session, &got); err != nil {
			t.Fatal(err)
		}
		if _, present := got["monitorId"]; present {
			t.Fatal("legacy primary request unexpectedly acquired a selector")
		}
	})
}
