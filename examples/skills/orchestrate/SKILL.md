---
name: orchestrate
description: Drive the beads-backed multi-agent orchestrator. Dispatch ready bd items to fresh thurbox sessions, poll for the result sentinel, close beads on success.
---

# orchestrate

You are the **orchestrator** session. Your job is to drain ready `bd`
items by dispatching each to a fresh thurbox worker session, watching
for its result sentinel, and closing the bead when the worker reports
`status: "ok"`.

You drive this entirely through the `orch.*` MCP tools exposed by the
`thurbox-plugin-orchestrator` plugin. Do **not** call the worker
sessions directly — the plugin owns session lifecycle.

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

## Filing new work

When the user asks you to file a new item for the orchestrator to
work, do it with plain `bd` — it's cheaper than a round-trip MCP
call and keeps the contract in one place.

```bash
bd --db "$THURBOX_ORCH_BD_DB" create "<title>" --label <label>
# The id is printed on the last line; reuse it below.
bd --db "$THURBOX_ORCH_BD_DB" update <bd-id> \
  --set-metadata repo_path="$HOME/some/repo" \
  --set-metadata role=worker \
  --set-metadata skills=ship-pr,run-tests
```

Only `repo_path` is required (and even that can be omitted when the
plugin has `THURBOX_ORCH_DEFAULT_REPO` set — see *repo_path resolution*
below). `role` and `skills` are passed straight through to
`create_session`.

## repo_path resolution

`orch.dispatch` resolves the worker's working directory in this order:

1. `repo_path_override` on the dispatch call (one-off escape hatch).
2. Bead `metadata.repo_path`.
3. `THURBOX_ORCH_DEFAULT_REPO` env var on the plugin daemon.

If none are set, dispatch errors. For the common case of "always work
this one repo", set `THURBOX_ORCH_DEFAULT_REPO` once and skip the
metadata entirely when filing.

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

Be terse in chat. Show the poll result tails only on errors or when
asked.
