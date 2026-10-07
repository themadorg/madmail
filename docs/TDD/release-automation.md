# Standalone release automation

## Contract and migration

`scripts/release.py` replaces the former release analyzer, release-note,
changelog, exec and Git plugins. It uses only Python 3.11+ standard-library
modules and subprocess argument lists for Git and Cargo. There is no Node,
pip dependency, runtime daemon, GitHub API client or publishing side effect.
The old configuration and root release-only npm dependencies are removed;
landing and admin UI tooling is independent and retained. Existing bot commits
and tags are not rewritten. The first CI run continues from the latest reachable
stable `vMAJOR.MINOR.PATCH` tag already in the repository.

Rules match the configured Angular/Conventional Commit defaults: `feat` minor,
`fix`, `perf` and `revert` patch, `BREAKING CHANGE:`/`BREAKING-CHANGE:` or `!`
major, other types no release unless breaking. The highest applicable change
wins. A commit reverted within the analyzed range cancels with its revert;
reverting a previous release is a patch. Notes use Features, Bug Fixes,
Performance Improvements, Reverts and BREAKING CHANGES sections, compare/commit
links and UTC dates. Wording/Markdown layout may differ from the Node renderer;
versioning and release side effects follow the project's former configuration.
There are no prerelease branches or custom plugin rules in this repository.
A repository without a release tag starts at 1.0.0, like the previous tool;
manifest downgrades are refused rather than guessing a migration baseline.

The six root release files are synchronized. Cargo's actual workspace members
(not a name prefix) determine path package lock updates, including explicit
member versions. Registry/git dependency versions are untouched. Cargo metadata
with `--no-deps --offline --locked` validates the result. The existing lockfile
must be usable locally; initial dependency/toolchain setup belongs to CI.

## Commands and identity

```
python3 scripts/release.py --dry-run
python3 scripts/release.py --check
python3 scripts/release.py --write
python3 scripts/release.py --write --push
python3 scripts/release.py --write --push --sign
python3 -m unittest discover -s scripts -p test_release.py -v
```

Dry-run is the default; it reports JSON including version, files and notes. It
never changes tracked files, commits, tags, remotes or publication state.
`--check` validates existing versions without calculating a new release.
`--write` is main-only and requires a clean index/worktree and full history.
`--push` requires `--write` and atomically pushes main and the new tag to origin,
without force. The tool is POSIX (Linux/macOS), using `flock` to serialize
cooperating invocations across worktrees. Windows binary builds consume the
prepared version; Windows is not a release-preparation runner.

Defaults are **Madmail-CI <ci@madmail.chat>** for author, committer and tagger,
overridable with validated `--name`/`--email`. They do not alter global Git
configuration. Release commits use `chore(release): <version> [skip ci]`.
CI checks out current main under a release concurrency group, emits version and
exact SHA through `$GITHUB_OUTPUT`, and uploads `.version`. PR release generation
is skipped; tooling tests and consistency validation still run. Build and Docker
jobs use the exact emitted SHA, falling back to the triggering SHA on PRs.

## Signing and credentials

Preparation requires no network credential. Push uses Git's existing origin
credentials; CI grants `contents: write` only to the main-push release job via
`GITHUB_TOKEN`. It does not load secrets on `pull_request_target` or from forks.
No PAT, Telegram, FTP or binary-signing key is needed for this tool.

`--sign` enables Git commit and annotated-tag signing with the configured
`gpg.format`, `user.signingkey` and signer. Missing signing keys fail the write
and trigger rollback. CI intentionally does not pass `--sign` until a dedicated
release signing key and noninteractive signer are provisioned through protected
secrets. Unsigned CI release refs preserve the previous CI policy; the script
explicitly disables implicit signing when the flag is absent. Maintainers can
prepare signed release refs locally using their approved signing setup.
Binary signing/upload remains the role of local `scripts/publish.sh` and its
helpers, consuming the already-prepared version and tag. Never use an AI notes
helper to calculate versions or modify release refs.

## Failure recovery and security review

The tool rejects dirty/shallow checkouts, missing/untracked release files,
symlinked paths and manifests outside the repository. Version and identity
validation rejects control characters and option/command injection. Commit
messages are data, passed through stdin; subprocesses never invoke a shell.
Markdown descriptions are escaped to prevent raw HTML/link injection. Git hooks
are disabled; alternate Git index/worktree environment variables are removed.
Transport errors report operation and exit code without logging credential URLs
or subprocess stderr. The repository, Git/Cargo executables, Cargo configuration
and local Git signing configuration are trusted inputs, not a sandbox for
unreviewed repositories. Do not run a write on untrusted code or configuration.

Writes are atomic per file, backed by original files/index. Validation,
commit or tag failures restore them and the release branch if safe. Ref rollback
uses an expected HEAD to avoid discarding another writer's commit. A filesystem
or process crash can leave preparation incomplete; inspect/reset only the six
release files (and any explicit-version workspace manifests) in a disposable
checkout before retrying. The tool does not claim power-loss atomicity across
multiple files.

Push is atomic and non-forced: a competing branch/tag or server rejection leaves
remote refs untouched. On push failure the prepared local commit/tag remain.
Retry `--write --push` from that same clean checkout to publish those refs
without duplicating changelog entries. If another main commit won, discard the
unpublished preparation in a disposable checkout, fetch the winning main/tags,
and recalculate; never force-push or delete a published release tag.

GitHub concurrency serializes release jobs; the local advisory lock rejects
simultaneous tool invocations. External Git writers must not modify the checkout
while preparation runs. The script does not publish GitHub Releases or binaries:
those were not part of the removed plugin configuration either.
