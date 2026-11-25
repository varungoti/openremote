# Threat model and security boundaries

Updated 18 November 2026. This describes the implemented trust model and planned
security work; it is not an independent security certification.

## Assets and actors

Assets are PC desktop pixels, input authority, clipboard text, transferred files,
PC enrollment identity, phone signing keys, bearer credentials and session/audit
metadata. The intended operator owns or administers the enrolled PCs and the
private deployment. A phone proves possession of its own key; that alone does not
make it a trusted member of the deployment or authorize access to any PC.

Threat actors include an unauthenticated Internet client, an authenticated but
unapproved phone, a revoked phone, a malicious authorized controller exceeding its
grant, a compromised relay and a malicious dependency/update. A compromised PC or
phone OS can access that endpoint's local data and is outside the current endpoint
protection claim.

The coordinator is currently a **trusted authorization and SDP-binding component**.
Its compromise is not covered by a claim of independently verified peer identity.
Future peer-signed negotiation must bind session ID, roles, ephemeral certificate
fingerprints, negotiation generation and expiry to verified identity keys.

## Trust boundaries

| Boundary | Implemented controls | Remaining evidence/work |
| --- | --- | --- |
| Internet → coordinator | HTTPS or exact trusted reverse proxy; bounded duplicate-free JSON; role/identity checks, rate and capacity limits | Deployment, external abuse tests, independent review |
| Phone → authorization | One-use random challenge, P-256 SPKI key, SHA256withECDSA proof, short-lived login bearer | Physical Keystore/biometric/recovery tests; richer pairing UX |
| Enrollment → host identity | Private deployment enrollment token; stable PC ID; bearer tied to configured origin and host identity | Native install/re-enroll/revoke scenarios |
| Phone → PC session | Local approval or exact explicitly trusted key; narrowed permission grant; persistent revocation tombstone | Native consent, spoofing/prompt-flood and unattended tests |
| Peer → media/input | WebRTC DTLS/SRTP and reliable data channels; validated protocol and host-side permission checks | Actual cross-device transport, independent negotiation binding |
| Phone → clipboard/files | Explicit capability request, local enable flags, bounded messages and scoped receive directory | New tool implementation/testing; native file-system/clipboard matrix |
| Coordinator → disk | Token hashes, private snapshot, atomic/fsynced save, single-writer lock, failure latch | Backup/restore, filesystem and crash-injection matrix |
| Application → OS | Current-user DPAPI, Android Keystore/encrypted settings, no privileged Windows service | Physical OS behavior; separately reviewed future service broker |
| Publisher → installed binary | Locked dependencies, checksum records, prepared signature/build paths | Production signing, complete notices, updater and rollback protection |

## Session and credential controls

A PC number is a routing identifier. It never replaces key possession, approval
or the session bearer. Controller identities are derived by the coordinator from
verified signatures rather than accepted from arbitrary session request text.
Challenges expire after 60 seconds and are consumed on the first verification
attempt. Default login credentials last ten minutes; pending requests last
90 seconds; approved sessions last at most two hours.

TURN credentials are minted for approved members, expire within ten minutes and
are bounded by session lifetime. The long-lived shared TURN secret stays in the
private deployment configuration. TURN relays forward encrypted peer traffic;
that does not independently authenticate a peer if the trusted coordinator is
malicious.

Host enrollment and phone access tokens are different authorities. Clients reject
HTTP redirects and do not send existing host credentials to a different origin.
Host secrets are protected with current-user DPAPI; copying their files to another
Windows account does not provide the original decryption identity. Android stores
its signing key in Keystore and keeps login/session bearers in memory.

## Authorization and visible control

The server enforces `pending → approved → connecting → active → ended`; rejected,
expired or ended sessions cannot continue signaling or obtain new relay credentials.
Approval can narrow requested permissions but cannot add a capability. Host tools
also check the final grant for each operation; capabilities are discovery only.

Unattended access needs both a local enable flag and the exact trusted phone key.
A durable coordinator revocation blocks later requests even if a stale local key
remains in the host's configuration. Explicit host re-trust is required to clear
that revocation. Current per-user sessions always display a local disconnect
window and provide an emergency shortcut.

Remote input is disabled until fresh decoded video is available and is subject to
Windows input-integrity boundaries. Held keys/buttons release on network loss and
teardown. Ordinary phone backgrounding ends the preview session. A deliberately
opened document picker may temporarily retain the encrypted session with input
paused; it must be bounded and must not silently turn into background control.

## Clipboard and file rules

The 0.3 contract is [session-tools.md](session-tools.md). Text clipboard operations
are explicit user actions, not automatic monitoring. Phone clipboard contents are
never read on a timer. Received PC text is previewed before the user copies it to
the phone. Text has a UTF-8 byte limit and rejects NUL.

Uploads use a separate reliable tools channel, bounded chunks, monotonically
acknowledged offsets and a final SHA-256. A remote client supplies a display
basename, never a PC path. The host uses a dedicated per-user receive directory,
rejects traversal/reserved names/reparse directories, avoids overwrite and applies
Windows Internet-zone metadata before reporting completion. It never opens or
executes the received file. Incomplete files are removed on ordinary cancellation,
disconnect, invalid content and expiry; process-crash recovery must be tested and
stale partial cleanup reviewed before release.

One active upload, per-file and per-session byte quotas, idle timeout, rate limits
and bounded request-ID tracking limit resource use. These controls do not provide
malware scanning or establish that an authorized user's chosen file is safe. Future
bidirectional transfer must use a host-selected export scope and explicit phone
save action rather than arbitrary remote filesystem paths.

## Selected displays and input ownership

The 0.4 host binds capture and input to one immutable display selection per
session. The nine-digit PC number routes the request; an optional opaque
`monitorId` chooses a display only after the same authentication, consent and
permission checks. A display selector is neither a credential nor a filesystem
path. Catalogs remain inside an approved encrypted session.

Attended approval and the visible session notice name the selected monitor.
Explicit unattended trust permits selection from the host's available monitors
under its existing screen/input grants; per-monitor unattended allowlists are not
implemented. View-only selection cannot add input, clipboard or file permissions.

Capture and pointer mapping share physical-pixel geometry, including negative
virtual origins. Observed topology changes invalidate selectors, release held
input and end capture. A restored layout cannot revive an earlier observed
topology's token. Enumeration and OS changes are not atomic; the hardware test
matrix must measure the remaining check/injection race and the separate encoder's
DPI behavior. The product does not claim a zero-event hardware guarantee.

Switching monitors uses a new session, peer and renderer. The phone withholds
video presentation until the host confirms an explicitly selected display and
requires fresh decoded video plus the owned renderer's first-frame event for
input. Old peer/renderer callbacks cannot authorize the replacement session.
Recreating a renderer pauses input without broadening tool permissions.

See [display-selection.md](display-selection.md) for bounds and compatibility.

## Persistence and recovery

Coordinator identity, trust, revocation, audit and monotonic host event sequence
survive restart. Active sessions, short-lived phone tokens and SDP/ICE payloads do
not. Preserving event sequence independently prevents a new event from colliding
with a cursor retained by a connected host. Long polls recheck the persistence
failure latch before publishing authorization events.

A storage failure closes the relevant authorization path. The coordinator does not
claim multi-node safety; only one writer against a local filesystem is supported.
Backing up a data volume is not enough to claim recovery until restore and format
compatibility are exercised.

Installer checks run before activation: trusted FFmpeg hash, compatible capture
and encoder, staged host signature and origin/identity preservation. Caught file
switch errors restore previous configuration/shortcut files. Abrupt power loss
between those switches, repair and native filesystem semantics remain open gates.
No privileged or automatic update process is implemented.

## Logging and retention

Do not log screen pixels, clipboard contents, transferred data, typed text or
bearer credentials. Session metadata and bounded audit events are adequate for
basic authorization diagnosis. Future crash/support bundles must redact candidate
addresses when appropriate and make data collection explicit. Recording is a
separate future permission and product flow.

## Required security validation

Focused tests must exercise copied public-key impersonation, challenge replay,
wrong actor, pre-approval/ended signaling, grant escalation, stale authorization,
revocation, expired relay credentials, duplicate/deep JSON, message floods,
malformed SDP/ICE, invalid file offsets/digests/names, overwrite and reparse races,
interrupted transfer and input cleanup. Native tests must cover Windows secure
desktop/UIPI behavior without bypass, Android Keystore/lifecycle and installer
failures. Review service IPC and update trust before introducing either feature.

Security claims remain limited to the implemented controls and their recorded
verification. [release-gates.md](release-gates.md) lists the open requirements.

## Phone setup locators and public discovery (0.5)

Setup links arriving through paste or Android intents are untrusted input. They
carry only an HTTPS origin and PC number. Strict bounded parsing rejects unknown
or duplicate fields, malformed encoding, credentials, fragments and paths. The
phone stages the full origin/PC for explicit confirmation before changing settings;
lookup and connection remain separate explicit actions. It never consumes a setup
link as a permission, enrollment secret, phone trust entry or authentication proof.
Custom schemes are not verified domain App Links, so explicit in-app paste remains
available and the user compares the full origin with the PC.

Pending review and active sessions block origin changes. Encrypted settings writes
are serialized through cancellation; epochs reject stale lookup/check completion.
These protections have pure/API test coverage; native cold/warm intents, process
recreation, SAF overlap and actual secure-store behavior remain acceptance gates.

Public `/v1/info` exposes service/protocol versions and coarse relay configuration
only. It creates no authorization state and reveals no host IDs, keys, relay URLs
or credentials. HTTPS, storage failure and rate controls still apply. A service
label can be copied by any server; it is a compatibility check, not independent
server/peer identity verification. Configured relay flags cannot attest reachability,
allocation, selected route or media. The coordinator remains a trusted authority
for SDP/peer binding until the separately planned signed negotiation is implemented.
