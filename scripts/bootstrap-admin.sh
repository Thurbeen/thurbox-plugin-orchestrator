#!/usr/bin/env bash
# Initialise the gastown city in the thurbox admin workspace, idempotently.

set -euo pipefail

ADMIN_ROOT="${THURBOX_ADMIN_ROOT:-$HOME/.local/share/thurbox/admin}"

if [[ ! -d "$ADMIN_ROOT" ]]; then
    echo "thurbox admin workspace not found at $ADMIN_ROOT" >&2
    exit 1
fi

cd "$ADMIN_ROOT"

if [[ -d .gc ]]; then
    echo "==> $ADMIN_ROOT/.gc already exists; nothing to do"
    exit 0
fi

echo "==> Running gc init in $ADMIN_ROOT"
gc init
