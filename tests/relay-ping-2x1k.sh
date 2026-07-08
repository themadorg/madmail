#!/usr/bin/env bash
# 2×1000 encrypted SMTP send + IMAP IDLE receive in parallel; must finish in 3 minutes with 100% delivery.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RP="${RELAY_PING_BIN:-$ROOT/context/relay-ping/bin/relay-ping}"
DOMAIN="${RELAY_DOMAIN:-http://10.243.14.182/}"
COUNT="${RELAY_COUNT:-1000}"
WORKERS="${RELAY_WORKERS:-50}"
TIMEOUT="${RELAY_TIMEOUT:-3m}"
OUT="${RELAY_OUT:-/tmp/relay-ping-2x1k-$$}"

if [[ ! -x "$RP" ]]; then
	echo "relay-ping not built; run: make relay-ping-build" >&2
	exit 1
fi

mkdir -p "$OUT"/run1/tmp "$OUT"/run2/tmp

echo "== relay-ping 2×${COUNT} (workers=${WORKERS}, timeout=${TIMEOUT}, IMAP IDLE) =="
echo "   domain: $DOMAIN"
echo "   out:    $OUT"

start=$(date +%s)
deadline=$((start + 180))

run_one() {
	local id=$1
	local dir="$OUT/run$id"
	local result="$dir/result.txt"
	cd "$dir"
	if ! timeout --foreground "$TIMEOUT" "$RP" \
		-test throughput \
		-domain "$DOMAIN" \
		-insecure \
		-count "$COUNT" \
		-workers "$WORKERS" \
		-timeout "$TIMEOUT" \
		-log-file "run.log" \
		-color never >"$result" 2>&1; then
		echo "run$id: relay-ping exited non-zero" >&2
		return 1
	fi
	local delivered accepted
	delivered=$(grep -oE 'delivery\s*:\s*[0-9]+' "$result" | tail -1 | grep -oE '[0-9]+' || echo 0)
	accepted=$(grep -oE 'accepted\s*:\s*[0-9]+' "$result" | tail -1 | grep -oE '[0-9]+' || echo 0)
	if [[ -z "$delivered" || "$delivered" -lt "$COUNT" ]]; then
		delivered=$(grep -oE 'delivered=[0-9]+' "$result" | tail -1 | cut -d= -f2 || echo 0)
	fi
	echo "run$id: accepted=${accepted:-?} delivered=${delivered:-0}/${COUNT}"
	if [[ "${delivered:-0}" -ne "$COUNT" ]]; then
		echo "run$id: FAIL — not all messages delivered" >&2
		tail -20 "$result" >&2 || true
		return 1
	fi
	return 0
}

export -f run_one
export RP DOMAIN COUNT WORKERS TIMEOUT OUT

fail=0
seq 1 2 | xargs -P 2 -I{} bash -c 'run_one "$@"' _ {} || fail=1

end=$(date +%s)
elapsed=$((end - start))
echo "elapsed: ${elapsed}s (limit 180s)"

if (( elapsed > 180 )); then
	echo "FAIL — exceeded 3 minute wall clock" >&2
	fail=1
fi

if (( fail != 0 )); then
	echo "FAIL — see $OUT/run{1,2}/result.txt" >&2
	exit 1
fi

echo "PASS — 2×${COUNT} delivered via IMAP IDLE in ${elapsed}s"
exit 0