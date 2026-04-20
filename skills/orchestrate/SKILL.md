---
name: orchestrate
description: Drive the beads-backed multi-agent orchestrator. Dispatch ready bd items to fresh thurbox sessions, poll for the result sentinel, close beads on success.
---

# orchestrate

You are the **orchestrator** session. Your job is to drain ready `bd`
items by dispatching each to a fresh thurbox worker session, watching
for its result sentinel, and closing the bead when the worker reports
`status: "ok"`.

The dispatch / poll / close cycle goes through the `orch.*` MCP tools
exposed by the `thurbox-plugin-orchestrator` plugin — those wrap
multi-step session+kv operations and are worth the MCP envelope. For
**everything else, prefer the CLIs** (`bd`, `thurbox-cli`) over MCP to
keep token usage low.

Do **not** call the worker sessions directly — the plugin owns their
lifecycle.

## Loop

1. `orch.ready` → see what's ready, in priority order.
2. `orch.dispatch` (no args) → spawn a worker for the top item. Records
   the bd↔session mapping in `bd kv` (`orch:bead:*`, `orch:session:*`).
   The worker is created with the bead's `metadata.repo_path`,
   `metadata.role`, and `metadata.skills` (skills as comma-separated).
3. Poll with `orch.poll {session_id}` periodically (every 30–60s is
   plenty). Status values:
   - `running` — keep waiting.
   - `ok` → call `orch.close {bd_id}` to close the bead and delete the
     worker session.
   - `error` → leave the bead open. The worker's `notes` are surfaced
     in the poll result; append an audit note via plain `bd note`.
   - `malformed` → the worker's sentinel didn't parse. Investigate
     before closing.
4. For fan-out: call `orch.dispatch` N times in a row. The plugin does
   not enforce a max-concurrency cap — that judgment is yours.

## Required bead metadata

A bead can only be dispatched if it has `metadata.repo_path` set:

```bash
bd update <bd-id> --set-metadata repo_path=$HOME/some/repo
bd update <bd-id> --set-metadata role=worker
bd update <bd-id> --set-metadata skills=ship-pr,run-tests
```

If `repo_path` is missing, `orch.dispatch` errors loudly. Either fix
the bead or pass `repo_path_override` for a one-off.

## State invariants

- One session per bead at a time. If you re-dispatch a bead before
  closing it, the old kv mapping is overwritten and the old session is
  orphaned (and counted by `orch.list_active` only until the new
  dispatch). Don't.
- `orch:` is a convention-only prefix in `bd kv`; nothing enforces it.

## When to stop

- `orch.ready` returns `[]` and `orch.list_active` returns `[]` — drain
  complete.
- The user asks you to pause.

## CLI reference (use these instead of MCP wherever possible)

```bash
bd ready --json                       # same surface as orch.ready, free
bd show <bd-id> --json
bd note <bd-id> "<msg>"
bd kv get orch:bead:<bd-id>           # peek the bd↔session mapping
thurbox-cli session list
thurbox-cli session capture <uuid>    # raw inspection without orch.poll
```

Be terse in chat. Show the poll result tails only on errors or when
asked.
