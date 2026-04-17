# thurbox-orchestrator

Standalone orchestrator binary and library. Not yet ready for use — this
repository currently contains a scaffold only.

## Status

**Not ready for use.** The binary compiles and prints its version; the
library modules (`plan`, `dispatch`, `poll`, `review`, `bd`) are empty
stubs. Follow-up work will port real logic over incrementally.

## Purpose

`thurbox-orchestrator` is the home for the orchestrator logic that
currently lives as inline bash inside the `orchestrate` skill in
[`thurbeen-skills`](https://github.com/Thurbeen/thurbeen-skills). The
goal is to replace those bash helpers with a single Rust binary that the
skill can delegate to, giving us:

- a typed, testable core (`orchestrator-core`) for plan construction,
  worker dispatch, polling, and review gating
- a thin CLI (`thurbox-orchestrator`) that exposes those operations for
  the skill and for interactive use

## Relationship to the rest of the stack

- **[thurbox](https://github.com/Thurbeen/thurbox)** — the multi-session
  Claude Code TUI. The orchestrator runs *alongside* thurbox and
  coordinates work across sessions that thurbox spawns; it does not
  replace thurbox.
- **[thurbeen-skills](https://github.com/Thurbeen/thurbeen-skills)** —
  host of the `orchestrate` skill. Once this crate is feature-complete,
  the skill's inline bash helpers under
  `skills/orchestrate/scripts/` will call into
  `thurbox-orchestrator` instead of reimplementing the same logic in
  shell.
- **[beads (`bd`)](https://github.com/steveyegge/beads)** — the work
  tracker the orchestrator reads from and writes to. The `bd` module
  wraps the CLI so orchestrator flows can spawn workers against real
  `bd` items and close them when done.

## Layout

```text
crates/
  thurbox-orchestrator/   # binary crate — clap-based CLI
  orchestrator-core/      # library crate — plan/dispatch/poll/review/bd
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --all
```

## License

MIT — see [LICENSE](LICENSE).
