"""Stdlib unit and real Git/Cargo integration tests; never use live remotes."""

import contextlib
import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "madmail_release", Path(__file__).with_name("release.py")
)
release = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = release
SPEC.loader.exec_module(release)


class ConventionalCommitTests(unittest.TestCase):
    def level(self, message):
        changes = release.changes_from_commits([release.Commit("a" * 40, message)])
        return max((c.level for c in changes), default=0)

    def test_default_rules(self):
        for message, level in [
            ("fix: correct", 1),
            ("perf(db): faster", 1),
            ("feat(cli): new option", 2),
            ("docs: guide", 0),
            ("chore: deps", 0),
            ("refactor: cleanup", 0),
            ("test: coverage", 0),
            ("not conventional", 0),
            ("new API\n\nBREAKING CHANGE: old API removed", 3),
            ("feat!: break", 3),
            ("fix(api)!: break", 3),
            ("docs: guide\n\nBREAKING CHANGE: new format", 3),
            ("refactor: clean\n\nBREAKING-CHANGE: new format", 3),
        ]:
            with self.subTest(message=message):
                self.assertEqual(self.level(message), level)

    def test_reverts_cancel_original(self):
        original = release.Commit("a" * 40, "feat: feature")
        reverted = release.Commit(
            "b" * 40,
            'Revert "feat: feature"\n\nThis reverts commit ' + original.sha + ".",
        )
        self.assertEqual(release.changes_from_commits([original, reverted]), [])
        self.assertEqual(release.changes_from_commits([reverted])[0].level, 1)

    def test_bump_and_validation(self):
        self.assertEqual(release.bump("2.31.3", 1), "2.31.4")
        self.assertEqual(release.bump("2.31.3", 2), "2.32.0")
        self.assertEqual(release.bump("2.31.3", 3), "3.0.0")
        for bad in [
            "2.01.0",
            "v2.0.0",
            "2.0.0; touch owned",
            "2.0.0\nX=Y",
            "-1.0.0",
            "2.1\u0661.0",
        ]:
            with self.assertRaises(release.ReleaseError):
                release.version_tuple(bad)

    def test_notes_escape_untrusted_content_and_keep_breaking(self):
        changes = release.changes_from_commits(
            [
                release.Commit(
                    "a" * 40,
                    "feat(cli): <script>alert(1)</script>\n\nBREAKING CHANGE: old API removed",
                )
            ]
        )
        notes = release.release_notes("3.0.0", "v2.0.0", changes, "2026-10-07")
        self.assertIn("### Features", notes)
        self.assertIn("### BREAKING CHANGES", notes)
        self.assertIn("old API removed", notes)
        self.assertNotIn("<script>", notes)
        self.assertIn("/compare/v2.0.0...v3.0.0", notes)
        with self.assertRaises(release.ReleaseError):
            release.prepend_changelog(notes, "3.0.0", notes)

    def test_exact_toml_section(self):
        text = (
            '[package]\nversion = "1.0.0"\n\n[workspace.package]\nversion = "2.0.0"\n'
        )
        updated = release.replace_toml_version(text, "workspace.package", "2.1.0")
        self.assertIn('[package]\nversion = "1.0.0"', updated)
        self.assertIn('[workspace.package]\nversion = "2.1.0"', updated)

    def test_identity_rejects_injection(self):
        for name, email in [
            ("CI\nX", "ci@madmail.chat"),
            ("CI", "ci@madmail.chat\nX"),
            ("CI", "ci\x00@madmail.chat"),
            ("CI", "ci\x7f@madmail.chat"),
            ("CI", "invalid"),
            ("<CI>", "ci@madmail.chat"),
        ]:
            with self.assertRaises(release.ReleaseError):
                release.identity_env(name, email)

    def test_multiline_breaking_notes_and_issue_references(self):
        changes = release.changes_from_commits(
            [
                release.Commit(
                    "a" * 40,
                    "refactor(api): new API\n\nBREAKING CHANGE: first line\n"
                    "second line\n\nCloses #180",
                )
            ]
        )
        self.assertEqual(changes[0].breaking, "first line second line")
        notes = release.release_notes("2.0.0", "v1.2.3", changes, "2026-10-07")
        self.assertIn("first line second line", notes)
        self.assertNotIn("Closes", changes[0].breaking)

    def test_lock_replacement_preserves_registry_dependency(self):
        source = 'version = 3\n\n[[package]]\nname = "chatmail-test"\nversion = "1.2.3"\n\n[[package]]\nname = "chatmail-test"\nversion = "9.0.0"\nsource = "registry+https://example.invalid/index"\n'
        data = {
            ".version": "1.2.3\n",
            "Cargo.toml": '[workspace.package]\nversion = "1.2.3"\n',
            "Cargo.lock": source,
            "package.json": '{"version":"1.2.3"}',
            "package-lock.json": '{"version":"1.2.3","packages":{"":{"version":"1.2.3"}}}',
            "CHANGELOG.md": "# Changelog\n",
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "crates/app/Cargo.toml"
            path.parent.mkdir(parents=True)
            path.write_text(
                '[package]\nname = "chatmail-test"\nversion.workspace = true\n'
            )
            with mock.patch.object(release, "read_file", return_value=path.read_text()):
                updates = release.planned_files(
                    root,
                    data,
                    [{"name": "chatmail-test", "manifest_path": str(path)}],
                    "1.2.4",
                    "notes\n",
                )
        lock = release.tomllib.loads(updates["Cargo.lock"])["package"]
        self.assertEqual([entry["version"] for entry in lock], ["1.2.4", "9.0.0"])


@unittest.skipUnless(
    shutil.which("git") and shutil.which("cargo"), "Git and Cargo required"
)
class GitIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(
            prefix="madmail-release-tests-", dir=Path.home()
        )
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "repo"
        self.root.mkdir()
        # Keep global signing settings, user hooks and test credentials out.
        self.environment = mock.patch.dict(
            os.environ,
            {
                "GIT_CONFIG_GLOBAL": os.devnull,
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_TERMINAL_PROMPT": "0",
            },
        )
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        self.write(
            "Cargo.toml",
            '[workspace]\nmembers = ["crates/app", "crates/odd"]\nresolver = "2"\n\n[workspace.package]\nversion = "1.2.3"\nedition = "2021"\n',
        )
        self.write(
            "crates/app/Cargo.toml",
            '[package]\nname = "chatmail-test"\nversion.workspace = true\nedition.workspace = true\n',
        )
        self.write("crates/app/src/lib.rs", "pub fn answer() -> u32 { 42 }\n")
        # Explicit-version member with a non-chatmail name must also change.
        self.write(
            "crates/odd/Cargo.toml",
            '[package]\nname = "unusual-workspace-member"\nversion = "1.2.3"\nedition = "2021"\n',
        )
        self.write("crates/odd/src/lib.rs", "pub fn answer() -> u32 { 42 }\n")
        self.write(".version", "1.2.3\n")
        self.write(
            "package.json", json.dumps({"name": "test", "version": "1.2.3"}) + "\n"
        )
        self.write(
            "package-lock.json",
            json.dumps(
                {
                    "name": "test",
                    "version": "1.2.3",
                    "lockfileVersion": 3,
                    "packages": {"": {"name": "test", "version": "1.2.3"}},
                }
            )
            + "\n",
        )
        self.write("CHANGELOG.md", "# Changelog\n\n# 1.2.3\n\nOld release\n")
        self.command("cargo", "generate-lockfile", "--offline")
        self.git("add", ".")
        self.git("commit", "-m", "chore: initial")
        self.git("tag", "v1.2.3")

    def command(self, *args, check=True):
        result = subprocess.run(
            args, cwd=self.root, text=True, capture_output=True, check=False
        )
        if check and result.returncode:
            self.fail(f"{args}: {result.stderr}")
        return result.stdout.strip()

    def git(self, *args):
        return self.command("git", *args)

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self, message):
        self.write("change.txt", message)
        self.git("add", "change.txt")
        self.git("commit", "-m", message)

    def snapshot(self):
        return {
            p.relative_to(self.root).as_posix(): p.read_bytes()
            for p in self.root.rglob("*")
            if p.is_file() and ".git" not in p.parts
        }

    def test_no_release_and_dry_run_do_not_mutate(self):
        self.commit("docs: update guide")
        before, head, tags = (
            self.snapshot(),
            self.git("rev-parse", "HEAD"),
            self.git("tag"),
        )
        result = release.release(self.root)
        self.assertFalse(result["released"])
        self.commit("feat: new feature")
        before, head = self.snapshot(), self.git("rev-parse", "HEAD")
        result = release.release(self.root)
        self.assertEqual(result["version"], "1.3.0")
        self.assertTrue(result["dry_run"])
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertEqual(self.git("tag"), tags)

    def test_patch_minor_major_sync_and_identity(self):
        for message, expected in [
            ("fix: patch", "1.2.4"),
            ("feat: feature", "1.3.0"),
            ("feat!: new API", "2.0.0"),
        ]:
            with self.subTest(message=message):
                self.commit(message)
                result = release.release(self.root, write=True)
                self.assertEqual(result["version"], expected)
                version, _ = release.verify_versions(
                    self.root, release.cargo_metadata(self.root)
                )
                self.assertEqual(version, expected)
                self.assertEqual(
                    self.git("log", "-1", "--format=%an <%ae>|%cn <%ce>"),
                    "Madmail-CI <ci@madmail.chat>|Madmail-CI <ci@madmail.chat>",
                )
                self.assertEqual(
                    self.git("rev-parse", f"v{expected}-unstable^{{commit}}"),
                    result["sha"],
                )
                self.assertIn("[skip ci]", self.git("log", "-1", "--format=%s"))
                self.assertEqual(self.git("status", "--porcelain"), "")

    def test_rerun_is_idempotent(self):
        self.commit("fix: patch")
        first = release.release(self.root, write=True)
        before = self.snapshot()
        second = release.release(self.root, write=True)
        self.assertFalse(second["released"])
        self.assertEqual(first["sha"], second["sha"])
        self.assertEqual(self.snapshot(), before)
        self.assertEqual((self.root / "CHANGELOG.md").read_text().count("# [1.2.4]"), 1)

    def test_first_release_ignores_prerelease_tags(self):
        self.git("tag", "-d", "v1.2.3")
        self.git("tag", "v9.0.0-beta.1")
        self.commit("feat: initial feature")
        # Sem-release starts at 1.0.0; prevent silently downgrading existing manifests.
        with self.assertRaisesRegex(release.ReleaseError, "downgrade"):
            release.release(self.root, write=True)
        self.assertEqual(release.latest_release(self.root), None)

    def test_existing_unreachable_tag_is_not_overwritten(self):
        self.commit("fix: competing release")
        competing = self.git("rev-parse", "HEAD")
        self.git("tag", "v1.2.4-unstable")
        self.git("reset", "--hard", "HEAD~1")
        self.commit("fix: intended release")
        before = self.snapshot()
        with self.assertRaisesRegex(release.ReleaseError, "already exists"):
            release.release(self.root, write=True)
        self.assertEqual(self.git("rev-parse", "v1.2.4-unstable"), competing)
        self.assertEqual(self.snapshot(), before)

    def test_dirty_branch_and_symlink_refused(self):
        self.commit("fix: patch")
        self.write("untracked", "do not overwrite")
        with self.assertRaisesRegex(release.ReleaseError, "clean"):
            release.release(self.root, write=True)
        (self.root / "untracked").unlink()
        self.git("checkout", "-b", "feature")
        with self.assertRaisesRegex(release.ReleaseError, "only on main"):
            release.release(self.root, write=True)
        self.git("checkout", "main")
        self.git("rm", ".version")
        (self.root / ".version").symlink_to(self.root / "CHANGELOG.md")
        self.git("add", ".version")
        self.git("commit", "-m", "chore: unsafe symlink")
        with self.assertRaisesRegex(release.ReleaseError, "Unsafe"):
            release.release(self.root, write=True)

    def test_failed_write_cargo_commit_and_tag_restore_state(self):
        self.commit("fix: patch")
        before, head, tags = (
            self.snapshot(),
            self.git("rev-parse", "HEAD"),
            self.git("tag"),
        )
        original_write = release.atomic_write
        failed = False

        def fail_once(path, text):
            nonlocal failed
            if path.name == "Cargo.lock" and not failed:
                failed = True
                raise OSError("disk error")
            original_write(path, text)

        with (
            mock.patch.object(release, "atomic_write", side_effect=fail_once),
            self.assertRaises(OSError),
        ):
            release.release(self.root, write=True)
        original_git = release.git
        for operation in ("commit-tree", "tag"):

            def fail_git(root, *args, operation=operation, **kwargs):
                # Only fail actual mutation, not tag discovery.
                if operation in args and (operation == "commit-tree" or "-a" in args):
                    raise release.ReleaseError("injected failure")
                return original_git(root, *args, **kwargs)

            with (
                mock.patch.object(release, "git", side_effect=fail_git),
                self.assertRaises(release.ReleaseError),
            ):
                release.release(self.root, write=True)
            self.assertEqual(self.git("rev-parse", "HEAD"), head)
            self.assertEqual(self.snapshot(), before)
            self.assertEqual(self.git("tag"), tags)
            self.assertEqual(self.git("status", "--porcelain"), "")
        original_metadata = release.cargo_metadata

        def fail_new_version(root):
            if (root / ".version").read_text().strip() == "1.2.4":
                raise release.ReleaseError("cargo validation failed")
            return original_metadata(root)

        with (
            mock.patch.object(release, "cargo_metadata", side_effect=fail_new_version),
            self.assertRaises(release.ReleaseError),
        ):
            release.release(self.root, write=True)
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)

    def test_concurrent_local_release_refused(self):
        self.commit("fix: patch")
        with (
            release.release_lock(self.root),
            self.assertRaisesRegex(release.ReleaseError, "Another release"),
        ):
            release.release(self.root, write=True)

    def bare_remote(self):
        remote = Path(self.temp.name) / "remote.git"
        self.command("git", "init", "--bare", str(remote))
        self.git("remote", "add", "origin", str(remote))
        self.git("push", "origin", "main", "--tags")
        return remote

    def test_atomic_push_and_retry_after_failure(self):
        remote = self.bare_remote()
        self.commit("fix: patch")
        hook = remote / "hooks/pre-receive"
        hook.write_text("#!/bin/sh\nexit 1\n")
        hook.chmod(0o755)
        old_head = self.command("git", "--git-dir", str(remote), "rev-parse", "main")
        with self.assertRaises(release.ReleaseError):
            release.release(self.root, write=True, push=True)
        prepared = self.git("rev-parse", "HEAD")
        self.assertEqual(
            self.command("git", "--git-dir", str(remote), "rev-parse", "main"), old_head
        )
        self.assertNotIn(
            "v1.2.4-unstable", self.command("git", "--git-dir", str(remote), "tag")
        )
        hook.unlink()
        result = release.release(self.root, write=True, push=True)
        self.assertEqual(result["sha"], prepared)
        self.assertFalse(result["released"])
        self.assertEqual(
            self.command("git", "--git-dir", str(remote), "rev-parse", "main"), prepared
        )
        self.assertIn(
            "v1.2.4-unstable", self.command("git", "--git-dir", str(remote), "tag")
        )

    def test_concurrent_remote_release_rejects_branch_and_tag(self):
        remote = self.bare_remote()
        competitor = Path(self.temp.name) / "competitor"
        self.command("git", "clone", "--branch", "main", str(remote), str(competitor))
        subprocess.run(
            ["git", "-C", str(competitor), "config", "user.name", "Other"], check=True
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(competitor),
                "config",
                "user.email",
                "other@example.invalid",
            ],
            check=True,
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(competitor),
                "commit",
                "--allow-empty",
                "-m",
                "fix: competing",
            ],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["git", "-C", str(competitor), "push", "origin", "main"],
            check=True,
            capture_output=True,
        )
        self.commit("feat: our release")
        with self.assertRaises(release.ReleaseError):
            release.release(self.root, write=True, push=True)
        self.assertNotIn(
            "v1.3.0-unstable", self.command("git", "--git-dir", str(remote), "tag")
        )

    def test_no_hooks_shell_or_environment_injection(self):
        hook = self.root / ".git/hooks/pre-commit"
        hook.write_text("#!/bin/sh\ntouch owned\nexit 1\n")
        hook.chmod(0o755)
        # Commit before installing the hook cannot be blocked by it.
        self.git(
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--allow-empty",
            "-m",
            "fix: $(touch owned); new fix",
        )
        with mock.patch.dict(
            os.environ, {"GIT_INDEX_FILE": str(self.root / "hijack-index")}
        ):
            release.release(self.root, write=True)
        self.assertFalse((self.root / "owned").exists())
        self.assertFalse((self.root / "hijack-index").exists())

    def test_cli_default_and_outputs(self):
        self.commit("fix: patch")
        head = self.git("rev-parse", "HEAD")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(release.main(["--repo", str(self.root)]), 0)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        output = Path(self.temp.name) / "job-output"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(
                release.main(
                    [
                        "--repo",
                        str(self.root),
                        "--write",
                        "--github-output",
                        str(output),
                    ]
                ),
                0,
            )
        self.assertIn("version=1.2.4\n", output.read_text())
        self.assertIn("released=true\n", output.read_text())

    def test_missing_signing_key_rolls_back(self):
        self.commit("fix: patch")
        self.git("config", "gpg.program", "/bin/false")
        before, head = self.snapshot(), self.git("rev-parse", "HEAD")
        with self.assertRaises(release.ReleaseError):
            release.release(self.root, write=True, sign=True)
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertNotIn("v1.2.4-unstable", self.git("tag"))

    @unittest.skipUnless(shutil.which("ssh-keygen"), "SSH signer required")
    def test_signed_commit_and_tag(self):
        key = Path(self.temp.name) / "signing-key"
        self.command("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key))
        allowed = Path(self.temp.name) / "allowed-signers"
        allowed.write_text("ci@madmail.chat " + key.with_suffix(".pub").read_text())
        self.git("config", "gpg.format", "ssh")
        self.git("config", "user.signingkey", str(key))
        self.git("config", "gpg.ssh.allowedSignersFile", str(allowed))
        self.commit("fix: signed patch")
        release.release(self.root, write=True, sign=True)
        self.git("verify-commit", "HEAD")
        self.git("verify-tag", "v1.2.4-unstable")

    def test_shallow_check_allowed_but_analysis_rejected(self):
        shallow = Path(self.temp.name) / "shallow"
        self.command("git", "clone", "--depth=1", self.root.as_uri(), str(shallow))
        self.assertTrue(release.release(shallow, check=True)["check"])
        with self.assertRaisesRegex(release.ReleaseError, "full history"):
            release.release(shallow)

    def test_first_release_and_highest_bump(self):
        self.git("tag", "-d", "v1.2.3")
        self.write(".version", "0.1.0\n")
        self.write(
            "Cargo.toml",
            (self.root / "Cargo.toml").read_text().replace("1.2.3", "0.1.0"),
        )
        self.write(
            "crates/odd/Cargo.toml",
            (self.root / "crates/odd/Cargo.toml").read_text().replace("1.2.3", "0.1.0"),
        )
        for filename in ["package.json", "package-lock.json"]:
            self.write(
                filename, (self.root / filename).read_text().replace("1.2.3", "0.1.0")
            )
        self.command("cargo", "generate-lockfile", "--offline")
        self.git("add", ".")
        self.git("commit", "-m", "feat: first release")
        self.assertEqual(release.release(self.root, write=True)["version"], "1.0.0")
        self.commit("fix: small change")
        self.commit("feat: another feature")
        self.commit("refactor!: incompatible")
        self.assertEqual(release.release(self.root)["version"], "2.0.0")

    def test_cli_rejects_push_without_write(self):
        with contextlib.redirect_stderr(io.StringIO()) as errors:
            self.assertEqual(release.main(["--repo", str(self.root), "--push"]), 1)
        self.assertIn("--push requires --write", errors.getvalue())

    def test_subprocess_errors_redact_stderr(self):
        result = subprocess.CompletedProcess(
            ["git", "push"], 1, "", "https://user:secret@example.invalid/repo"
        )
        with (
            mock.patch.object(release.subprocess, "run", return_value=result),
            self.assertRaises(release.ReleaseError) as error,
        ):
            release.git(self.root, "push", "origin")
        self.assertNotIn("secret", str(error.exception))


if __name__ == "__main__":
    unittest.main()
