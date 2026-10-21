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

## Helix by hand (hxlib.py over run.py's Helix class)
- hx_actions.py: code actions per place. All "Not in Helix" items are absent in an Org file (inline task, keyword sets, property delete, ORDERED, remove dates, num mode, crypt, archive done, ID create, clock display/resolve, recalc line/iterate/debugger, detangle, hline+move, NAME_ALL cycling). But the agenda's entry line offers keyword-set switching and removing dates (helix-3).
- hx_flows.py: TODO→WAIT + note by rename, DONE+CLOSED, cookie, schedule +2d, tags typed `b:a` (kept in that order: Emacs sorts only with org-tags-sort-function), 1,3,2 renumber via Toggle the checkbox, recalc, clock in/out/last/cancel, capture t + finish (capture buffer replaced by the origin file), `%^{Name}`, gd on headline and file::*search links, archive. Refile targets: current file only (Emacs default).
- hx_agenda.py: week agenda, t, +, schedule `S => Scheduled to`, Space k hover, filter, gd, Space b back, later, Agenda commands → t. Save all does not save (helix-1); filter after an in-place edit does nothing (helix-2).
- hx_flows2.py: Space k on link, gr refs (3), rename Beta→Gamma updates links in both files, Space S workspace symbols, diagnostics picker, tangle ok; babel aborted (helix-7); clock table / dblock / sort lost (helix-6); edit special opens a-edit.sh. Footnote back (vim-15).
- Highlighting (hx_hl.py over tests/visual.org and a sample): helix-4 (markup inside comments/example/fixed-width), helix-5 (comment after keyword line; multi-word targets; \there4). Also uncolored but not claimed: radio targets, inline footnotes, `$E = mc^2$`, export snippets, `-----`, `src_python{}`, COMMENT headlines; `#+LINK:` value's URL colored as a link.

## Vim, more (lsp unless said)
- Column view (Emacs): header in status line, `1:30` sum, `e` → `EFFORT value [0:30]: :` (vim-11, lsp-5), the property becomes `:EFFORT:` as Emacs's org-entry-put does; `q` quits.
- Timers (Emacs): start/insert/item/pause/stop messages through showMessage (`Info: Lsp(org-lsp) …`), statusline `<0:00:10>`; countdown `How much time left? (minutes or h:mm:ss):` then `TODO Parent: time out` (Emacs's org-get-heading keeps TODO). Harness: tmux send-keys treats a trailing `;` as a separator: send `'\;'`.
- Export: menu, `h h` → Emacs's Babel yes/no (yegappan inputlist) → e.html written, `Exported to …`; the screen keeps the inputlist prompt until a key (yegappan).
- Babel `<Leader>obe`: lsp ok (`: 42`), coc ok, vim-lsp/ALE never (vim-17).
- showMessage per client: vim-18.
- download.sh: 4 clients × 4 ok.
- config.toml (HOME/.config/org-lsp) keywords + g:org_lsp settings on top: ok.
- Visual `>>` on two headlines ok; `.` repeats `>>`; `:Org toggle heading` on a selection impossible (vim-19).
- Insert `<CR>` mid item → `- first` / `  item text` (Emacs's); `<S-CR>` cannot be typed in tmux (vim-20); TAB on new `** ` → `* `.
- Dates: C-a/C-x on day, month, year, hour, minute, repeater; counts; S-Up/S-Down/S-Right; ois/oid typed with `i` (+2d, fri 10:00); cid; oi. twice → range; o> opens the calendar on the day without a prompt line.
- Refile (inputlist of targets) and archive (r.org_archive with ARCHIVE_* props): ok.
- Completion: `[[` pops link types with yegappan autocomplete.
- Inlay hints: vim-21.
- tests/visual.org: virtual indent shows; hidestars' hidden star drawn white on a dark terminal when 'background' is light (Vim's default when undetected) — user setting, not logged.

## Summary (end of round)
- Suites: Vim lsp/vim_lsp/ale 58 ok each; coc 55/58 once under heavy load (3 timeouts), then 42/42 cases alone, ×3 in parallel and with all four clients in parallel: flake not reproduced. Helix runner: 13 ok, 3 skip, 0 fail. download.sh: 16 ok.
- Issues: vim-1 … vim-21 (vim-2 withdrawn by its correction entry), helix-1 … helix-8.
- Biggest: vim-6 (every completed text prompt fails in Vim: E731), vim-7 (yegappan loses captures into empty files), vim-17/vim-18 (Emacs's questions and messages lost with vim-lsp/ALE), helix-6/helix-7 (Helix: sort, clock table, dblock, Babel do nothing), helix-1 (Save all lies).
- Not covered by hand: org-protocol, crypt (GnuPG), ctags, ol-man, LaTeX, bibtex commands, org-roam capture/alias/tag/refs, journal search/schedule/calendar keys, super-agenda, habits colors, appointments, Zed/VS Code (other part).
- Harness notes: tmux treats a trailing `;` as a command separator (send `'\;'`); `S-Tab` is `BTab`; there is no `S-Enter` key name; in the agenda `/` is the filter, so position with `:call search()`.
