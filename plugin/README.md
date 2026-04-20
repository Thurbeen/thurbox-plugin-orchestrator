# orchestrator — thurbox plugin

Beads-driven multi-agent orchestrator. Dispatches one thurbox session
per ready `bd` item.

Content-only plugin: ships two skills and two roles, no compiled
binaries. The orchestrator workflow runs entirely through `bd` and
`thurbox-cli` shell calls.

## Contents

- `skills/orchestrate/` — orchestrator session loop (decompose + dispatch + reap)
- `skills/orchestrate-worker/` — per-bead worker contract
- `roles/orchestrator.toml` — disables Write/Edit, pre-approves the shell
  surface the orchestrator needs (bd, thurbox-cli, mkdir, sleep, jq, orch-reap)
- `roles/worker.toml` — permissive (`dontAsk`) for per-bead jobs
- `bin/orch-reap` — shell reaper that polls worker sessions, parses the
  `===RESULT===` sentinel, and mutates bd/session state with zero LLM tokens.
  Invoked by the orchestrate skill in place of LLM-driven polling.

See the source repository
[README](https://github.com/Thurbeen/thurbox-plugin-orchestrator)
for the full two-session usage flow.
