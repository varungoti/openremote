# Keep Windows first launch visible and supply a real installer

The distributed 0.5.0 host returned an error before creating any window when
configuration was missing, incomplete or enrollment was not configured. When
opened from Explorer, the console disappeared and the user could not complete
setup. A raw host executable also lacked a normal install/uninstall flow.

This change adds a Win32 setup/progress form, password-masked local enrollment,
verified encoder preflight, recoverable visible startup errors and a graphical
Windows subsystem entry. Existing identity, server binding and explicit access
permissions are preserved. Registration cancellation finishes an already-sent
bounded transaction safely. Failed console-signal registration no longer stops
the GUI host, and runtime credential revocation retains a visible explanation.

The NSIS installer stages versioned per-user files, coordinates through the
host lock, provides host/settings shortcuts and an uninstall entry, verifies
shortcut ownership and restores previous activation on caught failures. It
preserves user data and rejects unsupported Windows builds/architectures.
The builder admits only a matching verified GUI host, checks system DLL imports,
and independently extracts all payloads and the generated native uninstaller.

## Validation

- 76 Rust tests (11 new startup cases), formatting and Linux/Windows all-target
 Clippy with warnings denied passed on the final source.
- Optimized Windows AMD64 GUI build passed; 22 exact Rust inputs match the committed
 implementation and independent source review.
- 10 installer admission/custody tests passed; NSIS zero-warning compilation and
 complete 7-Zip 26.06 extraction passed for the actual delivered host.
- Old console host rejection and guard-removal negative control were verified.
- Superseded builds, failed parser/cache checks and their resolutions are retained.

## Limits for review

No native Windows install/upgrade/uninstall, Win32 UI/DPAPI/capture/input or actual
phone session was executed. These remain explicit release gates. The installer
is unsigned; deployed coordinator/relay and a trusted separate FFmpeg build are
still required. Android and coordinator source remain 0.5.0 and were not rebuilt.

See BUILD_STATUS.md and evidence/windows-startup-0.5.1 for exact hashes and scope.
