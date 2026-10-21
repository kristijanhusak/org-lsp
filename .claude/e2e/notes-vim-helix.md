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
- Helix suite (first run): 13 ok, 3 skip (agenda-columns, calendar, journal-calendar: by design), 0 failed, 324 s.
- Clocking (lsp): over-effort message ok ("Info: Lsp(org-lsp) Task ‘Long task’ should be finished by now. (0:30)" — yegappan prefixes showMessage with `Info: Lsp(org-lsp)`), statusline `[0:40/0:30] (Long task)`, out/cancel/in-last/goto/display/clocktable ok; dangling clock after restart asks; K → "Keep how many minutes (default 0): :" (vim-11).
- Tables: recalc, >>, >s, <s ($INVALID), oTO, oTd, oJ, o-, o<CR>, o<Space>, next field past end, oD, o+, o?, o^ menu: all as Emacs (em.sh); no "The formulas in #+TBLFM have been updated" (vim-12).
- Links: headline/#cid/<<tgt>>/file::*/missing heading (y adds) ok; shell yes "hi" / no "org-lsp: Abort"; ]l [l with wrap messages ok; LspGotoDefinition ok; references ok; hover popup ok. Status line stale after async cursor moves (vim-13).
- Folding: `#+STARTUP: content` (drawers + childless folded; headline's own text visible: documented limitation), VISIBILITY children ok, TAB cycle, BTab global overview<->all. After `:e!` the startup folds are not applied again (not logged). Dashes after `...` (vim-14). Sparse tree: "2 TODO entries found", highlights, but almost nothing folds (a heading's own text cannot fold apart in Vim): limitation.
- Narrow ons → h-narrowed.org, :w writes back to the Org file on disk, onw closes (cursor back to 1:1, Emacs keeps point).
- Headings: M-RET at a headline adds right after it (as Emacs at eol), oiT/oih/oit, >>, >s, <<, oJ, oK, oiI (15 stars + END), }, 2}, {, ]], [[, g{ ("org-lsp: Already at top level of the outline").
- roam (lsp, with harness fix for vim-6: `fix_input.vim`): nf by alias, ni inserts `[[id:bbb-222][Beta]]` before the cursor char (in Normal mode after `see ` + Esc → `see[[…]] `), nl buffer beside with Backlinks (2), ndd makes daily/2026-10-21.org.
- journal: jj → journal/20261021 `* Wednesday, 10/21/26` `** 06:33 ` in another window (org-journal's find-file-other-window), jb "No journal entry before this one".
- emacs_options `{"org-attach-method": "mv"}` from g:org_lsp settings: <Leader>oxa a moves file.txt into data/bb/…/ (gone from notes/), ID + ATTACH tag. No "now an attachment" message (emacs-8).
