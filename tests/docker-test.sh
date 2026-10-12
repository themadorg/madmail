#!/usr/bin/env bash
# Test the shipping image with fresh state; never reuse docker-up resources.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
for tool in docker grep sed cmp; do
    command -v "$tool" >/dev/null || { echo "test-docker requires $tool" >&2; exit 1; }
done
docker info >/dev/null
mkdir -p target/docker-tests
WORK="$(mktemp -d "$ROOT/target/docker-tests/run-XXXXXXXX")"
NAME="madmail-test-$(basename "$WORK")"
IMAGE="${MADMAIL_DOCKER_TEST_IMAGE:-madmail-local:test-docker}"
BUILD_IMAGE="${IMAGE}-build"
TEST_IMAGE="${IMAGE}-workspace"
LANDING_IMAGE="${IMAGE}-landing"
STATE="${NAME}-state"
CONFIG="${NAME}-config"
RELAY_MIN="${MADMAIL_DOCKER_TEST_RELAY_MIN:-55000}"
RELAY_MAX="${MADMAIL_DOCKER_TEST_RELAY_MAX:-55010}"
if [[ ! "$RELAY_MIN" =~ ^[0-9]+$ || ! "$RELAY_MAX" =~ ^[0-9]+$ ]] ||
    (( RELAY_MIN < 1024 || RELAY_MAX > 65535 || RELAY_MIN > RELAY_MAX ||
       (RELAY_MIN <= 3478 && RELAY_MAX >= 3478) )); then
    echo "Invalid test relay range (use 1024-65535, excluding 3478)" >&2
    exit 1
fi

cleanup() {
    local result=$?
    trap - EXIT
    if docker inspect "$NAME" >/dev/null 2>&1; then
        docker logs "$NAME" > "$WORK/container.log" 2>&1 || true
    fi
    if (( result != 0 )); then
        echo "Docker tests failed; logs: $WORK" >&2
        [[ ! -f "$WORK/container.log" ]] || tail -40 "$WORK/container.log" >&2
    fi
    docker rm -fv "$NAME" >/dev/null 2>&1 || true
    docker rm -f "${NAME}-suite" "${NAME}-client" >/dev/null 2>&1 || true
    docker volume rm "$STATE" "$CONFIG" >/dev/null 2>&1 || true
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "Building $IMAGE (log: $WORK/build.log)"
if ! docker build -t "$IMAGE" . > "$WORK/build.log" 2>&1; then
    tail -60 "$WORK/build.log" >&2
    exit 1
fi
echo "Building Docker workspace test runner (log: $WORK/test-build.log)"
if ! docker build --target build-env -t "$BUILD_IMAGE" . > "$WORK/test-build.log" 2>&1 ||
    ! docker build -f tests/Dockerfile --build-arg "BUILD_IMAGE=$BUILD_IMAGE" \
    -t "$TEST_IMAGE" . >> "$WORK/test-build.log" 2>&1; then
    tail -60 "$WORK/test-build.log" >&2
    exit 1
fi
echo "Running all Rust workspace tests inside Docker (log: $WORK/workspace.log)"
if ! docker run --rm --name "${NAME}-suite" "$TEST_IMAGE" > "$WORK/workspace.log" 2>&1; then
    tail -80 "$WORK/workspace.log" >&2
    exit 1
fi
grep 'test result:' "$WORK/workspace.log"
echo "PASS: Rust workspace tests in Docker"
echo "Running landing site tests inside Docker (log: $WORK/landing.log)"
if ! docker build -f tests/Dockerfile --target landing-tests -t "$LANDING_IMAGE" . \
    > "$WORK/landing-build.log" 2>&1; then
    tail -60 "$WORK/landing-build.log" >&2
    exit 1
fi
if ! docker run --rm --name "${NAME}-suite" "$LANDING_IMAGE" > "$WORK/landing.log" 2>&1; then
    cat "$WORK/landing.log" >&2
    exit 1
fi
echo "PASS: landing site tests in Docker"
docker volume create "$STATE" >/dev/null
docker volume create "$CONFIG" >/dev/null
echo "Installing fresh self-signed IP relay"
if ! docker run --rm -v "$STATE:/var/lib/madmail" -v "$CONFIG:/etc/madmail" \
    "$IMAGE" install --simple --ip 127.0.0.1 --tls-mode self_signed \
    --skip-systemd --skip-user --non-interactive > "$WORK/install.log" 2>&1; then
    tail -60 "$WORK/install.log" >&2
    exit 1
fi
# Set the relay range in the native config before first boot.
docker run --rm --entrypoint sh -v "$CONFIG:/etc/madmail" "$IMAGE" -c \
    'sed -i "s/^[$](primary_domain) =.*/\$(primary_domain) = [127.0.0.1] b.com c.com/" /etc/madmail/madmail.conf; sed -i "/^turn udp:/a\\    relay_port_min $1\n    relay_port_max $2" /etc/madmail/madmail.conf' \
    sh "$RELAY_MIN" "$RELAY_MAX"
docker run -d --name "$NAME" --restart=no \
    -p 127.0.0.1::443 -p 127.0.0.1::465 -p 127.0.0.1::587 -p 127.0.0.1::993 \
    -p 127.0.0.1::3478/udp -p "127.0.0.1:$RELAY_MIN-$RELAY_MAX:$RELAY_MIN-$RELAY_MAX/udp" \
    -v "$STATE:/var/lib/madmail" -v "$CONFIG:/etc/madmail:ro" "$IMAGE" >/dev/null
# Client checks share the relay's network namespace and run in Docker too.
HTTPS="https://127.0.0.1:443"
SMTP_TLS="smtps://127.0.0.1:465"
SMTP_STARTTLS="smtp://127.0.0.1:587"
IMAP="imaps://127.0.0.1:993"
http() {
    docker run --rm --name "${NAME}-client" --network "container:$NAME" \
        -v "$WORK:$WORK" --entrypoint curl "$TEST_IMAGE" \
        -ksS --connect-timeout 2 --max-time 10 "$@"
}
ready() {
    local attempt
    for attempt in {1..60}; do
        if [[ "$(http -o /dev/null -w '%{http_code}' "$HTTPS/" 2>/dev/null || true)" = 200 ]]; then
            return
        fi
        [[ "$(docker inspect -f '{{.State.Running}}' "$NAME")" = true ]] || return 1
        sleep 1
    done
    echo "Timed out waiting for HTTPS" >&2
    return 1
}
ready
docker run --rm -i --name "${NAME}-client" --network "container:$NAME" \
    --entrypoint python3 "$TEST_IMAGE" - < tests/multidomain-docker.py
echo "PASS: multi-domain page and API registration, IMAP/SMTP login, domain allowlist"
ALICE='alice@[127.0.0.1]'
BOB='bob@[127.0.0.1]'
PASSWORD='docker-smoke-test-password'
docker exec "$NAME" madmail accounts create "$ALICE" --password "$PASSWORD"
docker exec "$NAME" madmail accounts create "$BOB" --password "$PASSWORD"
docker exec "$NAME" madmail admin-web enable
docker restart "$NAME" >/dev/null
ready
[[ "$(http -o "$WORK/admin.html" -w '%{http_code}' "$HTTPS/admin/")" = 200 ]]
! grep -q 'Admin Web UI Not Available' "$WORK/admin.html"
grep -q '_app/' "$WORK/admin.html"
echo "PASS: install, HTTPS, accounts, embedded dashboard"

ADMIN_TOKEN="$(docker exec "$NAME" madmail admin-token --raw)"
http -H 'Content-Type: application/json' \
    -d '{"method":"GET","resource":"/admin/status"}' "$HTTPS/api/admin" > "$WORK/unauthorized.json"
grep -q '"status":401' "$WORK/unauthorized.json"
http -H 'Content-Type: application/json' \
    -d "{\"method\":\"GET\",\"resource\":\"/admin/status\",\"headers\":{\"Authorization\":\"Bearer $ADMIN_TOKEN\"}}" \
    "$HTTPS/api/admin" > "$WORK/status.json"
grep -q '"status":200' "$WORK/status.json"
echo "PASS: admin API authentication"

cat > "$WORK/message.eml" <<EOF
From: $ALICE
To: $BOB
Subject: Docker smoke delivery
MIME-Version: 1.0
Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"; boundary="test"

--test
Content-Type: application/pgp-encrypted

Version: 1

--test
Content-Type: application/octet-stream

-----BEGIN PGP MESSAGE-----
Docker protocol fixture (not real ciphertext)
-----END PGP MESSAGE-----
--test--
EOF
send() {
    http --sasl-ir --user "$ALICE:$PASSWORD" --mail-from "$ALICE" \
        --mail-rcpt "$BOB" --upload-file "$WORK/message.eml" "$@"
}
send "$SMTP_TLS"
send --ssl-reqd "$SMTP_STARTTLS"
fetch() {
    http --login-options 'AUTH=+LOGIN' --user "$BOB:$PASSWORD" "$IMAP/INBOX/;UID=1" -o "$1"
}
fetch "$WORK/received.eml"
grep -q 'Docker smoke delivery' "$WORK/received.eml"
http --login-options 'AUTH=+LOGIN' --user "$BOB:$PASSWORD" "$IMAP/INBOX" \
    -X 'STATUS INBOX (MESSAGES)' > "$WORK/mailbox.txt"
grep -q 'MESSAGES 2 ' "$WORK/mailbox.txt"
if http --login-options 'AUTH=+LOGIN' --user "$BOB:wrong-password" "$IMAP/INBOX" \
    -X 'STATUS INBOX (MESSAGES)' > "$WORK/wrong-password.log" 2>&1; then
    echo "Wrong-password IMAP login unexpectedly succeeded" >&2
    exit 1
fi
printf 'From: %s\nTo: %s\nSubject: plaintext\n\nUnencrypted test\n' "$ALICE" "$BOB" > "$WORK/plaintext.eml"
if http -v --sasl-ir --user "$ALICE:$PASSWORD" --mail-from "$ALICE" --mail-rcpt "$BOB" \
    --upload-file "$WORK/plaintext.eml" "$SMTP_TLS" > "$WORK/plaintext.log" 2>&1; then
    echo "Plaintext delivery unexpectedly succeeded" >&2
    exit 1
fi
grep -q '523 5.7.1 Encryption Needed' "$WORK/plaintext.log"
echo "PASS: SMTP TLS/STARTTLS, IMAP retrieval, wrong password and plaintext rejection"
docker restart "$NAME" >/dev/null
ready
fetch "$WORK/after-restart.eml"
cmp "$WORK/received.eml" "$WORK/after-restart.eml"
echo "PASS: message persistence after restart"

TURN_SECRET="$(docker exec "$NAME" sh -c "awk '/^[[:space:]]*turn_secret / {print \$2; exit}' /etc/madmail/madmail.conf")"
export DOCKER_TURN_IMAP_ADDR="127.0.0.1:993"
# TURN binds non-loopback interfaces when configured with 0.0.0.0.
RELAY_IP="$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$NAME")"
export DOCKER_TURN_CONTROL_ADDR="$RELAY_IP:3478" DOCKER_TURN_ADVERTISED_PORT=3478
export DOCKER_TURN_USER="$BOB" DOCKER_TURN_PASS="$PASSWORD" DOCKER_TURN_SECRET="$TURN_SECRET"
export DOCKER_TURN_RELAY_MIN="$RELAY_MIN" DOCKER_TURN_RELAY_MAX="$RELAY_MAX"
if ! docker run --rm --name "${NAME}-client" --network "container:$NAME" \
    -e DOCKER_TURN_IMAP_ADDR -e DOCKER_TURN_CONTROL_ADDR -e DOCKER_TURN_ADVERTISED_PORT \
    -e DOCKER_TURN_USER -e DOCKER_TURN_PASS -e DOCKER_TURN_SECRET \
    -e DOCKER_TURN_RELAY_MIN -e DOCKER_TURN_RELAY_MAX "$TEST_IMAGE" \
    cargo test -p chatmail-integration --test docker_turn_e2e --locked --offline -- --ignored \
    > "$WORK/turn.log" 2>&1; then
    cat "$WORK/turn.log" >&2
    exit 1
fi
echo "PASS: live Docker IMAP metadata and TURN allocation"
echo "Docker tests passed. Logs: $WORK"
