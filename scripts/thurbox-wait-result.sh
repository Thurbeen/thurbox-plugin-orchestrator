#!/usr/bin/env bash
# gastown session_setup_script: wait for the worker sentinel and close
# the assigned bd item.
#
# Gastown invokes this once per dispatched session. The agent's nudge
# already told the worker what to do; we just block until the
# `===RESULT===` sentinel appears in the session output and then run
# `bd close` from inside the session's bd db.
#
# Required env (set by gastown):
#   GC_SESSION_ID  — thurbox session UUID
#   GC_BEAD_ID     — bd item being processed
# Optional env:
#   GC_BEAD_DB     — overrides the default bd db path

set -euo pipefail

if [[ -z "${GC_SESSION_ID:-}" || -z "${GC_BEAD_ID:-}" ]]; then
    echo "thurbox-wait-result: GC_SESSION_ID and GC_BEAD_ID are required" >&2
    exit 1
fi

ADMIN_ROOT="${THURBOX_ADMIN_ROOT:-$HOME/.local/share/thurbox/admin}"
BEAD_DB="${GC_BEAD_DB:-$ADMIN_ROOT/.beads/}"
WRAP_BIN="${THURBOX_WORKER_WRAP:-$ADMIN_ROOT/plugins/gastown/bin/thurbox-worker-wrap}"

exec "$WRAP_BIN" \
    --session "$GC_SESSION_ID" \
    --bd-id   "$GC_BEAD_ID" \
    --bd-db   "$BEAD_DB"
