"""Stable Windows installer policy. No live releases, keys, or uploads."""

import hashlib
import json
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

import yaml

import verify_signed_binary
import windows_release
from verify_signed_binary import VerifyError
from test_verify_signed_binary import ephemeral_private_key, public_hex, sign

REPO = Path(__file__).resolve().parents[1]
WORKFLOW = REPO / ".github/workflows/windows.yml"


def git(root, *args):
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise AssertionError(result.stderr or result.stdout)
    return result.stdout.strip()


def init_repo(root):
    git(root, "init", "-b", "main")
    git(root, "config", "user.email", "test@example.com")
    git(root, "config", "user.name", "Test")


def commit_version(root, version, message="release"):
    (root / ".version").write_text(version + "\n", encoding="utf-8")
    (root / "Cargo.toml").write_text(
        textwrap.dedent(
            f"""\
            [workspace.package]
            version = "{version}"
            """
        ),
        encoding="utf-8",
    )
    git(root, "add", ".version", "Cargo.toml")
    git(root, "commit", "-m", message)
    return git(root, "rev-parse", "HEAD")


def release_meta(tag, assets, **extra):
    normalized = []
    for item in assets:
        if isinstance(item, str):
            normalized.append({"name": item})
        else:
            normalized.append(dict(item))
    meta = {
        "tagName": tag,
        "isDraft": False,
        "isPrerelease": False,
        "targetCommitish": "main",
        "assets": normalized,
    }
    meta.update(extra)
    return meta


class WindowsReleaseTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        init_repo(self.root)
        self.commit = commit_version(self.root, "1.2.3")
        git(self.root, "tag", "v1.2.3", self.commit)
        self.private = ephemeral_private_key(self.root)
        self.public = public_hex(self.private, self.root)
        self.signed = sign(
            self.private,
            b"#!/bin/sh\nif [ \"$1\" = version ]; then echo madmail-v2 1.2.3; fi\nexit 0\n",
        )

    def patch_key(self):
        original = verify_signed_binary.PUBLIC_KEY_HEX
        verify_signed_binary.PUBLIC_KEY_HEX = self.public
        self.addCleanup(setattr, verify_signed_binary, "PUBLIC_KEY_HEX", original)

    def write_exe(self, folder):
        path = folder / windows_release.EXE_NAME
        path.write_bytes(self.signed)
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        return path

    def write_tray(self, folder, data=b"tray-from-this-release"):
        path = folder / windows_release.TRAY_NAME
        path.write_bytes(data)
        return path

    def test_annotated_and_lightweight_tags_select_the_commit(self):
        self.patch_key()
        later = commit_version(self.root, "1.2.4", "later")
        git(self.root, "tag", "-a", "v1.2.3-annotated", "-m", "stable", self.commit)
        # The lightweight tag under test is v1.2.3; the annotated tag is separate.
        tag_object = git(self.root, "rev-parse", "v1.2.3-annotated")
        self.assertNotEqual(tag_object, self.commit)
        self.assertEqual(
            windows_release.peeled_commit(self.root, "v1.2.3-annotated"), self.commit
        )
        self.assertEqual(windows_release.peeled_commit(self.root, "v1.2.3"), self.commit)
        self.assertNotEqual(git(self.root, "rev-parse", "HEAD"), self.commit)
        self.assertEqual(later, git(self.root, "rev-parse", "HEAD"))
        exe = self.write_exe(self.root)
        tray = self.write_tray(self.root)
        dest = self.root / "build" / windows_release.EXE_NAME
        result = windows_release.prepare(
            self.root,
            "v1.2.3",
            release_meta(
                "v1.2.3",
                [windows_release.EXE_NAME, windows_release.TRAY_NAME],
            ),
            exe,
            self.root,
            dest,
        )
        self.assertEqual(result["sha"], self.commit)
        self.assertEqual(result["version"], "1.2.3")
        self.assertEqual(result["tray_checksum"], "absent")
        self.assertEqual(dest.read_bytes(), self.signed)
        self.assertEqual((dest.parent / windows_release.TRAY_NAME).read_bytes(), tray.read_bytes())

    def test_bad_signature_unsigned_file_and_version_mismatch_do_not_stage(self):
        self.patch_key()
        exe = self.write_exe(self.root)
        dest = self.root / "out.exe"
        tampered = self.root / windows_release.EXE_NAME
        data = bytearray(exe.read_bytes())
        data[0] ^= 0x01
        tampered.write_bytes(data)
        meta = release_meta("v1.2.3", [windows_release.EXE_NAME])
        with self.assertRaisesRegex(VerifyError, "Ed25519 signature rejected"):
            windows_release.prepare(self.root, "v1.2.3", meta, tampered, self.root, dest)
        self.assertFalse(dest.exists())
        unsigned = self.root / "unsigned" / windows_release.EXE_NAME
        unsigned.parent.mkdir()
        unsigned.write_bytes(b"not-signed" + b"\0" * 64)
        with self.assertRaisesRegex(VerifyError, "Ed25519 signature rejected"):
            windows_release.prepare(self.root, "v1.2.3", meta, unsigned, self.root, dest)
        self.assertFalse(dest.exists())
        exe.write_bytes(self.signed)
        with self.assertRaisesRegex(VerifyError, "does not match"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                meta,
                exe,
                self.root,
                dest,
                version_text="madmail-v2 9.9.9\n",
            )
        self.assertFalse(dest.exists())

    def test_missing_prerelease_and_inconsistent_metadata_fail(self):
        self.patch_key()
        exe = self.write_exe(self.root)
        dest = self.root / "out.exe"
        with self.assertRaisesRegex(VerifyError, "not on the release"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta("v1.2.3", []),
                exe,
                self.root,
                dest,
            )
        with self.assertRaisesRegex(VerifyError, "draft or prerelease"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta("v1.2.3", [windows_release.EXE_NAME], isPrerelease=True),
                exe,
                self.root,
                dest,
            )
        with self.assertRaisesRegex(VerifyError, "vX.Y.Z"):
            windows_release.prepare(
                self.root,
                "v1.2.3-unstable",
                release_meta("v1.2.3-unstable", [windows_release.EXE_NAME]),
                exe,
                self.root,
                dest,
            )
        with self.assertRaisesRegex(VerifyError, "commit SHA"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta(
                    "v1.2.3",
                    [windows_release.EXE_NAME],
                    targetCommitish="b" * 40,
                ),
                exe,
                self.root,
                dest,
            )
        (self.root / ".version").write_text("9.9.9\n", encoding="utf-8")
        git(self.root, "add", ".version")
        git(self.root, "commit", "-m", "wrong tree is not the tag")
        # The tagged tree still matches. A wrong SHA cross-check was above.
        self.write_tray(self.root)
        windows_release.prepare(
            self.root,
            "v1.2.3",
            release_meta(
                "v1.2.3",
                [windows_release.EXE_NAME, windows_release.TRAY_NAME],
                targetCommitish=self.commit,
            ),
            exe,
            self.root,
            dest,
        )
        self.assertEqual(dest.read_bytes(), self.signed)
        self.assertEqual(
            (dest.parent / windows_release.TRAY_NAME).read_bytes(),
            b"tray-from-this-release",
        )

    def test_manifest_checksum_and_source_mismatches_fail(self):
        self.patch_key()
        other = commit_version(self.root, "9.9.9", "other")
        git(self.root, "tag", "-d", "v1.2.3")
        git(self.root, "tag", "v1.2.3", other)
        exe = self.write_exe(self.root)
        with self.assertRaisesRegex(VerifyError, r"\.version"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta("v1.2.3", [windows_release.EXE_NAME]),
                exe,
                self.root,
                self.root / "nope.exe",
            )
        git(self.root, "tag", "-d", "v1.2.3")
        git(self.root, "tag", "v1.2.3", self.commit)
        digest = hashlib.sha256(self.signed).hexdigest()
        sums = self.root / "SHA256SUMS"
        sums.write_text(f"{'ab' * 32}  {windows_release.EXE_NAME}\n", encoding="utf-8")
        meta = release_meta(
            "v1.2.3",
            [windows_release.EXE_NAME, "SHA256SUMS"],
        )
        with self.assertRaisesRegex(VerifyError, "SHA256SUMS"):
            windows_release.prepare(self.root, "v1.2.3", meta, exe, self.root, self.root / "nope.exe")
        sums.write_text(f"{digest}  {windows_release.EXE_NAME}\n", encoding="utf-8")
        source = self.root / "SOURCE.txt"
        source.write_text("c" * 40 + "\n", encoding="utf-8")
        meta["assets"].append({"name": "SOURCE.txt"})
        with self.assertRaisesRegex(VerifyError, "SOURCE.txt"):
            windows_release.prepare(self.root, "v1.2.3", meta, exe, self.root, self.root / "nope.exe")
        source.write_text(self.commit + "\n", encoding="utf-8")
        tray = b"tray-from-this-release"
        tray_digest = hashlib.sha256(tray).hexdigest()
        self.write_tray(self.root, tray)
        sums.write_text(
            f"{digest}  {windows_release.EXE_NAME}\n{tray_digest}  {windows_release.TRAY_NAME}\n",
            encoding="utf-8",
        )
        meta["assets"][0]["digest"] = "sha256:" + digest
        meta["assets"].append({"name": windows_release.TRAY_NAME, "digest": "sha256:" + tray_digest})
        dest = self.root / "staged.exe"
        # dest name is not required to be the asset name; the downloaded file is.
        result = windows_release.prepare(self.root, "v1.2.3", meta, exe, self.root, dest)
        self.assertEqual(result["sha"], self.commit)
        self.assertEqual(result["tray_checksum"], "matched")
        self.assertEqual(dest.read_bytes(), self.signed)
        self.assertEqual((dest.parent / windows_release.TRAY_NAME).read_bytes(), tray)

    def test_installer_conflict_reuses_identical_bytes_and_never_uploads_the_exe(self):
        fresh = b"installer-bytes"
        self.assertEqual(windows_release.installer_plan(None, fresh), "upload")
        self.assertEqual(windows_release.installer_plan(fresh, fresh), "reuse")
        with self.assertRaisesRegex(VerifyError, "conflicts"):
            windows_release.installer_plan(b"other", fresh)
        meta = release_meta("v1.2.3", [windows_release.EXE_NAME, windows_release.SETUP_NAME])
        self.assertEqual(
            windows_release.decide_upload("v1.2.3", meta, fresh, fresh), "reuse"
        )
        with self.assertRaisesRegex(VerifyError, "draft or prerelease"):
            windows_release.decide_upload(
                "v1.2.3",
                release_meta("v1.2.3", [windows_release.SETUP_NAME], isPrerelease=True),
                fresh,
                None,
            )
        setup = self.root / windows_release.SETUP_NAME
        setup.write_bytes(fresh)
        command = windows_release.upload_command("v1.2.3", setup)
        self.assertEqual(command[:4], ["gh", "release", "upload", "v1.2.3"])
        self.assertNotIn("--clobber", command)
        self.assertNotIn(windows_release.EXE_NAME, command)
        self.assertTrue(command[-1].endswith(windows_release.SETUP_NAME))
        with self.assertRaisesRegex(VerifyError, "except the installer"):
            windows_release.upload_command("v1.2.3", self.root / windows_release.EXE_NAME)

    def test_stage_downloads_the_signed_executable_and_does_not_upload(self):
        self.patch_key()
        calls = []

        def runner(args, cwd, timeout):
            calls.append(list(args))
            if args[:2] == ["git", "fetch"]:
                return ""
            if args[:3] == ["gh", "release", "view"]:
                return json.dumps(
                    release_meta(
                        "v1.2.3",
                        [windows_release.EXE_NAME, windows_release.TRAY_NAME],
                    )
                )
            if args[:3] == ["gh", "release", "download"]:
                folder = Path(args[args.index("--dir") + 1])
                name = args[args.index("--pattern") + 1]
                if name == windows_release.EXE_NAME:
                    self.write_exe(folder)
                elif name == windows_release.TRAY_NAME:
                    self.write_tray(folder)
                else:
                    raise AssertionError(name)
                return ""
            raise AssertionError(args)

        dest = self.root / "build" / windows_release.EXE_NAME
        result = windows_release.stage(self.root, "v1.2.3", dest, runner=runner)
        self.assertEqual(result["sha"], self.commit)
        self.assertEqual(result["tray_checksum"], "absent")
        self.assertEqual(dest.read_bytes(), self.signed)
        self.assertEqual(
            (dest.parent / windows_release.TRAY_NAME).read_bytes(),
            b"tray-from-this-release",
        )
        flat = [" ".join(call) for call in calls]
        self.assertFalse(any("upload" in call or "--clobber" in call for call in flat))
        self.assertTrue(any(windows_release.EXE_NAME in call for call in flat))

    def test_plan_upload_blocks_conflicts_and_prereleases(self):
        setup = self.root / windows_release.SETUP_NAME
        setup.write_bytes(b"new-installer")
        calls = []

        def view(assets, **extra):
            return json.dumps(release_meta("v1.2.3", assets, **extra))

        def runner(args, cwd, timeout):
            calls.append(list(args))
            if args[:3] == ["gh", "release", "view"]:
                return view([windows_release.EXE_NAME, windows_release.SETUP_NAME])
            if args[:3] == ["gh", "release", "download"]:
                folder = Path(args[args.index("--dir") + 1])
                (folder / windows_release.SETUP_NAME).write_bytes(b"published-installer")
                return ""
            raise AssertionError(args)

        with self.assertRaisesRegex(VerifyError, "conflicts"):
            windows_release.plan_upload(self.root, "v1.2.3", setup, runner=runner)
        self.assertFalse(any(call[2] == "upload" for call in calls if call[:2] == ["gh", "release"]))

        calls.clear()

        def reuse_runner(args, cwd, timeout):
            calls.append(list(args))
            if args[:3] == ["gh", "release", "view"]:
                return view([windows_release.SETUP_NAME])
            if args[:3] == ["gh", "release", "download"]:
                folder = Path(args[args.index("--dir") + 1])
                (folder / windows_release.SETUP_NAME).write_bytes(b"new-installer")
                return ""
            raise AssertionError(args)

        self.assertEqual(
            windows_release.plan_upload(self.root, "v1.2.3", setup, runner=reuse_runner),
            "reuse",
        )
        self.assertFalse(any("upload" in " ".join(call) for call in calls))

        calls.clear()

        def upload_runner(args, cwd, timeout):
            calls.append(list(args))
            if args[:3] == ["gh", "release", "view"]:
                return view([windows_release.EXE_NAME])
            if args[:3] == ["gh", "release", "upload"]:
                return ""
            raise AssertionError(args)

        self.assertEqual(
            windows_release.plan_upload(self.root, "v1.2.3", setup, runner=upload_runner),
            "upload",
        )
        upload = [call for call in calls if "upload" in call]
        self.assertEqual(len(upload), 1)
        self.assertNotIn("--clobber", upload[0])
        self.assertNotIn(windows_release.EXE_NAME, upload[0])

        def prerelease_runner(args, cwd, timeout):
            if args[:3] == ["gh", "release", "view"]:
                return view([windows_release.SETUP_NAME], isPrerelease=True)
            raise AssertionError(args)

        with self.assertRaisesRegex(VerifyError, "draft or prerelease"):
            windows_release.plan_upload(self.root, "v1.2.3", setup, runner=prerelease_runner)

    def test_older_release_tree_is_packaged_by_current_tooling(self):
        self.patch_key()
        later = commit_version(self.root, "9.9.9", "tooling after the release")
        planted = self.root / "scripts" / "windows_release.py"
        planted.parent.mkdir()
        planted.write_text("raise SystemExit('untrusted')\n", encoding="utf-8")
        git(self.root, "add", "scripts/windows_release.py")
        git(self.root, "commit", "-m", "untrusted helper on the later commit")
        self.assertNotEqual(later, self.commit)
        self.assertNotEqual(git(self.root, "rev-parse", "HEAD"), self.commit)
        missing = subprocess.run(
            ["git", "cat-file", "-e", f"{self.commit}:scripts/windows_release.py"],
            cwd=self.root,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(missing.returncode, 0)
        self.assertEqual((self.root / ".version").read_text(encoding="utf-8").strip(), "9.9.9")
        exe = self.write_exe(self.root)
        self.write_tray(self.root)
        dest = self.root / "build" / windows_release.EXE_NAME
        result = windows_release.prepare(
            self.root,
            "v1.2.3",
            release_meta(
                "v1.2.3",
                [windows_release.EXE_NAME, windows_release.TRAY_NAME],
            ),
            exe,
            self.root,
            dest,
        )
        self.assertEqual(result["sha"], self.commit)
        self.assertEqual(result["version"], "1.2.3")
        self.assertNotEqual(result["sha"], git(self.root, "rev-parse", "HEAD"))
        self.assertEqual(dest.read_bytes(), self.signed)
        self.assertEqual(
            (dest.parent / windows_release.TRAY_NAME).read_bytes(),
            b"tray-from-this-release",
        )

    def test_tray_must_come_from_the_release_and_match_published_checksums(self):
        self.patch_key()
        exe = self.write_exe(self.root)
        tray = b"tray-from-this-release"
        downloaded = self.write_tray(self.root, tray)
        dest = self.root / "build" / windows_release.EXE_NAME
        unrelated = dest.parent / windows_release.TRAY_NAME
        unrelated.parent.mkdir()
        unrelated.write_bytes(b"unrelated-tray-from-main")
        with self.assertRaisesRegex(VerifyError, "tray executable is not on the release"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta("v1.2.3", [windows_release.EXE_NAME]),
                exe,
                self.root,
                dest,
            )
        self.assertEqual(unrelated.read_bytes(), b"unrelated-tray-from-main")
        self.assertFalse(dest.exists())

        result = windows_release.prepare(
            self.root,
            "v1.2.3",
            release_meta("v1.2.3", [windows_release.EXE_NAME, windows_release.TRAY_NAME]),
            exe,
            self.root,
            dest,
        )
        self.assertEqual(result["tray_checksum"], "absent")
        self.assertEqual(unrelated.read_bytes(), tray)
        self.assertEqual(dest.read_bytes(), self.signed)

        downloaded.write_bytes(b"")
        with self.assertRaisesRegex(VerifyError, "tray executable is empty"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta("v1.2.3", [windows_release.EXE_NAME, windows_release.TRAY_NAME]),
                exe,
                self.root,
                dest,
            )
        other = self.root / "other-tray"
        other.write_bytes(tray)
        downloaded.unlink()
        downloaded.symlink_to(other)
        with self.assertRaisesRegex(VerifyError, "tray executable was not downloaded"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta("v1.2.3", [windows_release.EXE_NAME, windows_release.TRAY_NAME]),
                exe,
                self.root,
                dest,
            )
        downloaded.unlink()
        downloaded.write_bytes(tray)

        digest = hashlib.sha256(tray).hexdigest()
        downloaded.write_bytes(b"corrupted-tray")
        with self.assertRaisesRegex(VerifyError, "digest does not match the downloaded tray"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta(
                    "v1.2.3",
                    [
                        windows_release.EXE_NAME,
                        {"name": windows_release.TRAY_NAME, "digest": "sha256:" + digest},
                    ],
                ),
                exe,
                self.root,
                dest,
            )
        downloaded.write_bytes(tray)
        with self.assertRaisesRegex(VerifyError, "unsupported asset digest"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta(
                    "v1.2.3",
                    [
                        windows_release.EXE_NAME,
                        {"name": windows_release.TRAY_NAME, "digest": "sha512:" + digest},
                    ],
                ),
                exe,
                self.root,
                dest,
            )

        sums = self.root / "SHA256SUMS"
        sums.write_text(
            f"{hashlib.sha256(self.signed).hexdigest()}  {windows_release.EXE_NAME}\n"
            f"{'ab' * 32}  {windows_release.TRAY_NAME}\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(VerifyError, "SHA256SUMS does not match the downloaded tray"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta(
                    "v1.2.3",
                    [windows_release.EXE_NAME, windows_release.TRAY_NAME, "SHA256SUMS"],
                ),
                exe,
                self.root,
                dest,
            )
        sums.write_text(
            f"{hashlib.sha256(self.signed).hexdigest()}  {windows_release.EXE_NAME}\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(VerifyError, "does not list the tray"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta(
                    "v1.2.3",
                    [windows_release.EXE_NAME, windows_release.TRAY_NAME, "SHA256SUMS"],
                ),
                exe,
                self.root,
                dest,
            )

        source = self.root / "SOURCE.txt"
        source.write_text(("d" * 40) + "\n", encoding="utf-8")
        sums.write_text(
            f"{hashlib.sha256(self.signed).hexdigest()}  {windows_release.EXE_NAME}\n"
            f"{digest}  {windows_release.TRAY_NAME}\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(VerifyError, "SOURCE.txt"):
            windows_release.prepare(
                self.root,
                "v1.2.3",
                release_meta(
                    "v1.2.3",
                    [
                        windows_release.EXE_NAME,
                        windows_release.TRAY_NAME,
                        "SHA256SUMS",
                        "SOURCE.txt",
                    ],
                ),
                exe,
                self.root,
                dest,
            )
        source.write_text(self.commit + "\n", encoding="utf-8")
        # A later checkout must not replace the tray that belongs to the tag.
        commit_version(self.root, "9.9.9", "unrelated tray would be built here")
        result = windows_release.prepare(
            self.root,
            "v1.2.3",
            release_meta(
                "v1.2.3",
                [
                    windows_release.EXE_NAME,
                    {"name": windows_release.TRAY_NAME, "digest": "sha256:" + digest},
                    "SHA256SUMS",
                    "SOURCE.txt",
                ],
            ),
            exe,
            self.root,
            dest,
        )
        self.assertEqual(result["sha"], self.commit)
        self.assertEqual(result["version"], "1.2.3")
        self.assertEqual(result["tray_checksum"], "matched")
        self.assertEqual((dest.parent / windows_release.TRAY_NAME).read_bytes(), tray)
        module = (REPO / "scripts" / "windows_release.py").read_text(encoding="utf-8")
        self.assertNotIn("target/release", module)
        self.assertNotIn("madmail-windows-amd64-ci", module)
        setup = self.root / windows_release.SETUP_NAME
        setup.write_bytes(b"installer")
        command = windows_release.upload_command("v1.2.3", setup)
        self.assertNotIn(windows_release.TRAY_NAME, command)
        self.assertNotIn(windows_release.EXE_NAME, command)

    def test_workflow_packages_only_a_verified_stable_installer(self):
        source = WORKFLOW.read_text(encoding="utf-8")
        document = yaml.safe_load(source)
        setup = document["jobs"]["windows-amd64-setup"]
        smoke = document["jobs"]["windows-amd64-smoke"]
        rendered = yaml.dump(setup)
        self.assertEqual(
            setup["if"],
            "github.event_name == 'release' && github.event.release.prerelease == false",
        )
        self.assertIn("windows_release.py", source)
        self.assertIn("stage --repo $repo", source)
        self.assertIn("plan-upload --repo $repo", source)
        self.assertIn('-Version "${{ steps.signed.outputs.version }}"', source)
        self.assertEqual(source.count("github.event.repository.default_branch"), 2)
        self.assertIn("path: source", source)
        self.assertIn("path: tools", source)
        self.assertIn("tools\\scripts\\windows_release.py", source)
        self.assertIn("tools\\packaging\\windows\\build-setup.ps1", source)
        self.assertIn("--repo $repo", source)
        self.assertNotIn("ref: ${{ github.event.release.tag_name }}", source)
        self.assertNotIn("ref: ${{ github.sha }}", source)
        self.assertNotIn("--clobber", rendered)
        self.assertNotIn("gh release upload", rendered)
        self.assertNotIn("madmail-windows-amd64-ci", rendered)
        self.assertNotIn("download-artifact", rendered)
        self.assertNotIn("cargo build", rendered)
        self.assertNotIn("path: source", yaml.dump(smoke))
        self.assertNotIn("--clobber", WORKFLOW.read_text(encoding="utf-8"))
        self.assertNotIn("--clobber", (REPO / "scripts/windows_release.py").read_text(encoding="utf-8"))
        self.assertEqual(smoke["if"], "github.event_name != 'release'")
        self.assertNotIn("needs", setup)
        self.assertNotIn("windows-amd64-smoke", rendered)
        linux = document["jobs"]["linux-windows-crates"]
        arm64 = document["jobs"]["windows-arm64-compile"]
        self.assertEqual(linux["if"], "github.event_name != 'release'")
        self.assertEqual(arm64["if"], "github.event_name != 'release'")
        self.assertIn("cargo build", yaml.dump(smoke))
        iss = (REPO / "packaging/windows/Madmail.iss").read_text(encoding="utf-8")
        script = (REPO / "packaging/windows/build-setup.ps1").read_text(encoding="utf-8")
        self.assertIn("#ifndef MyAppVersion", iss)
        self.assertNotIn('MyAppVersion "2.17.3"', iss)
        self.assertIn('"/DMyAppVersion=$AppVersion"', script)
        self.assertIn('"/DRequireTray=1"', script)
        self.assertIn("Resolve-AppVersion", script)
        self.assertIn("-RequireTray", source)
        self.assertNotIn("-Testing", source)
        self.assertNotIn("TestingBuild", source)
        self.assertIn('#if RequireTray == "1"', iss)
        self.assertIn('#ifndef TestingBuild', iss)
        self.assertIn('#if TestingBuild == "1"', iss)
        self.assertIn("TESTING ONLY", iss)
        self.assertIn('#define MyAppName "Madmail"', iss)
        self.assertIn("skipifsourcedoesntexist", iss)
        self.assertIn("install-autostart", iss)
        self.assertIn('Name: "firewall"', iss)
        required = iss.split('#if RequireTray == "1"', 1)[1].split("#else", 1)[0]
        optional = iss.split('#if RequireTray == "1"', 1)[1].split("#else", 1)[1].split("#endif", 1)[0]
        self.assertNotIn("skipifsourcedoesntexist", required)
        self.assertIn("skipifsourcedoesntexist", optional)
        self.assertIn("{#MyTrayExeName}", required)


if __name__ == "__main__":
    unittest.main()
