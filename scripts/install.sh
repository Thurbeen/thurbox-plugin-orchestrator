#!/usr/bin/env bash
# Build and install thurbox-plugin-gastown into the thurbox admin workspace.
# Idempotent: re-running upgrades the binaries and re-syncs the manifest.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ADMIN_ROOT="${THURBOX_ADMIN_ROOT:-$HOME/.local/share/thurbox/admin}"
PLUGIN_DST="$ADMIN_ROOT/plugins/gastown"
CITY_TOML="$ADMIN_ROOT/city.toml"

cd "$REPO_ROOT"

echo "==> Building binaries (release)"
cargo build --release \
    -p thurbox-plugin-gastown \
    -p gc-session-thurbox \
    -p thurbox-worker-wrap

echo "==> Staging plugin payload at $PLUGIN_DST"
mkdir -p "$PLUGIN_DST/bin"
install -m 0644 plugin/thurbox-plugin.toml "$PLUGIN_DST/thurbox-plugin.toml"
install -m 0644 plugin/README.md           "$PLUGIN_DST/README.md"
install -m 0755 target/release/thurbox-plugin-gastown "$PLUGIN_DST/bin/thurbox-plugin-gastown"
install -m 0755 target/release/gc-session-thurbox     "$PLUGIN_DST/bin/gc-session-thurbox"
install -m 0755 target/release/thurbox-worker-wrap    "$PLUGIN_DST/bin/thurbox-worker-wrap"

if [[ ! -f "$CITY_TOML" ]]; then
    echo "==> Seeding $CITY_TOML"
    mkdir -p "$ADMIN_ROOT"
    install -m 0644 examples/admin/city.toml "$CITY_TOML"
else
    echo "==> $CITY_TOML already exists; leaving it alone"
fi

cat <<'EOF'

==> Done.

Next steps:
  1. Restart thurbox so it picks up the plugin (or call register_plugin via MCP).
  2. ./scripts/bootstrap-admin.sh   # idempotent `gc init`
  3. Verify with: thurbox-cli mcp call list_plugins
EOF
