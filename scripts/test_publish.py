"""Release-channel integration tests use disposable Git repos and a fake GitHub."""

import argparse
import io
import json
import os
import shutil
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import artifact_signing
import publish
import registry_image
import release
import test_release as release_tests

try:
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
except ImportError:
    Ed25519PrivateKey = None


class FakeGitHub(publish.GitHub):
    def __init__(self, root):
        super().__init__(root)
        self.items = []
        self.files = {}
        self.calls = []
        self.fail_upload = None

    def releases(self):
        return self.items

    def add(
        self, tag, sha, *, draft=False, prerelease=False, date="2026-10-01T00:00:00Z"
    ):
        self.items.append(
            {
                "tag_name": tag,
                "target_commitish": sha,
                "draft": draft,
                "prerelease": prerelease,
                "published_at": date,
                "assets": [],
            }
        )

    def exact_tag(self, tag, sha):
        actual = release.git(self.root, "rev-parse", f"{tag}^{{commit}}").strip()
        if actual != sha:
            raise release.ReleaseError(
                "Remote tag does not match selected source commit"
            )

    def gh(self, *args):
        self.calls.append(args)
        if args[:2] == ("release", "create"):
            self.add(
                args[2],
                args[args.index("--target") + 1],
                draft=True,
                prerelease="--prerelease" in args,
            )
        elif args[:2] == ("release", "upload"):
            tag, path = args[2], Path(args[3])
            if path.name == self.fail_upload:
                self.fail_upload = None
                raise release.ReleaseError("Injected interrupted upload")
            self.files[tag, path.name] = path.read_bytes()
            self.get(tag)["assets"].append({"name": path.name})
        elif args[:2] == ("release", "download"):
            tag, directory = args[2], Path(args[args.index("--dir") + 1])
            for (asset_tag, name), content in self.files.items():
                if asset_tag == tag:
                    (directory / name).write_bytes(content)
        elif args[:2] == ("release", "edit"):
            self.get(args[2])["draft"] = False
        else:
            raise AssertionError(args)
        return ""

    def download(self, tag, name, directory):
        path = Path(directory) / name
        path.write_bytes(self.files[tag, name])
        return path


@unittest.skipUnless(
    Ed25519PrivateKey and shutil.which("cargo"), "cryptography and Cargo required"
)
class PublishingTests(unittest.TestCase):
    setUp = release_tests.GitIntegrationTests.setUp
    command = release_tests.GitIntegrationTests.command
    git = release_tests.GitIntegrationTests.git
    write = release_tests.GitIntegrationTests.write
    commit = release_tests.GitIntegrationTests.commit
    bare_remote = release_tests.GitIntegrationTests.bare_remote

    def setup_candidate(self):
        self.key = Ed25519PrivateKey.generate()
        self.public = self.key.public_key().public_bytes(
            serialization.Encoding.Raw, serialization.PublicFormat.Raw
        )
        key_path = Path(self.temp.name) / "private.hex"
        key_path.write_text(
            self.key.private_bytes(
                serialization.Encoding.Raw,
                serialization.PrivateFormat.Raw,
                serialization.NoEncryption(),
            ).hex()
        )
        self.write(
            "crates/chatmail/src/upgrade.rs",
            f'const PUBLIC_KEY_HEX: &str = "{self.public.hex()}";\n',
        )
        self.git("add", "crates/chatmail/src/upgrade.rs")
        self.git("commit", "-m", "fix: candidate")
        result = release.release(self.root, write=True)
        self.bare_remote()
        github = FakeGitHub(self.root)
        github.add("v1.2.3", self.git("rev-parse", "v1.2.3^{commit}"))
        github.add(result["tag"], result["sha"], prerelease=True)
        self.builder = Path(self.temp.name) / "builder.py"
        self.builder.write_text(
            "from pathlib import Path\np=Path('build'); p.mkdir()\n"
            + "\n".join(
                f"(p/{name!r}).write_bytes(b'unsigned-'+{name.encode()!r})"
                for name in publish.REQUIRED
            )
        )
        self.builder.write_text(
            self.builder.read_text()
            + "\n(p/'madmail-linux-amd64').write_text("
            + repr("#!/bin/sh\nprintf 'madmail-v2 1.2.4\\n'\n")
            + ")\n(p/'madmail-linux-amd64').chmod(0o755)\n"
        )
        self.args = argparse.Namespace(
            candidate=result["tag"],
            write=True,
            key=key_path,
            output=Path(self.temp.name) / "publication",
            build_command=f"{sys.executable} {self.builder}",
            test_command=f'{sys.executable} -c "pass"',
        )
        return result, github

    def stage(self):
        result, github = self.setup_candidate()
        with mock.patch("builtins.print"):
            publish.stage(self.root, github, self.args)
        return result, github

    def setup_package(self, result, github):
        output = Path(self.temp.name) / "installer"
        output.mkdir()
        (output / publish.SETUP).write_bytes(b"installer of signed server")
        receipt = {
            "source_commit": result["sha"],
            "server_sha256": artifact_signing.digest(
                self.args.output / "madmail-windows-amd64.exe"
            ),
        }
        (output / "setup-source.json").write_text(json.dumps(receipt))
        publish.write_checksums(output, list(output.iterdir()), "setup-checksums.txt")
        with zipfile.ZipFile(output / "setup-package.zip", "w") as archive:
            for name in (publish.SETUP, "setup-source.json", "setup-checksums.txt"):
                archive.write(output / name, name)
        github.upload("v1.2.4", output / "setup-package.zip")

    def test_stage_exact_older_candidate_signs_and_preserves_main(self):
        result, github = self.setup_candidate()
        self.commit("feat: development advances")
        newer = release.release(self.root, write=True)
        self.git("push", "origin", "main", "--tags")
        head = self.git("rev-parse", "HEAD")
        with mock.patch("builtins.print"):
            publish.stage(self.root, github, self.args)
        self.assertNotEqual(result["sha"], newer["sha"])
        self.assertEqual(self.git("rev-parse", "v1.2.4^{commit}"), result["sha"])
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertTrue(github.get("v1.2.4")["draft"])
        for name in publish.REQUIRED:
            artifact_signing.verify(self.args.output / name, self.public)
        manifest = json.loads((self.args.output / "release.json").read_text())
        self.assertEqual(manifest["source_commit"], result["sha"])
        self.assertEqual(manifest["previous_stable"], "v1.2.3")

    def test_stage_upload_retry_does_not_resign_or_overwrite(self):
        _result, github = self.setup_candidate()
        github.fail_upload = "madmail-windows-amd64.exe"
        with mock.patch("builtins.print"), self.assertRaises(release.ReleaseError):
            publish.stage(self.root, github, self.args)
        before = {p.name: p.read_bytes() for p in self.args.output.iterdir()}
        with mock.patch("builtins.print"):
            publish.stage(self.root, github, self.args)
        self.assertEqual(
            before, {p.name: p.read_bytes() for p in self.args.output.iterdir()}
        )
        self.assertEqual(sum(c[:2] == ("release", "create") for c in github.calls), 1)

    def test_finalize_requires_setup_and_verifies_every_signature(self):
        result, github = self.stage()
        with mock.patch("builtins.print"), self.assertRaises(OSError):
            publish.finalize(self.root, github, self.args)
        self.assertTrue(github.get("v1.2.4")["draft"])
        self.setup_package(result, github)
        with mock.patch("builtins.print"):
            publish.finalize(self.root, github, self.args)
        self.assertFalse(github.get("v1.2.4")["draft"])
        self.assertIn(("v1.2.4", "SHA256SUMS"), github.files)
        with self.assertRaisesRegex(release.ReleaseError, "already published"):
            publish.finalize(self.root, github, self.args)
        self.assertFalse(any("telegram" in str(c).lower() for c in github.calls))

    def test_finalize_retry_after_asset_upload_failure(self):
        result, github = self.stage()
        self.setup_package(result, github)
        github.fail_upload = "setup-source.json"
        with mock.patch("builtins.print"), self.assertRaises(release.ReleaseError):
            publish.finalize(self.root, github, self.args)
        with mock.patch("builtins.print"):
            publish.finalize(self.root, github, self.args)
        self.assertFalse(github.get("v1.2.4")["draft"])

    def test_tampered_remote_asset_is_refused(self):
        result, github = self.stage()
        self.setup_package(result, github)
        github.files["v1.2.4", "madmail-linux-amd64"] += b"tampered"
        with mock.patch("builtins.print"), self.assertRaises(ValueError):
            publish.finalize(self.root, github, self.args)
        self.assertTrue(github.get("v1.2.4")["draft"])

    def test_checksum_rewritten_unsigned_server_still_rejected(self):
        result, github = self.stage()
        self.setup_package(result, github)
        github.files["v1.2.4", "madmail-linux-amd64"] = b"X" * 120
        with tempfile.TemporaryDirectory(dir=self.root.parent) as tmp:
            path = Path(tmp) / "madmail-linux-amd64"
            path.write_bytes(b"X" * 120)
            old = artifact_signing.digest(self.args.output / "madmail-linux-amd64")
            github.files["v1.2.4", "binary-checksums.txt"] = github.files[
                "v1.2.4", "binary-checksums.txt"
            ].replace(old.encode(), artifact_signing.digest(path).encode())
        with (
            mock.patch("builtins.print"),
            self.assertRaisesRegex(ValueError, "signature"),
        ):
            publish.finalize(self.root, github, self.args)

    def test_conflicting_asset_cannot_be_clobbered(self):
        _result, github = self.stage()
        changed = Path(self.temp.name) / "madmail-linux-amd64"
        changed.write_bytes(b"new bytes")
        with self.assertRaisesRegex(release.ReleaseError, "Conflicting existing asset"):
            github.upload("v1.2.4", changed)
        self.assertEqual(
            github.files["v1.2.4", "madmail-linux-amd64"],
            (self.args.output / changed.name).read_bytes(),
        )

    def test_dry_run_does_not_build_sign_or_create_refs(self):
        _result, github = self.setup_candidate()
        self.args.write = False
        before = self.git("show-ref")
        with mock.patch("builtins.print"):
            publish.stage(self.root, github, self.args)
        self.assertEqual(before, self.git("show-ref"))
        self.assertFalse(self.args.output.exists())
        self.assertEqual(github.calls, [])

    def test_candidate_version_mismatch_and_injection_rejected(self):
        _result, _github = self.setup_candidate()
        for invalid in ("v1.2.3", "--help", "v1.2.4-unstable; touch owned"):
            with self.assertRaises(release.ReleaseError):
                publish.candidate(self.root, invalid)
        self.git("tag", "v9.0.0-unstable")
        with self.assertRaisesRegex(release.ReleaseError, "numeric version"):
            publish.candidate(self.root, "v9.0.0-unstable")

    def test_stable_notes_use_published_history_not_latest_local_tag(self):
        result, github = self.setup_candidate()
        first_sha = result["sha"]
        self.commit("feat: included stable feature")
        second = release.release(self.root, write=True)
        self.git("push", "origin", "main", "--tags")
        github.add(
            second["tag"], second["sha"], prerelease=True, date="2026-10-09T00:00:00Z"
        )
        version, sha, stable, base, _ = publish.stable_plan(
            self.root, github, second["tag"]
        )
        self.assertEqual(base, "v1.2.3")
        notes, short, changes = publish.notes_data(
            self.root, version, sha, base, stable
        )
        self.assertIn("candidate", notes)
        self.assertIn("included stable feature", notes)
        self.assertEqual(len(changes), 2)
        self.assertNotEqual(first_sha, sha)
        self.assertIn("candidate", short)

    def test_unstable_warning_and_resume(self):
        result, github = self.setup_candidate()
        # Start with no existing prerelease to exercise draft creation.
        github.items = [
            item for item in github.items if item["tag_name"] != result["tag"]
        ]
        artifacts = Path(self.temp.name) / "unsigned"
        artifacts.mkdir()
        (artifacts / "madmail-linux-amd64").write_bytes(b"unsigned")
        github.fail_upload = "release.json"
        with mock.patch("builtins.print"), self.assertRaises(release.ReleaseError):
            publish.unstable(self.root, github, result["tag"], artifacts, True)
        with mock.patch("builtins.print"):
            publish.unstable(self.root, github, result["tag"], artifacts, True)
        self.assertTrue(github.get(result["tag"])["prerelease"])
        self.assertFalse(github.get(result["tag"])["draft"])
        create = next(c for c in github.calls if c[:2] == ("release", "create"))
        self.assertIn("--prerelease", create)
        self.assertIn("UNSUITABLE FOR PRODUCTION", publish.WARNING)
        self.assertIn("--latest=false", github.calls[-1])

    def test_unstable_retry_after_stable_promotion_keeps_original_notes(self):
        result, github = self.setup_candidate()
        github.items = [
            item for item in github.items if item["tag_name"] != result["tag"]
        ]
        artifacts = Path(self.temp.name) / "unsigned"
        artifacts.mkdir()
        (artifacts / "madmail-linux-amd64").write_bytes(b"unsigned")
        with mock.patch("builtins.print"):
            publish.unstable(self.root, github, result["tag"], artifacts, True)
        original = github.files[result["tag"], "release.json"]
        self.git("tag", "v1.2.4", result["sha"])
        with mock.patch("builtins.print"):
            publish.unstable(self.root, github, result["tag"], artifacts, True)
        self.assertEqual(original, github.files[result["tag"], "release.json"])

    def test_wrong_signing_key_refused(self):
        _result, github = self.setup_candidate()
        self.args.key.write_text(
            Ed25519PrivateKey.generate()
            .private_bytes(
                serialization.Encoding.Raw,
                serialization.PrivateFormat.Raw,
                serialization.NoEncryption(),
            )
            .hex()
        )
        with (
            mock.patch("builtins.print"),
            self.assertRaisesRegex(ValueError, "does not match"),
        ):
            publish.stage(self.root, github, self.args)
        self.assertIsNone(github.get("v1.2.4"))

    def test_announcement_is_claimed_before_send_and_not_duplicated(self):
        result, github = self.stage()
        self.setup_package(result, github)
        with mock.patch("builtins.print"):
            publish.finalize(self.root, github, self.args)
        args = argparse.Namespace(tag="v1.2.4", channel="@test", write=True)
        response = io.BytesIO(b'{"ok": true, "result": {"message_id": 42}}')
        with (
            mock.patch.dict(os.environ, {"TELEGRAM_BOT_TOKEN": "123:TEST"}),
            mock.patch("builtins.print"),
            mock.patch.object(
                publish.urllib.request, "urlopen", return_value=response
            ) as send,
        ):
            publish.announce(self.root, github, args)
            publish.announce(self.root, github, args)
            self.assertEqual(send.call_count, 1)
        self.assertIn(("v1.2.4", "telegram-announcement.json"), github.files)
        self.assertIn(("v1.2.4", "telegram-announcement-sent.json"), github.files)

    def test_ambiguous_announcement_failure_retains_claim_and_redacts_token(self):
        result, github = self.stage()
        self.setup_package(result, github)
        with mock.patch("builtins.print"):
            publish.finalize(self.root, github, self.args)
        args = argparse.Namespace(tag="v1.2.4", channel="@test", write=True)
        with (
            mock.patch.dict(os.environ, {"TELEGRAM_BOT_TOKEN": "123:SECRET"}),
            mock.patch("builtins.print"),
            mock.patch.object(
                publish.urllib.request,
                "urlopen",
                side_effect=OSError("URL contains 123:SECRET"),
            ) as send,
        ):
            with self.assertRaises(release.ReleaseError) as error:
                publish.announce(self.root, github, args)
            self.assertNotIn("SECRET", str(error.exception))
            publish.announce(self.root, github, args)
            self.assertEqual(send.call_count, 1)

    def test_installer_zip_traversal_and_wrong_receipt_refused(self):
        result, github = self.stage()
        self.setup_package(result, github)
        package = Path(self.temp.name) / "setup-package.zip"
        with zipfile.ZipFile(package, "w") as archive:
            archive.writestr("../owned", b"unexpected")
        github.files["v1.2.4", "setup-package.zip"] = package.read_bytes()
        with (
            mock.patch("builtins.print"),
            self.assertRaisesRegex(
                release.ReleaseError, "Unexpected installer package"
            ),
        ):
            publish.finalize(self.root, github, self.args)
        self.assertTrue(github.get("v1.2.4")["draft"])

    def test_archive_must_contain_exact_signed_binary(self):
        _result, _github = self.stage()
        with zipfile.ZipFile(
            self.args.output / "madmail-windows-amd64.zip", "w"
        ) as archive:
            archive.writestr("madmail.exe", b"unsigned replacement")
        with self.assertRaisesRegex(release.ReleaseError, "differs from signed"):
            publish.verify_archives(self.args.output)

    def test_stable_published_history_prevents_backward_promotion(self):
        result, github = self.setup_candidate()
        github.add("v1.3.0", result["sha"], date="2026-10-10T00:00:00Z")
        self.git("tag", "v1.3.0", result["sha"])
        with self.assertRaisesRegex(release.ReleaseError, "newer than previous"):
            publish.stable_plan(self.root, github, result["tag"])

    def test_source_change_in_secure_build_is_rejected(self):
        _result, github = self.setup_candidate()
        self.builder.write_text(
            self.builder.read_text()
            + "\nPath('Cargo.toml').write_text(Path('Cargo.toml').read_text()+"
            + repr("\n# modified source\n")
            + ")\n"
        )
        with mock.patch("builtins.print"), self.assertRaises(release.ReleaseError):
            publish.stage(self.root, github, self.args)
        self.assertIsNone(github.get("v1.2.4"))


@unittest.skipUnless(Ed25519PrivateKey, "cryptography required")
class SignatureTests(unittest.TestCase):
    def test_signing_is_idempotent_and_tampering_fails(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as directory:
            root = Path(directory)
            private = Ed25519PrivateKey.generate()
            public = private.public_key().public_bytes(
                serialization.Encoding.Raw, serialization.PublicFormat.Raw
            )
            key = root / "key.hex"
            key.write_text(
                private.private_bytes(
                    serialization.Encoding.Raw,
                    serialization.PrivateFormat.Raw,
                    serialization.NoEncryption(),
                ).hex()
            )
            binary = root / "binary"
            binary.write_bytes(b"production-binary")
            artifact_signing.sign(binary, key, public)
            before = binary.read_bytes()
            artifact_signing.sign(binary, key, public)
            self.assertEqual(binary.read_bytes(), before)
            binary.write_bytes(b"X" + before[1:])
            with self.assertRaises(ValueError):
                artifact_signing.verify(binary, public)

    def test_checksum_traversal_and_duplicates_refused(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as directory:
            root = Path(directory)
            (root / "SHA256SUMS").write_text("a" * 64 + "  ../secret\n")
            with self.assertRaises(ValueError):
                artifact_signing.verify_checksums(root)


class RegistryTests(unittest.TestCase):
    def test_only_manifest_404_allows_new_image(self):
        from urllib.error import HTTPError

        for status in (404, 401, 403, 500):
            with mock.patch.object(
                registry_image,
                "fetch",
                side_effect=[
                    {"token": "testing"},
                    HTTPError("https://ghcr.io", status, "error", {}, None),
                ],
            ):
                if status == 404:
                    self.assertFalse(
                        registry_image.image_exists(
                            "themadorg/madmail", "2.31.0-unstable", "a" * 40
                        )
                    )
                else:
                    with self.assertRaises(HTTPError):
                        registry_image.image_exists(
                            "themadorg/madmail", "2.31.0-unstable", "a" * 40
                        )

    def test_existing_image_revision_and_index_are_verified(self):
        for revision in ("a" * 40, "b" * 40):
            responses = [
                {"token": "testing"},
                {
                    "manifests": [
                        {
                            "platform": {"os": "linux", "architecture": "amd64"},
                            "digest": "sha256:test",
                        }
                    ]
                },
                {"config": {"digest": "sha256:config"}},
                {"config": {"Labels": {"org.opencontainers.image.revision": revision}}},
            ]
            with mock.patch.object(registry_image, "fetch", side_effect=responses):
                if revision == "a" * 40:
                    self.assertTrue(
                        registry_image.image_exists(
                            "themadorg/madmail", "2.31.0", "a" * 40
                        )
                    )
                else:
                    with self.assertRaisesRegex(ValueError, "conflicts"):
                        registry_image.image_exists(
                            "themadorg/madmail", "2.31.0", "a" * 40
                        )
