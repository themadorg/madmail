#!/usr/bin/env bash
# Isolated shipping-image E2E with a Rust native-mail WebSocket client.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
for tool in docker cargo openssl curl; do
    command -v "$tool" >/dev/null || { echo "Required tool: $tool" >&2; exit 1; }
done
IMAGE="${MADMAIL_BRIDGE_TEST_IMAGE:-madmail-local:websocket-bridge-test}"
WORK="$(mktemp -d)"
NAME="madmail-bridge-test-$$"
cleanup() {
    local result=$?
    trap - EXIT
    docker logs "$NAME" > "$WORK/server.log" 2>&1 || true
    if (( result != 0 )); then cat "$WORK/server.log" >&2; fi
    docker rm -fv "$NAME" >/dev/null 2>&1 || true
    rm -rf "$WORK"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ "${MADMAIL_BRIDGE_TEST_SKIP_BUILD:-0}" != 1 ]]; then
    docker build -t "$IMAGE" .
fi
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=localhost \
    -addext subjectAltName=DNS:localhost -addext basicConstraints=critical,CA:FALSE \
    -addext extendedKeyUsage=serverAuth -keyout "$WORK/key.pem" -out "$WORK/cert.pem" >/dev/null 2>&1
cat > "$WORK/madmail.conf" <<'CONF'
$(hostname) = localhost
$(primary_domain) = test
state_dir /var/lib/madmail
runtime_dir /run/madmail
tls file /etc/madmail/cert.pem /etc/madmail/key.pem
auth.pass_table local_authdb {
 table sql_table {
  driver sqlite3
  dsn credentials.db
  table_name passwords
 }
}
storage.imapsql local_mailboxes {
 driver sqlite3
 dsn imapsql.db
}
smtp tcp://0.0.0.0:2525 {
 hostname localhost
}
submission tls://0.0.0.0:1465 {
 auth &local_authdb
}
imap tls://0.0.0.0:1993 tcp://0.0.0.0:1143 {
 auth &local_authdb
}
chatmail tls://0.0.0.0:8443 tcp://0.0.0.0:8080 {
 mail_domain test
 admin_token bridge-e2e-token-012345678901234567890123456789
}
CONF
docker run -d --name "$NAME" --add-host host.docker.internal:host-gateway -v "$WORK:/etc/madmail:ro" \
    -p 127.0.0.1::8443 "$IMAGE" >/dev/null
PORT="$(docker port "$NAME" 8443/tcp | sed 's/.*://')"
URL="https://localhost:$PORT"
ready=0
for attempt in $(seq 1 60); do
    if curl --silent --fail --cacert "$WORK/cert.pem" "$URL/" >/dev/null; then ready=1; break; fi
    sleep 1
done
[[ "$ready" == 1 ]] || { echo "Server did not become ready" >&2; exit 1; }
docker exec "$NAME" /bin/madmail --config /etc/madmail/madmail.conf accounts create bridgeuser@test --password bridge-secret
rpc() {
    local payload="${1:1}"
    curl --silent --show-error --fail --cacert "$WORK/cert.pem" \
        -H 'Authorization: Bearer bridge-e2e-token-012345678901234567890123456789' \
        -H 'Content-Type: application/json' --data "{\"headers\":{\"Authorization\":\"Bearer bridge-e2e-token-012345678901234567890123456789\"},$payload" "$URL/api/admin"
}
client() {
    BRIDGE_TEST_CONTAINER="$NAME" BRIDGE_TEST_ADDR="127.0.0.1:$PORT" BRIDGE_TEST_CA="$WORK/cert.pem" BRIDGE_TEST_MODE="$1" \
        cargo test -p chatmail-integration --test websocket_bridge_e2e --locked docker_websocket_bridge -- --ignored --exact
}
client disabled
# Exercise both CLI and API controls on a live server, without a restart.
docker exec "$NAME" /bin/madmail --config /etc/madmail/madmail.conf websocket-bridge enable
rpc '{"method":"GET","resource":"/admin/services/websocket_bridge"}' | grep -q 'enabled'
rpc '{"method":"POST","resource":"/admin/services/websocket_bridge","body":{"action":"disable"}}' | grep -q 'disabled'
client disabled
rpc '{"method":"POST","resource":"/admin/services/websocket_bridge","body":{"action":"enable"}}' | grep -q 'enabled'
# A real browser client is accepted only with an explicit origin allowlist.
docker exec "$NAME" /bin/madmail --config /etc/madmail/madmail.conf webmail-cors add https://client.example
client enabled
docker exec "$NAME" /bin/madmail --config /etc/madmail/madmail.conf websocket-bridge disable
client disabled
echo 'PASS: Docker WSS bridge, native mail, and live CLI/API controls'
