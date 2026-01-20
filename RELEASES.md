# Release Process

This document describes how versions and releases are managed in the Madmail project.

## Versioning and Automatic Bumping

We use a semantic release process triggered by pushes to the `main` branch. 

- **Automatic Bumping**: Every time code is merged into `main`, a process (e.g., Semantic Release) is triggered to increment the software version.
- **Tracking Changes**: These automatic version bumps are intended for **tracking internal changes** in the project and do not represent a public release.
- **Maintenance**: Version updates are pushed back to the repository to keep the metadata synchronized with the development state.

## Official Releases

Official releases (publicly available binaries) are handled **manually** by the maintainers.

### Publishing Steps:
1. **Manual Trigger**: A maintainer decides when a version is stable enough for a release.
2. **GitHub Releases**: The release is manually created on GitHub, and the signed binaries are uploaded.
3. **Telegram Channel**: The same signed binary and its changelog are posted to the official [Telegram Channel](https://t.me/the_madmail).

### Signing:
All official binaries are digitally signed with the developer's private key before being uploaded to GitHub or Telegram.

## Developer Note
Do not rely on the `main` branch's automatic version bump alone to consider something a "release". Always check the [GitHub Releases](https://github.com/themadorg/madmail/releases) section or the Telegram channel for official stable builds.
