# Standalone release automation and release channels

## Contract

`scripts/release.py` uses Python 3.11+ standard-library modules, Git and Cargo.
It retains automatic Conventional Commit versioning: `feat` is minor;
`fix`, `perf`, and `revert` are patch; `!` or a breaking-change trailer is major.
Other commits create no release unless breaking. A revert cancels an original
within the analyzed range. Existing versions/tags are never rewritten.

Release-worthy main merges synchronize `.version`, workspace/package manifests,
Cargo workspace lock entries, root npm manifests, and the changelog. Manifest
versions remain numeric. The generated annotated tag is
`vMAJOR.MINOR.PATCH-unstable`; no automatic merge creates a stable tag. The
latest reachable numeric stable or unstable tag is the next versioning baseline,
with the unstable tag preferred when both refer to the same numeric version.
Older beta/other prerelease tag formats are ignored. A fresh repository starts at
1.0.0; manifest downgrades and conflicting tags are refused.

CI builds unsigned Linux amd64 and Windows amd64 testing binaries from the exact
prepared commit. After the build/tests pass, it publishes a GitHub prerelease
with checksums, source metadata, and a prominent testing-only warning about
regressions, incompatible changes, and unsupported upgrade/downgrade paths.
No production private signing key is used by any Actions job. Non-release-worthy
merges still run validation but publish neither a new prerelease nor Docker tag.

Docker uses immutable numeric distribution tags and distinct moving aliases:

| Channel | Version tag | Moving tag | Publication trigger |
| --- | --- | --- | --- |
| Unstable | `2.31.0-unstable` | `unstable` | successful testing prerelease |
| Stable | `2.31.0` | `latest` | completed published stable GitHub Release |

The registry check permits a new build only on HTTP 404; authentication and
network failures abort. An existing version image must have the expected source
revision label; it is
reused, never deliberately rebuilt over the tag. The unstable alias advances
only for the newest versioned main build. The stable alias advances only for the
current latest stable GitHub Release. Retrying an older run cannot move either
alias backwards. Stable Docker verifies the publication's source tag, candidate
tag, checksum metadata and lockfile digests, then builds that selected revision,
not the newest main commit. Registry credentials are scoped to the Docker jobs.

## Version preparation

```bash
python3 scripts/release.py --dry-run
python3 scripts/release.py --check
python3 scripts/release.py --write --push
python3 scripts/release.py --write --push --sign
```

Dry-run is the default. `--check` validates current manifests without analysis.
Writes require full history, a clean index/worktree, and the main branch. Actual
Cargo workspace membership determines lockfile updates; registry/git dependency
versions are untouched. Cargo metadata validates changes offline with `--locked`.
Preparation is POSIX-only, using a common Git-directory advisory lock.

Default commit/tag identity is `Madmail-CI <ci@madmail.chat>`, overridable with
validated `--name`/`--email`; no global Git configuration changes. The prepared
commit is `chore(release): VERSION [skip ci]`. `--sign` signs the Git commit and
annotated unstable tag with the configured local signer. Actions does not pass
`--sign`, preserving its unsigned Git-ref policy. This is separate from the
mandatory Ed25519 signature on production binaries.

Branch/tag updates use compare-and-swap, atomic non-forced pushes, and rollback
on preparation errors. A rejected push leaves a resumable local release. Retry
`--write --push` in that same clean checkout; the existing candidate is emitted
without another version/changelog entry. CI uses the emitted exact SHA and tag
for downstream jobs. The tool rejects unsafe/symlinked release paths, dirty or
shallow analysis, manifest mismatches, and malformed versions/identities. Commit
messages are data, subprocesses never invoke a shell, and Git transport errors
omit stderr that could contain credentials.

## Manual stable promotion

The secure publisher selects an **existing published unstable prerelease**.
It may be earlier than the latest main version: for example, select
`v2.31.0-unstable` even when main has reached `v2.31.1-unstable`. Stable `v2.31.0`
points to the candidate's exact source commit. No new main commit/version is
created by promotion. A candidate older than/equal to the previous published
stable version is refused; main may continue advancing independently.

Use the current publishing tooling from a trusted local checkout with full tags.
`uv`, Python 3.11+, Git, Cargo, `gh`, and the existing secure cross-build toolchains
are needed. The wrapper pins `cryptography` for signature operations. GitHub
credentials must allow releases, tags and manual workflow dispatch; keys remain
in the secure environment. The script does not read/copy a project `.env` into
candidate worktrees or CI. `make publish` forwards `PUBLISH_ARGS`; it does not
build current main before selecting the candidate.

```bash
git fetch origin --tags

# Inspect the selected source and previous published stable comparison (no writes).
./scripts/publish.sh stage --candidate v2.31.0-unstable

# On the secure build machine: checkout exact candidate, build/test, sign/verify,
# create the exact stable tag, then upload signed binaries/archives to a DRAFT.
./scripts/publish.sh --write stage \
  --candidate v2.31.0-unstable --key /secure/private_key.hex

# The default build/test commands are:
#   make init build-publish
#   cargo test --workspace --locked
# Explicit alternatives: --build-command '...' --test-command '...'
# These are trusted argument lists parsed with shlex, never executed through a shell.

# Package Windows from the signed draft executable, without rebuilding that server.
gh workflow run windows.yml --ref main -f candidate=v2.31.0-unstable

# After Windows setup succeeds: download/verify ALL draft artifacts, then publish.
./scripts/publish.sh --write finalize --candidate v2.31.0-unstable
```

The stable workflow must already be present on the repository's default branch.
It gets its public verification helper from the workflow dispatch revision,
checks out the selected exact source, downloads the signed server, verifies its
Ed25519 trailer using that source's updater public key, checks its SHA256 and
runs its version command. Inno Setup consumes that exact executable. There is
no release-event server rebuild, no `--clobber`, and no upload of a replacement
portable executable. The workflow never publishes the draft itself.

Secure staging runs in a detached disposable worktree, initializes submodules,
uses the existing build targets, tests the candidate and checks the native
server version. Older candidates without the local static-build helper use the
trusted publisher's helper, whose digest is recorded. Selected tracked source
changes fail the build. Server signatures are cryptographically verified after
signing and on every retry; an already valid trailer is not appended twice.
The private key must match the public key embedded in the selected source.
Executable modes are preserved in archives. Installer/tray PEs receive no
Ed25519 trailer, preserving the existing packaging convention.

`release.json` records the source commit, candidate, numeric version, previous
published stable, dependency lock hashes, submodule revisions, public key,
publisher commit, build/test commands, Rust toolchain and shared change data.
Detailed GitHub notes cover the previous **published stable GitHub Release** to
the selected candidate, rather than a local tag or unstable release. Unstable
notes cover the preceding versioned build. Short Telegram text uses the same
change data; neither notes renderer calculates versions.

The draft contains `binary-checksums.txt` for the secure binaries, archives and
provenance. Windows uploads one `setup-package.zip` containing the installer,
source/server-hash receipt and installer checksums. Finalization validates that
package, verifies every required server signature, checks that archive payloads
match the exact signed binaries, uploads individual installer files and final
`SHA256SUMS`, then makes the complete stable release public. Stable Docker follows
that publication event, or can be retried explicitly:

```bash
gh workflow run stable-docker.yml --ref main -f tag=v2.31.0
```

## Retry and announcements

All publishing commands are read-only until the explicit global `--write` flag.
Stable staging stores immutable artifacts in `build/publish-vVERSION/` (override
with `--output`). After an interrupted network upload, repeat the same stage
command: verify and reuse local signed bytes rather than rebuild/re-sign. An
existing same-name asset is accepted only if its downloaded bytes match. A
conflict aborts; no published binary/asset/tag is replaced. A build/process crash
before the completed local manifest/checksums may leave an incomplete output
folder or source worktree; inspect and remove those disposable paths before
rebuilding. Do not discard a completed upload's local artifacts.

Retrying the Windows workflow reuses and verifies an already uploaded installer
package. Retry finalization after an interrupted individual asset upload; the
existing bytes are checked, and the release remains draft until complete.
Already published/conflicting stable versions are refused. For an interrupted
final publication, inspect GitHub: if already public, only retry downstream
Docker/announcement steps. No GitHub Release or Telegram message is published as
part of a PR test or local dry-run.

Announcements are an explicit optional final step, never part of automatic CI
or stable finalization. This also avoids duplicate announcements on publication
retries. Configure `TELEGRAM_BOT_TOKEN` only in the secure local environment:

```bash
./scripts/publish.sh announce --tag v2.31.0 --channel @your_release_channel
./scripts/publish.sh --write announce --tag v2.31.0 --channel @your_release_channel
```

An exclusive non-clobber `telegram-announcement.json` asset claims the send before
the Bot API call; a subsequent run skips it. A successful send adds a message-ID
receipt. If the HTTP outcome is uncertain, the claim remains: inspect Telegram
before deliberate manual recovery. This gives at-most-once automatic sending,
not an impossible exactly-once guarantee across GitHub and Telegram. API errors
never print token-bearing URLs. Announcement metadata is added separately from
the immutable binary checksum set. The previous local FTP/default-credential
and automatic binary-broadcast flow is not invoked by this secure publisher;
maintainers can mirror verified published assets separately.

## Existing updater policy

No updater/channel switching is introduced. `madmail upgrade latest` continues
using GitHub's latest stable release, which excludes prereleases. Existing
mandatory Ed25519 verification and rollback/version manager behavior are
unchanged. Unsigned unstable downloads are for manual isolated testing and will
be rejected by the production upgrade mechanism.

## Validation

```bash
uv run --with cryptography==46.0.3 python -m unittest discover -s scripts -p 'test_*.py' -v
```

Tests use disposable real Git/Cargo repositories, ephemeral test Ed25519 keys,
and a fake GitHub transport. They cover version rules, signature/tag signing,
failed atomic pushes, older-candidate promotion, stable comparison ranges,
manifest/source mismatch, signing-key mismatch, tampered signatures/checksums,
archive payload verification, asset conflicts, resumable draft/final uploads,
installer traversal rejection, backward-stable rejection, dry runs and
at-most-once announcements (including ambiguous HTTP failure/token redaction).
Production keys, real GitHub publication, real Telegram sends and Windows Inno
execution are not part of these local tests; Windows execution belongs to its
manual draft workflow.
