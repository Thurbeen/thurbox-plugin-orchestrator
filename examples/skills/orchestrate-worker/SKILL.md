---
name: orchestrate-worker
description: Worker contract for sessions spawned by the orchestrator plugin. Do the work in the bead, then emit the ===RESULT=== sentinel.
---

# orchestrate-worker

You were spawned by the `thurbox-plugin-orchestrator` plugin to work
exactly one `bd` item. The dispatch prompt told you the bead id and
title. The orchestrator session is watching your output for a result
sentinel and will close the bead once it sees `status: "ok"`.

## Contract

1. Read the full bead with `bd show <bd-id>` if you need the
   description, labels, or metadata beyond what was in the prompt.
2. Do the work. You are running in the bead's `metadata.repo_path`.
3. As the **last lines** of your final response, emit:

   ```
   ===RESULT===
   {"status":"ok","artifact":"<short summary>","notes":"<details>"}
   ```

   On failure:

   ```
   ===RESULT===
   {"status":"error","notes":"<what went wrong>"}
   ```

   `artifact` and `notes` are free-form strings. Optional fields
   `pr_url` and `bd_id` are accepted by the parser but unused by v1.

## Rules

- **Do not** call `bd close` yourself. The orchestrator closes on
  `status:"ok"`.
- **Do not** emit the sentinel until you are truly done — the
  orchestrator polls and acts on the *last* sentinel in your output.
- A malformed sentinel (missing `status`, invalid JSON, no payload
  line) leaves the bead open and gets flagged for human review.
- Be concise. Long output is fine but the sentinel must be the last
  non-blank lines.

## Quick reference

```bash
bd show <bd-id>                       # full record
bd note <bd-id> "progress note"       # audit trail (optional)
```
