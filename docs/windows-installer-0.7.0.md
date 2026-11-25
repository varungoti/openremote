# OpenRemote 0.7.0 — one button, Server ID and PIN

19 November 2026. **Unconfigured development preview.**

The setup screens have been removed. You do not enter a server address, SSH key,
enrollment key, encoder path or encoder checksum. Those are not connection steps.
The app publisher supplies the service configuration and installer runtime.

**This preview is not connected to a public OpenRemote service.** A fresh install
shows an unavailable message. Installing it does not yet enable an Internet remote
desktop session. A configured build from the publisher is needed; there is no
technical setup form for you to fill in. The missing deployment has not been
replaced with a guessed or unrelated public server.

## The connection flow in a configured build

### On the PC you want to control

1. Open **OpenRemote Host** and click **Connect remote desktop**.
2. Choose your **six-digit Security PIN**, then use the same button to start.
3. The app prepares the PC automatically and displays its **six-digit Server ID**.
4. Keep this window open. The same button becomes **Stop remote desktop**.

The Server ID includes any leading zeroes. It identifies one connection invitation
on this service, is unique among active invitations, expires after ten minutes and
works once. It is not a permanent machine number. Connect again to obtain a new
ID after expiry or a completed session. Each PC creates its own invitation.

### On your Android phone or receiving PC

Enter **Server ID** and **Security PIN**, then choose **Connect remote desktop**.
No address, enrollment key, fingerprint comparison or import link is requested.
The phone shows Cancel while its initial request is running, then a visible Stop
remote session action. The PC browser viewer has the same two-field home page.
The receiving PC opens the deployed service's `/connect` page; this version does
not supply a separate native Windows receiving app.

The host's explicit action authorizes one screen-viewing session and mouse/keyboard
control if the host allows it. There is no second approval dialog for that matching
invitation. An unrelated request does not inherit it. PIN access does not authorize
clipboard, file transfer, audio, power actions or permanent unattended access.

## Install or update Windows

Run **OpenRemote-0.7.0-Windows-x64-Preview-Setup.exe** for the current Windows user.
It creates **OpenRemote Host** and can create a desktop shortcut. There is no new
Settings shortcut. A recognized older OpenRemote Settings shortcut is removed
using the installer's existing ownership and rollback checks.

The host and its verified encoder are bundled together. You do not install Rust,
download another encoder or type a checksum. The package also contains the encoder's
corresponding source and notices. A failed prerequisite or launch stays visible in
the host window; the app does not fall back to a technical setup wizard.

This build is an unsigned development installer for Intel/AMD Windows x64. The
existing target minimum is Windows 10 version 1703, or Windows 11. The new Media
Foundation encoder path still needs verification on the supported Windows matrix.
A checksum identifies exact bytes; it is not a publisher signature. Windows
installation, upgrade, rollback and uninstall have not run in this Linux workspace.

Close a running OpenRemote host before updating. Existing encrypted identity,
configuration and received files are retained. An already enrolled PC stays bound
to its existing service and cannot silently move to another one. That service must
provide the required compatible connection APIs.

## Install or update Android

**OpenRemote-0.7.0-Android-ARM64-Preview.apk** targets ARM64 Android 8 or later. Its
launcher name is **OpenRemote Preview**. It keeps the 0.6 preview package and exact
preview signing certificate, so it is built for an update over 0.6 without removing
its data. An actual device update has not been executed. Older differently signed
packages remain separate. No iPhone application or Play Store release is included.

A fresh install of this unconfigured preview disables Connect and explains that
the provider has not configured the service. An existing authorized saved service
can be reused. The app no longer exposes Advanced, service entry, setup-link
imports, nine-digit lookup, saved-PC lists or fingerprint controls in its home flow.

## Stop and reconnect

Use **Stop remote desktop** on the host or **Stop remote session** on the phone.
Closing the host stops access. Stop also cancels preparation, clears the local
invitation and performs bounded session/process cleanup. If cleanup cannot be
confirmed, the host requires a restart instead of starting overlapping work.

A wrong, expired, consumed or locked Server ID/PIN requires a fresh host invitation.
Five wrong PIN guesses lock an invitation; additional request limits apply. PINs
are cleared from input and memory on submission and relevant lifecycle paths;
they are not saved in configuration or logs. A lost one-use redemption response
is not blindly retried. Pending-session expiry and host Stop remain recovery paths.

The host must remain open in the logged-in interactive desktop. This version does
not supply a background Windows service, lock-screen/UAC desktop control or reboot
recovery. Phone backgrounding ends the preview session.

## What was verified

The source includes Windows state/protocol tests and target compilation, Android
JVM tests and APK inspection, coordinator normal/race tests, and actual isolated
HTTP/verified-HTTPS protocol checks. The installer is independently extracted and
its payload hashes checked. These do not establish physical capture, decoded video,
remote input or Internet/TURN connectivity. Exact results are in `BUILD_STATUS.md`
and `evidence/one-button-0.7.0/verification.json` in the source bundle.

## One-time publisher work still required

Provision this project's HTTPS coordinator and relay, enable automatic registration,
and put that verified public origin into the shared build profile. Rebuild both
apps with that profile, complete native and Internet session testing, resolve the
remaining release notices, and use production signing identities. The server's
administrator secret remains on the server. None of these are end-user form fields.

Default installer/release packaging rejects an empty profile. This explicitly
labelled preview uses the internal unconfigured-preview exception. The app is not
being described as release-ready or already connected.
