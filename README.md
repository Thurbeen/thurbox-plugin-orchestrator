# thurbox-plugin-gastown

A [thurbox](https://github.com/Thurbeen/thurbox) plugin that bridges
thurbox sessions and [gastown](https://github.com/gastownhall/gastown).

The plugin is two things in one repo:

- **A thurbox plugin** (`thurbox-plugin-gastown`) — long-running JSON-RPC
  daemon under `~/.local/share/thurbox/admin/plugins/gastown/` that
  exposes `gastown.*` MCP tools so admin/Claude sessions can drive
  gastown conversationally.
- **A gastown exec session provider** (`gc-session-thurbox`) — per-op
  fork/exec binary that gastown invokes to spawn workers *inside*
  thurbox sessions, plus a `thurbox-worker-wrap` helper that polls the
  session for the result sentinel and closes the matching `bd` item.

## Status

Pre-MVP. Modules compile and unit tests pass; install scripts and
end-to-end verification are still in progress.

## Layout

```text
crates/
  orchestrator-core/         # shared library: jsonrpc, thurbox, gastown,
                             # plugin, rig, sentinel
  thurbox-plugin-gastown/    # plugin daemon binary
  gc-session-thurbox/        # gastown exec session provider binary
  thurbox-worker-wrap/       # session_setup_script helper
plugin/                      # what gets copied into
                             # ~/.local/share/thurbox/admin/plugins/gastown/
examples/admin/              # reference city.toml
scripts/                     # install + bootstrap helpers
```

## MCP tools exposed

| tool                   | wraps                                            |
|------------------------|--------------------------------------------------|
| `gastown.status`       | `gt status` in admin/                            |
| `gastown.list_agents`  | `gt list agents` (parsed JSON)                   |
| `gastown.sling`        | `gt sling <agent> <bd-id>`                       |
| `gastown.show_bead`    | `gt bead show <bd-id>`                           |
| `gastown.tail_events`  | `gt events tail --since=<dur>`                   |

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
./scripts/bootstrap-admin.sh
```

`install.sh` builds all three binaries, copies the `plugin/` tree to
`~/.local/share/thurbox/admin/plugins/gastown/`, and seeds
`examples/admin/city.toml` if no `city.toml` exists yet.
`bootstrap-admin.sh` runs `gc init` idempotently.

## Relationship to the rest of the stack

- **[thurbox](https://github.com/Thurbeen/thurbox)** — multi-session
  Claude Code TUI. Loads this plugin from `admin/plugins/gastown/` and
  routes `gastown.*` MCP calls to it.
- **[gastown](https://github.com/gastownhall/gastown)** — the
  orchestrator. We don't reimplement dispatch/poll/review/merge-queue;
  we hook into gastown via its exec session provider protocol.
- **[beads (`bd`)](https://github.com/gastownhall/beads)** — work
  tracker. Workers close their assigned items from inside their thurbox
  session via `bd close`, driven by the sentinel in
  `thurbox-worker-wrap`.

## License

MIT — see [LICENSE](LICENSE).
