Release process (maintainers)

This repository uses GoReleaser and GitHub Actions to publish release artifacts to GitHub Releases. Tagging the repository with a semver tag (for example `v0.8.3`) will trigger the `goreleaser` GitHub Actions workflow which builds binaries for Linux, macOS and Windows (amd64 and arm64), creates archives, and uploads them to the release.

Requirements and notes for maintainers:

- CI requires standard GitHub Actions permissions. The workflow uses `${{ secrets.GITHUB_TOKEN }}` to upload artifacts.
- For additional publishers (Homebrew, Winget, notarization, or pushing Docker images to GHCR), configure additional secrets (GH_PAT, MACOS_SIGN_P12, etc.) and update `.goreleaser.yaml` accordingly.
- To run a local smoke test of the release builds without publishing, install goreleaser locally and run:

  goreleaser build --snapshot --clean

- To produce a real release from your local machine, create and push a tag and push it to remote:

  git tag -a vX.Y.Z -m "Release vX.Y.Z"
  git push origin vX.Y.Z

The GitHub Actions workflow will run and create the GitHub Release with artifacts.
