#!/usr/bin/env python3
"""Prepare Conventional Commit releases using Python 3.11+, Git and Cargo only."""

import argparse
import contextlib
import dataclasses
import datetime
import fcntl
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import tomllib

VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
HEADER = re.compile(r"^(\w+)(?:\(([^\r\n)]+)\))?(!)?: (.+)$")
BREAKING = re.compile(r"^BREAKING[ -]CHANGE:\s*(.*)$", re.MULTILINE)
FILES = (
    ".version",
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "package-lock.json",
    "CHANGELOG.md",
)
REPOSITORY = "https://github.com/themadorg/madmail"


class ReleaseError(Exception):
    """An expected validation or release failure safe to display in CI."""


def run(root, args, *, env=None, input_text=None):
    """Never execute commit text, versions or credentials through a shell."""
    child_env = os.environ.copy()
    # An inherited alternate index/worktree must not redirect release writes.
    for key in ("GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_DIR"):
        child_env.pop(key, None)
    child_env.update(env or {})
    result = subprocess.run(
        args,
        cwd=root,
        env=child_env,
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        # Git transport diagnostics can contain credential-bearing remote URLs.
        raise ReleaseError(f"{args[0]} {args[1]} failed (exit {result.returncode})")
    return result.stdout


def git(root, *args, env=None, input_text=None):
    return run(
        root,
        ["git", "-c", "core.hooksPath=/dev/null", *args],
        env=env,
        input_text=input_text,
    )


def version_tuple(value):
    if not VERSION.fullmatch(value):
        raise ReleaseError("Expected a stable MAJOR.MINOR.PATCH version")
    return tuple(map(int, value.split(".")))


def bump(version, level):
    major, minor, patch = version_tuple(version)
    if level == 3:
        return f"{major + 1}.0.0"
    if level == 2:
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


@dataclasses.dataclass(frozen=True)
class Commit:
    sha: str
    message: str


@dataclasses.dataclass(frozen=True)
class Change:
    sha: str
    kind: str
    scope: str
    description: str
    breaking: str
    level: int
    references: tuple = ()


def changes_from_commits(commits):
    """Angular defaults: feat minor; fix/perf/revert patch; breaking major."""
    cancelled = set()
    # Match real Git revert trailers; do not suppress an arbitrary same subject.
    for commit in reversed(commits):
        if commit.sha in cancelled:
            continue
        match = re.search(
            r"^This reverts commit ([0-9a-f]{7,40})\.$", commit.message, re.MULTILINE
        )
        if match:
            original = next((c for c in commits if c.sha.startswith(match[1])), None)
            if original:
                cancelled.update((original.sha, commit.sha))
    changes = []
    for commit in commits:
        if commit.sha in cancelled:
            continue
        subject, _, body = commit.message.partition("\n")
        match = HEADER.match(subject)
        revert = subject.startswith('Revert "') and "This reverts commit " in body
        breaking_match = BREAKING.search(body)
        if not match and not revert and not breaking_match:
            continue
        kind, scope, bang, description = (
            match.groups() if match else ("revert" if revert else "", "", "", subject)
        )
        breaking = breaking_match[1].strip() if breaking_match else ""
        if breaking_match:
            remainder = body[breaking_match.end() :].splitlines()
            continuation = []
            for line in remainder:
                if not line.strip() and continuation:
                    break
                if re.match(r"^[A-Za-z][A-Za-z-]*(?::| #\d+)", line):
                    break
                if line.strip():
                    continuation.append(line.strip())
            breaking = " ".join([breaking, *continuation]).strip()
        if breaking_match and not breaking:
            breaking = description
        if bang and not breaking:
            breaking = description
        level = (
            3
            if breaking
            else {"feat": 2, "fix": 1, "perf": 1, "revert": 1}.get(kind, 0)
        )
        if level:
            changes.append(
                Change(
                    commit.sha,
                    kind,
                    scope or "",
                    description,
                    breaking,
                    level,
                    tuple(dict.fromkeys(re.findall(r"#([1-9]\d*)", commit.message))),
                )
            )
    return changes


def markdown_text(value):
    # Release notes are Markdown, never executable HTML or workflow output.
    value = re.sub(r"[\x00-\x1f\x7f]", " ", value)
    return re.sub(r"([\\`*_{}\[\]<>])", r"\\\1", value)


def release_notes(version, base, changes, date):
    compare = (
        f"{REPOSITORY}/compare/{base}...v{version}"
        if base
        else (f"{REPOSITORY}/releases/tag/v{version}")
    )
    notes = [f"# [{version}]({compare}) ({date})", ""]
    groups = (
        ("feat", "Features"),
        ("fix", "Bug Fixes"),
        ("perf", "Performance Improvements"),
        ("revert", "Reverts"),
    )
    for kind, title in groups:
        entries = [c for c in changes if c.kind == kind]
        if not entries:
            continue
        notes.extend((f"### {title}", ""))
        for change in sorted(entries, key=lambda c: (c.scope, c.description, c.sha)):
            scope = f"**{markdown_text(change.scope)}:** " if change.scope else ""
            references = ", ".join(
                f"[#{number}]({REPOSITORY}/issues/{number})"
                for number in change.references
            )
            suffix = f", {references}" if references else ""
            notes.append(
                f"* {scope}{markdown_text(change.description)} "
                f"([{change.sha[:7]}]({REPOSITORY}/commit/{change.sha})){suffix}"
            )
        notes.append("")
    breaking = [c for c in changes if c.breaking]
    if breaking:
        notes.extend(("### BREAKING CHANGES", ""))
        notes.extend(f"* {markdown_text(c.breaking)}" for c in breaking)
        notes.append("")
    return "\n".join(notes).rstrip() + "\n"


def prepend_changelog(text, version, notes):
    # Never append a second entry after an interrupted/manual preparation.
    if re.search(rf"^#{{1,2}} \[?{re.escape(version)}(?:\]|\s|$)", text, re.MULTILINE):
        raise ReleaseError("Changelog already contains the planned release")
    if text.startswith("# Changelog\n"):
        return "# Changelog\n\n" + notes + "\n" + text[len("# Changelog\n") :].lstrip()
    return notes + "\n" + text.lstrip()


def replace_toml_version(text, section, version):
    pattern = re.compile(
        rf"(^\[{re.escape(section)}\]\s*\n)(.*?)(?=^\[|\Z)", re.MULTILINE | re.DOTALL
    )
    matches = list(pattern.finditer(text))
    if len(matches) != 1:
        raise ReleaseError(f"Expected one [{section}] section")
    match = matches[0]
    body, count = re.subn(
        r'(?m)^version\s*=\s*"[^"\n]+"', f'version = "{version}"', match[2]
    )
    if count != 1:
        raise ReleaseError(f"Expected one version in [{section}]")
    return text[: match.start(2)] + body + text[match.end(2) :]


def read_file(root, relative):
    path = root / relative
    # Do not follow symlinks into credentials, another worktree or system files.
    if path.is_symlink() or root not in path.resolve().parents:
        raise ReleaseError(f"Unsafe release path: {relative}")
    if not path.is_file():
        raise ReleaseError(f"Missing release file: {relative}")
    if not git(root, "ls-files", "--", relative).strip():
        raise ReleaseError(f"Release file is not tracked: {relative}")
    return path.read_text(encoding="utf-8")


def cargo_metadata(root):
    metadata = json.loads(
        run(
            root,
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--offline",
                "--locked",
                "--format-version",
                "1",
            ],
        )
    )
    ids = set(metadata["workspace_members"])
    return [p for p in metadata["packages"] if p["id"] in ids]


def verify_versions(root, packages):
    data = {name: read_file(root, name) for name in FILES}
    version = data[".version"].strip()
    version_tuple(version)
    root_toml = tomllib.loads(data["Cargo.toml"])
    package = json.loads(data["package.json"])
    lock = json.loads(data["package-lock.json"])
    values = [
        root_toml["workspace"]["package"]["version"],
        package["version"],
        lock["version"],
        lock["packages"][""]["version"],
    ]
    values.extend(p["version"] for p in packages)
    cargo_lock = tomllib.loads(data["Cargo.lock"])
    for package in packages:
        entries = [
            p
            for p in cargo_lock["package"]
            if p["name"] == package["name"] and "source" not in p
        ]
        if len(entries) != 1:
            raise ReleaseError("Missing or ambiguous workspace lockfile package")
        values.append(entries[0]["version"])
    if any(value != version for value in values):
        raise ReleaseError("Release manifests or workspace lockfile versions disagree")
    return version, data


def planned_files(root, data, packages, version, notes):
    updates = dict(data)
    updates[".version"] = version + "\n"
    updates["Cargo.toml"] = replace_toml_version(
        data["Cargo.toml"], "workspace.package", version
    )
    for package in packages:
        path = Path(package["manifest_path"])
        if root not in path.resolve().parents:
            raise ReleaseError("Workspace manifest is outside the repository")
        relative = path.relative_to(root).as_posix()
        text = read_file(root, relative)
        declaration = tomllib.loads(text)["package"]["version"]
        if not isinstance(declaration, dict) or not declaration.get("workspace"):
            data[relative] = text
            updates[relative] = replace_toml_version(text, "package", version)
    names = {p["name"] for p in packages}

    def update_lock(match):
        block = match[0]
        parsed = tomllib.loads(block)["package"][0]
        if parsed["name"] in names and "source" not in parsed:
            return re.sub(
                r'(?m)^version = "[^"\n]+"', f'version = "{version}"', block, count=1
            )
        return block

    updates["Cargo.lock"] = re.sub(
        r"(?ms)^\[\[package\]\].*?(?=^\[\[package\]\]|\Z)",
        update_lock,
        data["Cargo.lock"],
    )
    for name in ("package.json", "package-lock.json"):
        document = json.loads(data[name])
        document["version"] = version
        if name == "package-lock.json":
            document["packages"][""]["version"] = version
        updates[name] = json.dumps(document, indent=2, ensure_ascii=False) + "\n"
    updates["CHANGELOG.md"] = prepend_changelog(data["CHANGELOG.md"], version, notes)
    return updates


@contextlib.contextmanager
def release_lock(root):
    directory = Path(
        git(root, "rev-parse", "--path-format=absolute", "--git-common-dir").strip()
    )
    path = directory / "madmail-release.lock"
    with path.open("a") as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ReleaseError("Another release is running in this repository") from exc
        yield


def atomic_write(path, text):
    mode = path.stat().st_mode & 0o777
    fd, temporary = tempfile.mkstemp(prefix=".release-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def latest_release(root):
    tags = git(root, "tag", "--merged", "HEAD", "--list", "v*").splitlines()
    stable = [tag for tag in tags if VERSION.fullmatch(tag[1:])]
    return max(stable, key=lambda tag: version_tuple(tag[1:]), default=None)


def commits_since(root, tag):
    span = f"{tag}..HEAD" if tag else "HEAD"
    rows = git(root, "log", "--reverse", "--format=%H%x00%B%x00", span).split("\0")
    return [
        Commit(rows[i].strip(), rows[i + 1].strip())
        for i in range(0, len(rows) - 1, 2)
        if rows[i].strip()
    ]


def identity_env(name, email):
    if not name or re.search(r"[\x00-\x1f\x7f<>]", name):
        raise ReleaseError("Invalid release identity name")
    if re.search(r"[\x00-\x1f\x7f]", email) or not re.fullmatch(
        r"[^\s<>@]+@[^\s<>@]+", email
    ):
        raise ReleaseError("Invalid release identity email")
    return {
        "GIT_AUTHOR_NAME": name,
        "GIT_AUTHOR_EMAIL": email,
        "GIT_COMMITTER_NAME": name,
        "GIT_COMMITTER_EMAIL": email,
    }


def push_release(root, tag, head):
    # Atomic and non-forced: neither branch nor tag changes on a concurrent push.
    git(
        root,
        "push",
        "--atomic",
        "origin",
        f"{head}:refs/heads/main",
        f"refs/tags/{tag}:refs/tags/{tag}",
    )


def prepare(root, updates, originals, version, notes, head, identity, sign):
    tag = f"v{version}"
    index = Path(
        git(root, "rev-parse", "--path-format=absolute", "--git-path", "index").strip()
    )
    index_before = index.read_bytes() if index.exists() else None
    committed = None
    try:
        for name, text in updates.items():
            atomic_write(root / name, text)
        verify_versions(root, cargo_metadata(root))
        if git(root, "rev-parse", "HEAD").strip() != head:
            raise ReleaseError("HEAD changed during release preparation")
        git(root, "add", "--", *updates)
        message = f"chore(release): {version} [skip ci]\n\n{notes}"
        tree = git(root, "write-tree").strip()
        commit_args = [
            "-c",
            f"commit.gpgsign={'true' if sign else 'false'}",
            "commit-tree",
            tree,
            "-p",
            head,
            "-F",
            "-",
        ]
        if sign:
            commit_args.append("-S")
        candidate = git(root, *commit_args, env=identity, input_text=message).strip()
        # Compare-and-swap the branch: an intervening writer is never overwritten.
        git(root, "update-ref", "refs/heads/main", candidate, head)
        committed = candidate
        git(
            root,
            "-c",
            f"tag.gpgsign={'true' if sign else 'false'}",
            "tag",
            "-s" if sign else "-a",
            tag,
            "-m",
            f"Release {version}",
            env=identity,
        )
    except BaseException:
        # Do not delete/overwrite a competing tag or undo an unrelated HEAD.
        if committed:
            git(root, "update-ref", "refs/heads/main", head, committed)
        for name, text in originals.items():
            atomic_write(root / name, text)
        if index_before is not None:
            index.write_bytes(index_before)
        elif index.exists():
            index.unlink()
        raise
    return committed


def release(
    root,
    *,
    write=False,
    push=False,
    sign=False,
    name="Madmail-CI",
    email="ci@madmail.chat",
    check=False,
):
    identity = identity_env(name, email)
    if push and not write:
        raise ReleaseError("--push requires --write")
    if (
        not check
        and git(root, "rev-parse", "--is-shallow-repository").strip() != "false"
    ):
        raise ReleaseError("Fetch full history and tags before releasing")
    if git(root, "status", "--porcelain", "--untracked-files=normal").strip():
        raise ReleaseError("Release requires a clean worktree and index")
    with release_lock(root):
        if git(root, "status", "--porcelain", "--untracked-files=normal").strip():
            raise ReleaseError("Release requires a clean worktree and index")
        if write and git(root, "branch", "--show-current").strip() != "main":
            raise ReleaseError("Release writes are allowed only on main")
        head = git(root, "rev-parse", "HEAD").strip()
        current, data = verify_versions(root, cargo_metadata(root))
        tag = latest_release(root)
        if check:
            return {"version": current, "sha": head, "released": False, "check": True}
        changes = changes_from_commits(commits_since(root, tag))
        level = max((change.level for change in changes), default=0)
        if not level:
            # Recover an atomic push failure without creating another commit/tag.
            if (
                push
                and tag
                and git(root, "rev-parse", f"{tag}^{{commit}}").strip() == head
            ):
                expected = f"chore(release): {current} [skip ci]"
                if (
                    tag != f"v{current}"
                    or git(root, "log", "-1", "--format=%s").strip() != expected
                ):
                    raise ReleaseError("HEAD is not a prepared release")
                push_release(root, tag, head)
            return {"version": current, "sha": head, "released": False}
        version = bump(tag[1:], level) if tag else "1.0.0"
        # Permit the first release only in a fresh 0.x/1.0.0 repository.
        if version_tuple(version) < version_tuple(current):
            raise ReleaseError("Planned version would downgrade the manifests")
        if git(root, "tag", "--list", f"v{version}").strip():
            raise ReleaseError("Planned release tag already exists")
        date = datetime.datetime.now(datetime.timezone.utc).date().isoformat()
        notes = release_notes(version, tag, changes, date)
        updates = planned_files(root, data, cargo_metadata(root), version, notes)
        result = {
            "version": version,
            "sha": head,
            "released": True,
            "dry_run": not write,
            "tag": f"v{version}",
            "files": sorted(updates),
            "notes": notes,
        }
        if write:
            result["sha"] = prepare(
                root, updates, data, version, notes, head, identity, sign
            )
            if push:
                push_release(root, f"v{version}", result["sha"])
        return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--dry-run", action="store_true", help="default: report without release writes"
    )
    mode.add_argument(
        "--write", action="store_true", help="update, validate, commit and tag on main"
    )
    mode.add_argument(
        "--check", action="store_true", help="check existing manifest consistency"
    )
    parser.add_argument(
        "--push", action="store_true", help="atomically push prepared branch and tag"
    )
    parser.add_argument(
        "--sign", action="store_true", help="sign release commit and annotated tag"
    )
    parser.add_argument("--name", default="Madmail-CI")
    parser.add_argument("--email", default="ci@madmail.chat")
    parser.add_argument(
        "--github-output",
        type=Path,
        help="append version, sha and released job outputs",
    )
    args = parser.parse_args(argv)
    try:
        root = Path(git(args.repo, "rev-parse", "--show-toplevel").strip()).resolve()
        result = release(
            root,
            write=args.write,
            push=args.push,
            sign=args.sign,
            name=args.name,
            email=args.email,
            check=args.check,
        )
        print(json.dumps(result, indent=2))
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8") as handle:
                handle.write(
                    f"version={result['version']}\nsha={result['sha']}\n"
                    f"released={str(result['released']).lower()}\n"
                )
        return 0
    except (ReleaseError, OSError, ValueError, KeyError) as exc:
        # Do not include arbitrary subprocess stderr, environment values or URLs.
        message = str(exc) if isinstance(exc, ReleaseError) else type(exc).__name__
        print(f"release: {message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
