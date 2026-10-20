#!/usr/bin/env python3
"""Verify the Ed25519 trailer on a Madmail release binary.

The last 64 bytes are the signature over the preceding bytes. This matches
crates/chatmail/src/upgrade.rs (Madmail ``clitools.VerifySignature``). The
default public key is the key embedded in the updater. The file is not
stripped or re-signed.

OpenSSL 3 verifies the signature. Tests and the Windows release job use the
system ``openssl``; this module does not add a project dependency.
"""

import argparse
import base64
import hashlib
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# Same key as crates/chatmail/src/upgrade.rs PUBLIC_KEY_HEX.
PUBLIC_KEY_HEX = "7cb0bcc1d8e91e51f631c9ad6025e8e6e0222a27c3eeaf8608cf1c8430a6c6b0"
SIGNATURE_LEN = 64
# SubjectPublicKeyInfo prefix for a 32-byte Ed25519 key (OpenSSL DER).
ED25519_SPKI_PREFIX = bytes.fromhex("302a300506032b6570032100")
VERSION_LINE = re.compile(r"madmail-v2\s+([0-9]+\.[0-9]+\.[0-9]+)\Z")
SUM_LINE = re.compile(r"([0-9a-fA-F]{64})\s+\*?(\S+)\Z")
SHA256_HEX = re.compile(r"[0-9a-fA-F]{64}\Z")


class VerifyError(Exception):
    """A verification failure safe to print in CI."""


def public_key_pem(public_key_hex):
    try:
        raw = bytes.fromhex(public_key_hex.strip())
    except ValueError as exc:
        raise VerifyError("public key is not hex") from exc
    if len(raw) != 32:
        raise VerifyError("public key must be 32 bytes")
    body = base64.b64encode(ED25519_SPKI_PREFIX + raw).decode("ascii")
    lines = [body[index : index + 64] for index in range(0, len(body), 64)]
    text = "-----BEGIN PUBLIC KEY-----\n" + "\n".join(lines) + "\n-----END PUBLIC KEY-----\n"
    return text.encode("ascii")


def verify_trailer(data, public_key_hex=None):
    """Accept only a valid 64-byte Ed25519 trailer over the preceding bytes."""
    if public_key_hex is None:
        public_key_hex = PUBLIC_KEY_HEX
    if len(data) < SIGNATURE_LEN:
        raise VerifyError("file too small to contain a signature")
    content, signature = data[:-SIGNATURE_LEN], data[-SIGNATURE_LEN:]
    pem = public_key_pem(public_key_hex)
    with tempfile.TemporaryDirectory(prefix="madmail-sig-") as directory:
        folder = Path(directory)
        (folder / "content").write_bytes(content)
        (folder / "sig").write_bytes(signature)
        (folder / "pub.pem").write_bytes(pem)
        try:
            result = subprocess.run(
                [
                    "openssl",
                    "pkeyutl",
                    "-verify",
                    "-pubin",
                    "-inkey",
                    str(folder / "pub.pem"),
                    "-rawin",
                    "-in",
                    str(folder / "content"),
                    "-sigfile",
                    str(folder / "sig"),
                ],
                capture_output=True,
                check=False,
            )
        except (OSError, subprocess.SubprocessError) as exc:
            raise VerifyError("openssl is required to verify the signature") from exc
    if result.returncode != 0:
        raise VerifyError("Ed25519 signature rejected")


def parse_version_output(text):
    """Return the numeric version from ``madmail version`` stdout."""
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        match = VERSION_LINE.fullmatch(stripped)
        if not match:
            raise VerifyError("binary version output is not madmail-v2 X.Y.Z")
        return match.group(1)
    raise VerifyError("binary version output is empty")


def version_from_executable(path):
    try:
        result = subprocess.run(
            [str(path), "version"],
            text=True,
            capture_output=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise VerifyError("binary version command failed") from exc
    if result.returncode != 0:
        raise VerifyError("binary version command failed")
    return parse_version_output(result.stdout)


def checksum_digest(sums_text, filename):
    found = None
    for raw in sums_text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        match = SUM_LINE.fullmatch(line)
        if not match:
            raise VerifyError("checksum line is not SHA256SUMS format")
        digest, name = match.group(1).lower(), match.group(2)
        if name != filename:
            continue
        if found is not None and found != digest:
            raise VerifyError("checksum file has conflicting digests")
        found = digest
    if found is None:
        raise VerifyError("checksum file does not list the executable")
    return found


def check_digest(data, expected):
    """Check a GitHub asset digest when the API provides one."""
    body = (expected or "").strip()
    if not body:
        return
    if body.lower().startswith("sha256:"):
        body = body.split(":", 1)[1]
    elif ":" in body:
        raise VerifyError("unsupported asset digest")
    if not SHA256_HEX.fullmatch(body):
        raise VerifyError("asset digest is not sha256")
    if hashlib.sha256(data).hexdigest() != body.lower():
        raise VerifyError("asset digest does not match the downloaded executable")


def check_sums(data, sums_text, filename):
    """Checksums cover the published bytes, including the signature trailer."""
    digest = checksum_digest(sums_text, filename)
    if hashlib.sha256(data).hexdigest() != digest:
        raise VerifyError("SHA256SUMS does not match the downloaded executable")


def check_source_text(text, expected_sha):
    line = text.strip().lower()
    if line != expected_sha.lower() or not re.fullmatch(r"[0-9a-f]{40}", line):
        raise VerifyError("SOURCE.txt does not match the stable tag commit")


def verify_release_binary(
    path,
    version,
    *,
    public_key_hex=None,
    sums_text=None,
    digest=None,
    source_text=None,
    expected_sha=None,
    version_text=None,
):
    """Verify signature, optional provenance, and the binary's own version.

    ``version_text`` is for tests that cannot execute the payload. The release
    job omits it and runs ``<binary> version``.
    """
    if public_key_hex is None:
        public_key_hex = PUBLIC_KEY_HEX
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise VerifyError("signed executable was not downloaded")
    data = path.read_bytes()
    verify_trailer(data, public_key_hex)
    if digest:
        check_digest(data, digest)
    if sums_text is not None:
        check_sums(data, sums_text, path.name)
    if source_text is not None:
        if not expected_sha:
            raise VerifyError("SOURCE.txt requires the tag commit")
        check_source_text(source_text, expected_sha)
    got = (
        parse_version_output(version_text)
        if version_text is not None
        else version_from_executable(path)
    )
    if got != version:
        raise VerifyError("binary version does not match the stable release")
    return data


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--file", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--public-key", default=PUBLIC_KEY_HEX)
    parser.add_argument("--sums", type=Path)
    parser.add_argument("--digest", default="")
    parser.add_argument("--source", type=Path)
    parser.add_argument("--sha", default="")
    args = parser.parse_args(argv)
    try:
        verify_release_binary(
            args.file,
            args.version,
            public_key_hex=args.public_key,
            sums_text=args.sums.read_text(encoding="utf-8") if args.sums else None,
            digest=args.digest,
            source_text=args.source.read_text(encoding="utf-8") if args.source else None,
            expected_sha=args.sha,
        )
    except VerifyError as exc:
        print(f"verify: {exc}", file=sys.stderr)
        return 1
    print(f"verified {args.version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
