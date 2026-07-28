#!/usr/bin/env bash
# Container / host harness for madmail update safety (issue #114).
#
# Scenarios:
#   G) live binary `version` works
#   A) unsigned binary rejected; live binary untouched; no *.prev
#   B) signed wrong-arch (arm64) fails preflight; live untouched
#   H) default amd64 asset on older glibc fails preflight; live untouched  (#114)
#   C) compatible signed upgrade succeeds; creates *.prev with prior bytes
#   E) re-upgrade after success still works (regression)
#   D) .tar.gz URL extract + upgrade succeeds
#   F) default-asset URL prints variant warning; live untouched on failed download
#
# Usage (repo root, after `cargo build -p chatmail --release`):
#   ./tests/upgrade-safety.sh              # host (partial) + docker matrix
#   ./tests/upgrade-safety.sh --docker-only
#   ./tests/upgrade-safety.sh --host-only  # used inside containers
#
# Env:
#   MADMAIL_BIN      path to built madmail (default: target/release/madmail)
#   RELEASE_TAG      GitHub release tag (default: latest via gh, or v2.18.2)
#   UPGRADE_ASSET    asset name for success path (auto: default or legacy)
#   FORCE_LEGACY=1   always use *-legacy for success scenarios
#   KEEP_WORKDIR=1   keep work dir
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MADMAIL_BIN="${MADMAIL_BIN:-$ROOT/target/release/madmail}"
WORKDIR="${WORKDIR:-$(mktemp -d /tmp/madmail-upgrade-safety.XXXXXX)}"
RELEASE_TAG="${RELEASE_TAG:-}"
REPO="${REPO:-themadorg/madmail}"
PASS=0
FAIL=0
SKIP=0

# All diagnostics go to stderr so command substitutions (e.g. pick_success_asset)
# only capture the intended return value on stdout.
red() { printf '\033[31m%s\033[0m\n' "$*" >&2; }
green() { printf '\033[32m%s\033[0m\n' "$*" >&2; }
yellow() { printf '\033[33m%s\033[0m\n' "$*" >&2; }
info() { printf '→ %s\n' "$*" >&2; }

cleanup() {
  if [[ "${KEEP_WORKDIR:-0}" == "1" ]]; then
    yellow "KEEP_WORKDIR=1 — left $WORKDIR"
  else
    rm -rf "$WORKDIR"
  fi
}
trap cleanup EXIT

assert_ok() {
  local name="$1"
  shift
  info "TEST: $name"
  # Use `|| rc=$?` so nested helpers that toggle `set -e` cannot abort the harness
  # when a scenario returns non-zero (e.g. skip=2).
  local rc=0
  "$@" || rc=$?
  if [[ "$rc" -eq 0 ]]; then
    green "  PASS: $name"
    PASS=$((PASS + 1))
  elif [[ "$rc" -eq 2 ]]; then
    yellow "  SKIP: $name"
    SKIP=$((SKIP + 1))
  else
    red "  FAIL: $name (rc=$rc)"
    FAIL=$((FAIL + 1))
  fi
}

assert_contains() {
  local hay="$1" needle="$2"
  [[ "$hay" == *"$needle"* ]]
}

sha() { sha256sum "$1" | awk '{print $1}'; }

need_bin() {
  if [[ ! -x "$MADMAIL_BIN" ]]; then
    red "madmail binary not found/executable: $MADMAIL_BIN"
    red "Build with: cargo build -p chatmail --release"
    exit 2
  fi
}

resolve_tag() {
  if [[ -n "$RELEASE_TAG" ]]; then
    echo "$RELEASE_TAG"
    return
  fi
  if command -v gh >/dev/null 2>&1; then
    gh release view --repo "$REPO" --json tagName --jq .tagName
  else
    echo "v2.18.2"
  fi
}

download_asset() {
  local tag="$1" name="$2" dest="$3"
  mkdir -p "$(dirname "$dest")"
  if [[ -f "$dest" && -s "$dest" ]] && file "$dest" 2>/dev/null | grep -qiE 'ELF|gzip compressed'; then
    return 0
  fi
  local url="https://github.com/${REPO}/releases/download/${tag}/${name}"
  info "Downloading $name ($tag)..."
  local tmp="${dest}.part"
  rm -f "$tmp" "$dest"
  curl -fsSL --retry 3 --retry-delay 1 -o "$tmp" "$url"
  mv -f "$tmp" "$dest"
  chmod +x "$dest" || true
  if ! file "$dest" 2>/dev/null | grep -qiE 'ELF|gzip compressed'; then
    red "downloaded $name does not look like a binary/archive"
    file "$dest" || true
    return 1
  fi
}

host_can_run() {
  local bin="$1"
  # Do not toggle set -e globally (breaks callers under `set -e`).
  if "$bin" version >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

# Pick a success-path asset: prefer default if it runs here, else legacy.
pick_success_asset() {
  local tag="$1"
  if [[ -n "${UPGRADE_ASSET:-}" ]]; then
    echo "$UPGRADE_ASSET"
    return
  fi
  if [[ "${FORCE_LEGACY:-0}" == "1" ]]; then
    echo "madmail-linux-amd64-legacy"
    return
  fi
  mkdir -p "$WORKDIR/assets"
  local tmp="$WORKDIR/assets/madmail-linux-amd64"
  if ! download_asset "$tag" "madmail-linux-amd64" "$tmp"; then
    # Fall back if probe download fails (network blip).
    echo "madmail-linux-amd64-legacy"
    return
  fi
  if host_can_run "$tmp"; then
    echo "madmail-linux-amd64"
  else
    echo "madmail-linux-amd64-legacy"
  fi
}

setup_install() {
  INSTALL_DIR="$WORKDIR/install"
  mkdir -p "$INSTALL_DIR" "$WORKDIR/assets" "$WORKDIR/http"
  cp -f "$MADMAIL_BIN" "$INSTALL_DIR/madmail"
  chmod 755 "$INSTALL_DIR/madmail"
  LIVE="$INSTALL_DIR/madmail"
  LIVE_SHA_BEFORE="$(sha "$LIVE")"
}

run_upgrade() {
  local path_or_url="$1"
  shift || true
  RC=0
  OUT="$("$LIVE" upgrade "$path_or_url" "$@" 2>&1)" || RC=$?
}

# ─── scenarios ──────────────────────────────────────────────────────────────

scenario_G_live_version() {
  setup_install
  local ver
  ver="$("$LIVE" version 2>&1)" || return 1
  echo "  version: $ver"
  return 0
}

scenario_A_unsigned() {
  setup_install
  local dummy="$WORKDIR/assets/unsigned.bin"
  printf 'NOT A SIGNED MADMAIL BINARY %s' "$(head -c 64 /dev/urandom | base64 -w0 2>/dev/null || head -c 64 /dev/urandom | base64)" >"$dummy"
  run_upgrade "$dummy"
  assert_contains "$OUT" "INVALID SIGNATURE" || {
    echo "$OUT" | tail -20
    return 1
  }
  [[ "$RC" -ne 0 ]] || return 1
  [[ "$(sha "$LIVE")" == "$LIVE_SHA_BEFORE" ]] || {
    echo "live binary was modified after unsigned upgrade attempt"
    return 1
  }
  [[ ! -e "${LIVE}.prev" ]] || {
    echo "unexpected .prev after failed unsigned upgrade"
    return 1
  }
  return 0
}

scenario_B_wrong_arch() {
  setup_install
  local tag="$1"
  local arm="$WORKDIR/assets/madmail-linux-arm64"
  download_asset "$tag" "madmail-linux-arm64" "$arm"
  if file "$arm" | grep -qiE 'x86-64|x86_64'; then
    return 2
  fi
  run_upgrade "$arm"
  [[ "$(sha "$LIVE")" == "$LIVE_SHA_BEFORE" ]] || {
    echo "BRICK RISK: live binary changed after wrong-arch upgrade"
    echo "$OUT" | tail -40
    return 1
  }
  if ! echo "$OUT" | grep -qiE 'preflight|failed to execute|exec format|cannot execute|NOT replaced|legacy'; then
    echo "expected preflight/loader abort, got:"
    echo "$OUT" | tail -40
    return 1
  fi
  [[ "$RC" -ne 0 ]] || return 1
  "$LIVE" version >/dev/null
  return 0
}

# Issue #114 core: default glibc build on host that cannot run it.
scenario_H_default_rejected_on_old_glibc() {
  setup_install
  local tag="$1"
  local def="$WORKDIR/assets/madmail-linux-amd64"
  download_asset "$tag" "madmail-linux-amd64" "$def"
  if host_can_run "$def"; then
    yellow "  host can run default amd64 — scenario is N/A here (covered on bookworm)"
    return 2
  fi
  run_upgrade "$def"
  [[ "$(sha "$LIVE")" == "$LIVE_SHA_BEFORE" ]] || {
    echo "BRICK RISK: live binary overwritten by incompatible default build"
    echo "$OUT" | tail -50
    return 1
  }
  [[ ! -e "${LIVE}.prev" ]] || {
    # Backup is only taken after preflight; must not exist on preflight fail.
    echo "unexpected .prev after preflight failure (backup should not run)"
    return 1
  }
  if ! echo "$OUT" | grep -qiE 'preflight|GLIBC|failed to execute|NOT replaced|legacy'; then
    echo "expected preflight/GLIBC abort, got:"
    echo "$OUT" | tail -40
    return 1
  fi
  [[ "$RC" -ne 0 ]] || return 1
  # Live still executable
  "$LIVE" version >/dev/null
  green "  (default asset correctly refused; live binary still runs)"
  return 0
}

scenario_C_success_prev() {
  setup_install
  local tag="$1"
  local asset="$2"
  local good="$WORKDIR/assets/$asset"
  download_asset "$tag" "$asset" "$good"
  if ! host_can_run "$good"; then
    yellow "  cannot run $asset on this host"
    return 2
  fi
  if [[ "$(id -u)" -ne 0 ]]; then
    yellow "  need root to replace binary"
    return 2
  fi
  run_upgrade "$good"
  if [[ "$RC" -ne 0 ]]; then
    echo "$OUT" | tail -50
    return 1
  fi
  assert_contains "$OUT" "Signature verification successful" || {
    echo "$OUT" | tail -30
    return 1
  }
  if ! echo "$OUT" | grep -qE 'Preflight OK|Preflight'; then
    echo "missing preflight success line"
    echo "$OUT" | tail -30
    return 1
  fi
  assert_contains "$OUT" "Upgrade complete" || return 1
  [[ -f "${LIVE}.prev" ]] || {
    echo "missing ${LIVE}.prev after successful upgrade"
    return 1
  }
  [[ "$(sha "${LIVE}.prev")" == "$LIVE_SHA_BEFORE" ]] || {
    echo ".prev is not the previous live binary"
    return 1
  }
  [[ "$(sha "$LIVE")" != "$LIVE_SHA_BEFORE" ]] || {
    echo "live binary unchanged after successful upgrade"
    return 1
  }
  "$LIVE" version >/dev/null
  echo "$asset" >"$WORKDIR/last_success_asset"
  return 0
}

scenario_E_reupgrade() {
  if [[ "$(id -u)" -ne 0 ]]; then
    return 2
  fi
  INSTALL_DIR="$WORKDIR/install"
  LIVE="$INSTALL_DIR/madmail"
  if [[ ! -f "$WORKDIR/last_success_asset" ]]; then
    return 2
  fi
  local asset
  asset="$(cat "$WORKDIR/last_success_asset")"
  local good="$WORKDIR/assets/$asset"
  [[ -x "$good" ]] || return 2

  # After scenario C, $LIVE is a published release binary (without this branch's
  # code). Re-seed the under-test build so we keep exercising the fixed path.
  cp -f "$MADMAIL_BIN" "$LIVE"
  chmod 755 "$LIVE"
  local before
  before="$(sha "$LIVE")"
  run_upgrade "$good"
  if [[ "$RC" -ne 0 ]]; then
    echo "$OUT" | tail -40
    return 1
  fi
  assert_contains "$OUT" "Upgrade complete" || return 1
  if [[ ! -f "${LIVE}.prev" ]]; then
    echo "missing ${LIVE}.prev after re-upgrade"
    return 1
  fi
  local prev_sha
  prev_sha="$(sha "${LIVE}.prev" 2>/dev/null || echo missing)"
  if [[ "$prev_sha" != "$before" ]]; then
    echo "re-upgrade did not snapshot previous live into .prev (prev=$prev_sha before=$before)"
    return 1
  fi
  "$LIVE" version >/dev/null
  return 0
}

scenario_D_tarball() {
  if [[ "$(id -u)" -ne 0 ]]; then
    return 2
  fi
  setup_install
  local tag="$1"
  local asset="$2" # e.g. madmail-linux-amd64-legacy.tar.gz
  local tgz_name="$asset"
  local tgz="$WORKDIR/http/$tgz_name"
  download_asset "$tag" "$tgz_name" "$tgz"

  local port
  port="$(python3 - <<'PY'
import socket
s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()
PY
)"
  (
    cd "$WORKDIR/http"
    python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1
  ) &
  local http_pid=$!
  sleep 0.4
  local url="http://127.0.0.1:${port}/${tgz_name}"
  RC=0
  OUT="$("$LIVE" update "$url" 2>&1)" || RC=$?
  kill "$http_pid" 2>/dev/null || true
  wait "$http_pid" 2>/dev/null || true

  if [[ "$RC" -ne 0 ]]; then
    echo "$OUT" | tail -50
    return 1
  fi
  if ! echo "$OUT" | grep -qiE 'Extracting madmail binary from archive|Extracting'; then
    echo "$OUT" | tail -30
    return 1
  fi
  assert_contains "$OUT" "Signature verification successful" || return 1
  assert_contains "$OUT" "Upgrade complete" || return 1
  [[ -f "${LIVE}.prev" ]] || return 1
  "$LIVE" version >/dev/null
  return 0
}

scenario_F_default_url_warning() {
  setup_install
  local port
  port="$(python3 - <<'PY'
import socket
s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()
PY
)"
  mkdir -p "$WORKDIR/empty_http"
  (
    cd "$WORKDIR/empty_http"
    python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1
  ) &
  local http_pid=$!
  sleep 0.3
  local url="http://127.0.0.1:${port}/madmail-linux-amd64.tar.gz"
  RC=0
  OUT="$("$LIVE" update "$url" 2>&1)" || RC=$?
  kill "$http_pid" 2>/dev/null || true
  wait "$http_pid" 2>/dev/null || true
  if ! echo "$OUT" | grep -qiE 'Default Linux build|legacy|\*-legacy'; then
    echo "$OUT" | tail -25
    return 1
  fi
  [[ "$(sha "$LIVE")" == "$LIVE_SHA_BEFORE" ]] || return 1
  return 0
}

run_host_scenarios() {
  need_bin
  local tag
  tag="$(resolve_tag)"
  info "Release tag: $tag"
  info "Workdir: $WORKDIR"
  info "Binary: $MADMAIL_BIN ($(file -b "$MADMAIL_BIN" 2>/dev/null || echo '?'))"
  info "uid=$(id -u) glibc=$(ldd --version 2>&1 | head -1)"

  local success_asset
  success_asset="$(pick_success_asset "$tag")"
  info "Success-path asset: $success_asset"
  local tgz_asset="${success_asset}.tar.gz"

  assert_ok "G: live binary version works" scenario_G_live_version
  assert_ok "A: unsigned rejected, live untouched" scenario_A_unsigned
  assert_ok "B: wrong-arch signed fails preflight" scenario_B_wrong_arch "$tag"
  assert_ok "H: default amd64 rejected when host cannot run it (#114)" scenario_H_default_rejected_on_old_glibc "$tag"
  assert_ok "C: success + *.prev ($success_asset)" scenario_C_success_prev "$tag" "$success_asset"
  assert_ok "E: re-upgrade still works" scenario_E_reupgrade
  assert_ok "D: tar.gz URL upgrade ($tgz_asset)" scenario_D_tarball "$tag" "$tgz_asset"
  assert_ok "F: default asset URL warns about variants" scenario_F_default_url_warning
}

run_one_docker() {
  local img="$1"
  local label="$2"
  local force_legacy="${3:-0}"
  local bin_on_host="${4:-$MADMAIL_BIN}"
  if [[ ! -x "$bin_on_host" ]]; then
    red "binary for $label not found: $bin_on_host"
    return 1
  fi
  local tag
  tag="$(resolve_tag)"
  info "Pulling $img ($label)..."
  docker pull -q "$img" >/dev/null

  info "Container matrix: $label ($img) FORCE_LEGACY=$force_legacy bin=$bin_on_host"
  # Verify the under-test binary actually runs in this image (else G fails noisily).
  if ! docker run --rm -v "$bin_on_host:/binaries/madmail:ro" "$img" /binaries/madmail version >/dev/null 2>&1; then
    red "under-test binary cannot run inside $img — rebuild with matching glibc"
    docker run --rm -v "$bin_on_host:/binaries/madmail:ro" "$img" \
      bash -c '/binaries/madmail version' 2>&1 | tail -5 || true
    return 1
  fi

  docker run --rm \
    -v "$ROOT:/src:ro" \
    -v "$bin_on_host:/binaries/madmail:ro" \
    -e MADMAIL_BIN=/binaries/madmail \
    -e RELEASE_TAG="$tag" \
    -e FORCE_LEGACY="$force_legacy" \
    -e KEEP_WORKDIR=0 \
    -e WORKDIR=/tmp/upgrade-safety \
    "$img" \
    bash -c '
      set -euo pipefail
      export DEBIAN_FRONTEND=noninteractive
      apt-get update -qq
      apt-get install -y -qq curl ca-certificates file python3 >/dev/null
      mkdir -p /tmp/upgrade-safety
      bash /src/tests/upgrade-safety.sh --host-only
    '
}

ensure_bookworm_bin() {
  local out="$ROOT/target/bookworm-release/release/madmail"
  if [[ -x "$out" ]] && "$out" version >/dev/null 2>&1; then
    # Prefer a bookworm-built binary if present; still verify it runs in bookworm below.
    echo "$out"
    return
  fi
  if [[ -x "$out" ]]; then
    echo "$out"
    return
  fi
  info "Building fixed madmail against bookworm glibc (for #114 old-distro matrix)..."
  mkdir -p "$ROOT/target/bookworm-release"
  docker pull -q rust:1-bookworm >/dev/null
  docker run --rm \
    -v "$ROOT:/src:rw" \
    -v madmail-cargo-registry:/usr/local/cargo/registry \
    -v madmail-cargo-git:/usr/local/cargo/git \
    -e CARGO_TARGET_DIR=/src/target/bookworm-release \
    -w /src \
    rust:1-bookworm \
    cargo build -p chatmail --release
  if [[ ! -x "$out" ]]; then
    red "bookworm build failed: missing $out"
    return 1
  fi
  echo "$out"
}

run_docker_matrix() {
  local rc=0
  local bookworm_bin
  bookworm_bin="$(ensure_bookworm_bin)" || return 1

  # Bookworm (glibc 2.36): default release needs 2.39 → scenario H; success via legacy.
  run_one_docker "debian:bookworm-slim" "bookworm/old-glibc" 1 "$bookworm_bin" || rc=1
  echo
  # Newer base: default amd64 should pass preflight (success via default asset).
  # Host release build is fine on trixie/new glibc.
  run_one_docker "debian:trixie-slim" "trixie/new-glibc" 0 "$MADMAIL_BIN" || rc=1
  return "$rc"
}

# ─── main ───────────────────────────────────────────────────────────────────

MODE="all"
for arg in "$@"; do
  case "$arg" in
    --docker-only) MODE="docker" ;;
    --host-only) MODE="host" ;;
    -h|--help)
      sed -n '2,35p' "$0"
      exit 0
      ;;
  esac
done

echo "=== madmail upgrade safety harness (issue #114) ==="

case "$MODE" in
  host)
    run_host_scenarios
    ;;
  docker)
    run_docker_matrix
    exit $?
    ;;
  all)
    echo "--- host (may skip root-only / old-glibc scenarios) ---"
    run_host_scenarios || true
    echo
    echo "--- docker matrix ---"
    run_docker_matrix
    ;;
esac

echo
echo "=== summary: $PASS passed, $FAIL failed, $SKIP skipped ==="
if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi
exit 0
