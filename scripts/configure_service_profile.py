#!/usr/bin/env python3
"""Set the public coordinator origin embedded in both client builds.

This configures source inputs only. It neither deploys a server nor enrolls a PC,
and it never accepts enrollment credentials, PINs or relay secrets.
"""
from __future__ import annotations

import argparse
import ipaddress
import json
import os
from pathlib import Path
import re
import tempfile
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
PROFILE = ROOT / "config/service-profile.json"


def canonical_origin(value: str) -> str:
    if not isinstance(value, str) or len(value) > 2048 or not value.isascii() or any(ord(c) <= 32 or ord(c) == 127 for c in value):
        raise ValueError("Use one ASCII HTTPS origin without whitespace")
    if not value.startswith("https://") or any(c in value for c in "\\%+@?#"):
        raise ValueError("Use an HTTPS origin without credentials, escapes, query or fragment")
    parsed = urlsplit(value)
    if parsed.scheme != "https" or not parsed.netloc or parsed.path not in ("", "/") or parsed.username or parsed.password:
        raise ValueError("Use an HTTPS origin without a path or credentials")
    host = parsed.hostname
    if not host or parsed.netloc.endswith(":"):
        raise ValueError("The service host or port is missing")
    port = parsed.port
    if port is not None and not 1 <= port <= 65535:
        raise ValueError("The service port must be between 1 and 65535")
    host = host.lower()
    try:
        address = ipaddress.ip_address(host)
        host = f"[{address.compressed}]" if address.version == 6 else str(address)
    except ValueError:
        dns = host.removesuffix(".")
        if len(dns) > 253 or any(not re.fullmatch(r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?", part) for part in dns.split(".")):
            raise ValueError("Use a valid DNS hostname or IP address")
        if re.fullmatch(r"[0-9.]+", dns) or any(re.fullmatch(r"0x[0-9a-f]+", part) for part in dns.split(".")):
            raise ValueError("Use canonical IP notation, not an ambiguous numeric host")
        if any(dns == suffix or dns.endswith("." + suffix) for suffix in ("example.com", "example.net", "example.org", "invalid", "example", "test")):
            raise ValueError("Use the real deployed service address, not an example domain")
    return f"https://{host}" + (f":{port}" if port is not None and port != 443 else "")


def unique_object(pairs):
    output = {}
    for key, value in pairs:
        if key in output:
            raise ValueError("Duplicate profile field")
        output[key] = value
    return output


def read_profile(path: Path) -> dict:
    raw = path.read_bytes()
    if len(raw) > 4096:
        raise ValueError("Service profile is too large")
    profile = json.loads(raw, object_pairs_hook=unique_object)
    if not isinstance(profile, dict) or set(profile) != {"schemaVersion", "serviceName", "apiOrigin"}:
        raise ValueError("The profile must contain only public schemaVersion, serviceName and apiOrigin fields")
    if type(profile["schemaVersion"]) is not int or profile["schemaVersion"] != 1 or profile["serviceName"] != "OpenRemote":
        raise ValueError("Unsupported service profile")
    origin = profile["apiOrigin"]
    if not isinstance(origin, str):
        raise ValueError("apiOrigin must be a string")
    if origin and canonical_origin(origin) != origin:
        raise ValueError("Use the canonical HTTPS origin in the service profile")
    return profile


def configure(path: Path, origin: str, replace: bool = False) -> dict:
    canonical = canonical_origin(origin)
    original = path.read_bytes()
    existing = read_profile(path)
    if existing["apiOrigin"] and existing["apiOrigin"] != canonical and not replace:
        raise ValueError("A different service is already configured; use --replace only for an intentional new client deployment")
    updated = {"schemaVersion": 1, "serviceName": "OpenRemote", "apiOrigin": canonical}
    encoded = (json.dumps(updated, indent=2) + "\n").encode()
    temporary = None
    try:
        fd, name = tempfile.mkstemp(prefix=".service-profile-", dir=path.parent)
        temporary = Path(name)
        with os.fdopen(fd, "wb") as stream:
            stream.write(encoded); stream.flush(); os.fsync(stream.fileno())
        if path.read_bytes() != original:
            raise ValueError("The profile changed during configuration; review it and retry")
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return updated


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-origin", help="The existing deployed HTTPS coordinator origin")
    parser.add_argument("--replace", action="store_true", help="Deliberately replace a previously configured origin")
    args = parser.parse_args()
    try:
        result = configure(PROFILE, args.api_origin, args.replace) if args.api_origin else read_profile(PROFILE)
    except (ValueError, OSError) as error:
        parser.error(str(error))
    print(json.dumps({"configured": bool(result["apiOrigin"]), "profile": result,
        "nextStep": "Rebuild both host and Android clients from these exact inputs; existing enrolled devices keep their origin." if result["apiOrigin"] else "Set the real service origin before building preconfigured clients."}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
