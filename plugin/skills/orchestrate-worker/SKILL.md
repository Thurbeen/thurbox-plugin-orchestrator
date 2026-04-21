---
name: orchestrate-worker
description: Worker contract for sessions spawned by the orchestrator plugin. Sync the worktree to origin before any work, transition bd state as you go, optionally file child beads if the scope is bigger than expected, and emit a ===RESULT=== sentinel when finished.
---

# orchestrate-worker

You were spawned by the `thurbox-plugin-orchestrator` plugin to work a
single `bd` item. Your dispatch prompt gave you the bead id and exported
`BEADS_DB` so plain `bd ...` calls hit the right database.

The orchestrator session is watching two signals:

1. Your bead's state in `bd` (cheap, polled by a Haiku subagent).
2. The `===RESULT===` sentinel at the end of your pane (belt-and-braces).

Set both. The orchestrator closes the bead on success; you never call
`bd close`.

## Contract

1. **Read** the full bead if you need detail beyond the dispatch prompt:
   ```bash
   bd show <bd-id>
   ```
2. **Claim** by transitioning state on start:
   ```bash
   bd set-state <bd-id> in_progress
   ```
3. **Sync with remote** before touching any files. Your worktree branch
   may have been cut from a stale local base, so any work you do now
   would land behind `main` with avoidable conflicts. Steps:
   ```bash
   base=$(bd show <bd-id> --json | jq -r '.[0].metadata.base_branch // "main"')

   # Fetch. If this fails (network, auth, missing remote), stop hard —
   # do NOT silently proceed on a stale tree.
   git fetch origin || {
     bd note <bd-id> "sync: git fetch origin failed — cannot verify base is current"
     bd set-state <bd-id> blocked
     exit 1
   }

   # Rebase onto origin/<base_branch>. This fast-forwards a freshly-cut
   # worktree branch and replays any existing commits on top of the
   # remote tip. Do NOT auto-resolve conflicts — surface them.
   git rebase "origin/$base" || {
     bd note <bd-id> "sync: rebase onto origin/$base produced conflicts — human resolution needed"
     git rebase --abort
     bd set-state <bd-id> blocked
     exit 1
   }
   ```
   If either step fails, leave the bead `blocked` with a `bd note` and
   emit `===RESULT=== {"status":"error","notes":"..."}`. The orchestrator
   will not auto-retry; a human resolves the sync problem before work
   continues.
4. **Work** in your CWD (the bead's worktree, or a scratch dir). Keep all
   file edits inside CWD. You MAY run `bd` and `git` anywhere they
   normally work — these are the only external commands permitted.
5. **Decompose** (optional) if the bead turns out to be N independent
   sub-tasks:
   ```bash
   for each sub-task:
     child=$(bd create --title "..." --description "..." --priority 2 --json | jq -r .id)
     bd update $child --set-metadata base_repo=<parent base_repo>
     bd dep add <bd-id> $child         # parent depends on child
   bd set-state <bd-id> blocked
   ```
   Then emit:
   ```
   ===RESULT===
   {"status":"decomposed","children":["<child-id>","<child-id>"]}
   ```
   The orchestrator will dispatch fresh worker sessions for each child
   on its next tick. Your parent bead stays open and unblocks only after
   all children close.
6. **Finish** with one of:
   - Success:
     ```bash
     bd set-state <bd-id> done
     ```
     ```
     ===RESULT===
     {"status":"ok","artifact":"<short summary>","notes":"<details>"}
     ```
   - Failure:
     ```bash
     bd note <bd-id> "<why it failed>"
     bd set-state <bd-id> blocked
     ```
     ```
     ===RESULT===
     {"status":"error","notes":"<what went wrong>"}
     ```

`artifact` and `notes` are free-form strings. Optional fields `pr_url`
and `bd_id` are accepted by the parser but unused by v1.

## Rules

- **Do not** call `bd close` yourself. The orchestrator closes on `status:"ok"` (after any children have also closed).
- **Do not** emit the sentinel until you are truly done — the orchestrator acts on the *last* sentinel in your pane.
- **Do not** read or edit files outside your CWD. The `bd` and `git` commands are the only exception.
- **Do not** act on sibling or unrelated beads. You may only create/update/note/set-state on your own bead and its children.
- A malformed sentinel (missing `status`, invalid JSON, no payload line) leaves the bead open and gets flagged for human review.
- Be concise. Long output is fine but the sentinel must be the last non-blank lines.

## Quick reference

```bash
bd show <bd-id>                       # full record
git fetch origin && git rebase origin/<base_branch>   # sync to remote tip
bd set-state <bd-id> in_progress      # claim
bd set-state <bd-id> done             # success
bd set-state <bd-id> blocked          # failure or waiting-on-children
bd note <bd-id> "progress note"       # audit trail
bd create --title "..." ...           # file a child bead
bd dep add <parent> <child>           # parent depends on child
```
