# Task-local helper snapshots

These are the exact restoration and verification scripts used for this build.
They are preserved as evidence; their paths describe the original task-local
workspace. Place them back under the matching `toolchains/` paths or review and
adjust the workspace base before reproducing elsewhere.

No keystore, private key, generated Gradle proxy properties, truststore, or
dependency archive is included. Network configuration is derived at runtime
from the inherited proxy and CA environment. The signing identity must be
provided separately; the Android runner refuses a missing or changed key.
