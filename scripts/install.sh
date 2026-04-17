#!/usr/bin/env bash
# Build and install thurbox-plugin-orchestrator into the thurbox admin workspace.
# Idempotent: re-running upgrades the binary and re-syncs the manifest.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ADMIN_ROOT="${THURBOX_ADMIN_ROOT:-$HOME/.local/share/thurbox/admin}"
PLUGIN_DST="$ADMIN_ROOT/plugins/orchestrator"
BD_DB="$ADMIN_ROOT/.beads"

cd "$REPO_ROOT"

echo "==> Building binary (release)"
cargo build --release -p thurbox-plugin-orchestrator

echo "==> Staging plugin payload at $PLUGIN_DST"
mkdir -p "$PLUGIN_DST/bin"
install -m 0644 plugin/thurbox-plugin.toml "$PLUGIN_DST/thurbox-plugin.toml"
install -m 0644 plugin/README.md           "$PLUGIN_DST/README.md"
install -m 0755 target/release/thurbox-plugin-orchestrator "$PLUGIN_DST/bin/thurbox-plugin-orchestrator"

if [[ ! -d "$BD_DB" ]]; then
    echo "==> Initialising bd database at $BD_DB"
    mkdir -p "$BD_DB"
    bd --db "$BD_DB" init >/dev/null 2>&1 || true
else
    echo "==> bd database $BD_DB already exists"
fi

cat <<'EOF'

==> Done.

Next steps:
  1. Restart thurbox so it picks up the plugin (or call register_plugin via MCP).
  2. Register the orchestrator skills (one-time):
        thurbox-mcp register_skill examples/skills/orchestrate/SKILL.md
        thurbox-mcp register_skill examples/skills/orchestrate-worker/SKILL.md
  3. Verify discovery:  thurbox-mcp list_plugins
EOF
