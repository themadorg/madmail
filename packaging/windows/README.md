# Windows packaging

Operator-facing Windows installers and release binaries for Madmail.

> Epic: [#103](https://github.com/themadorg/madmail/issues/103).  
> CI builds Windows artifacts on **version tags** (`v*`) and published GitHub Releases (see [CI](#ci)). This workflow does **not** create tags or attach Release assets by itself.

## Artifacts

| File | Arch | Notes |
|------|------|--------|
| `madmail-windows-amd64.exe` | x64 | Server + CLI (`chatmail` package) |
| `madmail-tray-windows-amd64.exe` | x64 | System tray helper |
| `madmail-windows-arm64.exe` | arm64 | Server + CLI |
| `madmail-tray-windows-arm64.exe` | arm64 | System tray helper |
| `madmail-windows-amd64-setup.exe` | x64 | Inno Setup wizard |
| `madmail-windows-arm64-setup.exe` | arm64 | Inno Setup wizard |

## Installer (Inno Setup)

**Requires:** [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`ISCC.exe`) on a Windows host, plus pre-built exes under `build/`.

```powershell
# From repo root on Windows, after building binaries:
make build-windows-amd64   # or build natively / CI artifacts
.\packaging\windows\build-setup.ps1 -Arch amd64
# → build\madmail-windows-amd64-setup.exe

.\packaging\windows\build-setup.ps1 -Arch arm64
# → build\madmail-windows-arm64-setup.exe
```

Or:

```text
iscc /DArch=amd64 packaging\windows\Madmail.iss
iscc /DArch=arm64 packaging\windows\Madmail.iss
```

### Wizard flow

1. License (AGPL summary)  
2. **Deployment mode** — local / public IP / domain  
3. **Identity** — IP or hostname  
4. **TLS** — self-signed or Let's Encrypt  
5. ACME email (if LE)  
6. Language + optional Shadowsocks (Iroh omitted until `iroh-relay.exe` is packaged)  
7. Install directory + tasks (firewall, start service, tray autostart)  
8. DNS checklist (domain mode)  
9. Post-install: `madmail install … --install-service [--start-service] [--firewall]`  
10. Optional: tray autostart + launch tray  

### Silent / unattended

Inno supports `/VERYSILENT /SUPPRESSMSGBOXES`. Wizard defaults are used unless you extend `[Setup]` with custom `/MODE=` parsing later. Recommended unattended path for automation:

```text
madmail.exe install --simple --ip 127.0.0.1 --tls-mode self_signed ^
  --config-dir "%ProgramData%\Madmail\config" ^
  --state-dir "%ProgramData%\Madmail\data" ^
  --install-service --start-service --firewall
```

### Uninstall

Add/Remove Programs prompts:

| Choice | Effect |
|--------|--------|
| **Yes** | Remove **`%ProgramData%\Madmail`** (config, certs, mail, `install.log`) |
| **No** | Keep ProgramData (mail/config survive; service + Program Files removed) |
| **Cancel** | Abort uninstall |

Then runs `madmail uninstall --force --keep-binary` (plus `--keep-data --keep-config` if you chose No), removes tray autostart, and deletes Program Files. After a wipe choice, Inno also `DelTree`s any leftover ProgramData files.

Silent uninstall (`/VERYSILENT`) **keeps** ProgramData (no prompt).

CLI examples:

```text
# Full wipe ProgramData
madmail uninstall --force --keep-binary --config … --state-dir …

# Keep mail + config
madmail uninstall --force --keep-data --keep-config --keep-binary --config … --state-dir …
```

## Build binaries (no publish)

From the repo root:

```bash
# Both arches when toolchains allow (arm64 may skip on plain Linux)
make build-windows

# Or:
./scripts/build-windows.sh amd64
./scripts/build-windows.sh arm64
./scripts/build-windows.sh all
```

### amd64 (Linux cross)

Requires `mingw-w64` (`x86_64-w64-mingw32-gcc`):

```bash
make build-windows-amd64
# → build/madmail-windows-amd64.exe
# → build/madmail-tray-windows-amd64.exe (may fail if tray deps need a Windows host)
```

### arm64

Preferred: **Windows** with VS arm64 toolset:

```powershell
rustup target add aarch64-pc-windows-msvc
cargo build -p chatmail -p madmail-tray --release --target aarch64-pc-windows-msvc
# copy *.exe to build/
```

Optional on Linux: [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin):

```bash
cargo install cargo-xwin
./scripts/build-windows.sh arm64
```

Until arm64 CI runners or cargo-xwin are available, **compile-only** checks and the manual lab checklist (epic #103 L6/L8) gate arm64 claims.

## Default install layout

| Item | Path |
|------|------|
| Binary | `C:\Program Files\Madmail\madmail.exe` |
| Tray | `C:\Program Files\Madmail\madmail-tray.exe` |
| Config | `%ProgramData%\Madmail\config\` |
| State | `%ProgramData%\Madmail\data\` |
| Service | `Madmail` |

## Related CLI

```text
madmail install --simple --ip … --install-service --start-service --firewall
madmail service install|start|stop|status
madmail firewall apply|remove
madmail admin-token
madmail-tray --smoke-exit
madmail-tray status | token
madmail-tray install-autostart
```

### Admin API vs admin web UI

| Surface | Windows packaging status |
|---------|--------------------------|
| `POST /api/admin` + bearer token | Supported (service running) |
| Embedded SPA at `/admin` | **Not** shipped in current Windows CI builds (stub / disabled) |
| Tray “Open admin UI” | **Removed** — no SPA; use token + CLI/API |

Token file: `%ProgramData%\Madmail\data\admin_token`.  
Do not open `/api/admin` in a browser (GET → **405**).

### Installer options (wizard)

- **Shadowsocks** — optional (default on).
- **Iroh** — **not** offered (no `iroh-relay.exe` on Windows yet; enabling it breaks service boot).
- Prefer **self-signed** in the lab if port **80** / ACME is not ready; **Let's Encrypt** (public IP or domain) needs inbound TCP 80 during install.

## CI

GitHub Actions workflow [`.github/workflows/windows.yml`](../../.github/workflows/windows.yml):

| Trigger | When |
|---------|------|
| **Push tags** `v*` | After semantic-release (or manual) cuts a version tag |
| **GitHub Release** published | When a release is published |
| **PR to `main`** | Only if Windows-related paths change |
| **`workflow_dispatch`** | Manual run from Actions UI |

| Job | Purpose |
|-----|---------|
| `linux-windows-crates` | Unit tests for tray / service / firewall / packaging file presence |
| `windows-amd64-smoke` | MSVC build, tray smoke, service status, local self-signed install, upload CI artifacts |
| `windows-amd64-setup` | Inno Setup (`choco install innosetup`) → `madmail-windows-amd64-setup.exe` artifact |
| `windows-arm64-compile` | `cargo check` for `aarch64-pc-windows-msvc` (server + tray) |

Download setup/binaries from the **Actions** run artifacts (`madmail-windows-amd64-setup`, `madmail-windows-amd64-ci`). The workflow does not attach files to GitHub Releases. Manual sign-off: [MANUAL-CHECKLIST.md](./MANUAL-CHECKLIST.md).

### Windows Defender, SmartScreen, and UAC

CI- and tag-built `setup.exe` / `madmail.exe` are **unsigned** (no Authenticode). That is expected for this open-source project.

| Prompt / alert | What it is | What to do |
|----------------|------------|------------|
| **UAC** — “Do you want to allow this app to make changes…?” | Installer and service registration need **Administrator** | Choose **Yes**. Prefer right-click setup → **Run as administrator**. |
| **SmartScreen** — “Windows protected your PC” / unknown publisher | Unsigned binary; SmartScreen has no publisher reputation | **More info** → **Run anyway** (only for builds you trust from this repo’s Actions/releases). |
| **Defender** — **Trojan:Win32/Bearfoos** (or similar ML) | Heuristic false positive on unsigned CI PE files | See workarounds below. Not a known real infection for official Madmail artifacts. |
| Installer **elevated** but configure fails | Often LE/TLS or missing elevation earlier | See `%ProgramData%\Madmail\install.log` |

**Defender / quarantine workarounds (lab and self-hosted):**

1. **Exclusions** for `C:\Program Files\Madmail`, `%ProgramData%\Madmail`, and the folder where you download `setup.exe` (while installing).  
2. **Protection history** → restore the file if Defender quarantined it mid-install.  
3. If setup is blocked, copy **`madmail.exe`** from the CI artifact and run elevated `madmail install …`.  
4. Prefer builds from this repository’s **GitHub Actions** (or a known mirror), not random re-uploads.  

There is no free public Authenticode path for this project; stay on the notice above rather than expecting signed SmartScreen trust.

If the tray says the service does not exist (error 1060), register it manually (elevated):

```powershell
& "C:\Program Files\Madmail\madmail.exe" `
  --config "$env:ProgramData\Madmail\config\madmail.conf" `
  --state-dir "$env:ProgramData\Madmail\data" `
  service install --start
```

Configure log (when installed via setup): `%ProgramData%\Madmail\install.log`.

## Local full E2E (Vagrant + libvirt)

Optional **release testing** on a Windows VM (not a GHA replacement): install, service, firewall, and **full mail path** (create users, SMTP submit, IMAP receive both directions).

→ **[vagrant/README.md](./vagrant/README.md)**

```bash
make build-windows-amd64
make windows-vagrant-up          # vagrant up --provider=libvirt
make windows-vagrant-e2e         # re-run mail E2E after binary updates
```
