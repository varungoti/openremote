# OpenRemote agent bridge 0.8

This local MCP server lets an AI client operate an **owner-activated OpenRemote
desktop** through the same browser controller used by a person. It connects with
the permanent nine-digit PC ID and current six-digit PIN, waits for decoded
desktop media, returns screenshots, and sends bounded mouse/keyboard actions.
The Windows owner sees an `AI agent · <name>` connection label. The label is a
display claim, not independent verification of the agent's identity.

The bridge speaks MCP over standard input/output only. It opens no HTTP server
or browser debugging listener and exposes nine tools: connect, status,
screenshot, click, move, scroll, key, type_text and disconnect. It has no shell,
arbitrary JavaScript, file-transfer, URL-navigation or filesystem tool.

## Install and start

Use Node.js 22 or later. From this directory:

```sh
npm ci --ignore-scripts
npx playwright install chromium
node src/index.mjs
```

The Node command is intended to be launched as a child process by an MCP-capable
AI client. It waits for protocol input; an empty terminal is normal. Configure
the client with a command and the absolute path to this entry point, for example:

```json
{
  "mcpServers": {
    "openremote": {
      "command": "node",
      "args": ["/absolute/path/to/remote-desktop-platform/agent-bridge/src/index.mjs"]
    }
  }
}
```

The example configuration is a conventional stdio MCP entry; exact settings
locations vary by client. This is an independent OpenRemote bridge, not a
published integration with Phaze or a claim that any particular AI app has been
configured. For Windows, use the corresponding absolute Windows path with JSON
backslashes escaped.

The publisher sets the same `config/service-profile.json` used by the other
OpenRemote clients. Its `apiOrigin` must be a canonical HTTPS origin. The bridge
does not accept a service address or PIN in command-line arguments or environment
variables. An unconfigured profile still permits MCP discovery and status, while
`connect` returns `service_not_configured` before launching a browser. This source
checkout currently has no published service origin.

Chromium's sandbox remains enabled. Browser debug modes (`DEBUG` or `PWDEBUG`)
are refused before connection because protocol debugging could print credential
arguments. The bridge does not change proxy, certificate or sandbox policy to
make an unsupported environment run.

## Connect and observe

Activate Connection on the Windows host using a six-digit PIN. Give the MCP
client the visible nine-digit PC ID and that PIN for the `connect` tool:

```json
{
  "pcId": "123456789",
  "pin": "001234",
  "name": "My assistant"
}
```

These are synthetic example credentials. The bridge keeps no PIN configuration
file or persistent browser profile. It never prints PINs, bearer tokens, raw
browser exceptions or browser request data to its protocol output or diagnostics.
The MCP client receives tool arguments and may retain its own conversation or
tool history; that is outside the bridge's storage behavior.

`connect` resolves only after the browser controller reports a decoded, current
desktop frame. Its total operation deadline is 45 seconds. Status returns only
`state`, `pcId`, `hostName` and `canControl`, with states `idle`, `connecting`,
`connected` or `view_only`. A host name is sanitized before display. Being
approved by the coordinator alone does not report a connected desktop.

An activation permits up to four pending/live peers. Only one session reserves
input. A later AI connection may therefore be view-only: screenshots work,
while input tools refuse the action. Existing viewers never gain input when
another controller leaves. The owner can stop every activation session from
the Windows host.

## Tools and bounds

| Tool | Arguments and behavior |
| --- | --- |
| `connect` | Nine-digit `pcId`, six-digit `pin`, optional plain `name` up to 64 UTF-8 bytes; replaces this bridge's previous session and waits for media |
| `status` | No arguments; returns sanitized local connection state and input permission |
| `screenshot` | No arguments; returns a PNG image and `width`, `height`, `frameToken` metadata |
| `click` | `frameToken`, integer `x`, `y` inside that image, optional `button`: `left`, `right` or `middle` |
| `move` | `frameToken`, integer `x`, `y` inside that image |
| `scroll` | `frameToken`, integer `dx`, `dy` from -1000 to 1000 |
| `key` | `frameToken` and 1–5 distinct supported DOM key `codes`, pressed and released as one chord |
| `type_text` | `frameToken` and 1–1000 Unicode characters, no NUL or unpaired surrogate, at most 4096 UTF-8 bytes |
| `disconnect` | No arguments; closes the local browser and requests remote session termination |

Supported keys are `KeyA`–`KeyZ`, `Digit0`–`Digit9`, `F1`–`F12`, Enter, Tab,
Space, Backspace, Delete, Insert, Home, End, PageUp, PageDown, the four Arrow
keys, Escape, and left/right Control, Alt, Shift and Meta. Arbitrary key names
and extra tool fields are rejected.

Every input operation needs a new screenshot observation. Its opaque token is
valid for **one action within 15 seconds** and remains bound to that connection,
display and image dimensions. The browser also checks that decoded video is
still arriving within its 2.5-second freshness window. Tokens are consumed before
input; repeating a tool call never repeats its action. The browser releases held
keys and buttons after each action, including partial-send failures.

Images are limited to 4096 pixels on each axis, at most 8,294,400 pixels total,
and at most 16 MiB of decoded PNG data. The bridge checks the PNG signature and
header dimensions before returning it. No screenshot is written to disk.

## Cancellation and shutdown

Desktop operations are serialized with a maximum of 16 queued/in-flight
operations. An ordinary operation has a 10-second deadline; a connection has a
45-second deadline. Five minutes without a successful tool operation closes the
browser. Disconnect preempts an in-progress connection or action and invalidates
its generation, so late setup or screenshot results cannot revive it.

MCP cancellation, closed standard input, SIGINT, SIGTERM and operation timeout
all close the owned browser and clear local permission/observation state. Remote
end is attempted before closing. `disconnect.stopConfirmed` is `true` only when
the browser reports the remote end acknowledgment, `null` when it had no admitted
session, and `false` when acknowledgment was unavailable. A false result still
means this bridge has disabled local input; it does not claim that the remote
coordinator confirmed termination. Cleanup has a bounded wait, and the Windows
host independently tears down failed/disconnected media tasks.

## Dependencies and verification

`package-lock.json` pins the official MCP server SDK 2.4.0, Playwright 1.62.1,
Zod 4.6.7 and their transitive package integrity values. The SDK's `serveStdio`
entry supports both legacy MCP initialization and the 2026-07-28 per-request
protocol. The incoming protocol buffer is limited to 64 KiB. Standard output
carries only SDK JSON-RPC messages.

Run the bounded tests with:

```sh
npm test
```

The suite exercises the actual stdio SDK framing and production child-process
startup/EOF, both protocol eras, tool listing and invalid arguments, credential
error redaction, view-only behavior, image bounds, single-use observations,
cancellation, late completion, queue limits, idle expiry and shutdown. Browser
media and input tests use an explicitly injected mock adapter; they are not
reported as a real remote desktop test. Production never substitutes a mock
adapter or generated screenshot.

This workspace cannot launch Chromium because its socket operation is denied
with `EPERM`, and the bundled service profile is unconfigured. End-to-end agent
media, remote input, public Internet relay and third-party MCP client integration
remain unverified. The separate browser-controller lifecycle tests cover the
actual exported `agentAPI` module with controlled browser fixtures.

The implementation was checked against the official
[MCP stdio serving guide](https://ts.sdk.modelcontextprotocol.io/v2/serving/stdio),
[cancellation guidance](https://ts.sdk.modelcontextprotocol.io/v2/servers/logging-progress-cancellation),
and [transport specification](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports).
