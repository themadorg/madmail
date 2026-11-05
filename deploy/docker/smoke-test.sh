#!/usr/bin/env bash
# Smoke-test deploy/docker/compose.yaml against a local image:
# bootstrap install (IP relay, self-signed), start, health, non-root runtime
# and file modes, SMTP banner, IMAPS greeting, HTTPS page, admin defaults,
# then tear everything down.
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

# Publish only a few ports on 127.0.0.1:<high> (no TURN range).
cat >"$OVERRIDE" <<'EOF'
services:
  madmail:
    container_name: !reset null
    ports: !override
      - "127.0.0.1:12525:25"
      - "127.0.0.1:11443:443"
      - "127.0.0.1:11993:993"
EOF

# HTTP status of a request to the self-signed listener.
https_code() { curl -sk --max-time 10 -o /dev/null -w '%{http_code}' "$@"; }

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
	# Capture first: grep -q closing the pipe early would fail under pipefail.
	out="$(echo | openssl s_client -connect "127.0.0.1:$port" -servername 127.0.0.1 2>/dev/null || true)"
	grep -q 'BEGIN CERTIFICATE' <<<"$out" || { echo "no TLS on $port" >&2; exit 1; }
	echo "   ok: $port"
done

echo "-- runs as non-root, state and config closed to other users"
uid="$(compose exec -T madmail id -u)"
[[ "$uid" != 0 ]] || { echo "madmail runs as root" >&2; exit 1; }
modes="$(compose exec -T madmail stat -c '%a %n' /var/lib/madmail /etc/madmail /etc/madmail/madmail.conf)"
sed 's/^/   /' <<<"$modes"
[[ "$modes" == $'700 /var/lib/madmail\n700 /etc/madmail\n600 /etc/madmail/madmail.conf' ]] ||
	{ echo "unexpected modes" >&2; exit 1; }

echo "-- IMAPS greeting"
greeting="$(printf 'a LOGOUT\r\n' | openssl s_client -connect 127.0.0.1:11993 -quiet 2>/dev/null | head -n1 || true)"
echo "   $greeting"
[[ "$greeting" == '* OK'* ]] || { echo "no IMAP greeting" >&2; exit 1; }

echo "-- HTTPS registration page and admin defaults"
[[ "$(https_code https://127.0.0.1:11443/)" == 200 ]] || { echo "no registration page" >&2; exit 1; }
# The admin web UI stays off until `madmail admin-web enable`.
[[ "$(https_code https://127.0.0.1:11443/admin/)" == 404 ]] || { echo "admin web UI is exposed" >&2; exit 1; }
# The admin API refuses requests without the token.
body="$(curl -sk --max-time 10 -H 'content-type: application/json' \
	-d '{"method":"GET","resource":"/admin/status"}' https://127.0.0.1:11443/api/admin)"
[[ "$body" == *'"status":401'* ]] || { echo "admin API answered without a token: $body" >&2; exit 1; }
echo "   ok"

echo "-- admin token"
token="$(compose exec -T madmail madmail admin-token --raw)"
[[ ${#token} -ge 32 ]] || { echo "unexpected admin token" >&2; exit 1; }

echo "== compose smoke test passed"
