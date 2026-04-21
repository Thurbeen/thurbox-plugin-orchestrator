# thurbox-plugin-orchestrator

A [thurbox](https://github.com/Thurbeen/thurbox) plugin that drives a
beads-backed multi-agent orchestrator. One ready
[`bd`](https://github.com/gastownhall/beads) item → one fresh thurbox
session → one worker that emits a result sentinel.

The orchestrator itself is a Claude session (not a daemon) running the
`orchestrate` skill. All work happens through plain shell calls —
`bd` for state, `thurbox-cli` for session lifecycle. No MCP tools and
no plugin daemon binary.

## Status

Content-only plugin. Ships two skills and a role; zero runtime
dependencies beyond `bd` and `thurbox-cli` being on `$PATH`.

## Layout

```text
plugin/                              # installable payload — copied verbatim by
                                     # thurbox into ~/.local/share/thurbox/admin/
                                     # plugins/orchestrator/
├── thurbox-plugin.toml              # manifest (name, version, contributes.*)
├── README.md                        # ships with the installed plugin
├── skills/                          # contributed via [[contributes.skills]]
│   ├── orchestrate/SKILL.md
│   └── orchestrate-worker/SKILL.md
└── roles/                           # contributed via [[contributes.roles]]
    ├── orchestrator.toml            # disables Write/Edit; pre-approves
    │                                # Bash(bd:*) / Bash(thurbox-cli:*)
    └── worker.toml                  # permissive (dontAsk) for per-bead jobs
```

## Two-session model

| session                | role                                                                |
|------------------------|---------------------------------------------------------------------|
| **admin / creator**    | human + Claude, uses raw `bd create` / `bd update` to file work     |
| **admin / orchestrator** | Claude with the `orchestrate` skill + `orchestrator` role — drains ready bd items using `bd` + `thurbox-cli` |
| **worker (per bead)**  | spawned by the orchestrator, runs the `orchestrate-worker` skill, emits `===RESULT===\n{json}` |

Spawn the orchestrator session so `bd` can auto-discover `.beads/`:

```bash
thurbox-cli session create \
  --name orchestrator \
  --repo-path ~/.local/share/thurbox/admin \
  --role orchestrator \
  --skill orchestrate
```

The `orchestrator` role disables `Write`/`Edit`/`MultiEdit`/`NotebookEdit`
at the tool layer and pre-approves `Bash(bd:*)`, `Bash(thurbox-cli:*)`,
`Bash(mkdir:*)`, and `Task` (so the orchestrator can spawn Haiku poll
and Sonnet decompose subagents).

## The workflow

The `orchestrate` skill runs a continuous, reactive loop:

1. **Dispatch** every ready bead from `bd ready` — there is no
   concurrency cap; all ready beads go out as soon as they surface.
   Each worker spawns on a fresh **git worktree**:
   ```bash
   thurbox-cli session create \
     --name "<bead title>" \
     --repo-path <base_repo> \
     --worktree-branch <branch|bd/<id>> \
     --base-branch <base_branch|main> \
     --role worker --skill orchestrate-worker
   ```
   Scratch beads (no `base_repo`) fall back to plain `--repo-path`.
   The orchestrator sends `export BEADS_DB=...; work on bd item <id>...`
   so the worker can call plain `bd ...`.
2. **Poll (cheap)** by delegating to a **Haiku** `Task` subagent every
   30–60s. The subagent checks `bd show <id>` state and scans each
   worker pane for the `===RESULT===` sentinel, then returns a single
   JSON line bucketing workers into `done / decomposed / blocked /
   in_progress`. The main orchestrator never reads pane output
   directly — keeps its context cheap.
3. **React** to any new user prompt arriving mid-orchestration by
   spawning a **Sonnet** `Task` subagent to decompose it into new
   beads, then filing them with `bd create` + `bd update
   --set-metadata` + `bd dep add`. Priority and dependencies re-order
   `bd ready` naturally; the next tick picks the new beads up.
4. **Close** on `status:"ok"`: `bd close <id>`, clear kv, delete
   session. On `status:"error"`: leave the bead blocked, append a `bd
   note`, keep the session for inspection. On `status:"decomposed"`:
   leave the parent blocked on its children — they appear in `bd
   ready` and dispatch just like user-filed beads.

Workers self-transition state in `bd` (`set-state in_progress` on
start; `done` or `blocked` on finish) and may file child beads via
`bd create` + `bd dep add` when mid-work decomposition makes sense.
Only the orchestrator calls `bd close`.

State lives in `bd kv`:
- `orch:bead:<bd-id>` → thurbox session uuid
- `orch:session:<uuid>` → bd id

## Per-bead config

Set on the bd item with `bd update <id> --set-metadata key=val`.

| key           | required                    | meaning                                                    |
|---------------|-----------------------------|------------------------------------------------------------|
| `base_repo`   | yes, for real code work     | Abs path to the target git repo → `--repo-path`            |
| `branch`      | no (default `bd/<bead-id>`) | Worktree branch → `--worktree-branch`                      |
| `base_branch` | no (default `main`)         | Base branch → `--base-branch`                              |
| `repo_path`   | legacy scratch fallback     | Used when `base_repo` is unset (e.g. `/tmp/hello-py`)      |
| `role`        | no                          | Role name passed to `thurbox-cli session create --role`    |
| `skills`      | no                          | Comma-separated skill names for the worker session         |

## Install

From inside thurbox (TUI):

1. `Ctrl+E` → tab to **Plugins** → press `i`.
2. Paste `https://github.com/Thurbeen/thurbox-plugin-orchestrator`.
3. `Enter`.

Or via the `thurbox-mcp` server's `install_plugin` tool:

```text
install_plugin source="https://github.com/Thurbeen/thurbox-plugin-orchestrator"
```

Either route copies the `plugin/` directory into
`~/.local/share/thurbox/admin/plugins/orchestrator/`. The skills and
roles are auto-loaded from the manifest's `[[contributes.*]]` rows on
the next thurbox tick — no restart required.

Then create a bd item and ask the orchestrator to dispatch:

```bash
# Scratch work (no worktree):
bd --db ~/.local/share/thurbox/admin/.beads/ create --title "echo hello" --label demo
bd --db ~/.local/share/thurbox/admin/.beads/ update <id> \
   --set-metadata repo_path=$HOME/scratch/hello

# Real code work (worker runs on a fresh worktree off main):
bd --db ~/.local/share/thurbox/admin/.beads/ create --title "Add /status endpoint"
bd --db ~/.local/share/thurbox/admin/.beads/ update <id> \
   --set-metadata base_repo=$HOME/Repositories/my-service
# branch defaults to bd/<id>, base_branch defaults to main — override with --set-metadata if needed
```

## License

MIT — see [LICENSE](LICENSE).
