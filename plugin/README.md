# orchestrator — thurbox plugin

Beads-driven multi-agent orchestrator. Dispatches one thurbox session
per ready `bd` item — each worker runs in its own git worktree,
self-reports state into `bd`, and may file child beads if its work
turns out to be bigger than a single unit. The orchestrator's main
loop stays reactive: new user prompts *and* worker-filed children land
in `bd ready` and are picked up on the next tick.

Content-only plugin: ships two skills and two roles, no compiled
binaries. The orchestrator workflow runs entirely through `bd` and
`thurbox-cli` shell calls, plus two kinds of `Task` subagent.

## Contents

- `skills/orchestrate/` — orchestrator main loop (dispatch + Haiku poll + reactive decompose)
- `skills/orchestrate-worker/` — per-bead worker contract (self-report + optional child beads)
- `roles/orchestrator.toml` — disables Write/Edit, pre-approves `Bash(bd:*)` / `Bash(thurbox-cli:*)` / `Bash(mkdir:*)` / `Task`
- `roles/worker.toml` — permissive (`bypassPermissions`) for per-bead jobs; allows `bd` / `git` outside CWD, file edits only inside CWD

See the source repository
[README](https://github.com/Thurbeen/thurbox-plugin-orchestrator)
for the full usage flow.
