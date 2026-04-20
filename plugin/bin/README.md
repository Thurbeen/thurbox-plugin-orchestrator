# plugin/bin

Shell utilities bundled with the orchestrator plugin. Invoked by the
`orchestrate` skill; not meant to be called interactively (though they
are safe to run by hand for debugging).

## `orch-reap`

Sweeps every `orch:session:<uuid>` mapping in `bd kv`, captures each
worker session's pane, extracts the **last** `===RESULT===` sentinel,
classifies the result, mutates bd/session state, and prints one
compact JSON line.

The reaper exists so the orchestrator Claude session does not have to
poll worker sessions itself — each `thurbox-cli session capture` call
otherwise pulls ~200 lines into the LLM context every poll cycle, even
when the worker is still running. The reaper runs in shell with zero
LLM tokens and returns only a compact summary (plus, on failures, a
~20-line / 2 KB tail snippet).

### Usage

```sh
orch-reap          # pretty-print JSON (human-readable)
orch-reap --json   # compact one-line JSON (for programmatic consumers)
```

### Output shape

```json
{
  "ok":        ["admin-42"],
  "error":     [{"bd":"admin-43","uuid":"...","reason":"...","tail":"..."}],
  "running":   ["admin-44"],
  "malformed": [{"bd":"admin-45","uuid":"...","tail":"..."}],
  "vanished":  [{"bd":"admin-46","uuid":"..."}],
  "swept_at":  "2026-04-20T12:34:56Z"
}
```

| Bucket      | Meaning                                                                 | Side effects                                                               |
|-------------|-------------------------------------------------------------------------|----------------------------------------------------------------------------|
| `ok`        | Worker emitted `status:"ok"`.                                           | `bd close`; clear both kv keys; `thurbox-cli session delete`.              |
| `error`     | Worker emitted `status:"error"`.                                        | `bd note` with reason; session preserved; kv intact.                       |
| `running`   | No sentinel yet.                                                        | None.                                                                      |
| `malformed` | `===RESULT===` present but JSON unparseable or missing `status`.        | `bd note`; session preserved; kv intact.                                   |
| `vanished`  | `thurbox-cli session capture` failed (session deleted or unreachable).  | None — judgment left to the orchestrator LLM (clear kv or re-dispatch).    |

Exit code is `0` on normal completion regardless of bucket contents.
Non-zero only on internal reaper failure.

### Dependencies

- `bd`
- `thurbox-cli`
- `jq`
- `awk`
- `date` (for UTC timestamp)
