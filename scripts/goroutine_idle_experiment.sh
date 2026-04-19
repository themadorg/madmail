#!/usr/bin/env bash
# Samples maddy goroutines via HTTP pprof while one IMAP client stays idle.
# Case A (auto_logout 0s): after ~70s idle, connection still open → goroutines stay above baseline.
# Case B (auto_logout 1m): server idle deadline is 60s → by ~70s goroutines should match baseline.
# Requires: bash, curl; builds tests/test-client for IMAP idle (go toolchain).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKDIR="${WORKDIR:-/tmp/maddy-goroutine-exp}"
IMAP_PORT="${IMAP_PORT:-19993}"
PPROF_ADDR="${PPROF_ADDR:-127.0.0.1:16060}"
CURL_MAX_TIME="${CURL_MAX_TIME:-5}"
TESTCLIENT="${TESTCLIENT:-$ROOT/tests/test-client/test-client}"
# Client sleeps this long after NOOP while TCP stays open (must be > SAMPLE_CONNECTED_DELAY + SAMPLE_IDLE_DELAY).
# Default 78 → only ~8s silent wait after the ~70s snapshot (was 120 → ~50s of “nothing”).
CLIENT_HOLD_SEC="${CLIENT_HOLD_SEC:-78}"
# Sleep 10s → "connected"; +60s → ~70s wall time → "still idle on wire" snapshot.
SAMPLE_CONNECTED_DELAY="${SAMPLE_CONNECTED_DELAY:-10}"
SAMPLE_IDLE_DELAY="${SAMPLE_IDLE_DELAY:-60}"
BIN="${MADDY_BIN:-$ROOT/maddy}"

mkdir -p "$WORKDIR/state" "$WORKDIR/runtime"

build() {
	(cd "$ROOT" && go build -tags 'debugflags,cgo,!nosqlite3' -o "$BIN" ./cmd/maddy)
	(cd "$ROOT/tests/test-client" && go build -o "$TESTCLIENT" .)
}

goroutine_count() {
	# Go 1.23+ uses "goroutine profile: total N" for debug=1; older releases used one block per goroutine.
	local out
	out="$(curl -sf --max-time "${CURL_MAX_TIME}" "http://${PPROF_ADDR}/debug/pprof/goroutine?debug=1")" || return 0
	local n
	n="$(echo "$out" | awk '/^goroutine profile: total / { print $NF; exit }')"
	if [[ -n "${n:-}" ]]; then
		echo "$n"
		return 0
	fi
	echo "$out" | grep -cE '^goroutine [0-9]+ \[' || echo 0
}

write_config() {
	local auto="$1"
	local cfg="$WORKDIR/maddy.conf"
	cat >"$cfg" <<EOF
state_dir ${WORKDIR}/state
runtime_dir ${WORKDIR}/runtime

storage.imapsql test_store {
	driver sqlite3
	dsn ${WORKDIR}/state/imapsql.db
}

imap tcp://127.0.0.1:${IMAP_PORT} {
	tls off
	auto_logout ${auto}

	auth pass_table static {}
	storage &test_store
}
EOF
}

start_maddy() {
	local cfg="$1"
	"$BIN" -config "$cfg" "-debug.pprof=${PPROF_ADDR}" -log "$WORKDIR/run.log" run &
	local pid=$!
	echo "$pid" >"$WORKDIR/maddy.pid"
	for _ in $(seq 1 50); do
		if curl -sf --max-time "${CURL_MAX_TIME}" "http://${PPROF_ADDR}/debug/pprof/goroutine?debug=1" >/dev/null 2>&1; then
			return 0
		fi
		sleep 0.2
	done
	echo "maddy did not expose pprof in time (see ${WORKDIR}/run.log)" >&2
	kill "$pid" 2>/dev/null || true
	exit 1
}

stop_maddy() {
	if [[ -f "$WORKDIR/maddy.pid" ]]; then
		local pid
		pid=$(cat "$WORKDIR/maddy.pid")
		kill "$pid" 2>/dev/null || true
		wait "$pid" 2>/dev/null || true
		rm -f "$WORKDIR/maddy.pid"
	fi
}

run_idle_client() {
	local timeout_sec=$(( CLIENT_HOLD_SEC + 120 ))
	"$TESTCLIENT" imap \
		-addr "127.0.0.1:${IMAP_PORT}" \
		-security plain \
		-noop \
		-idle "${CLIENT_HOLD_SEC}s" \
		-timeout "${timeout_sec}s"
}

run_case() {
	local name="$1"
	local auto_directive="$2"
	echo ""
	echo "========== ${name} (auto_logout ${auto_directive}) =========="
	rm -f "$WORKDIR/state/imapsql.db"
	write_config "$auto_directive"
	start_maddy "$WORKDIR/maddy.conf"

	sleep 1
	local base connected late_while_hold
	base="$(goroutine_count)"
	base="${base:-0}"
	echo "baseline goroutines (server idle): ${base}"

	run_idle_client &
	local cpid=$!

	sleep "$SAMPLE_CONNECTED_DELAY"
	connected="$(goroutine_count)"
	connected="${connected:-0}"
	echo "~${SAMPLE_CONNECTED_DELAY}s after client start (session up): ${connected} (Δ vs baseline: $((connected - base)))"

	sleep "$SAMPLE_IDLE_DELAY"
	late_while_hold="$(goroutine_count)"
	late_while_hold="${late_while_hold:-0}"
	echo "~$((SAMPLE_CONNECTED_DELAY + SAMPLE_IDLE_DELAY))s after client start, client still holding TCP (test-client -idle): ${late_while_hold} (Δ vs baseline: $((late_while_hold - base)))"

	rem=$(( CLIENT_HOLD_SEC - SAMPLE_CONNECTED_DELAY - SAMPLE_IDLE_DELAY ))
	if (( rem < 0 )); then rem=0; fi
	echo "(waiting for test-client to finish -idle, up to ~${rem}s — script is not stuck)"
	wait "$cpid" || true
	sleep 2
	local after_close
	after_close="$(goroutine_count)"
	after_close="${after_close:-0}"
	echo "after client closed socket (+2s settle): ${after_close} (Δ vs baseline: $((after_close - base)))"

	stop_maddy
}

trap stop_maddy EXIT

echo "Building maddy (tags: debugflags,cgo,!nosqlite3) + tests/test-client ..."
build

echo "pprof=http://${PPROF_ADDR}  imap=127.0.0.1:${IMAP_PORT}  test-client=${TESTCLIENT}"
echo "client holds connection ${CLIENT_HOLD_SEC}s; snapshots at ~${SAMPLE_CONNECTED_DELAY}s and ~$((SAMPLE_CONNECTED_DELAY + SAMPLE_IDLE_DELAY))s"
echo ""

run_case "A — no idle deadline" "0s"
run_case "B — idle deadline 1 minute" "1m"

trap - EXIT
echo ""
echo "Done."
