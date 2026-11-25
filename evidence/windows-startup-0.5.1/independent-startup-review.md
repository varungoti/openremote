# Independent Windows startup review — 0.5.1

## Result and exact source

No open blocking source finding remains in the reviewed startup repair at
`b25975bf632902a6f65d2f6e71c3722b55010e19`. This is a source-review result,
not a native Windows test result or a production-release approval.

All **22 Rust build inputs** match that commit. Their SHA-256 map is in
`independent-startup-review.json` and `independent-startup-source-inputs.sha256`.
The aggregate of the sorted, compact JSON path-to-hash map is
`21cff28037ef411a562cfa0823a420408fb1c2fb2086705aa20498237d26a725`.
The JSON separately identifies the 13 files read for semantic review; hashing
the complete build-input set does not imply an entirely new audit of the media
stack.

## Requirements review

The original no-argument Windows entry called `Config::load_or_create` before
creating the host window. With no configuration, that function wrote a sample
containing an empty encoder hash and returned an error. Subsequent launches
also rejected the incomplete sample. This is a source-derived explanation
consistent with the reported immediate exit; the user's PC was not observed.

The new Windows GUI entry reaches a persistent setup window. Missing settings
become an editable state without writing a broken template. The old sample is
recognized as incomplete. Malformed JSON is preserved until the owner starts
setup, with a recovery copy before replacement. A valid existing configuration
retains its explicit trust and permission choices while the four visible
fields change. Server and encoder failures are displayed in the form and allow
retry. Fatal initialization failures use a persistent native message instead
of relying on a disappearing console.

The installer supplies Start menu host/settings entries and an optional desktop
shortcut. The host continues to require a running coordinator and a separately
trusted FFmpeg executable. No server, encoder, Windows service, automatic startup,
or permission grant is silently added by this repair.

## Engineering review

The setup HWNDs and controls remain on their creating thread. The inspected
RefCell accesses release their borrows before native calls that can reenter the
window procedure. Network and encoder work run through a background task;
progress crosses a channel, not a borrowed HWND. A completed worker is observed
with a second channel drain, and an unexpected worker exit does not leave the
form permanently disabled.

Cancellation before enrollment prevents the request. Once first enrollment is
sent, the bounded HTTP request and encrypted credential write finish before
Close is honored; cancellation still forbids heartbeat and host startup
afterward. Retry refreshes the persisted server binding. Save notifications
refresh the in-memory configuration snapshot before another attempt. The
registered origin cannot be silently reassigned to another server.

Enrollment input is masked, bounded and kept out of configuration, command
arguments, exported phone links and encoder child environments. Diagnostic
files contain fixed operation codes, not raw errors, tokens, URLs or response
bodies. HTTPS discovery precedes sending an enrollment key, redirects are
disabled, and the prior explicit loopback development exception remains narrow.

The review found and resolved these additional source defects:

- Console signal registration errors previously behaved like a Stop request.
- Revoked registration could close the GUI without retaining a visible reason.
- Registration/configuration reads checked size only after an unbounded read.
- Serialized configuration could exceed the reader's 64 KiB limit. The final
  guard runs before temporary or recovery files are created. Its regression
  checks both a missing file and preservation of an existing readable file.
- Installer desktop shortcut removal needed ownership and target/argument
  checks. The final script records ownership and preserves user-edited links.

The generated uninstaller extraction gate remains intact. The initial 7-Zip
25.01 parser could not recognize a valid NSIS 3.10+ command form; official
7-Zip 26.06 handles the same draft without `BadCmd=13` and exposes its uninstaller.
The exact before/after comparison and upstream issue are retained in
`extractor-repair-provenance.json`.

## Verification boundary

`final-checks.json` and the final host build evidence own the actual fresh build
and test outcomes. This review does not manufacture a passing result from a
successful compile or a code read. An intermediate test run reused a negative
control's cached library test executable; the root agent preserved that failure
and diagnosis, then rebuilt the affected package without changing the production
guard or test assertion. The fresh final record has now been read: formatting, all-target tests, Linux and Windows Clippy with warnings denied, and the Windows release cross-link all exited successfully. The test log records 76 passing tests; the exact source remained unchanged. No assertion or production source was weakened to resolve the cache issue.

Native Windows setup, DPI/focus/file-picker behavior, DPAPI, installation,
upgrade, rollback, COM shortcut lookup, encoder capture and Android connectivity
remain **not run**. Those are still device validation gates. Advanced settings
outside the four visible fields may require owner file repair, and actual loss
of a first-enrollment response can require server-owner recovery; the app's 409
message makes that condition explicit. Production signing and the broader
release gates are unchanged.

## Independent final artifact inspection

The final installer was independently extracted with official 7-Zip 26.06.
All four application payload files and the generated native uninstaller match
the packaging record byte for byte. The embedded host is the exact final
`b25975bf` build. LLVM `readobj`, separately from the Python packaging parser,
identifies it as an AMD64 Windows GUI executable. Its new serialized-size guard
diagnostic is present in the binary.

- Installer: `OpenRemote-0.5.1-Windows-x64-Setup.exe`, 3,291,954 bytes,
  SHA-256 `fe593f19f80b45647e6f434ad6951d5a245e4ea328e51a4a0093db56bbbce5b9`.
- Embedded host: 9,655,296 bytes, SHA-256
  `47c5c0a386be1f9b9d75950aefe56a0abc519a643c975bea3561bfe8c986f96b`.
- Static PE inspection covered the installer, host, uninstaller and both
  installer plugin DLLs. Their imports are Windows system/API-set or C runtime
  names; no separately redistributable DLL import was found. The host's added
  `comdlg32.dll` dependency supplies the Windows file picker. The separately
  supplied FFmpeg executable remains a prerequisite.

The record and exact static inspection logs are in
`independent-final-installer-inspection.json`. Extraction and PE inspection do
not execute an installer, a shortcut, a host window or a remote session. Both
host and installer remain unsigned, and native Windows validation remains not
run.
