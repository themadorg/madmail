#!/usr/bin/env bash
# Docker smoke for version manager dual-upgrade (PR #136 / TDD 24).
#
# Host usage:
#   ./tests/version-manager-docker-smoke.sh
#   MADMAIL_BIN=./target/release/madmail ./tests/version-manager-docker-smoke.sh
#   ./tests/version-manager-docker-smoke.sh --inner   # already inside container
#
# Verifies:
#   - signed v2.18.2 then v2.20.0 upgrades leave versions/2.18.2 bit-identical
#   - PATH entry is a symlink into the version tree after first upgrade
#   - versions list / current / path / use / prune / remove --yes
#   - unsigned binary cannot be activated
#   - --json prune/remove still require --yes
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE="${MADMAIL_DOCKER_SMOKE_IMAGE:-ubuntu:24.04}"
MADMAIL_BIN="${MADMAIL_BIN:-$ROOT/target/release/madmail}"
V1_TAG="${V1_TAG:-v2.18.2}"
V2_TAG="${V2_TAG:-v2.20.0}"
ASSET="${ASSET:-madmail-linux-amd64.tar.gz}"

red() { printf '\033[31m%s\033[0m\n' "$*" >&2; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
info() { printf '\033[36m==> %s\033[0m\n' "$*"; }

die() {
  red "FAIL: $*"
  exit 1
}

assert_eq() {
  local want="$1" got="$2" msg="$3"
  if [[ "$want" != "$got" ]]; then
    die "$msg (want=$want got=$got)"
  fi
}

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

run_host() {
  command -v docker >/dev/null || die "docker is required on PATH"
  [[ -x "$MADMAIL_BIN" ]] || die "madmail binary not found/executable: $MADMAIL_BIN (build with: cargo build -p chatmail --release)"

  info "Checking binary runs on host: $MADMAIL_BIN"
  "$MADMAIL_BIN" version >/dev/null

  info "Pulling $IMAGE..."
  docker pull -q "$IMAGE" >/dev/null

  info "Starting container smoke ($IMAGE) with $MADMAIL_BIN"
  docker run --rm \
    --name "madmail-vm-smoke-$$" \
    -v "$ROOT:/src:ro" \
    -v "$MADMAIL_BIN:/binaries/madmail:ro" \
    -e MADMAIL_BIN=/binaries/madmail \
    -e V1_TAG="$V1_TAG" \
    -e V2_TAG="$V2_TAG" \
    -e ASSET="$ASSET" \
    "$IMAGE" \
    bash /src/tests/version-manager-docker-smoke.sh --inner
}

run_inner() {
  export DEBIAN_FRONTEND=noninteractive
  info "Installing curl, ca-certificates, file..."
  apt-get update -qq
  apt-get install -y -qq curl ca-certificates file python3 >/dev/null

  # Always invoke the PR/under-test binary by absolute path. After upgrade #1 the
  # PATH entry may point at an *older* archived release that lacks the version
  # manager dual-upgrade fix — calling `madmail` on PATH would reintroduce clobber.
  local tool="${MADMAIL_BIN:-/binaries/madmail}"
  [[ -x "$tool" ]] || die "binary missing in container: $tool"
  # Keep a private copy so current_exe() is not the read-only mount.
  mkdir -p /opt/madmail-tool
  cp -f "$tool" /opt/madmail-tool/madmail
  chmod 755 /opt/madmail-tool/madmail
  tool=/opt/madmail-tool/madmail

  info "Under-test tool: $($tool version 2>&1 | head -1) ($tool)"
  if ! "$tool" versions --help >/dev/null 2>&1; then
    die "binary does not implement 'madmail versions' — rebuild from PR branch"
  fi

  # Stable PATH layout used by version manager on Unix.
  mkdir -p /usr/local/bin /opt/madmail /tmp/vm-smoke
  # Seed PATH with the under-test tool so first boot looks like a normal install.
  cp -f "$tool" /usr/local/bin/madmail
  chmod 755 /usr/local/bin/madmail
  hash -r 2>/dev/null || true

  # Skip real systemd unit churn for versions use; pointer flips still run.
  export MADMAIL_VERSION_MANAGER_NO_SERVICE=1
  export MADMAIL_INSTALL_ROOT=/opt/madmail

  info "Downloading signed release assets ${V1_TAG} + ${V2_TAG}..."
  local v1_url="https://github.com/themadorg/madmail/releases/download/${V1_TAG}/${ASSET}"
  local v2_url="https://github.com/themadorg/madmail/releases/download/${V2_TAG}/${ASSET}"
  curl -fsSL --retry 3 -o /tmp/vm-smoke/v1.tar.gz "$v1_url"
  curl -fsSL --retry 3 -o /tmp/vm-smoke/v2.tar.gz "$v2_url"

  mkdir -p /tmp/vm-smoke/v1 /tmp/vm-smoke/v2
  tar -xzf /tmp/vm-smoke/v1.tar.gz -C /tmp/vm-smoke/v1
  tar -xzf /tmp/vm-smoke/v2.tar.gz -C /tmp/vm-smoke/v2

  # Locate madmail payload (top-level or nested).
  local v1_bin v2_bin
  v1_bin="$(find /tmp/vm-smoke/v1 -type f -name 'madmail' | head -1)"
  v2_bin="$(find /tmp/vm-smoke/v2 -type f -name 'madmail' | head -1)"
  [[ -n "$v1_bin" && -f "$v1_bin" ]] || die "no madmail member in ${V1_TAG} archive"
  [[ -n "$v2_bin" && -f "$v2_bin" ]] || die "no madmail member in ${V2_TAG} archive"
  chmod 755 "$v1_bin" "$v2_bin"

  info "v1 asset version: $($v1_bin version 2>&1 | head -1)"
  info "v2 asset version: $($v2_bin version 2>&1 | head -1)"

  local v1_sha
  v1_sha="$(sha256_file "$v1_bin")"
  info "v1 payload sha256=$v1_sha"

  local id1="${V1_TAG#v}"
  local id2="${V2_TAG#v}"

  # --- Upgrade #1: seed version tree + PATH symlink (always via $tool) ---
  info "Upgrade #1 → ${V1_TAG} (tool=$tool)"
  "$tool" upgrade "$v1_bin" 2>&1 | tail -40

  local archived1="/opt/madmail/versions/${id1}/madmail"
  [[ -f "$archived1" ]] || die "missing archived binary after upgrade #1: $archived1"
  assert_eq "$v1_sha" "$(sha256_file "$archived1")" "archived v1 sha must match payload"

  if [[ -L /usr/local/bin/madmail ]]; then
    green "PATH entry is symlink after upgrade #1 → $(readlink /usr/local/bin/madmail)"
  else
    die "expected /usr/local/bin/madmail to be a symlink into the version tree after upgrade #1 (file type=$(file /usr/local/bin/madmail))"
  fi

  local resolved1
  resolved1="$(readlink -f /usr/local/bin/madmail)"
  [[ "$resolved1" == *"/versions/${id1}/"* ]] || die "PATH resolves to $resolved1, expected versions/${id1}"

  # Sanity: PATH madmail is the *old* release; upgrading via PATH would clobber.
  # We deliberately keep using $tool (PR binary) for upgrade #2.
  if /usr/local/bin/madmail versions --help >/dev/null 2>&1; then
    info "Note: active PATH binary also has versions CLI"
  else
    info "Note: active PATH binary is older release without versions CLI (expected for ${id1})"
  fi

  # --- Upgrade #2: must NOT clobber versions/<v1> ---
  info "Upgrade #2 → ${V2_TAG} (tool=$tool; PATH currently → $resolved1)"
  "$tool" upgrade "$v2_bin" 2>&1 | tee /tmp/vm-upgrade2.log | tail -40

  # Must take the versioned path, not legacy in-place into versions/2.18.2.
  if grep -q "Target binary (legacy in-place):.*/versions/" /tmp/vm-upgrade2.log; then
    die "upgrade #2 fell back to legacy replace under versions/ — dual-upgrade unsafe"
  fi
  if grep -qE "Target binary: /opt/madmail/versions/" /tmp/vm-upgrade2.log; then
    die "upgrade #2 used old in-place Target binary under versions/ (wrong binary/tool?)"
  fi
  grep -q "Installed candidate under\|Versioned activate\|Active version ${id2}" /tmp/vm-upgrade2.log \
    || die "upgrade #2 did not look like a versioned install (see /tmp/vm-upgrade2.log)"

  local archived2="/opt/madmail/versions/${id2}/madmail"
  [[ -f "$archived2" ]] || die "missing archived binary after upgrade #2: $archived2"
  assert_eq "$v1_sha" "$(sha256_file "$archived1")" \
    "CRITICAL: versions/${id1}/madmail clobbered by upgrade #2 (dual-upgrade regression)"

  local v2_sha
  v2_sha="$(sha256_file "$v2_bin")"
  assert_eq "$v2_sha" "$(sha256_file "$archived2")" "archived v2 sha must match payload"

  local resolved2
  resolved2="$(readlink -f /usr/local/bin/madmail)"
  [[ "$resolved2" == *"/versions/${id2}/"* ]] || die "PATH resolves to $resolved2, expected versions/${id2}"

  green "Dual-upgrade: versions/${id1} unchanged; active → ${id2}"

  # --- CLI: list / current / path (via $tool — always has versions CLI) ---
  info "CLI: versions list / current / path"
  "$tool" versions list
  "$tool" versions current
  "$tool" versions path
  "$tool" versions path "$id1"

  json_version() {
    "$tool" --json versions current | python3 -c \
      'import sys,json; d=json.load(sys.stdin); print(d.get("data",d).get("version") or "")'
  }

  local cur
  cur="$(json_version)"
  assert_eq "$id2" "$cur" "active version after dual upgrade"

  # --- versions use: switch back to v1 (signed) ---
  info "CLI: versions use ${id1}"
  MADMAIL_VERSION_MANAGER_NO_SERVICE=1 "$tool" versions use "$id1" 2>&1 | tail -15
  cur="$(json_version)"
  assert_eq "$id1" "$cur" "active after versions use ${id1}"
  assert_eq "$v1_sha" "$(sha256_file "$(readlink -f /usr/local/bin/madmail)")" "active file is still v1 bytes"

  # switch back to v2
  MADMAIL_VERSION_MANAGER_NO_SERVICE=1 "$tool" versions use "$id2" 2>&1 | tail -10
  cur="$(json_version)"
  assert_eq "$id2" "$cur" "active after versions use ${id2}"

  # --- unsigned reject ---
  info "CLI: unsigned activation must fail"
  mkdir -p "/opt/madmail/versions/9.9.9-unsigned"
  printf '#!/bin/sh\necho madmail-v2 9.9.9-unsigned\n' >"/opt/madmail/versions/9.9.9-unsigned/madmail"
  chmod 755 "/opt/madmail/versions/9.9.9-unsigned/madmail"
  if MADMAIL_VERSION_MANAGER_NO_SERVICE=1 "$tool" versions use 9.9.9-unsigned 2>/tmp/vm-unsigned.err; then
    die "unsigned versions use should have failed"
  fi
  grep -qiE 'signature|INVALID|refusing' /tmp/vm-unsigned.err \
    || die "unsigned use error should mention signature (got: $(cat /tmp/vm-unsigned.err))"
  green "unsigned versions use rejected"

  # --- --yes required even with --json ---
  info "CLI: --json prune/remove require --yes"
  if "$tool" --json versions remove 9.9.9-unsigned 2>/tmp/vm-yes.err; then
    die "--json versions remove without --yes should fail"
  fi
  grep -qi 'requires --yes' /tmp/vm-yes.err || die "expected requires --yes (got: $(cat /tmp/vm-yes.err))"

  if "$tool" --json versions prune --keep 0 2>/tmp/vm-prune.err; then
    die "--json versions prune without --yes should fail"
  fi
  grep -qi 'requires --yes' /tmp/vm-prune.err || die "expected prune requires --yes"

  "$tool" versions remove 9.9.9-unsigned --yes
  [[ ! -d /opt/madmail/versions/9.9.9-unsigned ]] || die "unsigned version dir still present"

  # prune should not remove active; keep 0 non-active → remove only non-active
  "$tool" versions prune --keep 0 --yes
  [[ -f "$archived2" ]] || die "prune removed active ${id2}"
  # v1 may be pruned (non-active) — that is OK; dual-upgrade already checked

  # --- remote list (network) ---
  info "CLI: versions list --remote (metadata only)"
  if "$tool" versions list --remote 2>&1 | tee /tmp/vm-remote.out | grep -qE '2\.[0-9]'; then
    green "versions list --remote returned release tags"
  else
    # Network flakes should not hard-fail the dual-upgrade proof.
    red "WARN: versions list --remote returned no version-looking lines (network?)"
    cat /tmp/vm-remote.out || true
  fi

  green "ALL DOCKER SMOKE CHECKS PASSED"
  echo
  echo "Summary:"
  echo "  tool binary:   $tool"
  echo "  install root:  /opt/madmail"
  echo "  dual-upgrade:  ${id1} sha preserved across upgrade to ${id2}"
  echo "  versions use:  signed switch ok; unsigned rejected"
  echo "  --yes:         required for prune/remove even with --json"
}

case "${1:-}" in
  --inner) run_inner ;;
  *) run_host ;;
esac
