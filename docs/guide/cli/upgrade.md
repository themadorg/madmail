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
4. Stops the systemd service (and iroh-relay when present).
5. Replaces the current executable.
6. **Custom www templates:** runs the new binary’s [`html-migrate`](html-migrate.md) against `--config`. If `www_dir` points at a custom site that still uses Go `html/template` syntax, you are prompted to convert files to Minijinja (backups as `*.go-template.bak`). Decline or non-interactive sessions leave files unchanged; re-run `madmail html-migrate` later if needed.
7. Restarts the systemd service when applicable and refreshes man/completions.

## Examples

```bash
madmail upgrade /tmp/madmail-signed
madmail upgrade https://relay.example/releases/madmail
madmail upgrade --accept-unsafe-https https://self-signed.example/madmail
madmail upgrade https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64.tar.gz
```

## Notes

- Only binaries signed with the official release key are accepted. There is **no** flag to install an unsigned or bad-signed binary — verification always runs and aborts on failure.
- Download URLs may be a raw signed binary or a GitHub-style `.tar.gz` / `.tgz` archive that contains a member named `madmail` (the signed binary). The archive is extracted first; signature verification always runs on that binary, never on the archive itself. Other archive formats (`.zip`, `.tar.bz2`, …) are rejected with a clear error.
- `--accept-unsafe-https` only relaxes **HTTPS transport** certificate checks (self-signed peers); it never weakens Ed25519 signature verification.
- Non-interactive / `--json` sessions cannot prompt; pass `--accept-unsafe-https` explicitly when needed.
- Requires appropriate permissions to replace `/usr/local/bin/madmail`.

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
