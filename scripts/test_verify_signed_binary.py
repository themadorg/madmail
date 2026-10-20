"""Ed25519 trailer checks. Uses ephemeral OpenSSL keys, never a production key."""

import hashlib
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

import verify_signed_binary
from verify_signed_binary import VerifyError

REPO = Path(__file__).resolve().parents[1]
SCRIPT = (
    "#!/bin/sh\n"
    'if [ "$1" = version ]; then echo madmail-v2 {version}; fi\n'
    "exit 0\n"
)


def openssl(*args, cwd=None):
    subprocess.run(
        ["openssl", *args],
        cwd=cwd,
        check=True,
        capture_output=True,
    )


def ephemeral_private_key(directory):
    private = directory / "private.pem"
    openssl("genpkey", "-algorithm", "ED25519", "-out", str(private))
    return private


def public_hex(private, directory):
    der = directory / "public.der"
    openssl(
        "pkey",
        "-in",
        str(private),
        "-pubout",
        "-outform",
        "DER",
        "-out",
        str(der),
    )
    raw = der.read_bytes()[-32:]
    if len(raw) != 32:
        raise AssertionError("expected a 32-byte Ed25519 public key")
    return raw.hex()


def sign(private, content):
    with tempfile.TemporaryDirectory() as directory:
        folder = Path(directory)
        (folder / "content").write_bytes(content)
        signature = folder / "sig"
        openssl(
            "pkeyutl",
            "-sign",
            "-inkey",
            str(private),
            "-rawin",
            "-in",
            str(folder / "content"),
            "-out",
            str(signature),
        )
        signed = content + signature.read_bytes()
    if len(signed) != len(content) + 64:
        raise AssertionError("signature trailer must be 64 bytes")
    return signed


class SignatureTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.private = ephemeral_private_key(self.root)
        self.public = public_hex(self.private, self.root)

    def test_public_key_matches_the_updater(self):
        source = (REPO / "crates/chatmail/src/upgrade.rs").read_text(encoding="utf-8")
        self.assertIn(
            f'PUBLIC_KEY_HEX: &str = "{verify_signed_binary.PUBLIC_KEY_HEX}"',
            source,
        )
        self.assertEqual(verify_signed_binary.SIGNATURE_LEN, 64)

    def test_ephemeral_signature_verifies_and_production_key_rejects_it(self):
        signed = sign(self.private, b"portable-binary")
        verify_signed_binary.verify_trailer(signed, self.public)
        with self.assertRaisesRegex(VerifyError, "Ed25519 signature rejected"):
            verify_signed_binary.verify_trailer(signed)

    def test_unsigned_truncated_and_tampered_files_are_rejected(self):
        signed = sign(self.private, b"portable-binary")
        with self.assertRaisesRegex(VerifyError, "too small"):
            verify_signed_binary.verify_trailer(b"short", self.public)
        unsigned = b"portable-binary" + b"\0" * 64
        with self.assertRaisesRegex(VerifyError, "Ed25519 signature rejected"):
            verify_signed_binary.verify_trailer(unsigned, self.public)
        tampered = bytearray(signed)
        tampered[0] ^= 0x01
        with self.assertRaisesRegex(VerifyError, "Ed25519 signature rejected"):
            verify_signed_binary.verify_trailer(bytes(tampered), self.public)

    def test_executed_binary_version_must_match_and_trailer_is_kept(self):
        content = SCRIPT.format(version="1.2.3").encode()
        signed = sign(self.private, content)
        path = self.root / "madmail-windows-amd64.exe"
        path.write_bytes(signed)
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        data = verify_signed_binary.verify_release_binary(
            path, "1.2.3", public_key_hex=self.public
        )
        self.assertEqual(data, signed)
        self.assertTrue(data.endswith(signed[-64:]))
        with self.assertRaisesRegex(VerifyError, "does not match the stable release"):
            verify_signed_binary.verify_release_binary(
                path, "9.9.9", public_key_hex=self.public
            )

    def test_checksum_digest_and_source_are_optional_until_present(self):
        content = b"signed-bytes"
        signed = sign(self.private, content)
        path = self.root / "madmail-windows-amd64.exe"
        path.write_bytes(signed)
        digest = hashlib.sha256(signed).hexdigest()
        sums = f"{digest}  madmail-windows-amd64.exe\n"
        verify_signed_binary.verify_release_binary(
            path,
            "1.2.3",
            public_key_hex=self.public,
            sums_text=sums,
            digest="sha256:" + digest,
            source_text="abc\n".replace("abc", "a" * 40),
            expected_sha="a" * 40,
            version_text="madmail-v2 1.2.3\n",
        )
        with self.assertRaisesRegex(VerifyError, "SHA256SUMS"):
            verify_signed_binary.verify_release_binary(
                path,
                "1.2.3",
                public_key_hex=self.public,
                sums_text=f"{'b' * 64}  madmail-windows-amd64.exe\n",
                version_text="madmail-v2 1.2.3\n",
            )
        with self.assertRaisesRegex(VerifyError, "asset digest"):
            verify_signed_binary.verify_release_binary(
                path,
                "1.2.3",
                public_key_hex=self.public,
                digest="sha256:" + "c" * 64,
                version_text="madmail-v2 1.2.3\n",
            )
        with self.assertRaisesRegex(VerifyError, "SOURCE.txt"):
            verify_signed_binary.verify_release_binary(
                path,
                "1.2.3",
                public_key_hex=self.public,
                source_text="b" * 40 + "\n",
                expected_sha="a" * 40,
                version_text="madmail-v2 1.2.3\n",
            )
        with self.assertRaisesRegex(VerifyError, "unsupported asset digest"):
            verify_signed_binary.verify_release_binary(
                path,
                "1.2.3",
                public_key_hex=self.public,
                digest="sha512:" + "d" * 128,
                version_text="madmail-v2 1.2.3\n",
            )

    def test_cli_rejects_an_unsigned_file(self):
        path = self.root / "unsigned.exe"
        path.write_bytes(b"unsigned")
        import contextlib
        from io import StringIO

        with contextlib.redirect_stderr(StringIO()) as captured:
            code = verify_signed_binary.main(
                ["--file", str(path), "--version", "1.2.3", "--public-key", self.public]
            )
        self.assertEqual(code, 1)
        self.assertIn("verify:", captured.getvalue())
        self.assertNotIn(self.private.read_text(encoding="utf-8"), captured.getvalue())


if __name__ == "__main__":
    unittest.main()
