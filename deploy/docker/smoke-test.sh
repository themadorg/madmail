#!/usr/bin/env bash
# Smoke-test deploy/docker/compose.yaml against a local image:
# bootstrap install (IP relay, self-signed), start, health, SMTP banner,
# IMAPS/HTTPS handshake, then tear everything down.
#
#   deploy/docker/smoke-test.sh [image]      # default: madmail-local:smoke
#
# Uses high host ports on 127.0.0.1 so it does not clash with a real server.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
IMAGE="${1:-madmail-local:smoke}"
PROJECT="madmail-smoke-$$"
WORK="$(mktemp -d)"
OVERRIDE="$WORK/compose.smoke.yaml"

cleanup() {
	status=$?
	if [[ $status -ne 0 ]]; then
		docker compose -p "$PROJECT" -f "$HERE/compose.yaml" -f "$OVERRIDE" logs --no-color || true
	fi
	docker compose -p "$PROJECT" -f "$HERE/compose.yaml" -f "$OVERRIDE" down -v --remove-orphans >/dev/null 2>&1 || true
	# Files were written by root inside the container.
	docker run --rm -v "$WORK:/w" --entrypoint /bin/sh "$IMAGE" -c 'rm -rf /w/lib /w/etc' >/dev/null 2>&1 || true
	rm -rf "$WORK"
	exit $status
}
trap cleanup EXIT

mkdir -p "$WORK/lib" "$WORK/etc"

# Remap published ports to 127.0.0.1:<high> and shrink the TURN range.
cat >"$OVERRIDE" <<'EOF'
services:
  bootstrap:
    ports: !reset []
  madmail:
    container_name: !reset null
    ports: !override
      - "127.0.0.1:12525:25"
      - "127.0.0.1:11443:443"
      - "127.0.0.1:11993:993"
EOF

export MADMAIL_IMAGE="$IMAGE"
export MADMAIL_PUBLIC_IP=127.0.0.1
export MADMAIL_TLS_MODE=self_signed
export MADMAIL_STATE_DIR="$WORK/lib"
export MADMAIL_CONFIG_DIR="$WORK/etc"
compose() { docker compose -p "$PROJECT" --env-file /dev/null -f "$HERE/compose.yaml" -f "$OVERRIDE" "$@"; }

echo "-- compose up ($IMAGE)"
compose up -d --wait --wait-timeout 180

echo "-- second bootstrap run must skip install"
compose run --rm bootstrap | grep -q 'skipping install'

echo "-- SMTP banner"
banner="$(docker run --rm --network host --entrypoint /bin/sh "$IMAGE" -c \
	"printf 'QUIT\r\n' | nc -w 5 127.0.0.1 12525 | head -n1")"
echo "   $banner"
[[ "$banner" == 220* ]]

echo "-- TLS handshakes"
for port in 11993 11443; do
	docker run --rm --network host --entrypoint /bin/sh "$IMAGE" -c \
		"nc -z -w 5 127.0.0.1 $port"
	echo | openssl s_client -connect "127.0.0.1:$port" -servername 127.0.0.1 2>/dev/null \
		| grep -q 'BEGIN CERTIFICATE' || { echo "no TLS on $port" >&2; exit 1; }
	echo "   ok: $port"
done

echo "-- admin token"
compose exec -T madmail madmail admin-token --raw | grep -qE '^[A-Za-z0-9_-]{16,}$'

echo "== compose smoke test passed"
