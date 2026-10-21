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
- Parallel rerun of all four clients' cases (42 each): all ok. coc flake did not recur (3 more runs).

## Interactive, yegappan/lsp (w1)
- g? popup OK. cit popup "Select a TODO state" with [t]... [SPC] clear, WAIT(w@) opens CAPTURE-Note.org, C-c writes `- State "WAIT" from "TODO" [..] \\` note: OK.
- DONE writes CLOSED before DEADLINE; tags realigned (org-auto-align-tags). OK.
- Harness pitfall: without 'ttimeout', Esc then C-c within 1 s is one key: set ttimeoutlen=50.
- ciR on [#A] removes it, cir twice from none → [#C]: as Emacs.
- vim-1: <Leader>o, is a text prompt (needs Enter).
- Agenda (lsp): dispatcher popup, `a` week view, `t` d on a line writes DONE + CLOSED in the loaded file, `+` → [#B], S-Right on deadline shows `=> <date>`, C-c C-s calendar (l j M-S-Right > . t k RET) → `S => Scheduled to <…>`, `s` "Saving all Org buffers... done" writes to disk. OK.
- In the agenda `/` is the filter: careful when searching with `/` in the agenda in the harness.
- vim-4 (org-lsp: prefix), vim-5 (calendar count), vim-6 (input() with a dict: E731 for every completed text prompt; MAJOR).
