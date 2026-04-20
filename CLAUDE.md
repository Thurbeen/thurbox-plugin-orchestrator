# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->


## Build & Test

_Add your build and test commands here_

```bash
# Example:
# npm install
# npm test
```

## Architecture Overview

Content-only plugin: two skills + two roles + one shell reaper.

- **orchestrate skill** — LLM decomposes the user's request into bd items,
  spawns worker thurbox sessions via `thurbox-cli session create`, records
  `orch:session:<uuid> -> <bd-id>` mappings in `bd kv`, then delegates
  polling to the reaper.
- **orchestrate-worker skill** — per-worker contract: do the work in the
  bead's `metadata.repo_path`, emit a single `===RESULT===` sentinel at
  the end of the final response.
- **plugin/bin/orch-reap** — shell script that sweeps every
  `orch:session:*` kv mapping, captures the session pane, extracts the
  **last** sentinel, classifies each session (`ok`/`error`/`running`/
  `malformed`/`vanished`), mutates bd/session state for ok/error, and
  emits one compact JSON summary line. Runs with zero LLM tokens — the
  orchestrator session only sees the summary, not raw pane output.

The dispatcher/reaper split exists specifically to keep the orchestrator
LLM context bounded regardless of worker count or poll cadence.
Historical `thurbox-cli session capture` polling from the LLM is gone
from the happy path; use `capture` only when inspecting a failed worker.

## Migration note

In-flight orchestrator sessions started under the pre-reaper skill keep
the old polling instructions in their own context — let them finish
naturally, or `thurbox-cli session delete` and restart. Workers and bd
state (`orch:bead:*` / `orch:session:*`) need no migration; the reaper
picks up existing mappings on its first sweep.

## Conventions & Patterns

- The orchestrator role blocks Write/Edit at the tool layer — if you want
  to create or modify a file as the orchestrator, file a bead and dispatch
  it to a worker. Never shell-around the restriction with `cat >`, `tee`,
  `sed -i`, etc.
- Any bead the orchestrator dispatches MUST have `metadata.repo_path` set
  to an absolute directory that exists — `mkdir -p` it first if the user
  only named a parent.
- Worker sentinels are **last-writer-wins** — the reaper acts on the last
  `===RESULT===` block in the pane. Workers must emit it exactly once, at
  the end.
