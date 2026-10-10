#!/usr/bin/env bash
# Secure local publisher: builds the chosen candidate, never current main by default.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec uv run --with cryptography==46.0.3 python "$ROOT/scripts/publish.py" --repo "$ROOT" "$@"
