# Windows packaging

Operator-facing Windows installers and release binaries for Madmail.

> Integration work lives on branch `feat/windows-installer` (epic [#103](https://github.com/themadorg/madmail/issues/103)).  
> This tree does **not** publish GitHub Releases by itself.

## Artifacts

| File | Arch | Notes |
|------|------|--------|
| `madmail-windows-amd64.exe` | x64 | Server + CLI (`chatmail` package) |
| `madmail-tray-windows-amd64.exe` | x64 | System tray helper |
| `madmail-windows-arm64.exe` | arm64 | Server + CLI |
| `madmail-tray-windows-arm64.exe` | arm64 | System tray helper |
| `madmail-windows-amd64-setup.exe` | x64 | Inno Setup (PR6) |
| `madmail-windows-arm64-setup.exe` | arm64 | Inno Setup (PR6) |

## Build (no publish)

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
madmail-tray --smoke-exit
madmail-tray install-autostart
```
