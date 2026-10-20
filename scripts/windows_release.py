#!/usr/bin/env python3
"""Stage a stable Windows installer from the signed executable already published.

The workflow downloads ``madmail-windows-amd64.exe`` and
``madmail-tray-windows-amd64.exe`` from a published stable GitHub Release.
It checks the server signature and stages both files for Inno Setup. It may
upload only ``madmail-windows-amd64-setup.exe``.

The tray asset has to be on that same release. A checksum, when the release
publishes one, checks the tray bytes. It does not prove which commit built
the tray. This module does not build a tray from another revision, and it
does not apply the server Ed25519 trailer to the tray.

This module never rebuilds, re-signs, or uploads the portable executable.
An existing installer is reused when the bytes match and rejected when they differ.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import verify_signed_binary
from verify_signed_binary import VerifyError

EXE_NAME = "madmail-windows-amd64.exe"
TRAY_NAME = "madmail-tray-windows-amd64.exe"
SETUP_NAME = "madmail-windows-amd64-setup.exe"
OPTIONAL_ASSETS = ("SHA256SUMS", "SOURCE.txt")
TRAY_MISSING = (
    "tray executable is not on the release; "
    "the private publisher must attach madmail-tray-windows-amd64.exe "
    "built for this release"
)
TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
SHA = re.compile(r"[0-9a-fA-F]{40}\Z")
VIEW_FIELDS = "isDraft,isPrerelease,tagName,targetCommitish,assets"


def run_command(args, cwd, timeout=120):
    try:
        result = subprocess.run(
            args,
            cwd=cwd,
            text=True,
            capture_output=True,
            check=False,
            timeout=timeout,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise VerifyError(f"{args[0]} failed") from exc
    if result.returncode != 0:
        raise VerifyError(f"{args[0]} {args[1] if len(args) > 1 else ''} failed".strip())
    return result.stdout


def asset(release, name):
    for item in release.get("assets") or []:
        if item.get("name") == name:
            return item
    return None


def peeled_commit(root, tag):
    result = subprocess.run(
        ["git", "rev-parse", "--verify", "--end-of-options", f"{tag}^{{commit}}"],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    sha = result.stdout.strip().lower()
    if result.returncode or not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise VerifyError("stable tag does not resolve")
    return sha


def manifests_match(root, sha, version):
    shown = subprocess.run(
        ["git", "show", f"{sha}:.version"],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if shown.returncode or shown.stdout.strip() != version:
        raise VerifyError(".version does not match the stable tag")
    cargo_show = subprocess.run(
        ["git", "show", f"{sha}:Cargo.toml"],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if cargo_show.returncode:
        raise VerifyError("Cargo.toml version does not match the stable tag")
    try:
        got = tomllib.loads(cargo_show.stdout)["workspace"]["package"]["version"]
    except (tomllib.TOMLDecodeError, KeyError, TypeError) as exc:
        raise VerifyError("Cargo.toml version does not match the stable tag") from exc
    if got != version:
        raise VerifyError("Cargo.toml version does not match the stable tag")


def download_plan(release):
    """Asset names to download. The server and tray executables are required."""
    if asset(release, EXE_NAME) is None:
        raise VerifyError("signed executable is not on the release")
    if asset(release, TRAY_NAME) is None:
        raise VerifyError(TRAY_MISSING)
    names = [EXE_NAME, TRAY_NAME]
    for name in OPTIONAL_ASSETS:
        if asset(release, name) is not None:
            names.append(name)
    return names


def write_exact(dest, data):
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    temporary = dest.with_name(dest.name + ".tmp")
    temporary.write_bytes(data)
    os.replace(temporary, dest)


def verify_tray(path, *, digest, sums_text, source_text, expected_sha):
    """Check the tray bytes already attached to this release.

    A published digest or ``SHA256SUMS`` entry must match. Neither value
    proves the commit that produced the tray. ``SOURCE.txt``, when the
    release has one, is the release's claimed source and must match the tag.
    The server Ed25519 trailer is not applied here.
    """
    path = Path(path) if path is not None else None
    if path is None or path.name != TRAY_NAME or path.is_symlink() or not path.is_file():
        raise VerifyError("tray executable was not downloaded")
    data = path.read_bytes()
    if not data:
        raise VerifyError("tray executable is empty")
    published_checksum = False
    if digest:
        try:
            verify_signed_binary.check_digest(data, digest)
        except VerifyError as exc:
            text = str(exc)
            if "does not match" in text:
                raise VerifyError("asset digest does not match the downloaded tray") from exc
            raise VerifyError(f"tray {text}") from exc
        published_checksum = True
    if sums_text is not None:
        try:
            verify_signed_binary.check_sums(data, sums_text, TRAY_NAME)
        except VerifyError as exc:
            text = str(exc)
            if "does not list" in text:
                raise VerifyError("SHA256SUMS does not list the tray executable") from exc
            if "does not match" in text:
                raise VerifyError("SHA256SUMS does not match the downloaded tray") from exc
            raise
        published_checksum = True
    if source_text is not None:
        if not expected_sha:
            raise VerifyError("SOURCE.txt requires the tag commit")
        verify_signed_binary.check_source_text(source_text, expected_sha)
    return data, "matched" if published_checksum else "absent"


def prepare(root, tag, release, exe, extras, dest, version_text=None):
    """Check the tag, manifests, and signed executable, then stage its exact bytes."""
    if not TAG.fullmatch(tag):
        raise VerifyError("stable installer requires tag vX.Y.Z")
    if release.get("isDraft") or release.get("isPrerelease") or release.get("tagName") != tag:
        raise VerifyError("refusing to package a draft or prerelease")
    version = tag[1:]
    sha = peeled_commit(root, tag)
    commitish = (release.get("targetCommitish") or "").strip()
    if SHA.fullmatch(commitish) and commitish.lower() != sha:
        raise VerifyError("release commit SHA does not match the Git tag")
    manifests_match(root, sha, version)
    if asset(release, EXE_NAME) is None:
        raise VerifyError("signed executable is not on the release")
    sums_text = None
    source_text = None
    extras = Path(extras) if extras is not None else None
    if asset(release, "SHA256SUMS") is not None:
        sums_path = None if extras is None else extras / "SHA256SUMS"
        if sums_path is None or sums_path.is_symlink() or not sums_path.is_file():
            raise VerifyError("SHA256SUMS is listed but was not downloaded")
        sums_text = sums_path.read_text(encoding="utf-8")
    if asset(release, "SOURCE.txt") is not None:
        source_path = None if extras is None else extras / "SOURCE.txt"
        if source_path is None or source_path.is_symlink() or not source_path.is_file():
            raise VerifyError("SOURCE.txt is listed but was not downloaded")
        source_text = source_path.read_text(encoding="utf-8")
    exe_asset = asset(release, EXE_NAME) or {}
    data = verify_signed_binary.verify_release_binary(
        exe,
        version,
        sums_text=sums_text,
        digest=exe_asset.get("digest") or "",
        source_text=source_text,
        expected_sha=sha,
        version_text=version_text,
    )
    if Path(exe).name != EXE_NAME:
        raise VerifyError("signed executable was not downloaded")
    tray_asset = asset(release, TRAY_NAME)
    if tray_asset is None:
        raise VerifyError(TRAY_MISSING)
    tray_data, tray_checksum = verify_tray(
        None if extras is None else Path(extras) / TRAY_NAME,
        digest=tray_asset.get("digest") or "",
        sums_text=sums_text,
        source_text=source_text,
        expected_sha=sha,
    )
    write_exact(dest, data)
    tray_dest = Path(dest).parent / TRAY_NAME
    write_exact(tray_dest, tray_data)
    if Path(dest).read_bytes() != data or tray_dest.read_bytes() != tray_data:
        raise VerifyError("failed to stage the verified executable")
    return {"version": version, "sha": sha, "tray_checksum": tray_checksum}


def installer_plan(existing, fresh):
    """Reuse an identical installer. Reject a different published file."""
    if existing is None:
        return "upload"
    if existing == fresh:
        return "reuse"
    raise VerifyError("installer asset conflicts with the published file")


def decide_upload(tag, release, fresh, existing):
    if not TAG.fullmatch(tag):
        raise VerifyError("refusing to upload an installer for this tag")
    if release.get("isDraft") or release.get("isPrerelease") or release.get("tagName") != tag:
        raise VerifyError("refusing to upload an installer to a draft or prerelease")
    if asset(release, SETUP_NAME) is None:
        return installer_plan(None, fresh)
    if existing is None:
        raise VerifyError("installer asset exists but was not downloaded")
    return installer_plan(existing, fresh)


def upload_command(tag, installer):
    path = Path(installer)
    if not TAG.fullmatch(tag) or path.name != SETUP_NAME:
        raise VerifyError("refusing to upload anything except the installer")
    return ["gh", "release", "upload", tag, str(path)]


def write_github_output(values):
    path = os.environ.get("GITHUB_OUTPUT")
    text = "".join(f"{key}={value}\n" for key, value in values.items())
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(text)


def stage(root, tag, dest, runner=run_command):
    if not TAG.fullmatch(tag):
        raise VerifyError("stable installer requires tag vX.Y.Z")
    root = Path(root).resolve()
    runner(["git", "fetch", "origin", f"refs/tags/{tag}:refs/tags/{tag}"], root, 120)
    raw = runner(["gh", "release", "view", tag, "--json", VIEW_FIELDS], root, 120)
    try:
        release = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise VerifyError("release metadata is not valid") from exc
    if release.get("isDraft") or release.get("isPrerelease") or release.get("tagName") != tag:
        raise VerifyError("refusing to package a draft or prerelease")
    names = download_plan(release)
    with tempfile.TemporaryDirectory(prefix="madmail-signed-") as directory:
        folder = Path(directory)
        for name in names:
            runner(
                ["gh", "release", "download", tag, "--dir", str(folder), "--pattern", name],
                root,
                120,
            )
        result = prepare(root, tag, release, folder / EXE_NAME, folder, dest)
    if result["tray_checksum"] == "matched":
        print(
            "tray checksum matched a published digest or SHA256SUMS entry; "
            "that does not prove which commit built the tray"
        )
    else:
        print(
            "tray has no published digest or SHA256SUMS entry; "
            "staged the release asset only"
        )
    write_github_output(result)
    return result


def plan_upload(root, tag, installer, runner=run_command):
    path = Path(installer)
    if path.is_symlink() or not path.is_file() or path.name != SETUP_NAME:
        raise VerifyError("refusing to upload anything except the installer")
    if not TAG.fullmatch(tag):
        raise VerifyError("refusing to upload an installer for this tag")
    root = Path(root).resolve()
    raw = runner(
        ["gh", "release", "view", tag, "--json", "isDraft,isPrerelease,tagName,assets"],
        root,
        120,
    )
    try:
        release = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise VerifyError("release metadata is not valid") from exc
    if release.get("isDraft") or release.get("isPrerelease") or release.get("tagName") != tag:
        raise VerifyError("refusing to upload an installer to a draft or prerelease")
    fresh = path.read_bytes()
    existing = None
    if asset(release, SETUP_NAME) is not None:
        with tempfile.TemporaryDirectory(prefix="madmail-setup-") as directory:
            folder = Path(directory)
            runner(
                [
                    "gh",
                    "release",
                    "download",
                    tag,
                    "--dir",
                    str(folder),
                    "--pattern",
                    SETUP_NAME,
                ],
                root,
                120,
            )
            downloaded = folder / SETUP_NAME
            if downloaded.is_symlink() or not downloaded.is_file():
                raise VerifyError("installer asset exists but was not downloaded")
            existing = downloaded.read_bytes()
    decision = decide_upload(tag, release, fresh, existing)
    if decision == "reuse":
        return "reuse"
    runner(upload_command(tag, path), root, 120)
    return "upload"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    stage_cmd = commands.add_parser("stage")
    stage_cmd.add_argument("--repo", type=Path, default=Path.cwd())
    stage_cmd.add_argument("--tag", required=True)
    stage_cmd.add_argument("--dest", type=Path, required=True)
    upload_cmd = commands.add_parser("plan-upload")
    upload_cmd.add_argument("--repo", type=Path, default=Path.cwd())
    upload_cmd.add_argument("--tag", required=True)
    upload_cmd.add_argument("--installer", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "stage":
            result = stage(args.repo, args.tag, args.dest)
            print(f"verified {result['version']} {result['sha']}")
        else:
            print(plan_upload(args.repo, args.tag, args.installer))
        return 0
    except VerifyError as exc:
        print(f"windows: {exc}", file=sys.stderr)
        return 1
    except (OSError, json.JSONDecodeError):
        print("windows: release asset check failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
