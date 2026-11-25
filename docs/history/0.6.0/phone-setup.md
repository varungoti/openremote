# Phone setup and diagnostics — version 1

Implemented in the 0.5 development candidate. This contract reduces manual
entry of a server origin and a PC number. It does not authenticate a PC, grant
access, enroll a host, or prove that a relay can carry video.

## Setup link

The Windows host displays a setup link only after it has an enrolled PC number
and a valid HTTPS coordinator origin. The canonical form is:

```text
openremote://connect?v=1&server=https%3A%2F%2Fremote.example.com&pc=123456789
```

Only `v`, `server` and `pc` are allowed, each exactly once, with nonempty values.
Field order does not carry meaning. The host emits the order above. `v` is `1`.
`pc` is exactly nine ASCII decimal digits. The outer URI uses the exact lowercase
scheme `openremote` and authority `connect`, with no user information, port,
path, fragment, whitespace or controls. The complete link is ASCII and at most
2,048 bytes. Malformed percent escapes and invalid UTF-8 are rejected. Unknown
or duplicate query fields, including equivalent percent-encoded duplicates,
are rejected. Values are decoded once.

`server` contains an HTTPS origin: host and optional port, with no credentials,
non-root path, query or fragment. A root slash is normalized away. The host uses
the URL library's canonical ASCII host spelling. A custom port and bracketed
IPv6 address may be used with a valid platform-trusted certificate. Development
HTTP origins do not produce a phone setup link. The link never carries an
enrollment secret, bearer token, private or public identity key, permission,
monitor selector or device name.

The generator restricts host spelling to canonical ASCII DNS/IP names accepted
by both clients. If a legacy configuration can run but cannot be exported under
this contract, the host home remains visible and explains that link export is
disabled. Manual configuration remains available; the app never emits a repaired
or misleading link for that origin.

The Android app accepts an explicitly pasted link or an incoming `ACTION_VIEW`
intent. Opening or parsing it only stages a review. The app displays the full
server origin and PC number, explains that a server change clears the current
server's saved PC list, and waits for the user to confirm. Before confirmation
there is no new network request, login, saved-setting change or remote session.
Cancel discards the staged import. An active or busy session cannot have its
server changed by an incoming link. Cold start, rotation and new intents must
not let asynchronous settings loading replace an accepted choice or repeat an
already handled import.

Confirmation stores the chosen origin, fills the PC number and persists a saved
PC entry so setup survives a phone process restart. Until authenticated lookup,
a new entry has the neutral local label `PC 123456789`; an existing name is kept
only on the same server. Cross-server names are not carried over, duplicate IDs
are removed and the list remains bounded to 100 entries. Lookup/checks
and remote connection remain explicit actions. Normal P-256 phone identity,
host approval, permission narrowing and revocation continue to apply. A setup
link can be copied by anyone who sees it; it is an address, not a credential.

Custom URI schemes are not verified web-domain App Links. If Android offers
another handler or does not open the link, open OpenRemote and paste it in its
setup screen. Compare the displayed server origin with the PC before confirming.
The implementation follows Android's documented deep-link/intent lifecycle;
the real device intent chooser and cold/warm-start flow remain pilot checks.

## Coordinator discovery

`GET /v1/info` is unauthenticated and returns bounded, nonsecret configuration:

```json
{
  "service": "openremote",
  "protocolVersion": 1,
  "version": "0.5.0",
  "phoneSetupVersion": 1,
  "monitorSelection": true,
  "relay": {"configured": true, "udp": true, "tcp": true, "tls": true},
  "relayOnly": false
}
```

It shares HTTPS enforcement, `Cache-Control: no-store`, request limits, storage
failure/closed-state rejection and IP rate limits with the other routes. Other
methods return 405. It creates no challenges, controller tokens, hosts or
sessions, and exposes no relay address, relay credentials, PC identities or
enrollment configuration. The protocol and setup versions must be supported;
the application version is informational and is not a hard equality check.

Relay flags describe configured client-to-relay transports. `udp` means `turn:`
with UDP (the default), `tcp` means `turn:` with `transport=tcp`, and `tls` means
`turns:` over TLS/TCP. These flags do not attest DNS, certificates, firewall
ports, authentication, allocation, selected candidates or moving video. An
actual remote session and the direct/TURN pilot cases are still required.

Coordinator startup accepts the documented RFC 7064/7065 provisioning subset:
ASCII DNS names or IPv4/bracketed IPv6, optional port 1–65535, no user
information, path, fragment, percent escapes or whitespace. STUN URLs have no
query. TURN queries are absent, `transport=udp`, or `transport=tcp`, exactly
once. This implementation supports `turns:` with TCP only. Ambiguous or
unsupported values fail startup rather than advertise misleading diagnostics.
Existing deployments with malformed strings must correct them before upgrade.

## Explicit phone checks

The phone checks the chosen endpoint in stages: valid HTTPS origin, compatible
OpenRemote discovery response, phone identity proof and (when a PC number is
provided) the authenticated PC lookup/online status. It reports the actual
failed stage with an action such as checking the server address/certificate,
starting the host, waiting after a rate limit or updating an older coordinator.

Network redirects are disabled. No check disables TLS verification, creates a
remote desktop session, obtains TURN credentials or claims transport success.
Changing the endpoint or cancelling the operation invalidates stale results.
Diagnostics do not print tokens, key material, SDP, clipboard/file contents or
raw server error bodies. Endpoint and PC metadata shown on the owner's screen
are never sent to a third-party support service.

## Physical acceptance checks (not yet executed)

1. Install the candidate APK on the target phone; launch it from the icon.
2. Start the candidate host and verify its persistent window, PC number and
   origin. Copy the link with its button and transfer it by the owner's chosen
   method. Check that the link contains only the three documented fields.
3. Paste/import on a fresh phone. Before confirmation, capture coordinator
   request counts and confirm there is no request. Cancel and check saved
   settings. Repeat and confirm; the PC number must be filled exactly.
4. Repeat with Android cold start, already-running app, rotation, rapid repeated
   links, a different server, an active remote session and an open SAF document
   picker. Settings/auth/session
   ownership must stay bound to the selected server.
5. Run checks with a valid server, wrong/non-OpenRemote URL, invalid TLS, an older
   coordinator, offline/unknown PC and disabled relay. Verify precise stages.
6. Connect, approve locally and demonstrate moving frames, pointer and keyboard
   on the chosen PC from a separate network. Record the actual candidate route.
7. Close the host home window during streaming; confirm session teardown,
   capture-process exit and held-key release. Repeat Stop and console Ctrl+C.
8. Test keyboard navigation, screen sizes/font scaling and link selection/copy
   on Windows, plus Android intent handling and phone layout/accessibility.

These supplement the 17 required scenarios in `docs/pilot-validation.md`.
Compilation or pure parser tests do not satisfy these native checks.

## Primary references

- Android deep links: https://developer.android.com/training/app-links/create-deeplinks
- Android task/intent lifecycle: https://developer.android.com/guide/components/activities/tasks-and-back-stack
- Windows thread-owned window destruction: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-destroywindow
- STUN URI syntax: https://www.rfc-editor.org/rfc/rfc7064
- TURN URI syntax: https://www.rfc-editor.org/rfc/rfc7065
