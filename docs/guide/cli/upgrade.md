# `upgrade`

Replace the running binary with a **signed** build from a local path or `http://` / `https://` URL.


## Synopsis

```bash
madmail upgrade <PATH_OR_URL> [--accept-unsafe-https]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |

## Command flags

| Flag | Description |
|------|-------------|
| `--accept-unsafe-https` | Allow HTTPS downloads when the server TLS certificate is self-signed or otherwise untrusted. Without this flag, certificate verification is enforced; on an interactive TTY you may be prompted `[y/N]`. Ed25519 signature verification of the binary always still runs. |

## Arguments

| Argument | Description |
|----------|-------------|
| `PATH_OR_URL` | Local path to signed binary, or URL to download a raw signed binary or `.tar.gz` / `.tgz` release archive (max 100 MB) |

## How it works

1. Downloads the file if a URL is given. **HTTPS verifies TLS certificates by default.** Self-signed/untrusted certs require `--accept-unsafe-https` or an interactive yes. `http://` is unchanged.
2. If the URL ends in `.tar.gz` or `.tgz`, extracts the `madmail` binary from the archive first.
3. Verifies an **Ed25519 signature** in the last 64 bytes of the binary.
4. **Host preflight:** runs the new binary with `version` on this machine. If it cannot execute (wrong glibc / loader error), the upgrade **aborts** — services stay up and the live binary is not touched.
5. Copies the current executable to a sibling `*.prev` backup (e.g. `/usr/local/bin/madmail.prev`).
6. Stops the systemd service (and iroh-relay when present).
7. Replaces the current executable.
8. Re-runs the smoke check on the installed path; on failure, **restores `*.prev`**, restarts services, and errors out.
9. **Custom www templates:** runs the new binary’s [`html-migrate`](html-migrate.md) against `--config`. If `www_dir` points at a custom site that still uses Go `html/template` syntax and/or legacy `/qr?data=` QR image URLs, you are prompted to convert (Minijinja + client-side QR; backups as `*.go-template.bak` / `main.js.qr-compat.bak`). Decline or non-interactive sessions leave files unchanged; re-run `madmail html-migrate --yes` later if needed.
10. Restarts the systemd service when applicable and refreshes man/completions. A previous working binary remains at `*.prev` for manual rollback if needed.

## Linux release variants

Official GitHub Releases ship more than one Linux build per architecture. **The default asset is not universal:**

| Asset (example) | When to use it |
|-----------------|----------------|
| `madmail-linux-amd64.tar.gz` | Default glibc build (newer distros) |
| `madmail-linux-amd64-legacy.tar.gz` | Older glibc (e.g. Ubuntu 22.04 / hosts that report `GLIBC_2.38` / `GLIBC_2.39` not found) |
| `madmail-linux-amd64-musl.tar.gz` | musl / static-ish alternative |
| `…-arm64…` / `…-arm64-legacy…` / `…-arm64-musl…` | Same matrix for arm64 |

If you originally installed `*-legacy` or `*-musl`, keep using that variant in `update` / `upgrade` URLs. Pointing a legacy host at the default tarball used to overwrite a working binary and brick the install; preflight + `*.prev` rollback prevent that.

## Examples

```bash
# Local signed binary
madmail upgrade /tmp/madmail-signed

# Default amd64 (newer glibc hosts)
madmail upgrade https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64.tar.gz

# Older distros / hosts that need the legacy build
madmail upgrade https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64-legacy.tar.gz

# musl alternative
madmail upgrade https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64-musl.tar.gz

madmail upgrade --accept-unsafe-https https://self-signed.example/madmail
```

## Notes

- Only binaries signed with the official release key are accepted. There is **no** flag to install an unsigned or bad-signed binary — verification always runs and aborts on failure.
- Download URLs may be a raw signed binary or a GitHub-style `.tar.gz` / `.tgz` archive that contains a member named `madmail` (the signed binary). The archive is extracted first; signature verification always runs on that binary, never on the archive itself. Other archive formats (`.zip`, `.tar.bz2`, …) are rejected with a clear error.
- `--accept-unsafe-https` only relaxes **HTTPS transport** certificate checks (self-signed peers); it never weakens Ed25519 signature verification.
- Non-interactive / `--json` sessions cannot prompt; pass `--accept-unsafe-https` explicitly when needed.
- Requires appropriate permissions to replace `/usr/local/bin/madmail`.
- After a successful upgrade, `madmail.prev` next to the install path is the previous binary (manual rollback: `sudo cp /usr/local/bin/madmail.prev /usr/local/bin/madmail && sudo systemctl start madmail`).

## JSON output (`--json`)

```bash
madmail upgrade --json
```

Success stdout:

```json
{"ok": true, "command": "upgrade", "data": { ... }}
```

Schema: [json-output.md](json-output.md#upgrade).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/upgrade.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/upgrade.rs)
