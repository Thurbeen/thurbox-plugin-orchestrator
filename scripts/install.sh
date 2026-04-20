#!/usr/bin/env bash
# Install thurbox-plugin-orchestrator into the thurbox admin workspace.
# Pure content-bundle plugin (skills + role, no binary). Idempotent.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ADMIN_ROOT="${THURBOX_ADMIN_ROOT:-$HOME/.local/share/thurbox/admin}"
PLUGIN_DST="$ADMIN_ROOT/plugins/orchestrator"
BD_DB="$ADMIN_ROOT/.beads"

cd "$REPO_ROOT"

echo "==> Staging plugin payload at $PLUGIN_DST"
mkdir -p "$PLUGIN_DST"
install -m 0644 plugin/thurbox-plugin.toml "$PLUGIN_DST/thurbox-plugin.toml"
install -m 0644 plugin/README.md           "$PLUGIN_DST/README.md"

echo "==> Syncing contributed skills"
rm -rf "$PLUGIN_DST/skills"
cp -r skills "$PLUGIN_DST/skills"

echo "==> Syncing contributed roles"
rm -rf "$PLUGIN_DST/roles"
cp -r roles "$PLUGIN_DST/roles"

# Drop any leftover binary from the previous MCP-capable iteration.
rm -rf "$PLUGIN_DST/bin"

if [[ ! -f "$BD_DB/config.yaml" ]]; then
    echo "==> Initialising bd database in $ADMIN_ROOT"
    rmdir "$BD_DB" 2>/dev/null || true
    (cd "$ADMIN_ROOT" && bd init --non-interactive --role maintainer)
else
    echo "==> bd database $BD_DB already initialised"
fi

cat <<EOF

==> Done.

Next steps:
  1. Restart thurbox so it picks up the plugin (or call register_plugin via MCP).
     Skills (orchestrate, orchestrate-worker) and the orchestrator role are
     auto-loaded from the plugin manifest — no separate registration step.
  2. Spawn the orchestrator session with cwd=$ADMIN_ROOT (so 'bd' auto-discovers
     .beads/) and --role orchestrator --skill orchestrate.
EOF
