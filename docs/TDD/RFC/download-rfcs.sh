#!/usr/bin/env bash
# Download RFC plain text from ietf.org (fallback: rfc-editor.org).
# Optional: fetch draft-uberti-behave-turn-rest-00 (TURN REST API) via fetch_draft.
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DIR"

fetch_one() {
  local n="$1"
  local out="rfc${n}.txt"
  if [[ -f "$out" && -s "$out" ]]; then
    return 0
  fi
  if curl --max-time 30 -fsSL "https://www.ietf.org/rfc/rfc${n}.txt" -o "$out"; then
    return 0
  fi
  if curl --max-time 30 -fsSL "https://www.rfc-editor.org/rfc/rfc${n}.txt" -o "$out"; then
    return 0
  fi
  echo "FAILED rfc${n}" >&2
  return 1
}

fetch_draft() {
  local name="$1"
  local out="$2"
  if [[ -f "$out" && -s "$out" ]]; then
    return 0
  fi
  local url="https://www.ietf.org/archive/id/${name}.txt"
  if curl --max-time 30 -fsSL "$url" -o "$out"; then
    return 0
  fi
  echo "FAILED draft ${name}" >&2
  return 1
}

# Mail + IMAP (existing inventory)
MAIL_RFCS=(
  2045 2046 2047 2048 2049
  2087 2177 2342 2971 3156 3348 3501 4616 4880 4954 4978 5256
  5321 5322 5464 6154 6409 6531 6750 6851 7162 7889
  8259 8264 8265 8314 8446 8555 9110 9580
)

# STUN/TURN/ICE — Delta Chat calls + turn-rs (see docs/TDD/11-proxy-services.md)
TURN_RFCS=(
  3489   # classic STUN (historic)
  5389   # STUN
  5766   # TURN (obsoleted by 8656; turn-rs README)
  5769   # STUN test vectors
  6062   # TURN TCP relaying
  6156   # TURN IPv6
  6263   # ICE bandwidth management (referenced by turn-rs)
  8445   # ICE
  8489   # STUN bis
  8656   # TURN (current)
)

failed=()
if (($#)); then
  nums=("$@")
else
  nums=("${MAIL_RFCS[@]}" "${TURN_RFCS[@]}")
fi

for n in "${nums[@]}"; do
  if fetch_one "$n"; then
    echo "ok rfc${n}"
  else
    failed+=("$n")
  fi
done

if fetch_draft "draft-uberti-behave-turn-rest-00" "draft-uberti-behave-turn-rest-00.txt"; then
  echo "ok draft-uberti-behave-turn-rest-00"
else
  failed+=("draft-uberti-behave-turn-rest-00")
fi

if ((${#failed[@]})); then
  echo "Failed: ${failed[*]}" >&2
  exit 1
fi
