#!/usr/bin/env python3
"""Map inherited proxy and CA settings into task-local Java configuration."""
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
from urllib.parse import urlsplit, unquote

ROOT = Path(__file__).resolve().parent
JAVA = ROOT / "jdk-21.0.13+9"
GRADLE = ROOT / "gradle-user"
GRADLE.mkdir(exist_ok=True)
truststore = ROOT / "java-cacerts"
if not truststore.exists():
    shutil.copyfile(JAVA / "lib/security/cacerts", truststore)
    certificates = set()
    for key in ["SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE"]:
        filename = os.environ.get(key)
        if not filename:
            continue
        for block in re.findall(r"-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----", Path(filename).read_text(), flags=re.S):
            fingerprint = hashlib.sha256(block.encode()).hexdigest()
            if fingerprint in certificates:
                continue
            certificates.add(fingerprint)
            temporary = ROOT / ("inherited-ca-" + fingerprint + ".pem")
            temporary.write_text(block + "\n")
            result = subprocess.run([str(JAVA / "bin/keytool"), "-importcert", "-noprompt", "-alias", "inherited-" + fingerprint, "-file", str(temporary), "-keystore", str(truststore), "-storepass", "changeit"], capture_output=True, text=True)
            temporary.unlink()
            if result.returncode:
                raise SystemExit("Unable to import inherited CA into task-local Java truststore")

def property_value(value):
    return str(value).replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")

properties = {"systemProp.javax.net.ssl.trustStore": str(truststore), "org.gradle.workers.max": "2", "org.gradle.parallel": "false", "kotlin.compiler.execution.strategy": "in-process"}
for scheme in ["http", "https"]:
    proxy = os.environ.get(scheme + "_proxy") or os.environ.get(scheme.upper() + "_PROXY")
    if not proxy:
        continue
    parsed = urlsplit(proxy)
    if not parsed.hostname:
        raise SystemExit("Inherited proxy setting has no hostname")
    properties[f"systemProp.{scheme}.proxyHost"] = parsed.hostname
    properties[f"systemProp.{scheme}.proxyPort"] = str(parsed.port or (443 if parsed.scheme == "https" else 80))
    if parsed.username is not None:
        properties[f"systemProp.{scheme}.proxyUser"] = unquote(parsed.username)
    if parsed.password is not None:
        properties[f"systemProp.{scheme}.proxyPassword"] = unquote(parsed.password)
non_proxy = os.environ.get("no_proxy") or os.environ.get("NO_PROXY")
if non_proxy:
    hosts = []
    for host in non_proxy.split(","):
        host = host.strip()
        if not host:
            continue
        if host.startswith("."):
            hosts.extend([host[1:], "*" + host])
        else:
            hosts.append(host)
    properties["systemProp.http.nonProxyHosts"] = "|".join(hosts)
path = GRADLE / "gradle.properties"
path.write_text("\n".join(key + "=" + property_value(value) for key, value in properties.items()) + "\n")
path.chmod(0o600)
print("Configured task-local Java proxy properties and inherited CA trust; TLS verification retained")
