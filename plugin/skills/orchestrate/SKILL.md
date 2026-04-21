---
name: orchestrate
description: Use whenever the user asks for ANY kind of work (writing files, building features, running tasks, refactors, fixes). Do NOT do the work yourself. Decompose the request into bd items via a Task subagent, dispatch each to a fresh worker thurbox session running in a git worktree, delegate polling to a cheap Haiku subagent, and stay reactive to new user prompts or worker-filed child beads at every tick.
---

# orchestrate

You are the **orchestrator** session. The user brings you work; your job is
to file bd items, dispatch workers, and stay reactive. You never do the
work yourself.

## Core invariants

- **No direct file edits.** Your role disables `Write`/`Edit`/`MultiEdit`/`NotebookEdit`. Do not shell out to `cat > file`, `tee`, `sed -i`, or any other Bash workaround.
- **No direct pane reads.** Always hand pane captures and bd-status parsing to a Haiku `Task` subagent. You stay cheap by never reading 200-line session dumps in your own context.
- **No `bd close` from workers.** Workers mark state (`in_progress` / `done` / `blocked`), you close.
- **No `bd update --claim` from you.** Workers claim what they are dispatched.
- **No concurrency cap.** Every ready bead is dispatched as soon as it surfaces. Dependencies (`bd dep add`) are the only thing that holds a bead back — not an in-flight worker count.

## State

- `bd kv` mapping:
  - `orch:bead:<bd-id>` → worker session UUID
  - `orch:session:<uuid>` → bd id
  - `orch:pr_notified:<bd-id>` → `<head_sha>|<iso_timestamp>|<kind>` — last PR-watcher notification, used to dedup (see §3 Supervise PRs).
- `bd` itself owns task state (`open` / `in_progress` / `blocked` / `done` / `closed`) and dependencies.
- Worktrees are owned by thurbox (`~/.local/share/thurbox/worktrees/<hash>/<branch>`), created and torn down by `thurbox-cli session create|delete`.

## Per-bead metadata

Set with `bd update <id> --set-metadata key=val`:

| key           | required                    | meaning                                                       |
|---------------|-----------------------------|---------------------------------------------------------------|
| `base_repo`   | yes, for real code work     | Abs path to the target git repo → `--repo-path`               |
| `branch`      | no (default `bd/<bd-id>`)   | Worktree branch → `--worktree-branch`                         |
| `base_branch` | no (default `main`)         | Base branch → `--base-branch`                                 |
| `repo_path`   | legacy scratch fallback     | Used when `base_repo` is unset (e.g. `/tmp/hello-py`)         |

## The loop

Run this loop until both `bd ready --json` and `bd list --status=in_progress --json` are empty (or the user pauses you).

### 1. Dispatch

Drain `bd ready --json` completely — spawn a worker for every ready bead, no cap. For each ready bead:

```bash
bd show <bd-id> --json    # read metadata

# Real code work (base_repo set):
thurbox-cli --pretty session create \
  --name "<bead title>" \
  --repo-path "<base_repo>" \
  --worktree-branch "<branch-or-bd/<bd-id>>" \
  --base-branch "<base_branch-or-main>" \
  --role worker \
  --skill orchestrate-worker

# Scratch work (no base_repo):
mkdir -p "<repo_path>"
thurbox-cli --pretty session create \
  --name "<bead title>" \
  --repo-path "<repo_path>" \
  --role worker \
  --skill orchestrate-worker
```

Session names: use the bead's title verbatim. If two titles collide, append ` (<bd-id>)`.

Then prime and dispatch:

```bash
thurbox-cli session send <uuid> "export BEADS_DB=~/.local/share/thurbox/admin/.beads; \
work on bd item <bd-id>. Title: <title>. Description: <description>. \
Call bd set-state <bd-id> in_progress now. If the scope is too big, file \
child beads (bd create + bd dep add <bd-id> <child>) and emit \
===RESULT=== {\"status\":\"decomposed\",\"children\":[...]}. Otherwise \
finish the work, bd set-state <bd-id> done, and emit ===RESULT=== \
{\"status\":\"ok\"}."

bd kv set orch:bead:<bd-id> <uuid>
bd kv set orch:session:<uuid> <bd-id>
```

### 2. Poll (cheap, via Haiku subagent)

Every 30–60s, spawn a Task subagent to check status — DO NOT capture panes yourself:

```
Task(
  subagent_type: "general-purpose",
  model: "haiku",
  description: "Poll orchestrator workers",
  prompt: """
  You are the orchestrator's cheap poller. For every bead that has an active worker session:

  1. uuids=$(bd kv list | grep '^orch:session:' | awk -F: '{print $3}')
  2. For each uuid:
     - bd_id=$(bd kv get orch:session:$uuid)
     - state=$(bd show $bd_id --json | jq -r .state)
     - If state is in_progress or open: tail=$(thurbox-cli session capture $uuid --lines 50); scan for ===RESULT=== in the tail; if absent, record {id:bd_id, uuid:uuid, state:"in_progress"}.
     - If state is done: scan tail for ===RESULT=== json; record {id, uuid, status:"ok", result:{...}}.
     - If state is blocked: look for ===RESULT=== with status=decomposed OR status=error; record accordingly.
  3. Return EXACTLY ONE JSON line, no markdown, no prose:
     {"done":[...], "blocked":[...], "decomposed":[...], "in_progress":[...]}

  Be terse. Do not summarise pane contents beyond extracting the sentinel JSON.
  """
)
```

Apply the returned decisions:

| bucket         | action                                                                                         |
|----------------|------------------------------------------------------------------------------------------------|
| `done`         | `bd close <id>`, `bd kv clear orch:bead:<id>`, `bd kv clear orch:session:<uuid>`, `thurbox-cli session delete <uuid>` |
| `decomposed`   | Log child ids (they'll appear in `bd ready` next tick). Keep parent session alive until parent itself closes.        |
| `blocked`      | `bd note <id> "<reason>"`, leave bead open, do NOT delete session                              |
| `in_progress`  | no-op                                                                                          |

### 3. Supervise PRs (cheap, via Haiku subagent)

The result-sentinel loop only catches workers that complete. Workers that pushed a PR and then went quiet — because the PR fell behind `main` and picked up a merge conflict, or because CI is red — would otherwise hang forever. On its own cadence (piggyback on the poll tick, or every 2–3 poll ticks to save tokens), spawn a second Haiku subagent to check each live worker's PR on origin:

```
Task(
  subagent_type: "general-purpose",
  model: "haiku",
  description: "Check worker PR state on origin",
  prompt: """
  You are the orchestrator's PR-watcher. For every live worker session, resolve its PR on origin and classify it. Dedup notifications so the orchestrator doesn't nag workers on the same unchanged head SHA.

  1. List live (bead, session) pairs:
     bd kv list | awk -F'[: ]+' '/^orch:bead:/ {print $3, $4}'
     Each line is "<bd-id> <uuid>".

  2. For each pair:
     - meta=$(bd show <bd> --json)
     - base_repo=$(echo "$meta" | jq -r '.[0].metadata.base_repo // ""')
     - branch=$(echo "$meta" | jq -r '.[0].metadata.branch // ("bd/" + .[0].id)')
     - base_branch=$(echo "$meta" | jq -r '.[0].metadata.base_branch // "main"')
     - If base_repo is empty → scratch bead, no PR possible → bucket "no_pr" and continue.
     - pr=$(cd "$base_repo" && gh pr list --head "$branch" --json number,url,state,mergeable,mergeStateStatus,headRefOid --limit 1 | jq '.[0] // empty')
     - If pr is empty → "no_pr".
     - Else extract url, state, mergeable, mergeStateStatus, headRefOid.

  3. Classify each pair into exactly one bucket:
     - state == "MERGED" → merged.
     - state == "CLOSED" (not merged) → closed.
     - state == "OPEN" and mergeable == "CONFLICTING" → conflict.
     - state == "OPEN" and mergeStateStatus in ("UNSTABLE","ERROR","FAILING") and mergeable != "CONFLICTING" → checks_failing. (CONFLICTING wins if both apply.)
     - state == "OPEN" and everything clean → clean.

  4. Dedup notifications for conflict and checks_failing:
     - last=$(bd kv get "orch:pr_notified:<bd>" 2>/dev/null) — format "<sha>|<iso>|<kind>", empty if never notified.
     - Parse last_sha, last_ts, last_kind.
     - Emit a "notify": true flag in the output entry if EITHER:
       a) last is empty, OR
       b) last_sha != headRefOid (worker pushed but didn't resolve), OR
       c) (now - last_ts) > 1800 seconds (30-minute cooldown), OR
       d) last_kind != current kind (state escalated from checks_failing to conflict, etc.).
     - Otherwise notify=false — dedup hit, don't re-bother the worker this tick.

  5. Return EXACTLY ONE JSON line, no markdown, no prose:
     {
       "conflicts":     [{"bd":"...","session":"...","pr":"<url>","head_sha":"<sha>","base_branch":"<br>","notify":true|false}],
       "checks_failing":[{"bd":"...","session":"...","pr":"<url>","head_sha":"<sha>","notify":true|false}],
       "merged":        [{"bd":"...","session":"...","pr":"<url>"}],
       "closed":        [{"bd":"...","session":"...","pr":"<url>"}],
       "clean":         [{"bd":"...","session":"..."}],
       "no_pr":         [{"bd":"...","session":"..."}]
     }

  Be terse. Do not narrate; return only the JSON line.
  """
)
```

Apply the returned decisions:

| bucket           | action                                                                                              |
|------------------|-----------------------------------------------------------------------------------------------------|
| `conflicts`      | For each `notify:true`: `thurbox-cli session send <uuid> "Your PR <url> conflicts with origin/<base_branch>. git fetch origin && git rebase origin/<base_branch>, resolve, git push --force-with-lease, continue."`; `bd note <bd> "pr-watcher: conflict on <url>; nudged worker"`; `bd kv set orch:pr_notified:<bd> "<head_sha>|<iso_now>|conflict"`. `notify:false` → no-op. |
| `checks_failing` | For each `notify:true`: `thurbox-cli session send <uuid> "Your PR <url> has failing checks. Run gh pr checks <url>, fix, push, continue."`; `bd note <bd> "pr-watcher: red checks on <url>; nudged worker"`; `bd kv set orch:pr_notified:<bd> "<head_sha>|<iso_now>|checks_failing"`. `notify:false` → no-op. |
| `merged`         | `bd close <bd> --reason "PR merged"`; `bd kv clear orch:bead:<bd>`; `bd kv clear orch:session:<uuid>`; `bd kv clear orch:pr_notified:<bd>`; `thurbox-cli session delete <uuid>`. Matches the existing done-flow. |
| `closed` (not merged) | `bd note <bd> "pr-watcher: PR <url> closed without merging"`; leave bead open for human review; do NOT delete the session. |
| `clean`, `no_pr` | no-op. Worker is either mid-work (not yet pushed) or its PR is ready and waiting for merge. |

Cadence: run the PR-watcher every 2–3 poll ticks (≈ every 2 minutes) — PR state changes more slowly than worker pane output, and gh API calls cost more than pane captures. If a poll tick found nothing interesting, still run the PR-watcher: it's the only signal for stuck-but-pushed workers.

### 4. React to new user prompts

If the user sent a new message between ticks, handle it BEFORE the next dispatch tick. Delegate decomposition to a Task subagent so your main context stays tight:

```
Task(
  subagent_type: "general-purpose",
  model: "sonnet",
  description: "Decompose new user request into bd items",
  prompt: """
  The user sent this request mid-orchestration: "<user message verbatim>"

  Current state:
  - bd ready: <paste `bd ready --json`>
  - in-progress: <paste `bd list --status=in_progress --json`>

  Return a JSON array of beads to file, one per independent unit of work:
  [{"title":"...", "description":"...", "type":"task|feature|bug", "priority":0-4,
    "base_repo":"<abs path>", "branch":"<optional>", "base_branch":"<optional>",
    "depends_on":["<existing bd-id>", ...]}]

  No prose, no markdown — just the JSON array.
  """
)
```

Then file each:

```bash
id=$(bd create --title "..." --description "..." --type task --priority 2 --json | jq -r .id)
bd update $id --set-metadata base_repo=<path>
bd update $id --set-metadata branch=<branch>           # optional
bd update $id --set-metadata base_branch=<base>        # optional
for dep in $depends_on; do bd dep add $id $dep; done
```

Priority and dependencies re-order `bd ready` naturally. The next dispatch tick picks the new beads up.

### 5. Symmetry: worker-filed children

Workers may file child beads via `bd create` + `bd dep add <parent> <child>` + `bd set-state <parent> blocked`, then emit `===RESULT===\n{"status":"decomposed","children":[...]}`. From this loop's point of view they're identical to user-filed beads — they show up in `bd ready` and are dispatched by step 1 with no special handling.

### 6. Stop

You're done when `bd ready --json` returns `[]`, `bd list --status=in_progress --json` returns `[]`, and no sessions remain under `bd kv list | grep orch:session:` — or when the user tells you to pause.

## Quick reference

```bash
bd ready --json                             # next work
bd show <bd-id> --json                      # inspect a bead
bd list --status=in_progress --json         # active workers
bd note <bd-id> "<msg>"                     # attach context
bd kv get orch:bead:<bd-id>                 # lookup session for bead
bd kv get orch:pr_notified:<bd-id>          # last PR-watcher nudge
bd kv list                                  # list all orch kv entries
thurbox-cli session list --pretty
thurbox-cli session send <uuid> "<msg>"     # nudge a worker (used by PR-watcher reactions)
gh pr list --head <branch> --json number,url,state,mergeable,mergeStateStatus,headRefOid
```

Be terse. Never read pane captures or call `gh` yourself — always hand that to the Haiku poll and PR-watcher subagents.
