# thurbox-plugin-orchestrator

A [thurbox](https://github.com/Thurbeen/thurbox) plugin that drives a
beads-backed multi-agent orchestrator. One ready
[`bd`](https://github.com/gastownhall/beads) item → one fresh thurbox
session → one worker that emits a result sentinel.

The orchestrator itself is a Claude session (not a daemon) running the
`orchestrate` skill. All work happens through plain shell calls —
`bd` for state, `thurbox-cli` for session lifecycle. No MCP tools and
no plugin daemon binary.

## Status

Content-only plugin. Ships two skills and a role; zero runtime
dependencies beyond `bd` and `thurbox-cli` being on `$PATH`.

## Layout

```text
plugin/                              # manifest + plugin README, copied to
                                     # ~/.local/share/thurbox/admin/plugins/orchestrator/
skills/                              # SKILL.md trees for orchestrate +
                                     # orchestrate-worker, contributed via
                                     # `[[contributes.skills]]` and staged
                                     # alongside the manifest at install time
roles/                               # `orchestrator` role TOML —
                                     # disables Write/Edit and pre-approves
                                     # Bash(bd:*) / Bash(thurbox-cli:*)
scripts/                             # install helper

crates/                              # legacy Rust daemon + core library from
                                     # the MCP-capable iteration. Unused by
                                     # the current content-only plugin; kept
                                     # until the design stabilises.
```

## Two-session model

| session                | role                                                                |
|------------------------|---------------------------------------------------------------------|
| **admin / creator**    | human + Claude, uses raw `bd create` / `bd update` to file work     |
| **admin / orchestrator** | Claude with the `orchestrate` skill + `orchestrator` role — drains ready bd items using `bd` + `thurbox-cli` |
| **worker (per bead)**  | spawned by the orchestrator, runs the `orchestrate-worker` skill, emits `===RESULT===\n{json}` |

Spawn the orchestrator session so `bd` can auto-discover `.beads/`:

```bash
thurbox-cli session create \
  --name orchestrator \
  --repo-path ~/.local/share/thurbox/admin \
  --role orchestrator \
  --skill orchestrate
```

The `orchestrator` role disables `Write`/`Edit`/`MultiEdit`/`NotebookEdit`
at the tool layer and pre-approves `Bash(bd:*)`, `Bash(thurbox-cli:*)`,
and `Bash(mkdir:*)` so the orchestrator can drive the workflow without
constant prompts.

## The workflow

The `orchestrate` skill tells the orchestrator to:

1. **Decompose** the user's request into independent `bd` items.
   `bd create`, then `bd update <id> --set-metadata repo_path=<path>`.
2. **Dispatch** each bead: `thurbox-cli session create` (with
   `--role worker --skill orchestrate-worker`), grab the UUID,
   `thurbox-cli session send <uuid> "<prompt>"`, record
   `bd kv set orch:bead:<id> <uuid>` and the reverse mapping.
3. **Poll** each session every 30–60 s:
   `thurbox-cli session capture <uuid> --lines 200`. Look for
   `===RESULT===` + a `{"status":...}` line.
4. **Close** on `status:"ok"`: `bd close <id>`, `bd kv clear` both
   keys, `thurbox-cli session delete <uuid>`. On `status:"error"`,
   leave the bead open and append a `bd note`.

State lives in `bd kv`:
- `orch:bead:<bd-id>` → thurbox session uuid
- `orch:session:<uuid>` → bd id

## Per-bead config

Set on the bd item with `bd update <id> --set-metadata key=val`.

| key         | required | meaning                                                          |
|-------------|----------|------------------------------------------------------------------|
| `repo_path` | yes      | working directory the worker session is spawned in               |
| `role`      | no       | role name to pass to `thurbox-cli session create --role ...`     |
| `skills`    | no       | comma-separated skill names for the worker session               |

## Install

```bash
./scripts/install.sh
```

Stages the manifest + skills + role into
`~/.local/share/thurbox/admin/plugins/orchestrator/`. Set
`THURBOX_ADMIN_ROOT` to target a different admin workspace (e.g.
`~/.local/share/thurbox-dev/admin` for dev builds).

Then in thurbox:
1. Restart so the plugin is picked up (or `register_plugin` via MCP).
   The skills and role are auto-loaded from the manifest's
   `[[contributes.*]]` rows.
2. Create a bd item and ask the orchestrator to dispatch:
   ```bash
   bd --db ~/.local/share/thurbox/admin/.beads/ create "echo hello" --label demo
   bd --db ~/.local/share/thurbox/admin/.beads/ update <id> \
      --set-metadata repo_path=$HOME/scratch/hello
   ```

## License

MIT — see [LICENSE](LICENSE).
