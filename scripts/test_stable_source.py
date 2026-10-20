"""Stable tag resolution. Uses local Git remotes only."""

import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

import yaml

import stable_source
from stable_source import StableSourceError

REPO = Path(__file__).resolve().parents[1]
WORKFLOW = REPO / ".github/workflows/stable-docker.yml"


def git(root, *args):
    result = subprocess.run(
        ["git", *args], cwd=root, text=True, capture_output=True, check=False
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


def published(tag, commitish="main"):
    return {
        "tagName": tag,
        "isDraft": False,
        "isPrerelease": False,
        "targetCommitish": commitish,
    }


class StableSourceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "repo"
        self.root.mkdir()
        init_repo(self.root)
        self.commit = commit_version(self.root, "1.2.3")

    def test_lightweight_and_annotated_tags_resolve_to_the_commit(self):
        git(self.root, "tag", "v1.2.3", self.commit)
        later = commit_version(self.root, "1.2.4", "later")
        resolved = stable_source.resolve_stable(self.root, "v1.2.3", published("v1.2.3"))
        self.assertEqual(resolved["sha"], self.commit)
        self.assertNotEqual(resolved["sha"], later)
        git(self.root, "tag", "-a", "v1.2.3-keep", "-m", "stable", self.commit)
        # Replace the lightweight tag with an annotated tag of the same name.
        git(self.root, "tag", "-d", "v1.2.3")
        git(self.root, "tag", "-a", "v1.2.3", "-m", "stable", self.commit)
        self.assertNotEqual(git(self.root, "rev-parse", "v1.2.3"), self.commit)
        annotated = stable_source.resolve_stable(
            self.root, "v1.2.3", published("v1.2.3", self.commit)
        )
        self.assertEqual(annotated["sha"], self.commit)
        self.assertEqual(annotated["version"], "1.2.3")

    def test_invalid_draft_and_mismatched_sources_fail(self):
        git(self.root, "tag", "v1.2.3", self.commit)
        with self.assertRaisesRegex(StableSourceError, "vX.Y.Z"):
            stable_source.resolve_stable(
                self.root, "v1.2.3-unstable", published("v1.2.3-unstable")
            )
        with self.assertRaisesRegex(StableSourceError, "published"):
            stable_source.resolve_stable(
                self.root,
                "v1.2.3",
                published("v1.2.3") | {"isDraft": True},
            )
        with self.assertRaisesRegex(StableSourceError, "published"):
            stable_source.resolve_stable(
                self.root,
                "v1.2.3",
                published("v1.2.3") | {"isPrerelease": True},
            )
        with self.assertRaisesRegex(StableSourceError, "commit SHA"):
            stable_source.resolve_stable(
                self.root, "v1.2.3", published("v1.2.3", "b" * 40)
            )
        stable_source.resolve_stable(self.root, "v1.2.3", published("v1.2.3", "main"))
        stable_source.resolve_stable(self.root, "v1.2.3", published("v1.2.3", "v1.2.3"))
        other = commit_version(self.root, "9.9.9", "wrong")
        git(self.root, "tag", "-d", "v1.2.3")
        git(self.root, "tag", "v1.2.3", other)
        with self.assertRaisesRegex(StableSourceError, r"\.version"):
            stable_source.resolve_stable(self.root, "v1.2.3", published("v1.2.3"))

    def test_historical_release_without_an_unstable_tag_is_accepted(self):
        git(self.root, "tag", "-a", "v1.2.3", "-m", "historical", self.commit)
        resolved = stable_source.resolve_stable(self.root, "v1.2.3", published("v1.2.3"))
        self.assertEqual(resolved["sha"], self.commit)
        self.assertIsNone(stable_source.optional_commit(self.root, "v1.2.3-unstable"))

    def test_existing_unstable_tag_must_match_the_stable_commit(self):
        git(self.root, "tag", "v1.2.3", self.commit)
        git(self.root, "tag", "v1.2.3-unstable", self.commit)
        matched = stable_source.resolve_stable(self.root, "v1.2.3", published("v1.2.3"))
        self.assertEqual(matched["sha"], self.commit)
        later = commit_version(self.root, "1.2.4", "drift")
        git(self.root, "tag", "-d", "v1.2.3-unstable")
        git(self.root, "tag", "v1.2.3-unstable", later)
        with self.assertRaisesRegex(StableSourceError, "unstable tag"):
            stable_source.resolve_stable(self.root, "v1.2.3", published("v1.2.3"))

    def test_older_version_does_not_advance_latest(self):
        self.assertFalse(stable_source.should_advance_latest(["1.0.0", "2.0.0"], "1.0.0"))
        self.assertTrue(stable_source.should_advance_latest(["1.0.0", "2.0.0"], "2.0.0"))
        with self.assertRaisesRegex(StableSourceError, "does not include"):
            stable_source.should_advance_latest(["2.0.0"], "1.0.0")

    def test_fetch_keeps_historical_tags_and_rejects_a_mismatch(self):
        origin = Path(self.tmp.name) / "origin.git"
        subprocess.run(
            ["git", "init", "--bare", "-b", "main", str(origin)],
            check=True,
            capture_output=True,
        )
        git(self.root, "remote", "add", "origin", str(origin))
        git(self.root, "tag", "-a", "v1.2.3", "-m", "stable")
        git(self.root, "push", "origin", "main", "v1.2.3")
        consumer = Path(self.tmp.name) / "consumer"
        subprocess.run(
            ["git", "clone", "--no-tags", str(origin), str(consumer)],
            check=True,
            capture_output=True,
        )
        stable_source.fetch_release_tags(consumer, "v1.2.3", "1.2.3")
        resolved = stable_source.resolve_stable(consumer, "v1.2.3", published("v1.2.3"))
        self.assertEqual(resolved["sha"], self.commit)
        self.assertNotEqual(git(consumer, "rev-parse", "v1.2.3"), self.commit)
        self.assertIsNone(stable_source.optional_commit(consumer, "v1.2.3-unstable"))

        later = commit_version(self.root, "1.2.4", "unstable drift")
        git(self.root, "tag", "v1.2.3-unstable", later)
        git(self.root, "push", "origin", "main", "v1.2.3-unstable")
        drifted = Path(self.tmp.name) / "drifted"
        subprocess.run(
            ["git", "clone", "--no-tags", str(origin), str(drifted)],
            check=True,
            capture_output=True,
        )
        stable_source.fetch_release_tags(drifted, "v1.2.3", "1.2.3")
        with self.assertRaisesRegex(StableSourceError, "unstable tag"):
            stable_source.resolve_stable(drifted, "v1.2.3", published("v1.2.3"))

    def test_main_checks_out_the_tagged_commit(self):
        git(self.root, "tag", "-a", "v1.2.3", "-m", "stable", self.commit)
        commit_version(self.root, "1.2.4", "main moved on")
        output = Path(self.tmp.name) / "github-output"
        meta = published("v1.2.3", self.commit)
        with (
            mock.patch.object(stable_source, "fetch_release_tags"),
            mock.patch.object(stable_source, "load_release", return_value=meta),
            mock.patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}),
        ):
            import contextlib
            import io

            with contextlib.redirect_stdout(io.StringIO()):
                code = stable_source.main(
                    ["--repo", str(self.root), "--tag", "v1.2.3", "--checkout"]
                )
        self.assertEqual(code, 0)
        self.assertEqual(git(self.root, "rev-parse", "HEAD"), self.commit)
        self.assertIn(f"sha={self.commit}\n", output.read_text(encoding="utf-8"))
        self.assertIn("version=1.2.3\n", output.read_text(encoding="utf-8"))

    def test_workflow_builds_the_resolved_commit_and_guards_latest(self):
        source = WORKFLOW.read_text(encoding="utf-8")
        document = yaml.safe_load(source)
        job = document["jobs"]["docker"]
        self.assertIn("prerelease == false", job["if"])
        self.assertIn("stable_source.py", source)
        self.assertIn("--checkout", source)
        self.assertIn("git submodule update --init --recursive", source)
        self.assertNotIn("pinned to a full commit SHA", source)
        self.assertNotIn("v${VERSION}-unstable^{commit}", source)
        build = next(
            step for step in job["steps"] if step.get("name") == "Build immutable stable image"
        )
        self.assertEqual(build["with"]["context"], "source")
        self.assertNotIn("latest", build["with"]["tags"])
        self.assertIn("steps.source.outputs.sha", build["with"]["labels"])
        advance = next(
            step
            for step in job["steps"]
            if step.get("name") == "Advance latest only when this is the highest stable version"
        )
        self.assertIn("should_advance_latest", advance["run"])
        self.assertIn(":latest", advance["run"])


if __name__ == "__main__":
    unittest.main()
