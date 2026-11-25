#!/usr/bin/env python3
"""Create private deployment config. Does not contact or deploy to any server."""
import argparse
import ipaddress
import os
from pathlib import Path
import re
import secrets

HOSTNAME = re.compile(r"(?=.{1,253}$)(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,63}$")


def hostname(value):
    if not HOSTNAME.fullmatch(value):
        raise argparse.ArgumentTypeError("Use a DNS hostname, without a scheme, port or path")
    if value.endswith((".example", ".invalid", ".test")):
        raise argparse.ArgumentTypeError("Set a real hostname before creating deployment configuration")
    return value.lower()


def render_turn(domain, public_ip, shared_secret, tls_port):
    # No process command line contains the shared secret. It lives in a private file.
    return f"""# Generated OpenRemote TURN configuration. Keep private.
listening-port=3478
tls-listening-port={tls_port}
external-ip={public_ip}
realm={domain}
server-name={domain}
fingerprint
use-auth-secret
static-auth-secret={shared_secret}
cert=/run/tls/fullchain.pem
pkey=/run/tls/privkey.pem
min-port=49160
max-port=49259
no-cli
no-multicast-peers
no-tcp-relay
stale-nonce=600
max-allocate-lifetime=600
user-quota=4
total-quota=200
max-bps=4000000
bps-capacity=200000000
pidfile=/tmp/turnserver.pid
userdb=/tmp/turndb
log-file=stdout
simple-log
# Prevent relay use as a route into private infrastructure.
denied-peer-ip=0.0.0.0-0.255.255.255
denied-peer-ip=10.0.0.0-10.255.255.255
denied-peer-ip=100.64.0.0-100.127.255.255
denied-peer-ip=127.0.0.0-127.255.255.255
denied-peer-ip=169.254.0.0-169.254.255.255
denied-peer-ip=172.16.0.0-172.31.255.255
denied-peer-ip=192.168.0.0-192.168.255.255
denied-peer-ip=224.0.0.0-255.255.255.255
denied-peer-ip=::
denied-peer-ip=::1
denied-peer-ip=fc00::-fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff
denied-peer-ip=fe80::-febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff
"""


def private_write(path, text):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as output:
        output.write(text)


def configure(root, api, turn, public_ip, email, tls_port):
    paths = [root / ".env", root / "secrets/controlplane.env", root / "secrets/turnserver.conf"]
    if any(p.exists() for p in paths):
        raise ValueError("Existing configuration found; refusing to rotate or overwrite secrets")
    if any(c in email for c in "\r\n\"'#$ ") or "@" not in email:
        raise ValueError("Provide a valid single-line ACME email address")
    if not 1 <= tls_port <= 65535:
        raise ValueError("TURN TLS port must be between 1 and 65535")
    ip = ipaddress.ip_address(public_ip)
    if ip.version != 4 or not ip.is_global:
        raise ValueError("TURN requires the relay server's public IPv4 address")
    root.mkdir(parents=True, exist_ok=True)
    private_dir = root / "secrets"
    private_dir.mkdir(mode=0o700, exist_ok=True)
    os.chmod(private_dir, 0o700)
    shared = secrets.token_hex(32)
    enrollment = secrets.token_urlsafe(48)
    private_write(paths[0], f"API_DOMAIN={api}\nACME_EMAIL={email}\n"
        f"TURN_UID={os.getuid()}\nTURN_GID={os.getgid()}\n")
    private_write(paths[1], f"OPENREMOTE_ENROLLMENT_TOKEN={enrollment}\n"
        f"OPENREMOTE_AUTOMATIC_HOST_REGISTRATION=true\n"
        f"OPENREMOTE_PUBLIC_ORIGIN=https://{api}\n"
        f"OPENREMOTE_STUN_URLS=stun:{turn}:3478\n"
        f"OPENREMOTE_TURN_URLS=turn:{turn}:3478?transport=udp,turn:{turn}:3478?transport=tcp,turns:{turn}:{tls_port}?transport=tcp\n"
        f"OPENREMOTE_TURN_SECRET={shared}\n")
    private_write(paths[2], render_turn(turn, str(ip), shared, tls_port))
    return paths


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-domain", type=hostname, required=True)
    parser.add_argument("--turn-domain", type=hostname, required=True)
    parser.add_argument("--turn-public-ip", required=True)
    parser.add_argument("--email", required=True)
    parser.add_argument("--turn-tls-port", type=int, default=5349,
                        help="Use 443 only on a separate relay IP/host")
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        paths = configure(args.output, args.api_domain, args.turn_domain, args.turn_public_ip, args.email, args.turn_tls_port)
    except (ValueError, OSError) as exc:
        parser.exit(1, f"Configuration not created: {exc}\n")
    print("Created private configuration files:")
    for path in paths:
        print(path)
    print("Install the TURN certificate/key at secrets/turn-cert.pem and secrets/turn-key.pem before starting relay.")
    print("Keep secrets/controlplane.env private. Its administrator token never goes into client apps.")
    print("Automatic PC registration is enabled. Embed this deployment's public origin in the Windows and phone builds before distribution.")


if __name__ == "__main__":
    main()
