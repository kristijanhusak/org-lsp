# Notes of the neovim tester, 2026-10-21

Scripts in `.claude/e2e/repro/neovim/` (harness `h.lua`, runner `run.sh`,
suites `suite.sh <version>`). Scratch data under the session scratchpad
`neovim/`.

## Log
- Started: first `scripts/test-nvim.sh` run on 0.12.0/stable/nightly (stops at first failing file).
  - 0.12.0: e2e.lua "timed out waiting for the server to attach" (first run, maybe load); re-running per file.
  - stable: e2e.lua "timed out waiting for the lines selected made headings".
  - nightly: e2e.lua passes (376 checks) but prints "vim.lsp.util.character_offset is deprecated"; highlight.lua aborts Neovim: `grid.c:586: grid_line_flush: Assertion grid_line_clear_to <= grid_line_maxcol failed`.
- Suites re-run per file (suite.sh): 0.12.0 and stable pass every file (e2e 376, agenda 215, scenarios 16, images 35, latex 4, download 20, protocol, highlight 89 looks); the first-run failures (0.12.0 attach timeout, stable "lines selected made headings") did not come back in 3 more runs each: load from other testers' builds. nightly: e2e/agenda/scenarios/images/latex/download/protocol pass, highlight aborts (neovim-1).
- vim.pack (pack.sh): cmd, PATH and download (fake file:// release) modes all attach on stable; PackChanged update → :OrgLspDownload → server restarted, on stable and 0.12.0. checkhealth fine.
- structure.lua (headings, lists, TODO, priorities, tags): all pass after correcting my expectations against Emacs (C-k on folded headline leaves an empty line; RET mid-item gives 3 spaces like Emacs; typed tags are not sorted in Emacs either — TESTING.org says "sorted": doc ambiguity; S-RIGHT from none goes to TODO as Emacs).
- config.lua: config.toml + setup() layering, user keys, g?, agenda from anywhere, special C-a/C-e/C-k, C-k protect, promote top level: pass; conceallevel overridden (neovim-5).
- dates.lua (bottom/float/cursor, week start 1, one month): pass; issues neovim-6 (no live date preview), -7 (counts), -8 (typeahead/E565 race). Delay/warning cookies -5d/-7d for answers -3d/-5d are Emacs's arithmetic (relative to today).
- agenda (TUI via tmux `tui.sh`, more reliable than headless for the agenda): menu with custom command `d`, week view (time grid, deadline warning, tags `:office::urgent:` as Emacs shows inherited tags), f/b/./vd/vw/vm/J, t, +, S-Up, C-c C-s/C-c C-d (agenda line gets ` S => Scheduled to <…>` as Emacs's `org-agenda-show-new-time`), z note, C-c C-q, C-c C-w, C-c C-x C-a, filters `\` TAB, `<`, `_ <`, `|`, m/B (bulk TODO, `Acted on 2 entries`), F follow, outline path echo, E, vl (`Log mode is on`), R, va, `[` (`Display now includes inactive timestamps as well`, gone after r), K hover (Org float, a FileType markdown autocmd's spell/number/cursorline undone), column view and `e` on an effort: all work. Issues: neovim-9 (typeahead before one-key prompts), -10 (link hover stays markdown), -11 (`: :` in Emacs prompts).
- columns/props: Emacs writes `:EFFORT:   2:00` when column view edits an `:Effort:` (checked in Emacs batch, same); M-Left/M-Right column follow (neovim-12), M-Up/M-Down need Org > 9.6 (neovim-13). `n`/`p` on a column need `_ALL` in that entry (Emacs says "Allowed values for this property have not been defined").
- misc (clocking, capture, refile/archive, links, footnotes): all behave as Emacs (checked in batch: datetree's leading blank line, `[fn:2]` placed before `[fn:1]`, `No link found` with the cursor after a definition's label, `:block today` S-Left → `today-1`, `org-archive-all-done` on a headline does its children → "0 trees archived"). Resolve prompt "Keep how many minutes (default 30): : " (neovim-11 again).
