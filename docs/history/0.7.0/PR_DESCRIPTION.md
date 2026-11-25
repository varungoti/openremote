# Remove technical setup from the normal remote desktop flow

The previous preview still sent a fresh user through a server URL, enrollment key
and separately configured encoder before code/PIN connection. That contradicted
the requested one-button experience and prevented the installer from being
self-contained.

0.7 replaces that setup with one persistent Windows Connect/Stop action, a
six-digit PIN and a displayed six-digit Server ID. Android and the PC browser
viewer now use only Server ID/PIN and one current connection action. The Windows
installer bundles a pinned Media Foundation encoder and corresponding source;
its old Settings shortcut is removed through ownership-aware upgrade handling.

The coordinator adds opt-in origin-bound Ed25519 automatic registration, bounded
same-key credential recovery and durable administrator revocation. No enrollment
secret goes into a client. Existing healthy credentials and their origin binding
are preserved. Server snapshots move to version 2 while reading legacy version 1.

Default release/installer packaging rejects an empty service profile. The explicit
unconfigured-preview build is labelled and remains unavailable for fresh Internet
connections. No service endpoint was supplied or deployed in this work.

## Validation

See `BUILD_STATUS.md` and `evidence/one-button-0.7.0/verification.json` for final
counts, command records and source/artifact custody. Verification includes Rust
state/protocol tests and Windows cross-linking, Android tests/lint/APK inspection,
Go normal/race tests, independent actual HTTPS registration/recovery/revocation
and existing PIN/session tests, browser module/lifecycle checks, and independent
installer payload extraction.

## Release boundaries

The Windows UI/installer/Media Foundation path, physical phone installation and
actual remote pixels/input over public relay have not run here. Public deployment,
production signing, unresolved dependency notices and the device/network release
matrix remain required before a customer release. These builds are previews.
