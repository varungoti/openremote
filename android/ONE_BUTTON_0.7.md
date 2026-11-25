# Android 0.7 managed connection behavior

## Public surface

The home screen contains one heading, two entry fields and one primary button.
The labels are **Server ID**, **PIN**, and **Connect remote desktop**. Both inputs
accept exactly six ASCII digits at submission. The PIN is masked. Invalid pasted
characters or extra digits are kept visible as invalid input rather than silently
normalized into a different credential. No old nine-digit PC number is required.

The button changes to **Cancel** during the authenticated initial request and
remains enabled after the submitted PIN is cleared. Back also cancels that request.
A successful redemption creates the existing session screen, where **Stop remote
session** remains visible during host/media setup and while connected. Input is
still disabled until permission, display binding, decoded video and current-renderer
readiness gates pass. Showing the session before renderer readiness is necessary
to attach the renderer; delaying its creation until it is ready would deadlock it.

No Advanced, Settings, Import, QR, saved PC, check-connection, URL or fingerprint
control is present on home. The legacy setup scheme was removed from the manifest,
and MainActivity has no setup-intent processing. An explicitly targeted external
intent cannot stage or save a different endpoint. Internal legacy parsers and API
helpers are retained for compatibility tests without a public navigation path.

## Automatic service and unavailable builds

The app reads the public build profile automatically. A legitimate origin saved
through an earlier explicitly reviewed setup remains authorized and takes priority;
it is revalidated before use. An empty new-install profile leaves Connect disabled
and displays: “This copy of OpenRemote isn't ready to connect. Please contact your
app provider.” Secure-settings/startup failures likewise use a plain provider
message. The user is never asked to fix configuration inside the app.

The profile in this source snapshot is empty. Consequently the supplied preview is
not a configured Internet product. A provider must deploy and operate HTTPS and
relay services, embed that public origin, configure host registration, and complete
real device/network acceptance before a ready-to-use release can exist.

Normal assemble/bundle release tasks depend on `verifyReleaseServiceProfile` and
reject the empty origin. An exact `-PopenremoteUnconfiguredPreview=true` permits
only unsigned development inspection; configured or injected release signing is
rejected with that exception. Debug can be built for inspection without this flag.
The evidence records the default rejection and the explicit exceptional build.

## Authentication and secrets

The Server ID is the existing ten-minute one-use pairing capability, not a permanent
PC identifier. The phone first fetches compatible service information, including
`pairingCodeVersion: 1`, and performs fresh P-256 challenge authentication. It sends
only code, PIN and controller name in the authenticated redemption JSON. It does
not request a monitor, extra permissions or permanent trust.

The new app accepts exactly six PIN digits. The backend can retain its legacy
6–12-digit acceptance for older clients without widening this app's validation.
Discovery also accepts optional `automaticHostRegistrationVersion` as integer 0 or
1 and rejects other types/versions; the phone does not use host-registration secrets.

PIN content stays only in local transient UI state and the bounded request closure.
It is cleared on submit, Cancel, Activity pause and home disposal; it is not saved
across Activity/process recreation, stored in settings or logged. Immutable string
copies can remain in process memory until collection, so secure erasure is not
claimed. Keystore identity and encrypted settings behavior remain unchanged.

Redemption uses the existing one-shot HTTP body with connection retries disabled.
An ambiguous lost result tells the user the Server ID may have been used and asks
for a new one. Cancellation invalidates the generation and prevents a late result
from opening or overwriting a newer session. If a bounded late response identifies
a real session, cleanup ends it. If the network never discloses its identity, the
client cannot invent a session ID; normal server expiry/cleanup still applies.

The admitted grant is restricted to screen and allowed input. The paired flow hides
clipboard/file controls and display switching, and grants neither tools, audio nor
power. The screen displays plain connection/recovery messages without reflecting
server responses, addresses, certificates or key details.

## Preview continuity and verification boundary

Version 0.7 uses version code 7 and the same `com.openremote.mobile.dev` preview
package/certificate as 0.6. The private preview key must be retained separately for
future updates; it is not in the source bundle or public downloads. The production
package remains `com.openremote.mobile`, with no production signing key supplied.

JVM tests and Gradle/manifest/signature/alignment inspection are reproducible
build evidence. They do not prove phone installation, Compose lifecycle execution,
Android Keystore, decoder behavior, PC input or cross-network relay connectivity.
Exact final results, source hashes and unresolved gates are in `VERIFICATION.md`
and `verification/one-button-0.7.0/`.
