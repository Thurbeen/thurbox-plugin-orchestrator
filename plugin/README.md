# orchestrator — thurbox plugin

Beads-driven multi-agent orchestrator. Dispatches one thurbox session
per ready `bd` item.

Content-only plugin: ships two skills and two roles, no compiled
binaries. The orchestrator workflow runs entirely through `bd` and
`thurbox-cli` shell calls.

## Contents

- `skills/orchestrate/` — orchestrator session loop
- `skills/orchestrate-worker/` — per-bead worker contract
- `roles/orchestrator.toml` — disables Write/Edit, pre-approves
  `Bash(bd:*)` / `Bash(thurbox-cli:*)` / `Bash(mkdir:*)`
- `roles/worker.toml` — permissive (`dontAsk`) for per-bead jobs

See the source repository
[README](https://github.com/Thurbeen/thurbox-plugin-orchestrator)
for the full two-session usage flow.
