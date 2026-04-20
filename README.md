# thurbox-plugin-orchestrator

A [thurbox](https://github.com/Thurbeen/thurbox) plugin that drives a
beads-backed multi-agent orchestrator. One ready
[`bd`](https://github.com/gastownhall/beads) item → one fresh thurbox
session → one worker that emits a result sentinel.

The orchestrator itself is a Claude session (not a daemon) using these
MCP tools in a loop. The plugin's job is to expose the small surface
that loop needs and stay otherwise out of the way.

## Status

Pre-MVP. The plugin daemon, `bd` wrapper, and `thurbox-mcp` client
compile and are unit tested. Upstream thurbox now spawns
`mcp-tools`-capability plugins and proxies their tool surface via the
control socket, so end-to-end exercise is unblocked.

## Layout

```text
crates/
  orchestrator-core/                 # shared library:
                                     #   bd, jsonrpc, orch, plugin,
                                     #   sentinel, thurbox
  thurbox-plugin-orchestrator/       # plugin daemon binary
plugin/                              # copied to
                                     # ~/.local/share/thurbox/admin/plugins/orchestrator/
examples/skills/                     # reference SKILL.md files for the
                                     # orchestrator and worker sessions
scripts/                             # install helper
```

## Two-session model

| session                | role                                                                |
|------------------------|---------------------------------------------------------------------|
| **admin / creator**    | human + Claude, uses raw `bd create` / `bd update` to file work     |
| **admin / orchestrator** | Claude with the `orchestrate` skill — drains ready bd items via the `orch.*` MCP tools |
| **worker (per bead)**  | spawned by the plugin, runs the `orchestrate-worker` skill, emits `===RESULT===\n{json}` |

Bootstrap both admin sessions with `thurbox-cli session create` (or
`session create` + `session send` together if you want to ship a
priming prompt). The orchestrator session must run with `cwd` =
`~/.local/share/thurbox/admin/` so `bd` auto-discovers `.beads/`.

### Token economy

Every call into `thurbox-mcp` costs Claude tokens (tool schemas plus
JSON-RPC envelopes). Prefer `bd` and `thurbox-cli` directly:

| use case                         | reach for                                       |
|----------------------------------|-------------------------------------------------|
| create / update / inspect a bead | `bd …` shell calls                              |
| spawn or steer admin sessions    | `thurbox-cli session …`                         |
| dispatch / poll / close a bead   | `orch.*` (one MCP call replaces 4–5 raw ops)   |

## MCP tools exposed

| tool                  | purpose                                                                 |
|-----------------------|-------------------------------------------------------------------------|
| `orch.ready`          | List ready bd items in priority order.                                  |
| `orch.dispatch`       | Spawn a thurbox session for one ready item, send the worker prompt.    |
| `orch.poll`           | Capture session output and report `running`/`ok`/`error`/`malformed`.  |
| `orch.close`          | Close the bead and (default) delete the session.                        |
| `orch.list_active`    | Inspect currently dispatched bd↔session pairs.                          |

State lives in `bd kv`:
- `orch:bead:<bd-id>` → thurbox session id
- `orch:session:<uuid>` → bd id

## Per-bead config

Set on the bd item with `bd update <id> --set-metadata key=val`.

| key         | required | meaning                                                          |
|-------------|----------|------------------------------------------------------------------|
| `repo_path` | yes      | working directory the worker session is spawned in               |
| `role`      | no       | passed through to `create_session` (project role)                |
| `skills`    | no       | comma-separated skill names attached to the worker session       |

## Configuration

| env var                  | default                                              |
|--------------------------|------------------------------------------------------|
| `THURBOX_ORCH_BD_DB`     | `~/.local/share/thurbox/admin/.beads/`               |
| `THURBOX_ORCH_MCP_BIN`   | `thurbox-mcp` (resolved via `$PATH`)                 |
| `THURBOX_ADMIN_ROOT`     | `~/.local/share/thurbox/admin` (used by `install.sh`) |

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --all
```

## Install

```bash
./scripts/install.sh
```

Then in thurbox:
1. Restart so the plugin is picked up (or `register_plugin` via MCP).
2. Register the orchestrator skills once:
   ```bash
   thurbox-mcp register_skill examples/skills/orchestrate/SKILL.md
   thurbox-mcp register_skill examples/skills/orchestrate-worker/SKILL.md
   ```
3. From an admin session, drain work:
   ```bash
   bd --db ~/.local/share/thurbox/admin/.beads/ create "echo hello" --label demo
   bd --db ~/.local/share/thurbox/admin/.beads/ update <id> \
      --set-metadata repo_path=$HOME/scratch/hello
   ```
   Then ask the admin orchestrator session (with the `orchestrate`
   skill loaded) to dispatch the ready item.

## Smoke test (no thurbox host needed)

```bash
printf '%s\n%s\n%s\n' \
  '{"id":1,"op":"handshake","params":{"api_version":1,"plugin_name":"orchestrator","effective_configuration":{}}}' \
  '{"id":2,"op":"mcp.list_tools","params":{}}' \
  '{"id":3,"op":"stop","params":{}}' \
  | THURBOX_ORCH_MCP_BIN=$(which thurbox-mcp) \
    ~/.local/share/thurbox/admin/plugins/orchestrator/bin/thurbox-plugin-orchestrator
```

Three `{"id":…,"ok":true,…}` lines back; the second lists all five
`orch.*` tools.

## License

MIT — see [LICENSE](LICENSE).
