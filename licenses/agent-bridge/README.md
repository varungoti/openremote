# Agent bridge dependency notices

These records correspond to `agent-bridge/package-lock.json` for OpenRemote
0.8.0. `dependencies.json` records the exact lock-file hash, source-input hashes,
package versions, registry tarball integrity values, installed manifest hashes,
and SHA-256 hashes of every copied original notice file.

| Package | Locked version | Package license metadata | Local original notices |
| --- | --- | --- | --- |
| `@modelcontextprotocol/server` | 2.4.0 | Apache-2.0 | LICENSE |
| `@modelcontextprotocol/core` | 2.4.0 | Apache-2.0 | LICENSE |
| `playwright` | 1.62.1 | Apache-2.0 | LICENSE, NOTICE, ThirdPartyNotices.txt |
| `playwright-core` | 1.62.1 | Apache-2.0 | LICENSE, NOTICE, ThirdPartyNotices.txt |
| `zod` | 4.6.7 | MIT | LICENSE |
| `fsevents` | 2.3.2 | MIT | Locked optional Darwin dependency; not installed on Linux, so no original notice was available locally |

The available original notice files were copied without editing from the exact
installed npm packages. Their packaged third-party notices are retained in full.
License identifiers above reproduce package metadata and do not constitute a
legal determination.

**Browser binary notices are not established by this record.** Playwright's npm
package and its notices are distinct from a separately downloaded Chromium
binary. This agent-bridge change does not package a browser binary. A distributor
that supplies one must retain the notices for that exact browser distribution.
