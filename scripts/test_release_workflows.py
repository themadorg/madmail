"""Publication order for the unstable channel. No registry or GitHub calls."""

import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml

import release

REPO = Path(__file__).resolve().parents[1]
CI = REPO / ".github/workflows/ci.yml"
MATRIX = REPO / ".github/workflows/unstable-matrix.yml"


def git(root, *args):
    result = subprocess.run(
        ["git", *args], cwd=root, text=True, capture_output=True, check=False
    )
    if result.returncode:
        raise AssertionError(result.stderr or result.stdout)
    return result.stdout.strip()


class UnstablePublicationTests(unittest.TestCase):
    def test_docker_waits_for_the_prerelease_and_does_not_move_latest(self):
        document = yaml.safe_load(CI.read_text(encoding="utf-8"))
        docker = document["jobs"]["docker-build-push"]
        prerelease = document["jobs"]["unstable-prerelease"]
        self.assertEqual(
            docker["needs"], ["release", "build", "test", "unstable-prerelease"]
        )
        self.assertIn("needs.unstable-prerelease.result == 'success'", docker["if"])
        self.assertIn("needs.release.outputs.tag != ''", docker["if"])
        self.assertIn("needs.build.result == 'success'", docker["if"])
        self.assertIn("needs.test.result == 'success'", docker["if"])
        required = [
            "unstable-admin-web",
            "unstable-linux-amd64",
            "unstable-linux-amd64-legacy",
            "unstable-linux-amd64-musl",
            "unstable-linux-arm64",
            "unstable-linux-arm64-musl",
            "unstable-windows",
        ]
        self.assertEqual(prerelease["needs"], ["release", "build", "test", "unstable-matrix"])
        self.assertIn("needs.unstable-matrix.result == 'success'", prerelease["if"])
        self.assertIn("needs.release.outputs.tag != ''", prerelease["if"])
        self.assertNotIn("pull_request", prerelease["if"])
        self.assertNotIn("unstable-matrix-pr", prerelease["needs"])
        self.assertNotIn("unstable-matrix-pr", docker["needs"])
        for name in ("unstable-prerelease", "docker-build-push"):
            self.assertNotIn("pull_request", document["jobs"][name]["if"])
        rendered = yaml.dump(prerelease)
        self.assertIn("assert-publish", rendered)
        self.assertIn("--generation", rendered)
        self.assertIn("unstable-bin-*", rendered)
        self.assertIn("unstable_release.py --write", rendered)
        self.assertNotIn("madmail-linux-x86_64-", rendered)
        self.assertNotIn("windows_release.py", rendered)
        self.assertNotIn("verify_signed_binary.py", rendered)
        self.assertNotIn("installer-launcher", document["jobs"])

        matrix_text = MATRIX.read_text(encoding="utf-8")
        ci_text = CI.read_text(encoding="utf-8")
        matrix = yaml.safe_load(matrix_text)
        self.assertIn("workflow_call:", matrix_text)
        self.assertNotIn("pull_request", matrix_text)
        self.assertNotIn("pull_request_target", matrix_text)
        self.assertNotIn("pull_request_target", ci_text)
        self.assertEqual(set(matrix["jobs"]), set(required) | {"unstable-validate"})
        for name in required:
            self.assertNotIn("if", matrix["jobs"][name])
            steps = matrix["jobs"][name]["steps"]
            self.assertEqual(steps[0]["uses"], "actions/checkout@v4")
            self.assertEqual(steps[0]["with"]["ref"], "${{ inputs.sha }}")
            self.assertFalse(steps[0]["with"]["persist-credentials"])
            self.assertEqual(steps[1]["uses"], "./.github/actions/unstable-prepare")
        validate = matrix["jobs"]["unstable-validate"]
        self.assertEqual(validate["needs"], required)
        self.assertEqual(validate["if"], "always() && inputs.mode == 'build-only'")
        validate_text = yaml.dump(validate)
        self.assertIn("require-roles", validate_text)
        self.assertIn("toJSON(needs)", validate_text)
        self.assertIn("stage-build-only", validate_text)
        self.assertNotIn("unstable_release.py", validate_text)
        self.assertNotIn("--write", validate_text)
        self.assertEqual(matrix_text.count("./.github/actions/unstable-prepare"), 8)
        self.assertNotIn("contents: write", matrix_text)
        self.assertNotIn("packages: write", matrix_text)
        for forbidden in (
            "gh release",
            "docker/login-action",
            "docker/build-push-action",
            "git push",
            "release.py",
            "secrets.",
            "0x4D,0x5A",
        ):
            self.assertNotIn(forbidden, matrix_text)
        recipes = (
            "cargo rustc -p chatmail --release --locked --bin madmail -- -C target-feature=+crt-static",
            "cargo zigbuild --release --locked --target x86_64-unknown-linux-musl --bin madmail",
            "cargo zigbuild --release --locked --target aarch64-unknown-linux-musl --bin madmail",
            "cargo build --release --locked --target aarch64-unknown-linux-gnu --bin madmail",
            "cargo build --release --locked --bin madmail --bin madmail-tray",
        )
        for recipe in recipes:
            self.assertEqual(matrix_text.count(recipe), 1, recipe)
            self.assertNotIn(recipe, ci_text)
        self.assertNotIn("verify-server", ci_text)
        self.assertGreaterEqual(matrix_text.count("verify-server"), 6)
        windows = matrix["jobs"]["unstable-windows"]
        names = [step.get("name") for step in windows["steps"]]
        self.assertLess(
            names.index("Build unsigned Windows server, tray, and testing installer"),
            names.index("Refuse testing install when stable markers exist"),
        )
        refuse = next(
            step
            for step in windows["steps"]
            if step.get("name") == "Refuse testing install when stable markers exist"
        )
        self.assertEqual(refuse["if"], "inputs.mode == 'build-only'")
        self.assertIn("test-testing-refusal.ps1", refuse["run"])
        self.assertIn("build\\madmail-windows-amd64-setup.exe", refuse["run"])
        windows_text = yaml.dump(windows)
        self.assertIn("-Testing", windows_text)
        self.assertIn("verify-installer", windows_text)
        self.assertNotIn("windows_release.py", windows_text)
        self.assertNotIn("verify_signed_binary.py", windows_text)
        self.assertNotIn("0x4D,0x5A", windows_text)
        stable_windows = (REPO / ".github/workflows/windows.yml").read_text(encoding="utf-8")
        self.assertNotIn("-Testing", stable_windows)
        self.assertNotIn("test-testing-refusal.ps1", stable_windows)
        script = (REPO / "packaging/windows/test-testing-refusal.ps1").read_text(encoding="utf-8")
        for needle in (
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/SP-",
            "UseShellExecute = $false",
            "WaitForExit(90000)",
            "8F3C2A91-6B4E-4D2A-9C71-0A1B2C3D4E5F",
            "MadmailTray",
            "sentinel.bin",
            "install.log",
            "MadmailTesting",
            "ProgramData",
        ):
            self.assertIn(needle, script)
        for forbidden in ("gh release", "git push", "Invoke-WebRequest", "curl "):
            self.assertNotIn(forbidden, script)

        pr = document["jobs"]["unstable-matrix-pr"]
        self.assertEqual(pr["if"], "github.event_name == 'pull_request'")
        self.assertEqual(pr["uses"], "./.github/workflows/unstable-matrix.yml")
        self.assertEqual(pr["with"], {
            "mode": "build-only",
            "sha": "${{ github.event.pull_request.head.sha }}",
        })
        self.assertEqual(pr["permissions"], {"contents": "read"})
        self.assertNotIn("secrets", pr)
        publication = document["jobs"]["unstable-matrix"]
        self.assertEqual(publication["needs"], "release")
        self.assertIn("needs.release.result == 'success'", publication["if"])
        self.assertIn("needs.release.outputs.tag != ''", publication["if"])
        self.assertEqual(publication["uses"], "./.github/workflows/unstable-matrix.yml")
        self.assertEqual(
            publication["with"],
            {
                "mode": "publish",
                "sha": "${{ needs.release.outputs.sha }}",
                "version": "${{ needs.release.outputs.version }}",
                "tag": "${{ needs.release.outputs.tag }}",
            },
        )
        self.assertEqual(publication["permissions"], {"contents": "read"})
        self.assertNotIn("secrets", publication)
        action = (REPO / ".github/actions/unstable-prepare/action.yml").read_text(encoding="utf-8")
        self.assertNotIn("actions/checkout", action)
        self.assertNotIn("github.sha", action)
        self.assertIn("scripts/unstable_matrix.py prepare", action)
        self.assertEqual(matrix_text.count("ref: ${{ inputs.sha }}"), 8)
        self.assertEqual(matrix_text.count("persist-credentials: false"), 8)
        validate_steps = validate["steps"]
        self.assertEqual(validate_steps[0]["uses"], "actions/checkout@v4")
        self.assertEqual(validate_steps[0]["with"]["ref"], "${{ inputs.sha }}")
        self.assertEqual(validate_steps[1]["uses"], "./.github/actions/unstable-prepare")
        confirm = next(
            step for step in docker["steps"] if step.get("name") == "Confirm the unstable tag commit"
        )
        self.assertIn("${TAG}^{commit}", confirm["run"])
        build = next(
            step for step in docker["steps"] if step.get("name") == "Build immutable unstable image"
        )
        self.assertNotIn("latest", build["with"]["tags"])
        self.assertIn("needs.release.outputs.sha", build["with"]["labels"])
        advance = next(
            step
            for step in docker["steps"]
            if step.get("name") == "Advance unstable alias only for the newest unstable version"
        )
        self.assertIn("--revision origin/main", advance["run"])
        self.assertIn('"$IMAGE:unstable"', advance["run"])
        self.assertNotIn("latest", advance["run"])

    def test_older_checkout_cannot_become_the_unstable_tip(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            git(root, "init", "-b", "main")
            git(root, "config", "user.email", "test@example.com")
            git(root, "config", "user.name", "Test")
            (root / "note").write_text("one\n", encoding="utf-8")
            git(root, "add", "note")
            git(root, "commit", "-m", "first")
            old = git(root, "rev-parse", "HEAD")
            git(root, "tag", "-a", "v1.0.0-unstable", "-m", "old")
            (root / "note").write_text("two\n", encoding="utf-8")
            git(root, "add", "note")
            git(root, "commit", "-m", "second")
            git(root, "tag", "-a", "v2.0.0-unstable", "-m", "new")
            git(root, "checkout", "--detach", old)
            self.assertEqual(
                release.channel_tip(root, "unstable", revision="HEAD"),
                "v1.0.0-unstable",
            )
            self.assertEqual(
                release.channel_tip(root, "unstable", revision="main"),
                "v2.0.0-unstable",
            )
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                code = release.main(
                    [
                        "--repo",
                        str(root),
                        "--channel-tip",
                        "unstable",
                        "--revision",
                        "main",
                    ]
                )
            self.assertEqual(code, 0)
            self.assertEqual(stdout.getvalue().strip(), "v2.0.0-unstable")
            errors = io.StringIO()
            with contextlib.redirect_stderr(errors):
                rejected = release.main(
                    ["--repo", str(root), "--channel-tip", "unstable", "--revision", "origin/../main"]
                )
            self.assertEqual(rejected, 1)
            self.assertIn("Invalid revision", errors.getvalue())
            with contextlib.redirect_stderr(io.StringIO()) as errors:
                plain = release.main(["--repo", str(root), "--revision", "main"])
            self.assertEqual(plain, 1)
            self.assertIn("--revision requires --channel-tip", errors.getvalue())


if __name__ == "__main__":
    unittest.main()
