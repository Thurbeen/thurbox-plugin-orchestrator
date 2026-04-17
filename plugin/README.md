# gastown — thurbox plugin

Bridges thurbox and [gastown](https://github.com/gastownhall/gastown).

This directory is the installable plugin payload. It is copied to
`~/.local/share/thurbox/admin/plugins/gastown/` by the repo's
`scripts/install.sh`. The `bin/` subdir is populated at install time
with three binaries built from the parent repo:

- `thurbox-plugin-gastown` — the plugin daemon (`exec` in this manifest).
- `gc-session-thurbox` — the gastown exec session provider, referenced
  from `~/.local/share/thurbox/admin/city.toml`.
- `thurbox-worker-wrap` — the per-session helper that polls for the
  result sentinel and runs `bd close`.

See the repo's top-level [README](../README.md) for full context.
