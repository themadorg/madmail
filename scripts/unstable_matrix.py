#!/usr/bin/env python3
"""One unsigned-matrix contract for pull-request builds and main publication.

Pull requests pass ``--mode build-only`` with one commit. That path reads the
numeric version, admin-web gitlink, and Cargo.lock from that commit. It does
not create a tag and it does not upload a GitHub Release.

Publication passes ``--mode publish`` with the unstable tag. Tag format and
``assert_local_tag`` stay in ``unstable_release``. This module does not weaken
those checks and it never calls the GitHub client.
"""

import argparse
import contextlib
import json
import os
import subprocess
import sys
from pathlib import Path

import unstable_assets
import unstable_release
from unstable_assets import AssetError
from unstable_release import UnstableError

REQUIRED_ROLES = (
    "unstable-admin-web",
    "unstable-linux-amd64",
    "unstable-linux-amd64-legacy",
    "unstable-linux-amd64-musl",
    "unstable-linux-arm64",
    "unstable-linux-arm64-musl",
    "unstable-windows",
)
PUBLICATION_RESULTS = ("release", "build", "test", "matrix")
PULL_REQUEST_EVENTS = frozenset({"pull_request", "pull_request_target"})


class MatrixError(Exception):
    """A matrix failure safe to print in CI."""


@contextlib.contextmanager
def _at(root):
    previous = Path.cwd()
    os.chdir(root)
    try:
        yield
    finally:
        os.chdir(previous)


def _fetch_tag(tag, root):
    if not unstable_release.TAG.fullmatch(tag):
        raise MatrixError("invalid unstable tag")
    result = subprocess.run(
        ["git", "fetch", "origin", f"refs/tags/{tag}:refs/tags/{tag}"],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise MatrixError("unstable tag fetch failed")


def prepare_source(
    root,
    mode,
    sha,
    version="",
    tag="",
    expect_version="",
    expect_generation="",
    fetch_tag=None,
):
    """Check out metadata for one mode. Build-only never accepts a tag."""
    root = Path(root)
    version = version or ""
    tag = tag or ""
    expect_version = expect_version or ""
    expect_generation = expect_generation or ""
    if mode == "build-only":
        if version or tag:
            raise MatrixError("build-only does not accept a release tag or version")
        if fetch_tag:
            raise MatrixError("build-only does not fetch a release tag")
        found, generation = unstable_assets.check_build_only(root, sha)
    elif mode == "publish":
        if not version or not tag:
            raise MatrixError("publication requires the unstable tag and numeric version")
        unstable_release.validate(tag, sha, version)
        if fetch_tag is None or fetch_tag:
            _fetch_tag(tag, root)
        with _at(root):
            unstable_release.assert_local_tag(tag, sha)
        generation = unstable_assets.check_source(root, version, sha)
        found = version
    else:
        raise MatrixError("unknown matrix mode")
    if expect_version and expect_version != found:
        raise MatrixError("jobs disagree on the numeric version")
    if expect_generation and expect_generation != generation:
        raise MatrixError("jobs disagree on the admin-web generation")
    return found, generation


def require_role_results(payload):
    """Fail unless every required build role succeeded."""
    try:
        data = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise MatrixError("matrix role results are unreadable") from exc
    if not isinstance(data, dict):
        raise MatrixError("matrix role results are unreadable")
    missing = []
    for role in REQUIRED_ROLES:
        job = data.get(role)
        result = job.get("result") if isinstance(job, dict) else None
        if result != "success":
            missing.append(role)
    if missing:
        raise MatrixError(
            "required matrix roles did not succeed: " + ", ".join(missing)
        )


def assert_publication(event, ref, tag, results):
    """Refuse every publication path except a successful main push."""
    if event in PULL_REQUEST_EVENTS:
        raise MatrixError("publication cannot run for a pull request")
    if event != "push" or ref != "refs/heads/main":
        raise MatrixError("publication runs only for a main push")
    if not unstable_release.TAG.fullmatch(tag or ""):
        raise MatrixError("publication requires the unstable tag")
    waiting = [
        name for name in PUBLICATION_RESULTS if results.get(name) != "success"
    ]
    if waiting:
        raise MatrixError(
            "publication is waiting for required jobs: " + ", ".join(waiting)
        )


def stage_build_only(root, sha, assets_dir, dest, relays=None):
    """Stage the 16-asset set for one commit. Never creates a release."""
    version, generation = unstable_assets.check_build_only(root, sha)
    if relays is None:
        relays = unstable_assets.fetch_relays()
    files = unstable_release.stage(
        assets_dir, sha, version, generation, relays, Path(dest)
    )
    return {"version": version, "generation": generation, "files": files}


def _write_output(path, version, generation):
    if path is None:
        return
    text = f"version={version}\ngeneration={generation}\n"
    if "\n\n" in text or any(char in text for char in "\r"):
        raise MatrixError("refusing to write an unsafe job output")
    with Path(path).open("a", encoding="utf-8") as handle:
        handle.write(text)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    prepare = commands.add_parser("prepare")
    prepare.add_argument("--mode", required=True, choices=("build-only", "publish"))
    prepare.add_argument("--sha", required=True)
    prepare.add_argument("--root", type=Path, default=Path.cwd())
    prepare.add_argument("--version", default="")
    prepare.add_argument("--tag", default="")
    prepare.add_argument("--expect-version", default="")
    prepare.add_argument("--expect-generation", default="")
    prepare.add_argument("--github-output", type=Path)

    commands.add_parser("require-roles")

    publish = commands.add_parser("assert-publish")
    publish.add_argument("--event", required=True)
    publish.add_argument("--ref", required=True)
    publish.add_argument("--tag", required=True)
    publish.add_argument("--release", required=True)
    publish.add_argument("--build", required=True)
    publish.add_argument("--test", required=True)
    publish.add_argument("--matrix", required=True)

    stage = commands.add_parser("stage-build-only")
    stage.add_argument("--sha", required=True)
    stage.add_argument("--root", type=Path, default=Path.cwd())
    stage.add_argument("--assets", type=Path, required=True)
    stage.add_argument("--dest", type=Path, required=True)

    args = parser.parse_args(argv)
    try:
        if args.command == "prepare":
            version, generation = prepare_source(
                args.root,
                args.mode,
                args.sha,
                version=args.version,
                tag=args.tag,
                expect_version=args.expect_version,
                expect_generation=args.expect_generation,
            )
            _write_output(args.github_output, version, generation)
            print(f"version={version}")
            print(f"generation={generation}")
        elif args.command == "require-roles":
            raw = os.environ.get("MATRIX_NEEDS", "")
            if not raw:
                raise MatrixError("matrix role results are missing")
            require_role_results(raw)
        elif args.command == "assert-publish":
            assert_publication(
                args.event,
                args.ref,
                args.tag,
                {
                    "release": args.release,
                    "build": args.build,
                    "test": args.test,
                    "matrix": args.matrix,
                },
            )
        elif args.command == "stage-build-only":
            staged = stage_build_only(args.root, args.sha, args.assets, args.dest)
            print(
                f"build-only {args.sha}; version {staged['version']}; "
                f"validated {len(staged['files'])} asset(s)"
            )
        return 0
    except (MatrixError, AssetError, UnstableError, OSError, ValueError) as exc:
        message = (
            str(exc)
            if isinstance(exc, (MatrixError, AssetError, UnstableError))
            else type(exc).__name__
        )
        print(f"unstable matrix: {message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
