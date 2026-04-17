# orchestrator — thurbox plugin

Beads-driven multi-agent orchestrator. Dispatches one thurbox session
per ready `bd` item.

This directory is the installable plugin payload. It is copied to
`~/.local/share/thurbox/admin/plugins/orchestrator/` by the repo's
`scripts/install.sh`. The `bin/` subdir is populated at install time
with one binary built from the parent repo:

- `thurbox-plugin-orchestrator` — the plugin daemon (`exec` in this
  manifest). Capabilities: `mcp-tools`. Exposes `orch.ready`,
  `orch.dispatch`, `orch.poll`, `orch.close`, `orch.list_active`.

Configuration env vars (read by the daemon at startup):

| env var                | default                                    |
|------------------------|--------------------------------------------|
| `THURBOX_ORCH_BD_DB`   | `~/.local/share/thurbox/admin/.beads/`     |
| `THURBOX_ORCH_MCP_BIN` | `thurbox-mcp` (resolved via `$PATH`)       |

See the repo's top-level [README](../README.md) for full context and
two-session usage flow.
