# Notes of the neovim tester, 2026-10-21

Scripts in `.claude/e2e/repro/neovim/` (harness `h.lua`, runner `run.sh`,
suites `suite.sh <version>`). Scratch data under the session scratchpad
`neovim/`.

## Log
- Started: first `scripts/test-nvim.sh` run on 0.12.0/stable/nightly (stops at first failing file).
  - 0.12.0: e2e.lua "timed out waiting for the server to attach" (first run, maybe load); re-running per file.
  - stable: e2e.lua "timed out waiting for the lines selected made headings".
  - nightly: e2e.lua passes (376 checks) but prints "vim.lsp.util.character_offset is deprecated"; highlight.lua aborts Neovim: `grid.c:586: grid_line_flush: Assertion grid_line_clear_to <= grid_line_maxcol failed`.
