#!/usr/bin/env bash
# Regression for #230: real server restarts with persisted PostgreSQL statistics.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
for tool in docker openssl curl cargo; do
    command -v "$tool" >/dev/null || { echo "Required tool: $tool" >&2; exit 1; }
done
IMAGE="${MADMAIL_POSTGRES_TEST_IMAGE:-madmail-local:postgres-restart-test}"
PG_IMAGE="${MADMAIL_POSTGRES_TEST_DB_IMAGE:-postgres:18-alpine}"
mkdir -p target/postgres-restart-tests
WORK="$(mktemp -d "$ROOT/target/postgres-restart-tests/run-XXXXXXXX")"
NAME="madmail-pg-test-$$"
cleanup() {
    local result=$?
    trap - EXIT
    docker logs "$NAME" > "$WORK/server.log" 2>&1 || true
    docker logs "$NAME-db" > "$WORK/postgres.log" 2>&1 || true
    if (( result != 0 )); then cat "$WORK/server.log" >&2; fi
    docker rm -fv "$NAME" "$NAME-db" >/dev/null 2>&1 || true
    docker network rm "$NAME" >/dev/null 2>&1 || true
    echo "Logs: $WORK"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ "${MADMAIL_POSTGRES_TEST_SKIP_BUILD:-0}" != 1 ]]; then
    docker build -t "$IMAGE" .
fi
docker network create "$NAME" >/dev/null
docker run -d --name "$NAME-db" --network "$NAME" --network-alias postgres \
    -p 127.0.0.1::5432 -e POSTGRES_PASSWORD=regression-only -e POSTGRES_DB=madmail "$PG_IMAGE" >/dev/null
ready=0
for attempt in $(seq 1 60); do
    if docker exec "$NAME-db" pg_isready -U postgres -d madmail >/dev/null 2>&1; then ready=1; break; fi
    sleep 1
done
[[ "$ready" == 1 ]] || { echo 'PostgreSQL did not become ready' >&2; exit 1; }
sql() { docker exec -i "$NAME-db" psql -X -v ON_ERROR_STOP=1 -U postgres -d madmail -Atc "$1"; }
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
  driver postgres
  dsn postgres://postgres:regression-only@postgres/madmail
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
}
CONF
docker run -d --name "$NAME" --network "$NAME" -v "$WORK:/etc/madmail:ro" \
    -p 127.0.0.1::8443 "$IMAGE" >/dev/null
wait_ready() {
    local PORT
    PORT="$(docker port "$NAME" 8443/tcp | sed 's/.*://')"
    for attempt in $(seq 1 60); do
        [[ "$(docker inspect -f '{{.State.Running}}' "$NAME")" == true ]] || return 1
        # The unauthenticated preflight is denied, proving HTTPS routing is live.
        if [[ "$(curl --silent --request OPTIONS --max-time 2 --cacert "$WORK/cert.pem" -o /dev/null -w '%{http_code}' "https://localhost:$PORT/webimap/send")" == 403 ]]; then return 0; fi
        sleep 1
    done
    echo 'Madmail did not become ready' >&2
    return 1
}
wait_ready
echo 'PASS: empty PostgreSQL cold start'
docker stop "$NAME" >/dev/null
sql "INSERT INTO federation_server_stats VALUES ('peer.example.invalid',1,2,3,4,5,6,7,8,9,10,11)"
BEFORE="$(sql 'SELECT * FROM federation_server_stats ORDER BY domain')"
docker start "$NAME" >/dev/null
wait_ready
[[ "$(sql 'SELECT * FROM federation_server_stats ORDER BY domain')" == "$BEFORE" ]]
echo 'PASS: populated PostgreSQL restart preserves every counter'

assert_bigint() {
    [[ "$(sql "SELECT count(*) FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'federation_server_stats' AND data_type = 'bigint'")" == 11 ]]
}
assert_bigint
# Reconstruct an already-migrated pre-fix database without editing migration
# checksums. The populated table must be upgraded before startup hydration.
docker stop "$NAME" >/dev/null
sql "DELETE FROM _sqlx_migrations WHERE version = 20240701000000"
downgrade_counters() {
    sql "DO \$\$ DECLARE col TEXT; BEGIN
        FOR col IN SELECT column_name FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'federation_server_stats' AND data_type = 'bigint'
        LOOP EXECUTE format('ALTER TABLE federation_server_stats ALTER COLUMN %I TYPE INTEGER', col); END LOOP;
    END \$\$;"
}
downgrade_counters
[[ "$(sql "SELECT count(*) FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'federation_server_stats' AND data_type = 'integer'")" == 11 ]]
docker start "$NAME" >/dev/null
wait_ready
assert_bigint
[[ "$(sql 'SELECT * FROM federation_server_stats ORDER BY domain')" == "$BEFORE" ]]
echo 'PASS: populated pre-fix schema upgrades without checksum errors or lost counters'
# Legacy imports bypass sqlx migrations and may use Go HTTPS column names.
docker stop "$NAME" >/dev/null
sql 'DROP TABLE _sqlx_migrations'
downgrade_counters
sql 'ALTER TABLE federation_server_stats RENAME COLUMN failed_https TO failed_http_s;
     ALTER TABLE federation_server_stats RENAME COLUMN success_https TO success_http_s;'
docker start "$NAME" >/dev/null
wait_ready
assert_bigint
[[ "$(sql 'SELECT * FROM federation_server_stats ORDER BY domain')" == "$BEFORE" ]]
echo 'PASS: legacy INT4 counters and Go column names upgrade safely'
# Exercise every field above INT4 range, plus idempotent legacy startup.
docker stop "$NAME" >/dev/null
sql "DO \$\$ DECLARE col TEXT; BEGIN
    FOR col IN SELECT column_name FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'federation_server_stats' AND data_type = 'bigint'
    LOOP EXECUTE format('UPDATE federation_server_stats SET %I = 3000000000', col); END LOOP;
END \$\$;"
BEFORE="$(sql 'SELECT * FROM federation_server_stats ORDER BY domain')"
docker start "$NAME" >/dev/null
wait_ready
[[ "$(sql 'SELECT * FROM federation_server_stats ORDER BY domain')" == "$BEFORE" ]]
echo 'PASS: all 64-bit counters survive a second legacy restart'
# Use the real runtime tracker/flusher against a separate empty database.
sql 'CREATE DATABASE tracker_runtime'
PG_PORT="$(docker port "$NAME-db" 5432/tcp | sed 's/.*://')"
MADMAIL_POSTGRES_TEST_DSN="postgres://postgres:regression-only@127.0.0.1:$PG_PORT/tracker_runtime" \
    cargo test --locked -p chatmail-state --test postgres_restart -- --ignored --exact postgres_runtime_stats_survive_restart
echo 'PASS: runtime persistence and hydration with latency above INT4 range'
