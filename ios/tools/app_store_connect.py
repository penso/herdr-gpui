"""The App Store Connect API, for the tools beside this file.

Reads ASC_KEY_ID, ASC_ISSUER_ID, and ASC_KEY_P8 (the .p8 file's text) as
testflight.sh does, and signs the API token with `openssl`, so it needs
nothing beyond the standard library.
"""

import base64
import json
import os
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request

API = "https://api.appstoreconnect.apple.com/v1"


def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def raw_signature(der: bytes) -> bytes:
    """ES256 in a JWT is r and s as 32 bytes each; openssl writes the
    DER sequence of two integers."""
    if len(der) < 8 or der[0] != 0x30:
        raise ValueError("not a DER ECDSA signature")
    index = 2 if der[1] < 0x80 else 2 + (der[1] & 0x7F)
    parts = []
    for _ in range(2):
        if der[index] != 0x02:
            raise ValueError("not a DER ECDSA signature")
        length = der[index + 1]
        value = der[index + 2 : index + 2 + length].lstrip(b"\0")
        if len(value) > 32:
            raise ValueError("signature integer longer than 32 bytes")
        parts.append(value.rjust(32, b"\0"))
        index += 2 + length
    return parts[0] + parts[1]


def token(key_id: str, issuer: str, key_pem: str) -> str:
    now = int(time.time())
    header = b64url(json.dumps({"alg": "ES256", "kid": key_id, "typ": "JWT"}).encode())
    claims = b64url(
        json.dumps(
            {"iss": issuer, "iat": now, "exp": now + 600, "aud": "appstoreconnect-v1"}
        ).encode()
    )
    signing_input = f"{header}.{claims}".encode()
    with tempfile.TemporaryDirectory() as work:
        key_path = os.path.join(work, "key.p8")
        descriptor = os.open(key_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as key_file:
            key_file.write(key_pem.strip() + "\n")
        der = subprocess.run(
            ["openssl", "dgst", "-sha256", "-sign", key_path],
            input=signing_input,
            capture_output=True,
            check=True,
        ).stdout
    return f"{header}.{claims}.{b64url(raw_signature(der))}"


def bearer_from_env() -> str:
    """A token good for ten minutes; raises KeyError naming a missing variable."""
    return token(
        os.environ["ASC_KEY_ID"], os.environ["ASC_ISSUER_ID"], os.environ["ASC_KEY_P8"]
    )


def get(path: str, query: dict, bearer: str) -> dict:
    url = f"{API}/{path}?{urllib.parse.urlencode(query)}"
    request = urllib.request.Request(url, headers={"Authorization": f"Bearer {bearer}"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def send(method: str, path: str, body: dict, bearer: str) -> dict:
    request = urllib.request.Request(
        f"{API}/{path}",
        data=json.dumps(body).encode(),
        method=method,
        headers={"Authorization": f"Bearer {bearer}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def app_id(bundle_id: str, bearer: str):
    """The app's App Store Connect ID, or None when it has no such app."""
    apps = get("apps", {"filter[bundleId]": bundle_id, "fields[apps]": "bundleId"}, bearer)
    return next(
        (item["id"] for item in apps["data"] if item["attributes"]["bundleId"] == bundle_id),
        None,
    )
