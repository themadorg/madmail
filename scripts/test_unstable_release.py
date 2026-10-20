"""Tests for the unsigned prerelease helper. No live GitHub or registry calls."""

import hashlib
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import importlib.util
import sys

SPEC = importlib.util.spec_from_file_location(
    "madmail_unstable", Path(__file__).with_name("unstable_release.py")
)
unstable = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = unstable
SPEC.loader.exec_module(unstable)

import unstable_assets
from test_unstable_assets import GENERATION, RELAYS, VERSION, write_inputs


class FakeGh:
    def __init__(self):
        self.created = None
        self.files = {}
        self.uploads = []

    def view(self, tag):
        if self.created is None:
            return None
        return {
            "tagName": tag,
            "isDraft": False,
            "isPrerelease": True,
            "targetCommitish": self.created[1],
            "assets": [{"name": name} for name in self.files],
        }

    def create(self, tag, sha, notes, files):
        if self.created is not None:
            raise AssertionError("create called twice")
        self.created = (tag, sha, notes)
        for path in files:
            self.files[path.name] = path.read_bytes()

    def upload(self, tag, files):
        for path in files:
            if path.name in self.files:
                raise AssertionError("upload would replace an asset")
            self.files[path.name] = path.read_bytes()
        self.uploads.append([path.name for path in files])

    def download(self, tag, name):
        handle = tempfile.NamedTemporaryFile(delete=False)
        handle.write(self.files[name])
        handle.close()
        return Path(handle.name)


class UnstableReleaseTests(unittest.TestCase):
    def setUp(self):
        self.sha = "a" * 40
        self.tag = "v1.2.4-unstable"
        self.notes = unstable.build_notes(
            "1.2.4",
            self.sha,
            "# [1.2.4](https://example.invalid) (2026-10-13)\n\n* fix\n",
        )

    def stage(self):
        directory = Path(self.enterContext(tempfile.TemporaryDirectory()))
        binaries = directory / "in"
        write_inputs(binaries)
        dest = directory / "out"
        return unstable.stage(binaries, self.sha, VERSION, GENERATION, RELAYS, dest)

    def test_notes_start_with_the_testing_warning(self):
        self.assertTrue(self.notes.startswith("TESTING ONLY — NOT FOR PRODUCTION USE.\n"))
        self.assertIn("NOT FOR PRODUCTION USE", self.notes)
        self.assertIn(f"Source revision: {self.sha}", self.notes)
        self.assertIn("* fix", self.notes)

    def test_checksums_cover_binaries_and_source(self):
        files = {path.name: path.read_bytes() for path in self.stage()}
        self.assertEqual(files["SOURCE.txt"], self.sha.encode() + b"\n")
        lines = files["SHA256SUMS"].decode().splitlines()
        names = [line.split("  ", 1)[1] for line in lines]
        self.assertEqual(len(files), 16)
        self.assertEqual(names, sorted(unstable_assets.PAYLOAD_NAMES + (unstable_assets.SOURCE_NAME,)))
        self.assertNotIn(unstable_assets.SUMS_NAME, names)
        for line in lines:
            digest, name = line.split("  ", 1)
            self.assertEqual(digest, hashlib.sha256(files[name]).hexdigest())

    def test_dry_run_does_not_call_github(self):
        gh = FakeGh()
        result = unstable.publish_testing_release(
            self.tag, self.sha, self.notes, self.stage(), gh, write=False
        )
        self.assertTrue(result["dry_run"])
        self.assertIsNone(gh.created)

    def test_create_then_identical_retry_uploads_nothing(self):
        gh = FakeGh()
        files = self.stage()
        first = unstable.publish_testing_release(
            self.tag, self.sha, self.notes, files, gh, write=True
        )
        self.assertTrue(first["created"])
        self.assertIn("TESTING ONLY", gh.created[2])
        second = unstable.publish_testing_release(
            self.tag, self.sha, self.notes, files, gh, write=True
        )
        self.assertFalse(second["created"])
        self.assertEqual(second["uploaded"], [])
        self.assertEqual(gh.uploads, [])

    def test_conflicting_asset_is_rejected(self):
        gh = FakeGh()
        files = self.stage()
        unstable.publish_testing_release(
            self.tag, self.sha, self.notes, files, gh, write=True
        )
        files[0].write_bytes(b"tampered")
        with self.assertRaisesRegex(unstable.UnstableError, "conflicts"):
            unstable.publish_testing_release(
                self.tag, self.sha, self.notes, files, gh, write=True
            )

    def test_partial_upload_resumes_without_replacing(self):
        gh = FakeGh()
        files = self.stage()
        unstable.publish_testing_release(
            self.tag, self.sha, self.notes, files, gh, write=True
        )
        removed = next(iter(gh.files))
        del gh.files[removed]
        result = unstable.publish_testing_release(
            self.tag, self.sha, self.notes, files, gh, write=True
        )
        self.assertEqual(result["uploaded"], [removed])

    def test_stable_release_and_other_commit_are_rejected(self):
        gh = FakeGh()
        gh.created = (self.tag, self.sha, "notes")
        gh.view = lambda tag: {
            "isDraft": False,
            "isPrerelease": False,
            "targetCommitish": self.sha,
            "assets": [],
        }
        with self.assertRaisesRegex(unstable.UnstableError, "stable"):
            unstable.publish_testing_release(
                self.tag, self.sha, self.notes, self.stage(), gh, write=True
            )
        gh.view = lambda tag: {
            "isDraft": False,
            "isPrerelease": True,
            "targetCommitish": "b" * 40,
            "assets": [],
        }
        with self.assertRaisesRegex(unstable.UnstableError, "different commit"):
            unstable.publish_testing_release(
                self.tag, self.sha, self.notes, self.stage(), gh, write=True
            )

    def test_commands_never_clobber_and_hide_stderr(self):
        calls = []

        def fake_run(args, **kwargs):
            calls.append(args)
            return subprocess.CompletedProcess(
                args, 1, stdout="", stderr="token ghp_secret leaked"
            )

        with mock.patch.object(unstable.subprocess, "run", side_effect=fake_run):
            gh = unstable.CommandGh()
            with self.assertRaises(unstable.UnstableError) as error:
                gh.view(self.tag)
        self.assertNotIn("ghp_secret", str(error.exception))
        self.assertNotIn("leaked", str(error.exception))
        self.assertNotIn("--clobber", calls[0])

        def create_ok(args, **kwargs):
            calls.append(args)
            if args[1:3] == ["release", "view"]:
                return subprocess.CompletedProcess(
                    args, 1, stdout="", stderr="release not found\n"
                )
            return subprocess.CompletedProcess(args, 0, stdout="", stderr="")

        calls.clear()
        with mock.patch.object(unstable.subprocess, "run", side_effect=create_ok):
            unstable.CommandGh().create(self.tag, self.sha, self.notes, [])
        create = next(args for args in calls if args[1:3] == ["release", "create"])
        self.assertIn("--prerelease", create)
        self.assertNotIn("--clobber", create)
        self.assertIn(self.sha, create)

    def test_write_refuses_a_pull_request_event(self):
        for event in ("pull_request", "pull_request_target"):
            with (
                mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": event}),
                mock.patch.object(unstable, "validate", side_effect=AssertionError("validate")),
                mock.patch.object(unstable, "CommandGh", side_effect=AssertionError("gh")),
            ):
                code = unstable.main(
                    [
                        "--write",
                        "--tag",
                        self.tag,
                        "--sha",
                        self.sha,
                        "--version",
                        "1.2.4",
                        "--generation",
                        GENERATION,
                        "--changelog",
                        "CHANGELOG.md",
                        "--assets",
                        "dist",
                    ]
                )
            self.assertEqual(code, 1)

    def test_cli_dry_run_checks_the_local_tag(self):
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        subprocess.run(["git", "init", "-b", "main"], cwd=root, check=True, capture_output=True)
        subprocess.run(
            ["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True
        )
        subprocess.run(["git", "config", "user.name", "Test"], cwd=root, check=True)
        (root / "CHANGELOG.md").write_text(
            "# [1.2.4](https://example.invalid) (2026-10-13)\n\n* fix\n",
            encoding="utf-8",
        )
        assets = root / "assets"
        write_inputs(assets)
        subprocess.run(["git", "add", "."], cwd=root, check=True)
        subprocess.run(
            ["git", "commit", "-m", "fix: seed"], cwd=root, check=True, capture_output=True
        )
        sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
        subprocess.run(["git", "tag", self.tag], cwd=root, check=True)
        previous = Path.cwd()
        try:
            os.chdir(root)
            with (
                mock.patch.object(unstable.unstable_assets, "fetch_relays", return_value=RELAYS),
                mock.patch.object(unstable, "CommandGh") as gh_cls,
            ):
                code = unstable.main(
                    [
                        "--tag",
                        self.tag,
                        "--sha",
                        sha,
                        "--version",
                        "1.2.4",
                        "--generation",
                        GENERATION,
                        "--changelog",
                        "CHANGELOG.md",
                        "--assets",
                        "assets",
                    ]
                )
            gh_cls.assert_not_called()
            self.assertEqual(code, 0)
        finally:
            os.chdir(previous)


if __name__ == "__main__":
    unittest.main()
