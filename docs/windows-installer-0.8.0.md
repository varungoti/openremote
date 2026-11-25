# OpenRemote 0.8 — installation and activation

**Engineering build, 20 November 2026. No customer installer has been produced.**
The public OpenRemote service is not configured or deployed for this project.
The installer builder refuses to package an empty profile. Installing the old
0.7 preview again will not fix its missing-service message.

## Everyday use after a configured, verified release

On the Windows PC being controlled, open OpenRemote. Startup checks the service
and registers the PC using its locally protected identity. **Your PC ID** is the
permanent nine-digit number to use from other devices. It is different from the
old, temporary six-digit invitation. The PIN has exactly six digits, including
leading zeroes if chosen.

Select **Activate Connection** to reveal the PIN field. Enter six digits and
select the same action again, or press Enter. The host confirms activation
before minimizing to its system tray icon. If activation or tray creation fails,
the window stays visible with an error. The host must remain running in the
logged-in desktop. Closing its active window hides it only when a working tray
icon exists; without that icon, closing revokes access and exits. **Quit** or
**Deactivate Connection** stops remote access. Normal preparation can be cancelled.

On Android, enter **PC ID** and **Security PIN** and choose **Connect remote desktop**.
The app reports the connection stage and opens the desktop only after media is
ready. On a laptop, use the host window's **Connect to a PC** entry to open the
configured web viewer, or use the deployed service's `/connect` page. It asks
for the same PC ID and PIN. There are no customer infrastructure settings.

## Tray and connection list

The tooltip shows whether hosting is active and the active-connection count.
Open the icon to see each connection's client name, state and input/view-only
permission. Android sends a bounded device-model name. The web client identifies
its browser platform; browser code cannot reliably retrieve a laptop's machine
name. Agent connections use `AI agent · <name>`. Names are display claims, not
proof of a person's identity. Pending sessions are visible but do not count as
active until host media has been sent and the service acknowledges connection.

An activation supports four pending/live connections in total. The first
eligible session reserves keyboard/mouse control; others are view-only. An
existing viewer stays view-only after the controller leaves. Reconnecting with
the active PIN can request a free control slot. No silent permission promotion
occurs.

Use **Deactivate Connection** from the window or tray to revoke the activation
and end its connections. The UI distinguishes cleanup in progress and failures;
it does not promise remote acknowledgment when the service cannot be reached.
Local cancellation still disables local work. Unverified native capture/input/peer
teardown requires restart before another activation. An unacknowledged coordinator
end stops the activation visibly but permits Activate Connection again to reset
access.

## PIN lifetime and availability

The PIN stays valid for the current activation, for at most 24 hours. It is not
saved for unattended restart. Five wrong PIN guesses lock the grant and end its
connections; request and capacity limits apply. Starting a new activation,
stopping the host, or a coordinator restart invalidates the old grant. The PC ID
remains stable as long as its original identity and service registration remain.

After a network loss the app reports recovery rather than claiming an active
session. A previously registered PC may still show its saved PC ID when the
service is unavailable. Failed startup service admission disables Activate; a
later outage after successful admission may allow another attempt, which rechecks
the service and stays inactive if unavailable. Missing publisher configuration
is detected before PIN entry. Users should not have to supply a server address,
SSH key, enrollment secret or encoder path.

## What is and is not installed

The installer source retains per-user installation under `%LOCALAPPDATA%`, Start
menu and optional desktop shortcuts, a pinned FFmpeg Media Foundation runtime,
matching source/notices and uninstall registration. Existing identity and user
data are preserved. To release the installer lock, choose Quit from the tray or
deactivate and then close the inactive window. Deactivate alone leaves the process
running. Native install, upgrade, rollback and uninstall still need
Windows acceptance testing.

The current host is an interactive application with a tray icon. No Windows
service, automatic launch at sign-in, pre-logon capture, secure-desktop control
or reboot recovery is installed. Those require the separately planned broker /
interactive-session architecture. Android preview builds use
`com.openremote.mobile.dev`; production signing and an iOS client are outstanding.

## Publisher work required before distribution

Deploy this project's durable HTTPS coordinator and TURN service, configure
the common public origin, then rebuild both clients and the agent bridge.
The Windows packaging gate checks service capabilities over verified HTTPS and
cannot be bypassed with an unconfigured-preview flag. Complete the native and
Internet acceptance matrix in [the product plan](../PRODUCT_PLAN.md), resolve
notice/provenance gaps, sign the final applications and verify upgrade behavior.
The evidence in [BUILD_STATUS.md](../BUILD_STATUS.md) describes engineering
validation; it does not certify a commercial release.
