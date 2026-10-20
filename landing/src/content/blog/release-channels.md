---
title: "How Madmail's new release channels work"
date: 2026-10-20
author: KamyarAT
language: en
translation: release-channels
description: "How unsigned testing builds are published separately from signed stable releases."
---

Madmail now publishes automated testing builds separately from signed stable releases. If you run a production relay, keep using the stable channel. If you want to help test changes, you can deliberately pick a release marked `unstable` and try it in a test environment.

The [release automation introduced in PR #210](https://github.com/themadorg/madmail/pull/210) is now joined by the [expanded build matrix in PR #218](https://github.com/themadorg/madmail/pull/218). Testing releases include Linux amd64 and ARM64 builds, several Linux variants, and Windows packages. The same build recipes also run on pull requests, so build and packaging failures can show up during review.

## How version numbers are chosen

Some merges don't need a new version. The automation uses release-worthy Conventional Commits to decide whether to bump the version and which part of the number to change:

| Change | Version bump |
| --- | --- |
| `fix`, `perf`, `revert` | Patch |
| `feat` | Minor |
| Breaking changes | Major |
| `docs`, `chore`, `ci`, `test` | Ordinarily no bump |

Numeric source manifests keep the form `X.Y.Z`. Testing Git tags and GitHub prereleases add the suffix: `vX.Y.Z-unstable`. Stable releases use `vX.Y.Z`.

CI checks gate release preparation, and successful builds gate publication. Passing those checks gets a candidate ready for testing; it doesn't establish that the build has no bugs or is suitable for production.

## The release targets are built on every PR

Every pull request now runs the full six-target testing-release matrix in build-only mode. Each job checks out the same explicit PR head commit and uses that commit's `Cargo.lock` and recorded admin-web revision. The Windows job builds the server, tray and testing installer together.

The final validation job requires every matrix role to succeed, then stages the 16-file package set as a workflow artifact named `unstable-build-only-validated`. These PR jobs have read-only repository permissions and no publication credentials. They don't bump the version, create tags or releases, or publish Docker images. Public testing releases still follow a release-worthy merge to `main` and the publication checks.

That gives reviewers build and packaging results for all the release targets. Runtime coverage is more limited: ARM64 servers are cross-compiled but aren't executed in this matrix. The Windows installer check tests refusal using simulated installation markers; it doesn't complete an installation. Testing on the intended system still matters.

## What you get with a testing release

Testing builds appear on GitHub as explicitly labelled prereleases. Their binaries are unsigned, and they may contain regressions or incompatible changes. Upgrade and downgrade compatibility isn't guaranteed, so use an environment you can reset rather than a relay people depend on.

Two files help identify and check a download. `SOURCE.txt` records the source revision used for the build. `SHA256SUMS` lists the hashes you can compare with the files you downloaded.

A matching checksum tells you that your file matches the hash recorded with the release. That's useful integrity information, but it doesn't replace the digital signature required by the stable updater.

The expanded package set is already available in releases such as [v2.38.0-unstable](https://github.com/themadorg/madmail/releases/tag/v2.38.0-unstable). It includes six server builds:

| Server build | Executable |
| --- | --- |
| Linux amd64, glibc | `madmail-linux-amd64` |
| Linux amd64, fully static legacy glibc | `madmail-linux-amd64-legacy` |
| Linux amd64, musl | `madmail-linux-amd64-musl` |
| Linux ARM64, glibc | `madmail-linux-arm64` |
| Linux ARM64, musl | `madmail-linux-arm64-musl` |
| Windows amd64 | `madmail-windows-amd64.exe` |

Each Linux build also has a `.tar.gz` archive containing an executable named `madmail`. The Windows server has a `madmail-windows-amd64.zip` archive. The Windows tray and unsigned testing installer are included as `madmail-tray-windows-amd64.exe` and `madmail-windows-amd64-setup.exe`.

Choose the executable for your operating system and CPU architecture, and on Linux, the appropriate libc variant.

That makes 16 uploaded files: six server binaries, six archives, the tray, the installer, `SOURCE.txt` and `SHA256SUMS`. GitHub adds [two automatic source archives](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases), bringing the release page's download list to 18 entries. Check each release's actual asset list when choosing a platform or package; older releases can have a different set.

The unsigned installer is labelled `Madmail TESTING` and is intended for a fresh, disposable Windows VM. It refuses installation when it detects an existing Madmail installation or shared service or data resources. Use it in a separate test environment.

## Stable releases keep the signature checks

Stable server binaries go through a separate build and digital signing process, using the selected source and separately maintained owner tooling. Production signing keys stay outside the public repository and public CI.

The ordinary updater continues to follow this signed stable path. Both `madmail update` and `madmail upgrade` skip GitHub drafts and prereleases and still require the existing Ed25519 signature verification. Publishing an unsigned testing build doesn't make it available through those commands.

Stable Windows packaging uses the signed server binary and matching tray attached to that stable release.

## Docker tags follow the same channels

Docker has version-specific tags and moving aliases for each channel:

| Channel | Version-specific tag | Moving alias |
| --- | --- | --- |
| Testing | `X.Y.Z-unstable` | `unstable` |
| Stable | `X.Y.Z` | `latest` |

The alias follows its channel, while a version-specific tag lets you select a particular release. Production deployments should stay on the stable tags. Using `unstable` is a deliberate choice to follow testing builds.

Testing image publication waits until the corresponding GitHub testing prerelease has been published successfully. The testing workflow doesn't move `latest`. The workflows also include checks intended to stop an older retry from moving a channel alias backward.

Retries have rules for the downloadable assets too. A matching asset already attached to a release can be reused; a conflicting asset causes the job to fail instead of silently replacing the file.

Version-specific Docker images are checked for matching source identity. A registry or authentication failure stops the operation rather than being treated as permission to overwrite an image.

## Making a test report useful

The benefit of a published testing build is that someone else can download the same candidate and try to reproduce a problem. If you find one, include:

- The release tag, or the PR number if you used a PR workflow artifact, and the source revision from `SOURCE.txt`.
- Your operating system, architecture and exact binary filename, plus the image tag if you used Docker.
- The steps to reproduce the problem, what you expected, and what happened.
- Relevant logs with credentials and personal data removed.

You can find the builds in the [release list](https://github.com/themadorg/madmail/releases) and report problems through [GitHub issues](https://github.com/themadorg/madmail/issues). Check each release's label, notes and available assets before downloading.
