#!/usr/bin/env bash
# Build Windows release artifacts (madmail + madmail-tray) into build/.
# Does NOT upload releases or create tags — local/CI artifact production only.
#
# Usage:
#   ./scripts/build-windows.sh              # amd64 (mingw) + arm64 if toolchain present
#   ./scripts/build-windows.sh amd64
#   ./scripts/build-windows.sh arm64
#   ARCH=amd64 ./scripts/build-windows.sh
#
# Environment:
#   CHATMAIL_ADMIN_WEB_BUILD  — path to admin-web embed (optional; same as Makefile)
#   SKIP_TRAY=1               — build server only
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

ADMIN_WEB_BUILD="${CHATMAIL_ADMIN_WEB_BUILD:-}"
if [[ -z "$ADMIN_WEB_BUILD" && -d "$ROOT/crates/chatmail-admin-web/embed" ]]; then
  # Prefer Makefile's ADMIN_WEB_BUILD when exported; otherwise leave empty for default embed.
  :
fi

export_admin() {
  if [[ -n "${CHATMAIL_ADMIN_WEB_BUILD:-}" ]]; then
    export CHATMAIL_ADMIN_WEB_BUILD
  fi
}

build_amd64() {
  echo "==> Windows amd64 (x86_64-pc-windows-gnu)"
  if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    echo "error: need mingw-w64 (x86_64-w64-mingw32-gcc)" >&2
    exit 1
  fi
  rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1 || true
  export_admin
  cargo build -p chatmail --release --target x86_64-pc-windows-gnu
  cp -f target/x86_64-pc-windows-gnu/release/madmail.exe build/madmail-windows-amd64.exe
  echo "    wrote build/madmail-windows-amd64.exe"
  if [[ "${SKIP_TRAY:-0}" != "1" ]]; then
    # tray-icon/winit may need extra link flags; fail soft with a clear note
    if cargo build -p madmail-tray --release --target x86_64-pc-windows-gnu; then
      cp -f target/x86_64-pc-windows-gnu/release/madmail-tray.exe build/madmail-tray-windows-amd64.exe
      echo "    wrote build/madmail-tray-windows-amd64.exe"
    else
      echo "warning: madmail-tray amd64 cross-build failed (tray deps often need a Windows host)" >&2
      echo "         build tray on windows-latest or use native MSVC later" >&2
    fi
  fi
}

build_arm64() {
  echo "==> Windows arm64 (aarch64-pc-windows-msvc preferred)"
  local target="aarch64-pc-windows-msvc"
  rustup target add "$target" >/dev/null 2>&1 || true

  # Native Windows arm64 or VS cross from x64 Windows.
  if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* || "$(uname -s)" == Windows_NT ]]; then
    export_admin
    cargo build -p chatmail --release --target "$target"
    cp -f "target/${target}/release/madmail.exe" build/madmail-windows-arm64.exe
    echo "    wrote build/madmail-windows-arm64.exe"
    if [[ "${SKIP_TRAY:-0}" != "1" ]]; then
      cargo build -p madmail-tray --release --target "$target"
      cp -f "target/${target}/release/madmail-tray.exe" build/madmail-tray-windows-arm64.exe
      echo "    wrote build/madmail-tray-windows-arm64.exe"
    fi
    return 0
  fi

  # Optional: cargo-xwin on Linux (install: cargo install cargo-xwin)
  if command -v cargo-xwin >/dev/null 2>&1; then
    echo "    using cargo-xwin for ${target}"
    export_admin
    cargo xwin build -p chatmail --release --target "$target"
    cp -f "target/${target}/release/madmail.exe" build/madmail-windows-arm64.exe
    echo "    wrote build/madmail-windows-arm64.exe"
    if [[ "${SKIP_TRAY:-0}" != "1" ]]; then
      if cargo xwin build -p madmail-tray --release --target "$target"; then
        cp -f "target/${target}/release/madmail-tray.exe" build/madmail-tray-windows-arm64.exe
        echo "    wrote build/madmail-tray-windows-arm64.exe"
      else
        echo "warning: madmail-tray arm64 xwin build failed" >&2
      fi
    fi
    return 0
  fi

  cat >&2 <<'EOF'
skip: Windows arm64 build needs one of:
  • Windows host with Visual Studio arm64 toolset:
      rustup target add aarch64-pc-windows-msvc
      cargo build -p chatmail -p madmail-tray --release --target aarch64-pc-windows-msvc
  • Linux with cargo-xwin:
      cargo install cargo-xwin
      cargo xwin build -p chatmail --release --target aarch64-pc-windows-msvc
Artifacts (when built): build/madmail-windows-arm64.exe, build/madmail-tray-windows-arm64.exe
EOF
  return 0
}

ARCH="${1:-${ARCH:-all}}"
case "$ARCH" in
  amd64|x86_64|x64) build_amd64 ;;
  arm64|aarch64) build_arm64 ;;
  all)
    build_amd64
    build_arm64
    ;;
  *)
    echo "usage: $0 [amd64|arm64|all]" >&2
    exit 2
    ;;
esac

echo "==> done (artifacts under build/)"
ls -la build/madmail*windows* 2>/dev/null || true
ls -la build/madmail-tray*windows* 2>/dev/null || true
