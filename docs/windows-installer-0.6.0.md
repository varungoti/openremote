# OpenRemote 0.6.0 — Install and connect with a code and PIN

19 November 2026. **Development preview, not a commercial release.**

The updated flow is: open the Windows host, choose a PIN, generate a six-digit
code, and enter the code and PIN on Android or the browser viewer on another PC.
The previous visible-startup repair and per-user Windows installer remain in place.

The files are built for Windows on Intel/AMD x64 and Android 8 or later. The
Windows minimum is Windows 10 version 1703 (build 15063), or Windows 11. Actual
Windows installation and physical phone sessions still need device testing.
This Android package does not install on an iPhone.

## What is automatic, and what still needs deployment

Once both devices are configured for the same running service, normal connection
uses **only the code and PIN**. The coordinator finds the host and creates the
session using the host's allowed permissions. It also supplies network/relay
settings to the clients.
There is no router configuration in the connection screen.

The Windows and Android builds now share one public service profile. A real
HTTPS origin can be embedded before building, so a fresh installation does not
ask for the service address. Existing saved settings and PC enrollment remain
authoritative; an update cannot silently move a PC to a different service.

**The supplied preview's profile is empty.** No public OpenRemote coordinator or
relay has been provisioned for this project. The preview still requires the
owner's running coordinator/relay, first host enrollment, and a trusted compatible
FFmpeg executable. An installer or six-digit code cannot replace that Internet
infrastructure. A previously configured host can keep its existing service and
encoder settings after upgrading that service to 0.6.0.

Fully packaged enrollment and encoder installation, production signing, and
physical Internet testing are required by the commercial-release plan. They are
not claimed as finished by this preview.

## 1. Install or update the Windows host

1. Stop OpenRemote on the Windows account you are updating.
2. Run **OpenRemote-0.6.0-Windows-x64-Setup.exe**. It installs for the current
   user, creates **OpenRemote Host** and **OpenRemote Settings** Start menu
   entries, and offers a desktop shortcut.
3. Open **OpenRemote Host**. A valid existing setup is checked and reused.
   A fresh or incomplete setup opens the visible setup window.
4. If needed, complete the owner setup below, then choose **Start host**.

The host does not require Rust to be installed. The installer is unsigned and
does not claim a verified publisher. Its checksum identifies these exact bytes;
it is not a publisher signature. No instruction in this guide requires disabling
Windows protection.

### First setup, for the deployment owner

| Field | Meaning |
| --- | --- |
| Server address | The real HTTPS origin of your 0.6.0 coordinator. A configured service profile fills this for a fresh install. |
| PC name | A name you recognize, such as Office PC. |
| FFmpeg executable | Your trusted standalone Windows encoder with `gdigrab` and `libx264`. |
| Expected FFmpeg SHA-256 | The checksum of that exact executable from the trusted build/distribution process. |
| Enrollment key | The coordinator's private first-enrollment credential. It is not the session PIN. |

The enrollment key is entered locally in a masked field and is not embedded in
the mobile profile, shared link or code. Enrolled host credentials remain
protected for the current Windows user. Startup failures stay visible so you can
correct them and retry. The enrollment transaction has bounded cancellation
handling; a lost response can still require owner recovery.

## 2. Generate a connection code on the host

1. In the running host window, enter a **6–12 digit security PIN** and confirm it.
2. Choose **Generate code**. This authorizes one screen-sharing session, with
   keyboard/mouse control only if the host's input setting allows it.
3. Wait for the six-digit code and expiry countdown. Leading zeroes count.
4. Share that code and the PIN with your receiving device or intended operator.

Each code expires after **ten minutes**, can be used **once**, and is unique
among active codes on your service. It is not a permanent PC number. Generating
a replacement invalidates the previous unused code. **Revoke code** removes the
local authorization immediately, even if the coordinator cannot be reached.
Five incorrect PIN guesses invalidate a code; additional request limits apply.

PINs are not saved in settings, setup links or application diagnostics. The
host remembers a one-use authorization only in memory. Restarting the host
cannot silently restore it. The generated grant does not turn on unattended
access, clipboard, file transfer, audio or power actions.

## 3. Connect from your Android phone

Install **OpenRemote-0.6.0-Android-ARM64-Preview.apk** on a compatible ARM64 phone.
The launcher label is **OpenRemote Preview**. This build uses a new development
certificate and a separate package identity, so it installs beside the earlier
app instead of asking you to remove that app's settings and identity. It is not
an in-place update or a signed Play Store release.

If the service profile is configured, the app opens to code and PIN entry. For
this unconfigured preview, use **Advanced** once to import the host's existing
setup link or save the same HTTPS service address. A setup link carries an
address and PC number only; it carries no session PIN or enrollment credential.
If both old and preview apps are installed, Android may ask which app should
open the link; select **OpenRemote Preview**.

Enter the six-digit **Connection code**, enter the **Security PIN**, and tap
**Connect**. The PIN clears from the form. The app authenticates its controller
identity, redeems the code, and waits for the host's matching local grant before
starting the existing video and input flow.

You do not need to approve this same generated grant a second time on the host.
The host still shows the connection and its Stop control. An unrelated ordinary
connection request retains the original local-approval policy.

## 4. Connect from another PC

Open your configured service's **`/connect`** page in a current browser supporting
WebRTC, H.264, Web Crypto and decoded-video frame callbacks. The host's
**Connect to another PC** button opens that page in your default browser.
This is an explicit browser viewer; the Windows host itself is not a native
Windows receiving client.

Enter the six-digit code and the chosen PIN. The viewer receives the remote
desktop and provides pointer, supported keyboard keys, Unicode text entry,
full screen, and a visible Disconnect button. Click the video to direct keyboard
input to it; **Escape** releases local keyboard focus. **Send Esc** sends Escape
to the remote PC. Browser/OS-reserved shortcuts remain subject to the browser.

Input remains disabled until the host confirms its primary display, the channel
and peer are connected, permissions allow control, and decoded video is fresh.
Code sessions begin on the primary display. The browser viewer does not provide
clipboard, files, audio or monitor switching in this preview.

## Stop, expire or reconnect

Use **Disconnect** on the controller, the host's session Stop control, or
**Stop host**. Closing the host ends its access. It must remain running in the
logged-in desktop; service, lock-screen/UAC desktop and reboot recovery are not
provided by this version.

If the code was used, expired or locked, generate a fresh code. A failed or
interrupted redemption is not automatically retried: the first request may
already have consumed it. Canceling an in-flight request lets its bounded reply
finish so a known late session can be explicitly ended. If the response is lost
entirely, the client cannot identify that session from a response it never
received; pending/media timeouts and the host Stop control remain necessary.

Android's existing monitor switch ends the old session and requests fresh host
approval. A consumed code does not silently authorize a second display/session.

## Troubleshooting

| Message or symptom | Next step |
| --- | --- |
| This build has no service address | The deployment owner must configure a real service; use the setup link once or rebuild with its public profile. |
| Server needs an update | Upgrade the coordinator to 0.6.0 before using six-digit codes. |
| Incorrect, expired or unavailable code/PIN | Check all digits, including zeroes. Generate a fresh code if the prior one expired, was consumed or reached its guess limit. |
| Too many attempts | Wait for the indicated limit to reset; do not repeatedly submit or generate codes. |
| The host could not authorize the code | Keep the same host process running; generate a fresh code after restart or revocation. |
| Connected but no usable video | Check the host encoder, physical display and relay configuration. The viewer intentionally pauses input without fresh video. |
| Setup error | Correct the visible field and retry; safe operation codes are in `%LOCALAPPDATA%\OpenRemote\startup.log`. |
| Browser cannot play the stream | Try a supported current browser with H.264/WebRTC support; use the Android app for the native phone controller. |

## Deployment profile and rebuild

For the deployment owner, after provisioning and validating the real coordinator:

```sh
python3 scripts/configure_service_profile.py --api-origin https://YOUR-REAL-HOST
```

This updates only `config/service-profile.json`. It does not deploy, buy, register,
enroll or expose a service. Rebuild both clients afterwards and verify that their
evidence includes that profile's hash. Existing enrolled installations stay bound
to their original origin. Never put the enrollment key, PIN or TURN secret in the
profile. The browser viewer uses its own origin and needs no separate endpoint field.

The deployment sequence is coordinator/relay first, then Windows host, then
Android or browser viewer. See `docs/deployment.md`, `docs/code-pin-contract.md`
and `BUILD_STATUS.md` in the source bundle for the exact implementation and
remaining release evidence.

## Update and uninstall

The installer preserves existing configuration, encrypted identity and received
files. It refuses to replace a running host and stages versioned per-user files.
Use Windows Installed apps to remove the new installation. User data and prior
release folders remain; uninstalling does not revoke the PC at the coordinator.
Physical clean-install, upgrade, rollback, and uninstall tests are still required.
