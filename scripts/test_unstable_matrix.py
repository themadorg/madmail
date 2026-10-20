"""Build-only matrix gates. No network, tags are local, and nothing is published."""

import contextlib
import io
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import unstable_assets as assets
import unstable_matrix as matrix
import unstable_release
from test_unstable_assets import (
    RELAYS,
    VERSION,
    _git,
    _git_commit,
    binary_for,
    pe,
)


def seed_checkout():
    root = Path(tempfile.mkdtemp(prefix="madmail-matrix-"))
    admin = root / "admin-origin"
    _git_commit(admin, {"README": "ui\n"}, message="admin")
    admin_sha = _git(admin, "rev-parse", "HEAD")
    checkout = root / "repo"
    checkout.mkdir()
    _git(checkout, "init", "-b", "main")
    _git(checkout, "config", "user.email", "test@example.invalid")
    _git(checkout, "config", "user.name", "Test")
    (checkout / "external").mkdir()
    cloned = subprocess.run(
        ["git", "clone", "--quiet", str(admin), str(checkout / "external/madmail-admin-web")],
        check=False,
        capture_output=True,
        text=True,
    )
    if cloned.returncode:
        raise AssertionError(cloned.stderr or cloned.stdout)
    (checkout / ".version").write_text(VERSION + "\n", encoding="utf-8")
    (checkout / "Cargo.toml").write_text(
        '[workspace.package]\nversion = "1.2.4"\n', encoding="utf-8"
    )
    (checkout / "Cargo.lock").write_text("# lock\n", encoding="utf-8")
    _git(checkout, "add", ".")
    _git(checkout, "commit", "-m", "seed")
    sha = _git(checkout, "rev-parse", "HEAD")
    return checkout, sha, admin_sha


def write_matching(directory, generation):
    directory.mkdir(parents=True)
    for server in assets.SERVERS:
        (directory / server.name).write_bytes(binary_for(server, generation=generation))
    (directory / assets.TRAY_NAME).write_bytes(
        pe(assets.PE_AMD64, assets.version_marker(VERSION).encode() + b"tray-only")
    )
    (directory / assets.SETUP_NAME).write_bytes(
        pe(
            assets.INSTALLER_MACHINE,
            f"{assets.TESTING_MARK}\nversion:{VERSION}\n".encode() + b"setup",
        )
    )


def role_payload(results):
    return json.dumps({role: {"result": result} for role, result in results.items()})


class MatrixGateTests(unittest.TestCase):
    def test_required_roles_match_the_sixteen_asset_builders(self):
        self.assertEqual(
            matrix.REQUIRED_ROLES,
            (
                "unstable-admin-web",
                "unstable-linux-amd64",
                "unstable-linux-amd64-legacy",
                "unstable-linux-amd64-musl",
                "unstable-linux-arm64",
                "unstable-linux-arm64-musl",
                "unstable-windows",
            ),
        )

    def test_missing_or_failed_roles_block_validation(self):
        good = {role: "success" for role in matrix.REQUIRED_ROLES}
        matrix.require_role_results(role_payload(good))
        for bad in ("failure", "skipped", "cancelled"):
            broken = dict(good)
            broken["unstable-linux-arm64-musl"] = bad
            with self.assertRaisesRegex(matrix.MatrixError, "unstable-linux-arm64-musl"):
                matrix.require_role_results(role_payload(broken))
        partial = dict(good)
        del partial["unstable-windows"]
        with self.assertRaisesRegex(matrix.MatrixError, "unstable-windows"):
            matrix.require_role_results(role_payload(partial))
        with self.assertRaisesRegex(matrix.MatrixError, "unreadable"):
            matrix.require_role_results("{")
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("MATRIX_NEEDS", None)
            self.assertEqual(matrix.main(["require-roles"]), 1)

    def test_publication_cannot_run_for_a_pull_request_or_a_failed_matrix(self):
        ok = {name: "success" for name in matrix.PUBLICATION_RESULTS}
        matrix.assert_publication("push", "refs/heads/main", "v1.2.4-unstable", ok)
        for event in ("pull_request", "pull_request_target"):
            with self.assertRaisesRegex(matrix.MatrixError, "pull request"):
                matrix.assert_publication(event, "refs/heads/main", "v1.2.4-unstable", ok)
        with self.assertRaisesRegex(matrix.MatrixError, "main push"):
            matrix.assert_publication("push", "refs/heads/feature", "v1.2.4-unstable", ok)
        with self.assertRaisesRegex(matrix.MatrixError, "unstable tag"):
            matrix.assert_publication("push", "refs/heads/main", "", ok)
        with self.assertRaisesRegex(matrix.MatrixError, "unstable tag"):
            matrix.assert_publication("push", "refs/heads/main", "v1.2.4", ok)
        failed = dict(ok)
        failed["matrix"] = "failure"
        with self.assertRaisesRegex(matrix.MatrixError, "matrix"):
            matrix.assert_publication("push", "refs/heads/main", "v1.2.4-unstable", failed)
        self.assertEqual(
            matrix.main(
                [
                    "assert-publish",
                    "--event",
                    "pull_request",
                    "--ref",
                    "refs/heads/main",
                    "--tag",
                    "v1.2.4-unstable",
                    "--release",
                    "success",
                    "--build",
                    "success",
                    "--test",
                    "success",
                    "--matrix",
                    "success",
                ]
            ),
            1,
        )

    def test_build_only_reads_the_commit_and_does_not_use_a_tag(self):
        checkout, sha, admin_sha = seed_checkout()
        self.addCleanup(lambda: subprocess.run(["rm", "-rf", str(checkout.parent)], check=False))
        _git(checkout, "tag", "v9.9.9-unstable")
        with (
            mock.patch.object(matrix, "_fetch_tag", side_effect=AssertionError("fetch")),
            mock.patch.object(
                matrix.unstable_release, "validate", side_effect=AssertionError("validate")
            ),
            mock.patch.object(
                matrix.unstable_release,
                "assert_local_tag",
                side_effect=AssertionError("tag"),
            ),
        ):
            version, generation = matrix.prepare_source(
                checkout, "build-only", sha, fetch_tag=False
            )
        self.assertEqual((version, generation), (VERSION, admin_sha))
        with self.assertRaisesRegex(matrix.MatrixError, "does not accept"):
            matrix.prepare_source(
                checkout, "build-only", sha, tag="v1.2.4-unstable", fetch_tag=False
            )
        with self.assertRaisesRegex(matrix.MatrixError, "does not accept"):
            matrix.prepare_source(checkout, "build-only", sha, version=VERSION, fetch_tag=False)
        with self.assertRaisesRegex(matrix.MatrixError, "does not fetch"):
            matrix.prepare_source(checkout, "build-only", sha, fetch_tag=True)
        output = checkout.parent / "output.txt"
        with mock.patch.object(matrix, "_fetch_tag", side_effect=AssertionError("fetch")):
            code = matrix.main(
                [
                    "prepare",
                    "--mode",
                    "build-only",
                    "--sha",
                    sha,
                    "--root",
                    str(checkout),
                    "--github-output",
                    str(output),
                ]
            )
        self.assertEqual(code, 0)
        self.assertIn(f"version={VERSION}", output.read_text(encoding="utf-8"))
        self.assertIn(f"generation={admin_sha}", output.read_text(encoding="utf-8"))
        (checkout / "Cargo.lock").write_text("# changed\n", encoding="utf-8")
        with self.assertRaisesRegex(assets.AssetError, "Cargo.lock"):
            assets.check_build_only(checkout, sha)

    def test_publication_still_requires_the_authoritative_tag(self):
        checkout, sha, admin_sha = seed_checkout()
        self.addCleanup(lambda: subprocess.run(["rm", "-rf", str(checkout.parent)], check=False))
        _git(checkout, "tag", "v1.2.4-unstable", sha)
        with mock.patch.object(matrix.unstable_release, "validate", wraps=unstable_release.validate) as validate:
            version, generation = matrix.prepare_source(
                checkout,
                "publish",
                sha,
                version=VERSION,
                tag="v1.2.4-unstable",
                fetch_tag=False,
            )
        validate.assert_called_once_with("v1.2.4-unstable", sha, VERSION)
        self.assertEqual((version, generation), (VERSION, admin_sha))
        with mock.patch.object(matrix, "_fetch_tag") as fetch:
            code = matrix.main(
                [
                    "prepare",
                    "--mode",
                    "publish",
                    "--sha",
                    sha,
                    "--root",
                    str(checkout),
                    "--version",
                    VERSION,
                    "--tag",
                    "v1.2.4-unstable",
                ]
            )
        self.assertEqual(code, 0)
        fetch.assert_called_once()
        self.assertEqual(fetch.call_args.args[0], "v1.2.4-unstable")
        self.assertEqual(Path(fetch.call_args.args[1]), checkout)
        _git(checkout, "commit", "--allow-empty", "-m", "later")
        later = _git(checkout, "rev-parse", "HEAD")
        with self.assertRaisesRegex(unstable_release.UnstableError, "tag does not point"):
            matrix.prepare_source(
                checkout,
                "publish",
                later,
                version=VERSION,
                tag="v1.2.4-unstable",
                fetch_tag=False,
            )
        _git(checkout, "checkout", "--detach", sha)
        with self.assertRaisesRegex(unstable_release.UnstableError, "invalid unstable tag"):
            matrix.prepare_source(
                checkout,
                "publish",
                sha,
                version=VERSION,
                tag="v1.2.4",
                fetch_tag=False,
            )

    def test_build_only_stage_uses_the_publisher_packer_without_publishing(self):
        checkout, sha, admin_sha = seed_checkout()
        self.addCleanup(lambda: subprocess.run(["rm", "-rf", str(checkout.parent)], check=False))
        binaries = checkout.parent / "binaries"
        write_matching(binaries, admin_sha)
        dest = checkout.parent / "staged"
        with (
            mock.patch.object(
                matrix.unstable_release,
                "publish_testing_release",
                side_effect=AssertionError("publish"),
            ),
            mock.patch.object(matrix.unstable_release, "CommandGh", side_effect=AssertionError("gh")),
            mock.patch.object(assets, "fetch_relays", side_effect=AssertionError("network")),
        ):
            staged = matrix.stage_build_only(
                checkout, sha, binaries, dest, relays=RELAYS
            )
        self.assertEqual(staged["version"], VERSION)
        self.assertEqual(staged["generation"], admin_sha)
        names = sorted(path.name for path in staged["files"])
        self.assertEqual(names, sorted(assets.UPLOADED_NAMES))
        self.assertEqual((dest / assets.SOURCE_NAME).read_text(encoding="utf-8"), sha + "\n")
        sums = (dest / assets.SUMS_NAME).read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(sums), 15)
        self.assertNotIn(assets.SUMS_NAME, "\n".join(sums))
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            matrix.main(["stage-build-only", "--write"])


if __name__ == "__main__":
    unittest.main()
