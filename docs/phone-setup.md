# Phone connection — OpenRemote 0.7.0

The current home has exactly two entry fields: **Server ID** and **Security PIN**.
Both require six ASCII digits, including leading zeroes. The one primary action
is **Connect remote desktop**; it becomes Cancel during the initial request.
The active session has Stop remote session. An ID is a one-use invitation, not
the older nine-digit persistent PC identifier.

The publisher embeds the common HTTPS service in `config/service-profile.json`.
Existing authorized saved origins can be retained; they are not silently changed.
A fresh build with an empty profile shows provider unavailable and disables
Connect. The user is not offered a technical configuration fallback.

Advanced/settings, service-address entry, setup-link imports, saved-PC lists,
fingerprint prompts and nine-digit lookup are removed from the public home.
The Android manifest has no `openremote://connect` VIEW/BROWSABLE intent filter,
and Activity intent handling cannot use a setup link to rebind the service.
Legacy parsing/storage code remains covered for existing data; it is not a new
setup flow. The previous protocol documentation is retained in
`docs/history/0.6.0/phone-setup.md` for maintenance only.

The PIN is masked and clears on submission, cancellation, backgrounding and home
disposal. Invalid extra digits are rejected without silently accepting a prefix.
One-use redemption is not automatically retried. A known late session is ended
on cancellation; if the response is entirely lost, host Stop and coordinator
expiry remain the recovery paths.

Discovery accepts the optional integer automatic-host-registration capability
(version 0 or 1). The phone authenticates its own P-256 controller key and redeems
the ID/PIN; it never receives the server administrator credential. The Windows
owner's matching in-memory grant must be consumed before media is admitted.
Connected status waits for the active renderer's first decoded frame, and remote
input requires fresh video plus the existing permission and transport checks.

PIN sessions grant view and optionally input, not clipboard, file transfer,
audio, power or unattended trust. Those unavailable actions and monitor switching
are hidden in this flow. See `android/ONE_BUTTON_0.7.md`, `android/VERIFICATION.md`
and `docs/windows-installer-0.7.0.md` for exact behavior and verification limits.

There is no public OpenRemote endpoint in this preview, and physical phone media
has not run in this workspace. This is not an already connected Internet release.
