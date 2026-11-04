# Release automation

Public CI versions the repository and publishes **unsigned testing** builds. Stable binaries are built, signed and published using owner-maintained tooling kept outside this repository. This repository does not contain that tooling, signing keys, or registry credentials.

`madmail update` / `madmail upgrade` skip draft and prerelease GitHub Releases and still require the existing Ed25519 trailer. Unsigned unstable binaries are not accepted by that updater.

## Versioning

`scripts/release.py` runs on `main` after lint, tests, and cargo audit succeed. It looks at Conventional Commits since the highest merged `vX.Y.Z` or `vX.Y.Z-unstable` tag:

| Commits | Bump |
|---------|------|
| `fix:`, `perf:`, `revert:` | patch |
| `feat:` | minor |
| Breaking change (`!` or a `BREAKING CHANGE` footer) | major |
| `docs:`, `chore:`, `ci:`, `test:`, and other types | none |

The numeric manifests stay `X.Y.Z`. The `-unstable` suffix is only on the Git tag, GitHub prerelease, and container tags.

A release updates:

- `.version`
- `[workspace.package] version` in `Cargo.toml`
- workspace packages in `Cargo.lock`
- member manifests that do not inherit the workspace version
- `CHANGELOG.md`

The root `package.json` was only the semantic-release package and is not a version manifest. Landing and `external/madmail-admin-web` keep their own versions.

The version commit is `chore(release): X.Y.Z [skip ci]`, authored by `Madmail-CI <ci@madmail.chat>`. CI does not sign that commit or its annotated tag. Actions has no production signing key. A local `--sign` run can sign with the operator's own Git key; that is separate from binary signing.

The push is atomic and not forced. A rejected push leaves the remote branch and tags unchanged. The same local checkout can retry the push. A newer `main` tip is not overwritten; the later workflow cuts the release. Re-running a workflow does not add a second changelog heading or tag for the same version.

`python3 scripts/release.py` is a dry run. `--check` only compares the manifests. `--write --push` commits and pushes. Tests live in `scripts/test_release.py` and use temporary repositories.

## Unstable channel

When the version commit is created, CI builds one admin web from the `external/madmail-admin-web` gitlink recorded on that commit, then builds every server from that same commit and `Cargo.lock`. Each server must contain that admin-web generation, its numeric package version, and the pinned iroh-relay v0.35.0 binary for its architecture. The relay archives are the public `n0-computer/iroh` v0.35.0 assets. Their checksums in `scripts/unstable_assets.py` were computed from those files. The relay binaries are not executed, and a Linux relay is not embedded in a Windows or ARM server.

The prerelease uploads these 16 assets:

| Asset | Role |
|-------|------|
| `madmail-linux-amd64` | glibc server, built on the Ubuntu runner and executed there with `madmail version` |
| `madmail-linux-amd64.tar.gz` | one `madmail` member, mode `0755` |
| `madmail-linux-amd64-legacy` | fully static x86_64 glibc server (`cargo rustc -- -C target-feature=+crt-static`) |
| `madmail-linux-amd64-legacy.tar.gz` | one `madmail` member, mode `0755` |
| `madmail-linux-amd64-musl` | musl server |
| `madmail-linux-amd64-musl.tar.gz` | one `madmail` member, mode `0755` |
| `madmail-linux-arm64` | glibc arm64 server, not executed |
| `madmail-linux-arm64.tar.gz` | one `madmail` member, mode `0755` |
| `madmail-linux-arm64-musl` | musl arm64 server, not executed |
| `madmail-linux-arm64-musl.tar.gz` | one `madmail` member, mode `0755` |
| `madmail-windows-amd64.exe` | unsigned Windows server, executed with `madmail version` on the Windows runner |
| `madmail-windows-amd64.zip` | one `madmail.exe` member, mode `0755` |
| `madmail-tray-windows-amd64.exe` | unsigned tray, smoke-exited on the Windows runner |
| `madmail-windows-amd64-setup.exe` | unsigned Inno installer labeled testing. Publication does not run it. A pull request runs only the refusal check |
| `SOURCE.txt` | full source revision of the unstable tag |
| `SHA256SUMS` | sha256 of every other uploaded file, including archives, tray, installer, and `SOURCE.txt`; it does not list itself |

`madmail-linux-amd64-legacy` is a fully static x86_64 glibc PIE. CI runs `cargo rustc -p chatmail --release --locked --bin madmail -- -C target-feature=+crt-static` and rejects the file if it has an ELF interpreter, a shared-library `DT_NEEDED` entry, or no GNU ABI note. `scripts/build-release-static.sh` is not in this repository; the job inlines that public recipe instead of calling the script. `zig cc` is not used for this file. The workflow runs `madmail version` on the Ubuntu builder. That is not a Debian 12 boot.

Every server links the SQLite amalgamation inside the locked `libsqlite3-sys` 0.30.1 crate. The workspace `sqlx` 0.8.6 feature `sqlite` enables `sqlx-sqlite/bundled`, which supplies `SQLITE_ENABLE_COLUMN_METADATA` and `SQLITE_ENABLE_UNLOCK_NOTIFY`. The builds do not download a second SQLite or link the runner's system library. The musl jobs still use `cargo-zigbuild` so that bundled C is compiled for the musl target. The arm64 glibc job sets the aarch64 gcc as the C compiler and linker for the same reason. ARMv7 and Windows ARM64 are not part of this prerelease.

The publisher accepts exactly those eight binaries, rejects duplicate bytes, and creates the archives itself. A missing, extra, or mismatched file fails the job. Uploads do not use `--clobber`. A retry reuses matching bytes and fails if an existing asset differs. Publication runs only after the generic build, the tests, the admin-web build, and all six platform jobs succeed. A failed required job skips the prerelease, and the unstable image is still published only after that prerelease succeeds.

A pull request runs that same admin-web build and the same six platform recipes. Every job checks out one commit, `github.event.pull_request.head.sha`, and reads the numeric version, the recorded admin-web gitlink, and `Cargo.lock` from that commit. The build-only path does not accept a release tag or a caller-supplied version. Its last job requires every platform role to have succeeded, then stages the same 16 files with the publication packager and uploads them only as a workflow artifact. That job has read-only contents permission. It does not create a version commit, tag, or GitHub Release, and it does not log in to a registry. Main publication still resolves the unstable tag, checks that the tag points at the release commit, and waits for the whole matrix plus the generic build and tests.

After those jobs succeed, CI creates a GitHub **prerelease** `vX.Y.Z-unstable` whose notes start with:

> TESTING ONLY — NOT FOR PRODUCTION USE.
>
> These builds are unsigned, may contain regressions or incompatible changes, and do not have guaranteed upgrade or downgrade compatibility. They are not supported by the signed production updater.

The container tags for that same commit are:

| Tag | Meaning |
|-----|---------|
| `X.Y.Z-unstable` | Immutable. Created only when the registry returns HTTP 404. Reused when `org.opencontainers.image.revision` is this commit. Any other registry response fails the job. |
| `unstable` | Moving tag. Advanced only when this candidate is the highest `v*-unstable` tag on `main`. |

The unstable image is published only after that GitHub prerelease upload succeeds. A failed Linux build, Windows build, or prerelease upload publishes nothing. The image is built from the commit the `vX.Y.Z-unstable` tag resolves to. Non-release commits do not publish a prerelease or move either tag. `latest` is not updated here.

The moving `unstable` tag is chosen from `origin/main` after a fetch. A retry of an older commit sees a newer tag and leaves the alias where it is.

## Stable channel

Stable binaries are built, signed and published using owner-maintained tooling kept outside this repository.

When a **published, non-prerelease** GitHub Release `vX.Y.Z` appears, `.github/workflows/stable-docker.yml` builds `ghcr.io/themadorg/madmail:X.Y.Z` from the commit that Git tag resolves to. Annotated tags are peeled. The workflow checks out that commit, then checks that `.version` and `[workspace.package]` equal `X.Y.Z`.

A full 40-character SHA in the release's `targetCommitish` must match the tag. A branch or tag name in that field is accepted. Historical stable tags do not need a `vX.Y.Z-unstable` tag. When that unstable tag exists, it must resolve to the same commit.

The version image uses the same 404-or-matching-revision rule. `latest` moves only when this version is the highest published stable semver. An older retry does not move `latest` backward. A registry or authentication failure is not treated as a missing image.

The workflow does not read private provenance files. Checking those files stays with owner-maintained tooling kept outside this repository and does not change the public versioning script.

## Windows installers

`windows.yml` builds `madmail-windows-amd64-setup.exe` only for a published stable release `vX.Y.Z`. Unstable prereleases do not run that job. Development smoke tests, the arm64 compile, and the Linux Windows-crate job do not run on a release event, and the installer job does not need them. An older release that no longer compiles on the current runner can still be packaged from its published binaries.

Packaging tools come from the repository default branch: `scripts/windows_release.py`, `scripts/verify_signed_binary.py`, `packaging/windows/Madmail.iss`, and `packaging/windows/build-setup.ps1`. The selected release is a separate checkout. The job fetches the `vX.Y.Z` tag there, including annotated tags, and reads `.version` and `[workspace.package]` from that commit. An older release does not need to contain the packaging scripts. The job does not run helper scripts from the release commit, and it does not build either binary from the default branch.

It downloads `madmail-windows-amd64.exe` from that GitHub Release and checks:

- the Ed25519 trailer (last 64 bytes) against the public key in `crates/chatmail/src/upgrade.rs`
- `madmail version` against the tag
- `.version` and `[workspace.package]` at the tagged commit
- a full commit SHA in the release's `targetCommitish` when that field is a SHA

A branch or tag name in `targetCommitish` is accepted. Those verified bytes are the server file Inno Setup packages. The installer version is that same `X.Y.Z`.

It also downloads `madmail-tray-windows-amd64.exe` from the same release and stages it at the path `Madmail.iss` already expects. The stable compile passes `/DRequireTray=1`, so a missing tray file fails Inno Setup instead of producing an installer without tray shortcuts or autostart. The tray is not checked with the server Ed25519 trailer. The tray program has no public version command that this job can run.

When the tray asset has a `sha256` digest, or `SHA256SUMS` lists it, those values must match the downloaded bytes. A checksum shows that the bytes match what the release published. It does not prove which commit built the tray. `SOURCE.txt`, when the release has one, must still be the tag commit; that is the release's claimed source, not a separate attestation inside the tray file. If neither a digest nor a `SHA256SUMS` entry is present, the job logs that and stages the release asset anyway. A missing tray asset fails the job.

The job uploads `madmail-windows-amd64-setup.exe` only. An identical installer is reused. A different published installer fails the job. The portable server executable and the tray executable are not uploaded or replaced. The smoke-test artifact is not reused. This stable job does not pass `-Testing` and does not accept the unsigned prerelease binaries.

Unsigned `vX.Y.Z-unstable` releases build their own Windows server and tray, then compile `madmail-windows-amd64-setup.exe` with `-Testing`. Inno Setup 6 emits an x86 setup launcher. Verification checks that launcher (`0x14c`) separately from the amd64 server and tray. The installer is labeled `Madmail TESTING`, carries the numeric `X.Y.Z` version, and is uploaded with the other prerelease assets. It is not checked with the Ed25519 trailer and is not substituted for the stable installer.

The testing installer is not a side-by-side install. It has its own Inno application id and installs under `Program Files\MadmailTesting`, with `UsePreviousAppDir=no`, so Inno will not restore a stable installation directory. The server and tray programs still use the production service name `Madmail`, `ProgramData\Madmail`, and the `MadmailTray` autostart value. Before any file is copied, `InitializeSetup` aborts if the stable uninstall entry or any of those shared resources is present. The supported environment for a real install is a fresh disposable Windows VM. A later run on that same VM is refused as well, because the first run creates those resources. The stable installer does not include this refusal and keeps its original application id, service, directories, and autostart.

The pull-request Windows job compiles that testing installer from the server and tray built in the same job. It then starts that installer on the GitHub-hosted runner with synthetic stable uninstall, autostart, and `ProgramData\Madmail` markers. The command uses `/VERYSILENT`, `/SUPPRESSMSGBOXES`, and a 90 second timeout. The refusal calls `SuppressibleMsgBox`, which returns immediately under those switches. A plain `MsgBox` is not suppressed and would wait. Setup must return before creating `Program Files\MadmailTesting` or changing the sentinel files. That check does not complete an installation, start the service, or touch a maintainer machine. Publication does not run the installer. The stable workflow does not pass `-Testing` and does not run this check.

Owner-maintained tooling kept outside this repository has to attach both `madmail-windows-amd64.exe` (Ed25519 trailer) and `madmail-tray-windows-amd64.exe` (built for that same release) before this job can succeed. If it publishes `SHA256SUMS`, that file must list the tray as well as the server. This repository does not define a new publishing protocol for that step. Local `build-setup.ps1` runs still treat the tray as optional unless `-RequireTray` is passed, and they do not upload anything to a release.

## What CI will not do

- No production Ed25519 keys, tokens, or private publish scripts in the repository or Actions.
- No new tracked `scripts/publish.sh`.
- No change to signed `update` / `upgrade`.
- No rewrite or deletion of existing tags or existing GitHub Releases.
- No stable image publish merely because `main` moved.
- No execution of the ARM servers or the iroh relay binaries. Header checks do not prove those programs run. A pull request builds the full unsigned matrix from one commit, compiles the testing installer from the Windows server and tray built for that commit, and runs only the refusal check above. It does not publish.
- No claim that a legacy binary was booted on Debian 12. The prerelease file is the public `+crt-static` recipe, checked as a static PIE. `scripts/build-release-static.sh` is still not in this repository.
