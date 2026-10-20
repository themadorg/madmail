#!/usr/bin/env python3
"""Resolve a published stable release from its Git tag.

The tag is the source of the commit. A full SHA in the GitHub Release
``targetCommitish`` is cross-checked. A branch or tag name is accepted.
A missing ``vX.Y.Z-unstable`` tag is allowed. An unstable tag that points
somewhere else fails. The script checks out that commit when asked and does
not publish an image.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

import release

TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
SHA = re.compile(r"[0-9a-fA-F]{40}\Z")


class StableSourceError(Exception):
    """A source-selection failure safe to print in CI."""


def version_of(tag):
    if not TAG.fullmatch(tag):
        raise StableSourceError("expected a stable vX.Y.Z tag")
    return tag[1:]


def git(root, args, check=True):
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if check and result.returncode != 0:
        raise StableSourceError("git command failed")
    return result


def peeled_commit(root, tag):
    result = git(
        root,
        ["rev-parse", "--verify", "--end-of-options", f"{tag}^{{commit}}"],
        check=False,
    )
    sha = result.stdout.strip().lower()
    if result.returncode or not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise StableSourceError("stable tag does not resolve")
    return sha


def optional_commit(root, tag):
    existing = git(
        root,
        ["rev-parse", "-q", "--verify", f"refs/tags/{tag}"],
        check=False,
    )
    if existing.returncode != 0:
        return None
    return peeled_commit(root, tag)


def manifests_match(root, sha, version):
    shown = git(root, ["show", f"{sha}:.version"], check=False)
    if shown.returncode or shown.stdout.strip() != version:
        raise StableSourceError(".version does not match the stable tag")
    cargo_show = git(root, ["show", f"{sha}:Cargo.toml"], check=False)
    if cargo_show.returncode:
        raise StableSourceError("Cargo.toml version does not match the stable tag")
    try:
        got = tomllib.loads(cargo_show.stdout)["workspace"]["package"]["version"]
    except (tomllib.TOMLDecodeError, KeyError, TypeError) as exc:
        raise StableSourceError("Cargo.toml version does not match the stable tag") from exc
    if got != version:
        raise StableSourceError("Cargo.toml version does not match the stable tag")


def resolve_stable(root, tag, meta):
    """Return the commit the stable tag records."""
    version = version_of(tag)
    if meta.get("isDraft") or meta.get("isPrerelease"):
        raise StableSourceError("stable release must be published")
    if meta.get("tagName") != tag:
        raise StableSourceError("release tag does not match")
    sha = peeled_commit(root, tag)
    commitish = (meta.get("targetCommitish") or "").strip()
    if SHA.fullmatch(commitish) and commitish.lower() != sha:
        raise StableSourceError("release commit SHA does not match the Git tag")
    unstable = optional_commit(root, f"v{version}-unstable")
    if unstable is not None and unstable != sha:
        raise StableSourceError("unstable tag points at a different commit")
    manifests_match(root, sha, version)
    return {"sha": sha, "version": version, "tag": tag}


def should_advance_latest(versions, candidate):
    """True only when this numeric version is the highest published stable version."""
    if candidate not in versions:
        raise StableSourceError("published release list does not include this version")
    return release.highest_version(versions) == candidate


def fetch_release_tags(root, tag, version):
    """Fetch the stable tag and, when published, its unstable counterpart."""
    version_of(tag)
    git(root, ["fetch", "origin", f"refs/tags/{tag}:refs/tags/{tag}"])
    unstable = f"v{version}-unstable"
    probe = git(
        root,
        ["ls-remote", "--exit-code", "origin", f"refs/tags/{unstable}"],
        check=False,
    )
    if probe.returncode == 0:
        git(root, ["fetch", "origin", f"refs/tags/{unstable}:refs/tags/{unstable}"])
    elif probe.returncode == 2:
        return
    else:
        raise StableSourceError("could not look up the unstable tag")


def load_release(tag):
    result = subprocess.run(
        [
            "gh",
            "release",
            "view",
            tag,
            "--json",
            "isDraft,isPrerelease,tagName,targetCommitish",
        ],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise StableSourceError("could not read the published release")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise StableSourceError("release metadata is not valid") from exc


def detach(root, sha):
    git(root, ["checkout", "--detach", sha])
    head = git(root, ["rev-parse", "HEAD"]).stdout.strip()
    if head != sha:
        raise StableSourceError("checkout does not match the stable tag")


def write_output(result):
    text = f"sha={result['sha']}\nversion={result['version']}\ntag={result['tag']}\n"
    path = os.environ.get("GITHUB_OUTPUT")
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(text)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--checkout", action="store_true")
    args = parser.parse_args(argv)
    try:
        root = args.repo.resolve()
        version = version_of(args.tag)
        fetch_release_tags(root, args.tag, version)
        meta = load_release(args.tag)
        result = resolve_stable(root, args.tag, meta)
        if args.checkout:
            detach(root, result["sha"])
        write_output(result)
        print(f"stable {result['tag']} {result['sha']}")
        return 0
    except StableSourceError as exc:
        print(f"stable: {exc}", file=sys.stderr)
        return 1
    except (OSError, json.JSONDecodeError, KeyError, TypeError):
        print("stable: source resolution failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
