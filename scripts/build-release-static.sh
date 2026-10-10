#!/usr/bin/env bash
# Build chatmail statically linked (runs on Debian 12+ and other Linux x86_64 without matching glibc).
set -euo pipefail
cd "$(dirname "$0")/.."

ADMIN_WEB_DIR="${ADMIN_WEB_DIR:-external/madmail-admin-web}"
echo "-- Building admin-web from ${ADMIN_WEB_DIR}..."
make build-admin-web

# Force chatmail-admin-web/build.rs to re-copy SPA assets into embed/.
# Cargo incremental builds may otherwise skip the embed crate when only JS/CSS changed.
touch crates/chatmail-admin-web/build.rs

echo "-- Building madmail (release, static-pie)..."
CHATMAIL_ADMIN_WEB_BUILD="$(pwd)/${ADMIN_WEB_DIR}/build" \
  cargo rustc -p chatmail --bin madmail --release --locked -- -C target-feature=+crt-static

bin=target/release/madmail
echo "-- Verifying $bin"
file "$bin"
if ! ldd "$bin" 2>&1 | grep -q 'not a dynamic executable\|statically linked'; then
  echo "ERROR: binary is not fully static:" >&2
  ldd "$bin" >&2 || true
  exit 1
fi
echo "OK: statically linked (deployable to Debian and other Linux x86_64 servers)"
