#!/usr/bin/env python3
"""Publish an unsigned testing prerelease. Never signs or replaces assets."""

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import unstable_assets
from unstable_assets import AssetError

WARNING = (
    "TESTING ONLY — NOT FOR PRODUCTION USE.\n"
    "\n"
    "These builds are unsigned, may contain regressions or incompatible changes, "
    "and do not have guaranteed upgrade or downgrade compatibility. They are not "
    "supported by the signed production updater.\n"
)
TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-unstable\Z")
SHA = re.compile(r"[0-9a-f]{40}\Z")
VERSION_HEADING = re.compile(r"^# \[([0-9]+\.[0-9]+\.[0-9]+)\].*$", re.MULTILINE)


class UnstableError(Exception):
    """A publish failure safe to print in CI."""


def changelog_section(text, version):
    match = VERSION_HEADING.search(text)
    while match:
        if match.group(1) == version:
            rest = text[match.end() :]
            nxt = VERSION_HEADING.search(rest)
            body = rest[: nxt.start()] if nxt else rest
            return (text[match.start() : match.end()] + body).strip() + "\n"
        match = VERSION_HEADING.search(text, match.end())
    return ""


def build_notes(version, sha, changelog_text):
    section = changelog_section(changelog_text, version)
    if not section.strip():
        raise UnstableError("changelog has no section for this version")
    return (
        WARNING
        + "\n"
        + f"Source revision: {sha}\n"
        + f"Version: {version}-unstable\n"
        + "\n"
        + section
    )


def validate(tag, sha, version):
    if not TAG.fullmatch(tag) or not SHA.fullmatch(sha):
        raise UnstableError("invalid unstable tag or commit")
    if tag != f"v{version}-unstable":
        raise UnstableError("tag does not match numeric version")


def assert_local_tag(tag, sha):
    result = subprocess.run(
        ["git", "rev-parse", f"{tag}^{{commit}}"],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode or result.stdout.strip() != sha:
        raise UnstableError("tag does not point at the expected commit")


def stage(directory, sha, version, generation, relays, dest):
    """Pack the required payloads, then add SOURCE.txt and SHA256SUMS."""
    try:
        staged = unstable_assets.prepare(directory, dest, version, generation, relays)
    except AssetError as exc:
        raise UnstableError(str(exc)) from exc
    source = dest / unstable_assets.SOURCE_NAME
    source.write_text(sha + "\n", encoding="utf-8")
    staged.append(source)
    try:
        text = unstable_assets.checksum_text(staged)
    except AssetError as exc:
        raise UnstableError(str(exc)) from exc
    sums = dest / unstable_assets.SUMS_NAME
    sums.write_text(text, encoding="utf-8")
    if sums.name in text:
        raise UnstableError("checksum file lists itself")
    staged.append(sums)
    names = [path.name for path in staged]
    if len(names) != len(set(names)) or set(names) != set(unstable_assets.UPLOADED_NAMES):
        raise UnstableError("staged upload is not the 16-asset set")
    return sorted(staged, key=lambda item: item.name)


class CommandGh:
    """GitHub CLI adapter. Asset uploads never use --clobber."""

    def _run(self, args):
        result = subprocess.run(
            ["gh", *args],
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode:
            if args[:2] == ["release", "view"] and "release not found" in result.stderr.lower():
                return None
            raise UnstableError(f"gh {args[0]} {args[1]} failed (exit {result.returncode})")
        return result.stdout

    def view(self, tag):
        import json

        stdout = self._run(
            [
                "release",
                "view",
                tag,
                "--json",
                "tagName,isDraft,isPrerelease,targetCommitish,assets",
            ]
        )
        if stdout is None:
            return None
        return json.loads(stdout)

    def create(self, tag, sha, notes, files):
        fd, name = tempfile.mkstemp(prefix="unstable-notes-", suffix=".md")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                handle.write(notes)
            self._run(
                [
                    "release",
                    "create",
                    tag,
                    "--prerelease",
                    "--target",
                    sha,
                    "--title",
                    tag,
                    "--notes-file",
                    name,
                    *[str(path) for path in files],
                ]
            )
        finally:
            os.unlink(name)

    def upload(self, tag, files):
        self._run(["release", "upload", tag, *[str(path) for path in files]])

    def download(self, tag, name):
        directory = Path(tempfile.mkdtemp(prefix="unstable-asset-"))
        self._run(["release", "download", tag, "--pattern", name, "--dir", str(directory)])
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise UnstableError("downloaded asset is missing")
        return path


def publish_testing_release(tag, sha, notes, files, gh, *, write):
    names = [path.name for path in files]
    if len(names) != len(set(names)):
        raise UnstableError("duplicate asset names")
    if not write:
        return {"created": False, "uploaded": names, "dry_run": True}
    existing = gh.view(tag)
    if existing is None:
        gh.create(tag, sha, notes, files)
        return {"created": True, "uploaded": names, "dry_run": False}
    if existing.get("isDraft") or not existing.get("isPrerelease"):
        raise UnstableError("refusing to change a draft or stable release")
    if existing.get("targetCommitish") != sha:
        raise UnstableError("existing prerelease points at a different commit")
    have = {asset.get("name") for asset in existing.get("assets", [])}
    for path in files:
        if path.name not in have:
            continue
        downloaded = gh.download(tag, path.name)
        try:
            if downloaded.read_bytes() != path.read_bytes():
                raise UnstableError(
                    f"asset {path.name} conflicts with the published file"
                )
        finally:
            downloaded.unlink(missing_ok=True)
    missing = [path for path in files if path.name not in have]
    if missing:
        gh.upload(tag, missing)
    return {
        "created": False,
        "uploaded": [path.name for path in missing],
        "dry_run": False,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--generation", required=True)
    parser.add_argument("--changelog", type=Path, required=True)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.write and os.environ.get("GITHUB_EVENT_NAME", "") in {
            "pull_request",
            "pull_request_target",
        }:
            raise UnstableError("publication cannot run for a pull request")
        validate(args.tag, args.sha, args.version)
        assert_local_tag(args.tag, args.sha)
        notes = build_notes(
            args.version, args.sha, args.changelog.read_text(encoding="utf-8")
        )
        relays = unstable_assets.fetch_relays()
        with tempfile.TemporaryDirectory(prefix="unstable-stage-") as directory:
            files = stage(
                args.assets,
                args.sha,
                args.version,
                args.generation,
                relays,
                Path(directory),
            )
            result = publish_testing_release(
                args.tag,
                args.sha,
                notes,
                files,
                CommandGh() if args.write else None,
                write=args.write,
            )
        print(
            f"{'created' if result['created'] else 'reused'} {args.tag}; "
            f"uploaded {len(result['uploaded'])} asset(s)"
        )
        return 0
    except (UnstableError, AssetError, OSError, ValueError) as exc:
        message = str(exc) if isinstance(exc, (UnstableError, AssetError)) else type(exc).__name__
        print(f"unstable: {message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
