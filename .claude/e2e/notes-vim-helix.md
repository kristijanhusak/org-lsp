# vim-helix notes (2026-10-21)

Harness: `.claude/e2e/repro/vim-helix/vh.sh` (Vim in tmux socket `vh`,
`vimrc` there loads the plugin + one client, settings from a JSON file,
`:VHDump` writes buffers, cursor and :messages to WORK/dump.txt).

## Suites, first run (all four clients + Helix at once, binary at e6f7229)

- lsp: 58 ok (cases + highlighting + 16 scenarios), 0 failed
- vim_lsp: 58 ok, 0 failed
- ale: 58 ok, 0 failed
- coc: 55 ok, 3 not ok (cases): "folds and cycles as TAB does: timed out
  waiting for folds", "a change from the agenda shows in it: timed out
  waiting for the agenda showing TODO", "a hover shows the entry of an
  agenda line: timed out waiting for the entry in a popup". Rerun alone:
  42/42 cases ok; 3 coc runs in parallel: all ok. => flake under load.
