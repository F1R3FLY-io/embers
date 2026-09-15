#!/usr/bin/env bash
#
# Run the explicit-state lifecycle checker over docs/formal/ctl/*.lts.json. A
# dependency-free, CI-independent re-check of the run/publication/reveal FSMs (the
# same models the pgmcp behavioral_check tool checks and the A5 lifecycle property
# tests mirror). Exits non-zero on any property failure.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2   # docs/formal/
command -v python3 >/dev/null 2>&1 || { echo "error: python3 not found on PATH" >&2; exit 2; }
exec python3 ctl/check_lifecycles.py
