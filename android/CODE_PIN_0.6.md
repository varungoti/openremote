# Android code + PIN client — 0.6.0

The primary home screen connects using the host's **six-digit code** and the
**6–12 digit security PIN** set when generating it. Leading zeroes are preserved.
The phone first checks for pairing protocol version 1, proves its existing P-256
identity, and redeems the credentials in an HTTPS JSON request. The result enters
the existing bounded approval, media, display binding and input lifecycle.

The PIN field uses password input, is not saved in Compose saved state, settings,
favorites or links, and clears on submission, cancellation, leaving the home
screen or pausing the Activity. Screenshots/recent-app previews remain protected
by `FLAG_SECURE`. A managed runtime may retain transient string copies until
garbage collection; this implementation does not claim guaranteed memory erasure.

## Service settings

Both native clients read the same public `../config/service-profile.json`. A
configured HTTPS origin becomes the initial encrypted setting. A saved origin
always takes precedence; an update cannot silently move an existing controller
to another coordinator. No enrollment key, PIN, session bearer or TURN secret is
embedded in the profile.

The checked-in profile has an empty origin because this project has no deployed
public OpenRemote service. Such a build visibly requests a one-time service
address or a reviewed host setup-link import. Once configured, the main flow
needs only code + PIN. Service settings, diagnostics, nine-digit PC lookup and
saved computers are under **Advanced**.

## One-use behavior and cancellation

Pairing grants screen access and, only if the host grants it, keyboard/mouse
control. It grants no clipboard, file transfer, audio or power capability.
Permission ceilings and pairing identity are checked again during session polls.
A monitor switch ends this session and requests fresh host approval; the UI says
so explicitly. It does not reuse the consumed code or silently retain trust.

The redemption POST is not replayed on authentication, transport, 408 or 503
responses. Both OkHttp connection retries and follow-up body replays are disabled
for that request. Before the POST, cancellation stops the operation normally.
After sending it, the request has a 35-second network bound and settles its
response even after UI cancellation so a known late session can be ended with a
bounded five-second cleanup. Generation checks prevent late results from opening
a session in a newer UI or service. If a response is lost completely, the client
cannot recover its session token from a consumed code: it tells the user to
generate another code. The host/coordinator's existing pending/connection expiry
and visible stop control remain necessary.

## Installing the preview

The original development signing key was unavailable in the recovered workspace.
The new debug build therefore uses package **`com.openremote.mobile.dev`**, with
the launcher name **OpenRemote Preview**, and a fresh development certificate.
It installs as a separate app rather than attempting an incompatible update of
the previous `com.openremote.mobile` test app. Existing app settings and keys are
not changed or deleted. The preview creates its own Android Keystore identity
and needs service setup or a configured build profile.

Both applications may register the same `openremote://connect` address scheme;
Android may show an app chooser. Select **OpenRemote Preview** for the new client.
The direct code + PIN screen does not use that link handler.

The unsigned release build retains package **`com.openremote.mobile`** and the
OpenRemote name. It requires an owner-controlled production signing key and
device/network release validation before public distribution. The debug APK is
development software, not a production signing identity or evidence of a real
phone-to-PC session. Build/test evidence is stored separately under
`verification/code-pin-0.6.0/`.

The final preview signing key is retained separately as a private development
checkpoint. Keep it outside source bundles and public downloads. Later updates
to `com.openremote.mobile.dev` must use that same key; otherwise Android rejects
an in-place update. Its public certificate fingerprint, alias and validity are
recorded with the build evidence. No production release key is supplied.
