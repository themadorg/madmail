#!/usr/bin/env python3
"""Publish testing prereleases or securely promote a selected unstable candidate.

Stable publication is split into stage (secure build/sign), Windows setup (CI),
and finalize (verify all assets and publish). All network writes require --write.
"""

import argparse
import json
import os
import re
import shlex
import subprocess
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path

import release
from artifact_signing import (
    digest,
    public_key_from_source,
    sign,
    verify,
    verify_checksums,
)

REPO = "themadorg/madmail"
WARNING = (
    "> **TESTING ONLY — UNSUITABLE FOR PRODUCTION.** These unsigned builds may "
    "contain regressions and incompatible changes. Upgrade/downgrade paths are "
    "unsupported. Download manually and test in isolated environments; the "
    "signed production updater will reject these binaries.\n\n"
)
REQUIRED = (
    "madmail-linux-amd64",
    "madmail-linux-amd64-legacy",
    "madmail-linux-amd64-musl",
    "madmail-linux-arm64",
    "madmail-linux-arm64-musl",
    "madmail-windows-amd64.exe",
)
SETUP = "madmail-windows-amd64-setup.exe"


def candidate(root, tag):
    if not re.fullmatch(
        r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-unstable", tag
    ):
        raise release.ReleaseError("Select an exact vMAJOR.MINOR.PATCH-unstable tag")
    sha = release.git(
        root, "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}"
    ).strip()
    version = tag[1:].removesuffix("-unstable")
    if release.git(root, "show", f"{sha}:.version").strip() != version:
        raise release.ReleaseError("Candidate tag and numeric version disagree")
    cargo = release.tomllib.loads(release.git(root, "show", f"{sha}:Cargo.toml"))
    if cargo["workspace"]["package"]["version"] != version:
        raise release.ReleaseError("Candidate Cargo version disagrees")
    for name in ("package.json", "package-lock.json"):
        doc = json.loads(release.git(root, "show", f"{sha}:{name}"))
        if doc["version"] != version or (
            name.endswith("lock.json") and doc["packages"][""]["version"] != version
        ):
            raise release.ReleaseError("Candidate package version disagrees")
    return version, sha


class GitHub:
    def __init__(self, root):
        self.root = root

    def gh(self, *args):
        return release.run(self.root, ["gh", *args])

    def api(self, path, *args):
        return json.loads(self.gh("api", f"repos/{REPO}/{path}", *args))

    def releases(self):
        pages = json.loads(
            self.gh(
                "api", "--paginate", "--slurp", f"repos/{REPO}/releases?per_page=100"
            )
        )
        return [item for page in pages for item in page]

    def get(self, tag):
        return next((item for item in self.releases() if item["tag_name"] == tag), None)

    def exact_tag(self, tag, sha):
        obj = self.api(f"git/ref/tags/{tag}")["object"]
        while obj["type"] == "tag":
            obj = self.api(f"git/tags/{obj['sha']}")["object"]
        if obj["type"] != "commit" or obj["sha"] != sha:
            raise release.ReleaseError(
                "Remote tag does not match selected source commit"
            )

    def download(self, tag, name, directory):
        self.gh(
            "release",
            "download",
            tag,
            "--repo",
            REPO,
            "--pattern",
            name,
            "--dir",
            str(directory),
        )
        return Path(directory) / name

    def upload(self, tag, path):
        # Never clobber. An existing same-name asset is accepted only byte-for-byte.
        current = self.get(tag)
        if any(asset["name"] == path.name for asset in current["assets"]):
            with tempfile.TemporaryDirectory(dir=self.root.parent) as directory:
                existing = self.download(tag, path.name, directory)
                if digest(existing) != digest(path):
                    raise release.ReleaseError(
                        "Conflicting existing asset: " + path.name
                    )
            return
        self.gh("release", "upload", tag, str(path), "--repo", REPO)


def previous_stable(root, github, sha, selected_version):
    published = [
        item
        for item in github.releases()
        if not item["draft"]
        and not item["prerelease"]
        and item["tag_name"].startswith("v")
        and release.VERSION.fullmatch(item["tag_name"][1:])
    ]
    # Publication history, not greatest Git tag or newest unstable build.
    previous = max(published, key=lambda item: item["published_at"], default=None)
    if not previous:
        return None
    tag = previous["tag_name"]
    if release.version_tuple(tag[1:]) >= release.version_tuple(selected_version):
        raise release.ReleaseError(
            "Stable candidate must be newer than previous published stable release"
        )
    if subprocess.run(
        ["git", "merge-base", "--is-ancestor", tag, sha],
        cwd=root,
        capture_output=True,
        check=False,
    ).returncode:
        raise release.ReleaseError("Previous stable is not an ancestor of candidate")
    return tag


def notes_data(root, version, sha, base, tag):
    rows = release.git(
        root,
        "log",
        "--reverse",
        "--format=%H%x00%B%x00",
        f"{base}..{sha}" if base else sha,
    ).split("\0")
    commits = [
        release.Commit(rows[i].strip(), rows[i + 1].strip())
        for i in range(0, len(rows) - 1, 2)
        if rows[i].strip()
    ]
    changes = release.changes_from_commits(commits)
    notes = release.release_notes(
        version,
        base,
        changes,
        release.git(root, "show", "-s", "--format=%cs", sha).strip(),
        tag=tag,
    )
    short = "\n".join("* " + release.markdown_text(c.description) for c in changes[:3])
    return notes, short, [vars(c) for c in changes]


def write_checksums(directory, paths, name):
    (directory / name).write_text(
        "".join(f"{digest(path)}  {path.name}\n" for path in sorted(paths))
    )


def archive_binaries(directory):
    for path in sorted(directory.glob("madmail-linux-*")):
        if path.is_file() and "." not in path.name:
            with tarfile.open(directory / (path.name + ".tar.gz"), "w:gz") as archive:
                archive.add(path, arcname="madmail")
    with zipfile.ZipFile(
        directory / "madmail-windows-amd64.zip", "w", zipfile.ZIP_DEFLATED
    ) as archive:
        archive.write(directory / "madmail-windows-amd64.exe", "madmail.exe")


def verify_archives(directory):
    for name in REQUIRED:
        if name.startswith("madmail-linux-"):
            with tarfile.open(directory / (name + ".tar.gz"), "r:gz") as archive:
                if archive.getnames() != ["madmail"]:
                    raise release.ReleaseError("Unexpected Linux archive entries")
                member = archive.getmember("madmail")
                if (
                    not member.isfile()
                    or archive.extractfile(member).read()
                    != (directory / name).read_bytes()
                ):
                    raise release.ReleaseError("Archive differs from signed server")
    with zipfile.ZipFile(directory / "madmail-windows-amd64.zip") as archive:
        if (
            archive.namelist() != ["madmail.exe"]
            or archive.read("madmail.exe")
            != (directory / "madmail-windows-amd64.exe").read_bytes()
        ):
            raise release.ReleaseError("Windows archive differs from signed server")


def new_draft(github, tag, sha, notes_file, prerelease):
    args = [
        "release",
        "create",
        tag,
        "--repo",
        REPO,
        "--verify-tag",
        "--target",
        sha,
        "--draft",
        "--title",
        f"Madmail {tag}",
        "--notes-file",
        str(notes_file),
    ]
    if prerelease:
        args.append("--prerelease")
    github.gh(*args)


def unstable(root, github, tag, artifacts, write):
    version, sha = candidate(root, tag)
    github.exact_tag(tag, sha)
    previous = release.latest_release_at(root, f"{sha}^")
    notes, short, changes = notes_data(root, version, sha, previous, tag)
    print(
        json.dumps(
            {
                "channel": "unstable",
                "tag": tag,
                "sha": sha,
                "base": previous,
                "write": write,
            }
        )
    )
    if not write:
        return
    artifacts = artifacts.resolve()
    paths = sorted(path for path in artifacts.glob("madmail-*") if path.is_file())
    if not paths or any(
        not re.fullmatch(r"madmail-[A-Za-z0-9._-]+", p.name) for p in paths
    ):
        raise release.ReleaseError(
            "Expected only unsigned madmail binaries in artifact directory"
        )
    (artifacts / "release.json").write_text(
        json.dumps(
            {
                "version": version,
                "source_commit": sha,
                "channel": "unstable",
                "changes": changes,
                "telegram": short,
            },
            indent=2,
        )
        + "\n"
    )
    paths.append(artifacts / "release.json")
    write_checksums(artifacts, paths, "SHA256SUMS")
    with tempfile.TemporaryDirectory(dir=root.parent) as temporary:
        notes_file = Path(temporary) / "notes.md"
        notes_file.write_text(WARNING + notes)
        current = github.get(tag)
        if current and (
            not current["prerelease"] or current["target_commitish"] != sha
        ):
            raise release.ReleaseError("Conflicting unstable release metadata")
        if not current:
            new_draft(github, tag, sha, notes_file, True)
        for path in [*paths, artifacts / "SHA256SUMS"]:
            github.upload(tag, path)
        github.gh(
            "release",
            "edit",
            tag,
            "--repo",
            REPO,
            "--draft=false",
            "--prerelease",
            "--latest=false",
        )


def stable_plan(root, github, tag):
    version, sha = candidate(root, tag)
    github.exact_tag(tag, sha)
    selected = github.get(tag)
    if (
        not selected
        or selected["draft"]
        or not selected["prerelease"]
        or selected["target_commitish"] != sha
    ):
        raise release.ReleaseError(
            "Candidate must be an existing published unstable prerelease"
        )
    stable = "v" + version
    existing = github.get(stable)
    if existing and (
        not existing["draft"]
        or existing["prerelease"]
        or existing["target_commitish"] != sha
    ):
        raise release.ReleaseError("Stable version already published or conflicting")
    refs = release.git(
        root,
        "ls-remote",
        "--tags",
        "origin",
        f"refs/tags/{stable}",
        f"refs/tags/{stable}^{{}}",
    ).splitlines()
    if refs:
        github.exact_tag(stable, sha)
    base = previous_stable(root, github, sha, version)
    return version, sha, stable, base, existing


def stage(root, github, args):
    version, sha, stable, base, existing = stable_plan(root, github, args.candidate)
    print(
        json.dumps(
            {
                "channel": "stable",
                "tag": stable,
                "candidate": args.candidate,
                "sha": sha,
                "base": base,
                "write": args.write,
            }
        )
    )
    if not args.write:
        return
    if not args.key or not args.build_command:
        raise release.ReleaseError("Secure staging requires --key and --build-command")
    # A persistent directory allows a failed upload to resume without rebuilding.
    output = (args.output or root / "build" / ("publish-" + stable)).resolve()
    output.mkdir(parents=True, exist_ok=True)
    manifest_path = output / "release.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
        if (
            manifest["source_commit"] != sha
            or manifest["candidate"] != args.candidate
            or manifest["previous_stable"] != base
        ):
            raise release.ReleaseError("Conflicting local publication state")
        verify_checksums(output, "binary-checksums.txt")
    else:
        if any(output.iterdir()):
            raise release.ReleaseError("Use an empty publication output directory")
        source = output.parent / (output.name + "-source")
        if source.exists():
            raise release.ReleaseError(
                "Build checkout already exists; inspect/remove it before retrying"
            )
        release.git(root, "worktree", "add", "--detach", str(source), sha)
        try:
            release.git(source, "submodule", "update", "--init", "--recursive")
            # Older candidates predate the tracked build helper; use the trusted local helper.
            helper = root / "scripts/build-release-static.sh"
            target = source / "scripts/build-release-static.sh"
            injected_helper = helper.is_file() and not target.exists()
            if injected_helper:
                target.parent.mkdir(exist_ok=True)
                target.write_bytes(helper.read_bytes())
                target.chmod(0o755)
            release.run(source, shlex.split(args.build_command))
            release.run(source, shlex.split(args.test_command))
            binary_version = release.run(
                source, [str(source / "build/madmail-linux-amd64"), "version"]
            ).strip()
            if not re.search(
                r"(?<![0-9.])" + re.escape(version) + r"(?![0-9.])", binary_version
            ):
                raise release.ReleaseError(
                    "Secure native binary version disagrees with candidate"
                )
            release.verify_versions(source, release.cargo_metadata(source))
            if release.git(source, "rev-parse", "HEAD").strip() != sha or release.git(
                source,
                "diff",
                "--exit-code",
                sha,
                "--",
                *(
                    [":(exclude)scripts/build-release-static.sh"]
                    if injected_helper
                    else []
                ),
            ):
                raise release.ReleaseError("Secure build changed selected source")
            public = public_key_from_source(source)
            for name in REQUIRED:
                binary = source / "build" / name
                if not binary.is_file() or binary.is_symlink():
                    raise release.ReleaseError("Missing secure build artifact: " + name)
                destination = output / name
                destination.write_bytes(binary.read_bytes())
                destination.chmod(binary.stat().st_mode & 0o777)
                sign(destination, args.key, public)
            for name in ("madmail-linux-arm", "madmail-tray-windows-amd64.exe"):
                binary = source / "build" / name
                if binary.is_file():
                    destination = output / name
                    destination.write_bytes(binary.read_bytes())
                    destination.chmod(binary.stat().st_mode & 0o777)
                    if name.startswith("madmail-linux-"):
                        sign(destination, args.key, public)
            notes, short, changes = notes_data(root, version, sha, base, stable)
            dependencies = {
                name: digest(source / name)
                for name in ("Cargo.lock", "package-lock.json")
            }
            manifest = {
                "version": version,
                "source_commit": sha,
                "candidate": args.candidate,
                "channel": "stable",
                "previous_stable": base,
                "dependencies": dependencies,
                "build_command": args.build_command,
                "test_command": args.test_command,
                "publisher_commit": release.git(root, "rev-parse", "HEAD").strip(),
                "build_helper_sha256": digest(helper) if injected_helper else None,
                "submodules": release.git(
                    source, "submodule", "status", "--recursive"
                ).splitlines(),
                "public_key": public.hex(),
                "changes": changes,
                "telegram": short,
                "notes": notes,
                "rustc": release.run(source, ["rustc", "--version"]).strip(),
            }
            archive_binaries(output)
            # Write the resumable state only after all immutable artifact bytes exist.
            manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
            write_checksums(output, list(output.iterdir()), "binary-checksums.txt")
        finally:
            # Git refuses removing a worktree with populated submodules.
            release.git(source, "submodule", "deinit", "--force", "--all")
            release.git(root, "worktree", "remove", "--force", str(source))
    # Verify every server signature again on resume as well as immediately after signing.
    public_source = release.git(root, "show", f"{sha}:crates/chatmail/src/upgrade.rs")
    public = bytes.fromhex(
        re.search(r'const PUBLIC_KEY_HEX: &str = "([0-9a-f]{64})";', public_source)[1]
    )
    for name in (*REQUIRED, "madmail-linux-arm"):
        if name in REQUIRED or (output / name).exists():
            verify(output / name, public)
    verify_archives(output)
    notes_path = output / "notes.md"
    notes_path.write_text(manifest["notes"])
    refs = release.git(
        root, "ls-remote", "--tags", "origin", f"refs/tags/{stable}"
    ).strip()
    if not refs:
        local = release.git(root, "tag", "--list", stable).strip()
        if local:
            if release.git(root, "rev-parse", f"{stable}^{{commit}}").strip() != sha:
                raise release.ReleaseError("Conflicting local stable tag")
        else:
            release.git(
                root,
                "-c",
                "tag.gpgsign=false",
                "tag",
                "-a",
                stable,
                sha,
                "-m",
                f"Promote {args.candidate}",
            )
        release.git(root, "push", "origin", f"refs/tags/{stable}:refs/tags/{stable}")
    github.exact_tag(stable, sha)
    if not existing:
        new_draft(github, stable, sha, notes_path, False)
    for name in sorted(
        verify_checksums(output, "binary-checksums.txt") | {"binary-checksums.txt"}
    ):
        github.upload(stable, output / name)
    print(
        "Draft staged. Run Windows stable-setup workflow with candidate "
        + args.candidate
        + ", then finalize."
    )


def finalize(root, github, args):
    version, sha, stable, base, existing = stable_plan(root, github, args.candidate)
    if not existing:
        raise release.ReleaseError("Stage the stable draft before finalizing")
    print(json.dumps({"tag": stable, "sha": sha, "write": args.write}))
    if not args.write:
        return
    with tempfile.TemporaryDirectory(dir=root.parent) as temporary:
        output = Path(temporary)
        github.gh("release", "download", stable, "--repo", REPO, "--dir", str(output))
        expected = verify_checksums(output, "binary-checksums.txt")
        manifest = json.loads((output / "release.json").read_text())
        if (
            manifest["source_commit"] != sha
            or manifest["version"] != version
            or manifest["candidate"] != args.candidate
            or manifest["previous_stable"] != base
        ):
            raise release.ReleaseError("Draft provenance disagrees with promotion")
        source = release.git(root, "show", f"{sha}:crates/chatmail/src/upgrade.rs")
        public = bytes.fromhex(
            re.search(r'const PUBLIC_KEY_HEX: &str = "([0-9a-f]{64})";', source)[1]
        )
        for name in REQUIRED:
            if name not in expected:
                raise release.ReleaseError("Missing required signed artifact: " + name)
            verify(output / name, public)
        if "madmail-linux-arm" in expected:
            verify(output / "madmail-linux-arm", public)
        verify_archives(output)
        # A single uploaded ZIP makes interrupted installer uploads resumable.
        package = output / "setup-package.zip"
        with zipfile.ZipFile(package) as archive:
            allowed_entries = {SETUP, "setup-source.json", "setup-checksums.txt"}
            if (
                set(archive.namelist()) != allowed_entries
                or len(archive.namelist()) != 3
            ):
                raise release.ReleaseError("Unexpected installer package entries")
            for name in allowed_entries:
                content = archive.read(name)
                destination = output / name
                if destination.exists() and destination.read_bytes() != content:
                    raise release.ReleaseError("Conflicting installer asset: " + name)
                destination.write_bytes(content)
        setup_names = verify_checksums(output, "setup-checksums.txt")
        if setup_names != {SETUP, "setup-source.json"}:
            raise release.ReleaseError("Unexpected installer checksum entries")
        receipt = json.loads((output / "setup-source.json").read_text())
        if receipt != {
            "source_commit": sha,
            "server_sha256": digest(output / "madmail-windows-amd64.exe"),
        }:
            raise release.ReleaseError("Installer source artifact disagrees")
        for name in (SETUP, "setup-source.json", "setup-checksums.txt"):
            github.upload(stable, output / name)
        paths = [
            path
            for path in output.iterdir()
            if path.is_file() and path.name != "SHA256SUMS"
        ]
        names = {path.name for path in paths}
        allowed = expected | {
            SETUP,
            "binary-checksums.txt",
            "setup-checksums.txt",
            "setup-source.json",
            "setup-package.zip",
        }
        if names != allowed:
            raise release.ReleaseError("Unexpected or missing draft assets")
        write_checksums(output, paths, "SHA256SUMS")
        github.upload(stable, output / "SHA256SUMS")
        # Re-check history immediately before publication; do not advance latest backwards.
        previous_stable(root, github, sha, version)
        github.gh(
            "release",
            "edit",
            stable,
            "--repo",
            REPO,
            "--draft=false",
            "--prerelease=false",
            "--latest",
        )
        print(
            "Stable release published. No Telegram message is sent automatically; use release.json telegram once."
        )


def announce(root, github, args):
    if not re.fullmatch(
        r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", args.tag
    ):
        raise release.ReleaseError("Expected a stable version tag")
    current = github.get(args.tag)
    if not current or current["draft"] or current["prerelease"]:
        raise release.ReleaseError("Announce only a published stable release")
    claim_name = "telegram-announcement.json"
    if any(asset["name"] == claim_name for asset in current["assets"]):
        print(
            "Announcement already claimed; skipping to prevent a duplicate. Inspect Telegram if the previous run was interrupted."
        )
        return
    with tempfile.TemporaryDirectory(dir=root.parent) as temporary:
        directory = Path(temporary)
        manifest_path = github.download(args.tag, "release.json", directory)
        checksums = github.download(args.tag, "SHA256SUMS", directory).read_text()
        expected = next(
            (
                line.split("  ")[0]
                for line in checksums.splitlines()
                if line.endswith("  release.json")
            ),
            None,
        )
        if digest(manifest_path) != expected:
            raise release.ReleaseError("Announcement provenance checksum mismatch")
        data = json.loads(manifest_path.read_text())
        github.exact_tag(args.tag, data["source_commit"])
        if data["version"] != args.tag[1:] or data["channel"] != "stable":
            raise release.ReleaseError("Announcement version mismatch")
        if not args.write:
            print(data["telegram"])
            return
        token = os.environ.get("TELEGRAM_BOT_TOKEN", "")
        if not re.fullmatch(r"[0-9]+:[A-Za-z0-9_-]+", token):
            raise release.ReleaseError(
                "Set TELEGRAM_BOT_TOKEN in the secure environment"
            )
        claim = directory / claim_name
        claim.write_text(
            json.dumps(
                {
                    "tag": args.tag,
                    "source_commit": data["source_commit"],
                    "channel": args.channel,
                }
            )
            + "\n"
        )
        # Claim BEFORE sending: an ambiguous HTTP failure cannot cause duplicate sends.
        # An exclusive, non-clobber upload is the cross-process announcement lock.
        github.gh("release", "upload", args.tag, str(claim), "--repo", REPO)
        payload = json.dumps(
            {
                "chat_id": args.channel,
                "text": f"Madmail {args.tag}\n\n{data['telegram']}\n\n{release.REPOSITORY}/releases/tag/{args.tag}",
            }
        ).encode()
        request = urllib.request.Request(
            "https://api.telegram.org/bot" + token + "/sendMessage",
            data=payload,
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                result = json.load(response)
            if not result.get("ok"):
                raise ValueError("Telegram rejected announcement")
        except Exception as exc:
            # API URL embeds the token; do not print urllib diagnostics.
            raise release.ReleaseError(
                "Announcement outcome uncertain; claim retained. Inspect Telegram before manual recovery."
            ) from exc
        receipt = directory / "telegram-announcement-sent.json"
        receipt.write_text(
            json.dumps({"tag": args.tag, "message_id": result["result"]["message_id"]})
            + "\n"
        )
        github.upload(args.tag, receipt)
        print("Stable announcement sent; retry will not send another message.")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--write", action="store_true")
    sub = parser.add_subparsers(dest="action", required=True)
    for action in ("stage", "finalize"):
        command = sub.add_parser(action)
        command.add_argument("--candidate", required=True)
        if action == "stage":
            command.add_argument("--key", type=Path)
            command.add_argument("--output", type=Path)
            command.add_argument("--build-command", default="make init build-publish")
            command.add_argument(
                "--test-command", default="cargo test --workspace --locked"
            )
    command = sub.add_parser("unstable")
    command.add_argument("--candidate", required=True)
    command.add_argument("--artifacts", type=Path, required=True)
    command = sub.add_parser("announce")
    command.add_argument("--tag", required=True)
    command.add_argument("--channel", required=True)
    args = parser.parse_args(argv)
    try:
        root = args.repo.resolve()
        github = GitHub(root)
        with release.release_lock(root):
            if args.action == "unstable":
                unstable(root, github, args.candidate, args.artifacts, args.write)
            elif args.action == "stage":
                stage(root, github, args)
            elif args.action == "finalize":
                finalize(root, github, args)
            else:
                announce(root, github, args)
        return 0
    except (release.ReleaseError, OSError, ValueError, KeyError) as exc:
        print(
            "publish: "
            + (
                str(exc)
                if isinstance(exc, release.ReleaseError)
                else type(exc).__name__
            )
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
