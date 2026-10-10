#!/usr/bin/env python3
"""Verify the existing Madmail Ed25519 trailer format (no private keys in CI)."""

import argparse
import hashlib
import re
from pathlib import Path


def public_key_from_source(root):
    source = (root / "crates/chatmail/src/upgrade.rs").read_text()
    match = re.search(r'const PUBLIC_KEY_HEX: &str = "([0-9a-f]{64})";', source)
    if not match:
        raise ValueError("Missing upgrade public key in selected source")
    return bytes.fromhex(match[1])


def verify(path, public_key):
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

    content = Path(path).read_bytes()
    if len(content) <= 64:
        raise ValueError("Artifact is too short for an Ed25519 trailer")
    from cryptography.exceptions import InvalidSignature

    try:
        Ed25519PublicKey.from_public_bytes(public_key).verify(
            content[-64:], content[:-64]
        )
    except InvalidSignature as exc:
        raise ValueError("Invalid Ed25519 artifact signature") from exc


def sign(path, private_key_path, public_key):
    from cryptography.exceptions import InvalidSignature
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

    private = bytes.fromhex(Path(private_key_path).read_text().strip())
    if len(private) not in (32, 64):
        raise ValueError("Expected a 32-byte seed or 64-byte Ed25519 private key")
    key = Ed25519PrivateKey.from_private_bytes(private[:32])
    from cryptography.hazmat.primitives import serialization

    actual = key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )
    if actual != public_key:
        raise ValueError("Signing key does not match selected source upgrade key")
    try:
        verify(path, public_key)
        return  # Resuming must not append a second signature.
    except (InvalidSignature, ValueError):
        pass
    path = Path(path)
    content = path.read_bytes()
    signature = key.sign(content)
    key.public_key().verify(signature, content)
    path.write_bytes(content + signature)
    verify(path, public_key)


def digest(path):
    with Path(path).open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def verify_checksums(directory, name="SHA256SUMS"):
    directory = Path(directory)
    seen = set()
    for line in (directory / name).read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9._-]+)", line)
        if not match or match[2] in seen:
            raise ValueError("Invalid or duplicate checksum entry")
        seen.add(match[2])
        path = directory / match[2]
        if path.is_symlink() or digest(path) != match[1]:
            raise ValueError("Artifact checksum mismatch: " + match[2])
    if not seen:
        raise ValueError("Empty checksums")
    return seen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("file", type=Path)
    parser.add_argument("--source", type=Path, default=Path.cwd())
    parser.add_argument("--checksums", type=Path)
    args = parser.parse_args()
    if args.checksums:
        verify_checksums(args.file.parent, args.checksums.name)
    verify(args.file, public_key_from_source(args.source))
    print("Verified Ed25519 signature:", args.file.name)


if __name__ == "__main__":
    main()
