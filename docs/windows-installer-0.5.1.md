# OpenRemote 0.5.1 — Windows installer and startup repair

19 November 2026. Windows host update; Android and coordinator remain at 0.5.0.

Use **OpenRemote-0.5.1-Windows-x64-Setup.exe** on an Intel/AMD x64 PC with
Windows 10 version 1703 (build 15063) or newer, or Windows 11. This is
the installer. The earlier `Windows-x64-Test.exe` was the host program itself and
required manual configuration before it could stay running.

This candidate fixes the source path that exited before showing a window when
configuration was absent or incomplete. It adds a graphical first-run flow and
an executable installer. It is an unsigned development candidate; physical
Windows installation and a real Windows-to-phone session remain unverified.
See the accompanying verification report for the checks actually performed.

## Install

1. Stop any running OpenRemote host on this Windows account.
2. Open **OpenRemote-0.5.1-Windows-x64-Setup.exe** and complete the installer.
   It installs for your current Windows user and creates Start menu entries.
   A desktop shortcut is optional. The installer contains the host and its
   available dependency notices. It does not need a separate Rust installation.
3. Open **OpenRemote Host** from the Start menu. First launch displays the setup
   window. An existing usable configuration is checked before the host starts.
4. To return to setup later, stop the host and open **OpenRemote Settings**.

Windows may identify this unsigned candidate as an unknown publisher. There is
no production publisher signature on this installer. The supplied SHA-256 lets
you check that the download matches this build; it does not establish a signed
publisher identity. Do not turn off Windows security protection to run it.

## Complete the visible setup

The host needs these settings before a phone can connect:

| Setting | What to enter |
| --- | --- |
| Server address | The HTTPS origin of your deployed OpenRemote coordinator, such as `https://remote.your-domain.com`. Use your real address. |
| PC name | A short name that identifies this computer, such as Office PC. |
| FFmpeg executable | The absolute path to your trusted standalone Windows FFmpeg executable, including `ffmpeg.exe`. It must support `gdigrab` capture and `libx264` encoding. |
| Expected FFmpeg SHA-256 | The 64 hexadecimal character checksum for that exact executable from your trusted distribution/build process. An archive checksum is different from its extracted executable's checksum. |
| Enrollment key | The private enrollment key configured on your coordinator, needed only when enrolling this PC for the first time. Enter it in the password-masked field. |

The installer does not provide a hosted coordinator, create a public server, or
bundle FFmpeg. Those are existing deployment prerequisites. If you have not
configured the coordinator and relay yet, follow [the deployment guide](deployment.md).
A sample address from the old template cannot connect to your PC.

The enrollment key stays out of the saved configuration and phone setup link.
After enrollment the host uses credentials protected for the current Windows
user. A PC's existing enrollment cannot simply be redirected to another server.
The setup flow preserves that origin binding.

Start from the setup window after completing the fields. The host checks the
settings and encoder, then attempts enrollment or checks its existing enrollment.
An unsuccessful attempt leaves an actionable message in the window so you can
correct the settings and retry. Cancel closes setup without starting remote access.
If a first-enrollment request has already started, cancellation waits for its
bounded request and local credential save to finish. The window explains that
delay. Closing during that step does not discard a registration response on
purpose or start the host afterward. A lost network response can still require
owner recovery; a duplicate-registration error explains this instead of
suggesting repeated enrollment attempts.

The host's existing local consent and trusted-phone policies still apply. This
installer does not enable unattended access, enable extra session permissions,
or add a Windows service.

## Connect the phone

Keep the existing **OpenRemote 0.5.0 Android test app**. This Windows repair does
not require an Android update or change the session protocol.

After startup succeeds, the Windows host home shows its nine-digit PC number,
server address and **Copy setup link** control. Import that link in the Android
app, review the server and PC number, and save the setup. You can also enter the
same server address and PC number manually. Run **Check connection**, request a
session, compare the phone identity shown on the PC, and approve locally.

Keep the host home running. **Stop host**, closing the home window, or signing
out ends access. This candidate operates in the logged-in user's desktop; it
does not provide lock-screen, UAC desktop, or reboot/sign-in access.

## If setup cannot start the host

| Visible issue | What to check |
| --- | --- |
| Another instance is running | Close the existing host before opening settings, installing or uninstalling. |
| Invalid server address | Enter the real HTTPS origin with no username, password, path, query or fragment. |
| Server unavailable or TLS failure | Check the deployed coordinator, DNS and valid HTTPS certificate. A completed local installation does not deploy the Internet service. |
| Enrollment rejected | Check the first-enrollment key with the coordinator's owner. A revoked PC requires the deliberate re-enrollment procedure. |
| FFmpeg file missing or checksum mismatch | Select the correct standalone Windows executable and confirm the expected checksum for that file. |
| Encoder probe failed | Use a compatible standalone build with the required capture and encoding capabilities. The host does not accept an unverified replacement automatically. |
| Existing enrollment belongs to another origin | Keep the original origin or deliberately re-enroll through the owner-controlled recovery procedure. Existing credentials are not sent to a different origin. |
| Configuration is unreadable | Inspect the local configuration and permissions. Preserve the original file and PC identity before making a repair. |

The host records fixed diagnostic operation codes in
`%LOCALAPPDATA%\OpenRemote\startup.log` and rotates the file when it grows past
64 KiB. These codes exclude enrollment keys, URLs, server-response bodies and
screen content. If setup displays an error, retain its text and this log if it
exists. Advanced settings outside the form's four editable fields still require
owner review in `config.json` if they are invalid.

If the new executable still closes before any window appears, retain the exact
Windows error or Event Viewer application error, the Windows version and whether
the installer or installed host was opened. A screenshot of the error is useful;
do not include enrollment keys or credential files.

## Update or uninstall

The installer keeps the existing per-user configuration, encrypted identity and
received files. Caught installation activation failures retain or restore the
previous launch settings; power-loss recovery still requires native testing.

Use Windows **Installed apps** to uninstall the new installation. User settings,
PC identity and received files are retained. Uninstalling does not revoke the PC
at the coordinator. Previous release folders are retained to avoid destroying
an earlier script-based installation or its separately configured encoder.

## Verification boundary

The downloadable verification record identifies the exact installer and embedded
host hashes, Rust tests, target compilation, installer compilation and extracted
payload checks. These are build and source checks. They do not substitute for:

- Installing, launching, retrying, cancelling, upgrading and uninstalling on a
  physical Windows PC, including small/high-DPI screens and non-ASCII paths.
- Windows screen capture and input with the chosen FFmpeg build.
- Android video/input over direct Internet and forced TURN UDP/TCP/TLS routes.
- Production code signing, complete dependency notice review, and remaining
  release gates documented in `docs/release-gates.md`.

## Source handoff

The accompanying source bundle preserves the previous development history and
the repair branch. Restore it with:

```sh
git clone --branch fix/windows-startup-installer OpenRemote-0.5.1-Source.bundle OpenRemote
```

The standalone setup executable is the user-facing installation route; building
the source is not required to install it.
