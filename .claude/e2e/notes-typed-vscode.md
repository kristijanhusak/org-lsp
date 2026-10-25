# Notes: VS Code typed, 2026-10-25

Everything typed into VSCodium 1.135 (`/root/vscodium/codium`) under Xvfb
with xdotool, prompts answered by typing into VS Code's own quick picks,
input boxes and toasts (`notifications.focusToasts`, Tab, Enter), checked
with screenshots (`import`) and the extension's test Api. Server: the
prebuilt `target/debug/org-lsp`. Scripts: `.claude/e2e/repro/typed-vscode/`
(`vx.sh CASES.cjs OUT.json SETTINGS.json`, `show.py OUT.json`; b1-b7 are the
batches, b2b/b4b/b5b/b5c the reruns). The Emacs keys unless said.

## The typed scenarios

- `npm test` (scenarios part) now types every step's keys, its text and
  every answer (key menus, quick picks, input boxes, the date prompt's `i`
  and date) with both presets; commands a preset has no key for get a key
  of the user's (run.mjs). 32 runs (16 x emacs/vim), 0 failed, 337 s.
- vscode-typed-1 (fixed): typing a key in a key menu (`n` among `[m]
  Meeting`, `[n] Note`) made VS Code take the first choice whose text has
  the letter (`Meeting`). Found by capture.json step 8 typed.
- Once boxes keep the text they start with (vscode-typed-2, -4), the
  adapter types after it what the answer adds, or selects it all (Ctrl+A)
  and types over it, as a user does: rescheduling starts with `09:00`.
- vscode-typed-7 (server-side, open): the bulk scatter (`B S`) starts its
  box with `7`, Emacs's `read-number` default, which Emacs shows in the
  prompt with an empty minibuffer: typing `1` gives `71` (Neovim's
  `vim.ui.input` too).

## TESTING.org, by group

- Setup: Org commands in the palette ok (screenshot); config.toml
  (`ORG_LSP_CONFIG`, `org_todo_keywords` TODO WAITING | DONE) reaches the
  server ok. Not checked: install.sh / .vsix (not this round; the earlier
  round did), VSCodeVim (not installed: `--disable-extensions`, no
  marketplace; the vim preset's keys are checked by the scenarios).
- Highlighting: tests/visual.org screenshots; links, entities, sub- and
  superscripts, emphasis, timestamps, drawers, keywords, blocks, Python code,
  inline code, tables, footnotes, macros, `org_hide_emphasis_markers`,
  `org_emphasis_alist`, hidden keywords, macro markers, pretty entities,
  custom times (and C-c C-x C-t), bullets, checkboxes, `headlineLevelColors`
  on and off ok. vscode-typed-5 (server-side, legend.rs, open): DONE has the
  same color as TODO, and a done headline (and `org_fontify_todo_headline`'s
  look) the same as any headline, in VSCodium's default theme (2026 Dark)
  and in Dark+/Dark Modern, whose `keyword` and `markup.heading` rules match
  `keyword.other.done.org` and `markup.heading.done.org` first.
- Folding and visibility: TAB / S-TAB cycles (status bar FOLDED, CHILDREN,
  SUBTREE, OVERVIEW, CONTENTS, SHOW ALL), `#+STARTUP: overview|content`,
  `:VISIBILITY:`, two blank lines, TAB on a new `** ` and `- `, narrow and
  widen (written back), sparse tree (highlights gone when typing), C-c C-r
  ok. Not checked this round: `org_cycle_include_plain_lists`,
  `org_cycle_global_at_bob`, ARCHIVE folding (the extension's tests cover
  them).
- Headings and structure: M-RET, M-S-RET, C-RET, C-S-RET, M-left/right,
  M-S-left/right, M-up/down, inline task (and demoting it), cut / copy /
  paste subtree, `org_allow_promoting_top_level_subtree`, sort (`Ctrl+C
  Shift+6`), heading motions and C-c C-u at the top ("Already at top level
  of the outline"), C-c C-j, special C-a / C-e / C-k, toggle heading,
  COMMENT, clone with `+1w`, a selection demoted and turned into headings,
  Outline ok. Not checked: `org_yank_adjusted_subtrees`, folded yank.
  Found (by design, written down): with the Emacs keys `Ctrl+K` is C-k in an
  Org file, so VS Code's Ctrl+K chords there are gone: Ctrl+K Ctrl+F
  (Format Selection) deletes the selection (C-k's fallback, VS Code's
  deleteAllRight; Emacs's C-k ignores the region and kills to the end of
  the line), Ctrl+K Ctrl+I (Show Hover) kills to the end of the line.
- Lists and checkboxes: M-RET (numbers go on), RET in an item, indent /
  outdent with and without children, M-up, bullet cycling, checkbox toggle
  with parent and cookie, renumber, sort, C-c - on lines and in a table
  ok.
- TODO keywords: dependencies (blocked messages as Emacs), fast selection by
  key, CLOSED with `org_log_done`, a note for `@` in the note buffer, a
  repeater, `#+TODO:` sets, keyword sets (`Keyword-Set 2/2: REPORT BUG
  FIXED`), cycle forward/backward from the palette ok. vscode-typed-3
  (fixed): the TODO menu showed no colors: its dots were given as
  `vscode-userdata:` URIs (not shown) and theme icons (shown in one color).
- Priorities: from the palette, C-c , with A and SPC ("Priority removed"),
  `#+PRIORITIES: 1 5 3` ok.
- Tags: fast selection by key with RET, exclusive groups, typed tags with
  completion, F2 rename (headline and tag, the file's `#+FILETAGS:` too)
  ok. vscode-typed-4 (fixed): typing tags in C-c C-q (or `:` in the agenda)
  replaced the headline's tags, which VS Code had selected; Emacs's
  minibuffer has them with the cursor after them.
- Properties and column view: column view (header in the status bar, sums),
  right, `e` on TODO, `v`, `q`, C-c C-x p with completion, S-left/right on
  NAME_ALL, C-c C-c in a drawer (property action), delete (offered upcased,
  as Emacs) and delete globally, effort, ORDERED ok. Not checked: moving and
  resizing columns.
- Dates and times: schedule `+2d`, deadline `fri 10:00`, remove both,
  delay and warning from the palette, active / inactive timestamps and a
  range, the preview under the box (`=> <2026-10-30 Fri>`), every calendar
  key (S-arrows, M-S-arrows, `>`, `.`, `t` with S-up, Enter, `q`, C-g),
  C-c > opening on the day, time rounding 12:44 → 12:45, week start Monday,
  increase / decrease / move / change the timestamp at the cursor ok.
  vscode-typed-2 (fixed): changing a timestamp asks with its time in the box
  (`11:00`), as Emacs's minibuffer; VS Code selected it, so typing replaced
  it and the time was lost. Note: the calendar, three months wide, is cut
  off and scrolls sideways when its editor group is narrow (1440 px window
  with the side bars open).
- Agenda: the menu (and a custom command), week view with time grid, f, b,
  ., v d / v w / v m, J (a date typed), habits' graphs, TODO list, tags
  match, search, Tab and Enter, t, +, S-up, C-c C-s, C-c C-d, `:` tags, z
  note, refile, archive, filters (`<`, `_` with `<` and `3`, `|`), marks, F
  follow mode, the outline path in the status bar, E, v l, R, v a, `[` and
  r, hover, I / O with the line highlighted, column view, s, q ok. Not
  checked: appointments (Emacs's real clock), `org-lsp.winSplitMode`, bulk
  action typed here (the typed scenario agenda-bulk does it).
- Clocking: clock in / out / cancel / last / goto, the status bar clock
  with effort, C-c C-x C-d ("Total file time"), clock table insert and
  update, a dangling clock with `K` and Enter, clocking in from the agenda
  ok. Not checked: the "should be finished by now" message (needs time to
  pass), S-left on `:block`.
- Capture: the menu, `%?`, `%^{Name}`, finish, abort, refile, a date tree
  (`file+olp+datetree`), `%a`, `%c`, `%[snippet.txt]`, `%(upcase "abc")`
  ok.
- org-protocol: `org-lsp protocol 'org-protocol://store-link?…'` says
  Emacs's message, and C-c C-l offers the page ok. Not checked: the
  bookmarklets in a browser, open-source, starting the editor.
- Notes (org-roam): find (by alias), insert, insert on a selection (the box
  starts with it), a new node through its capture buffer, random node,
  alias add / remove, tags, `roam:` link followed and rewritten, `[[roam:`
  completion to an `id:` link, the buffer beside (Backlinks (1), the cursor
  staying) following the cursor's node, hidden again, following its item,
  a dedicated buffer with Unlinked References (1), dailies today / capture /
  date (calendar "Find daily-note:") / next / previous / directory ok.
- A journal (org-journal): new entry (file without extension as Org, the
  TODO carried over), previous entry ("No journal entry before this one",
  also after `[` moved in the calendar without follow mode: Emacs's
  `org-journal--next-entry` says it then too), entry for a date, scheduled
  entry, search with two days typed in the date prompt, the TODOs to come,
  the schedule, rescheduling, the TODO list with the journal's file,
  calendar `[`, `j r`, `j d`, `j n`, `j s w`, follow mode (the entry shows
  after a second or two) ok. Note: the week view shows no undated TODO of
  today's journal file, as in Emacs; TESTING.org's wording ("the TODO
  entries of today's journal file are in the week view") fits the TODO
  list, not the week view.
- Refile and archive: copy, goto target, goto last, refile to another file,
  `org_log_refile` time and note (note buffer), archive to the archive
  file, to the sibling, ARCHIVE tag, archive done subtrees with `y` ok.
- Links: headline, custom ID, target, file::*search, `#+LINK:`
  abbreviation (opens github.com/org-lsp), missing heading made with `y`,
  shell link y ("hi") / n ("Abort"), timestamp to its day agenda,
  `org_file_apps` (Running cp …done), F12, store and insert with
  description, edit an existing link (the box starts with it, as Emacs),
  ID, next / previous link, hover, references, rename updating links,
  workspace symbols ok. Not checked: man / info (no man pages), ctags (no
  Universal Ctags).
- Footnotes: reference to definition and back, "No link found" after the
  label, the missing-definition diagnostic (Information), creating one
  through the toast's Yes (placed as Emacs places it), normalize ok.
- Tables: Format Document, next / previous field, moving and inserting
  columns and rows, rules, C-c RET, blank field, S-RET copying down (and off
  a table keeping its meaning), sum and field info messages, create (`3x2`
  and empty), sort, shrinking `<5>` ok. Found (server-side, minor):
  S-RET on the `|` that starts a row says Emacs's "Not in table data field"
  as an LSP Error message (an error toast), where other refusals are error
  answers the extension shows as warnings (`copy_down.py`).
- Table formulas: `$3=$1*$2`, C-c C-c on `#+TBLFM:`, vsum with `%.2f`, date
  arithmetic (`[2026-05-25 Mon]`), Elisp formula, `remote()`, C-c * on one
  line ok. Not checked: the formula debugger, C-c ' on formulas, C-c `.
- Code blocks (Babel): evaluation asks through a toast (Yes typed), results,
  C-c C-v k, C-c C-c, inline results `{{{results(=42=)}}}`, next / previous
  ("No previous code blocks"), goto by name, tangle, C-c ' (file with the
  language's extension, written back indented as Emacs) ok. Note: in that
  `.el` file Ctrl+X is VS Code's Cut (the Emacs keys are bound in Org files
  only), so C-x C-s cuts the line before saving. Not checked: LOB ingest,
  detangle, buffer / subtree evaluation, a slow block.
- Export and publishing: the menu with Emacs's backends and C-b … toggles,
  to an HTML file ("Exported to …"), `#` default template, `&` stack ok.
  Not checked: "and open", the subtree as a buffer, the Markdown backend.
- Citations: unknown key with suggestion (Information), possibly missing
  PRINT_BIBLIOGRAPHY, hover, following to the .bib, inserting (the second
  question offers "(done)") ok. Not checked: BibTeX commands.
- Diagnostics (org-lint): misplaced heading (Hint), unknown fuzzy link,
  unknown header argument, incomplete block (Information),
  `org_lint_ignore` ok.
- Indentation and formatting: `#+STARTUP: indent hidestars` (virtual
  indent), num on and off from the palette ok. Not checked properly:
  `org_adapt_indentation` (the file had org-indent-mode on), Format
  Selection (see Ctrl+K above; from the palette it is fine in the
  extension's tests).
- Notes and logging: C-c C-z note buffer, LOG_INTO_DRAWER, the agenda's z,
  `org_log_reschedule`, done yesterday (CLOSED 23:59) ok.
- Completion and editing helpers: entities (`\alp` → `\alpha`),
  structure template `s`, emphasis on a selection, drawer ok; TODO keyword
  and `#+STARTUP:` completion popups seen in screenshots.
- Timers: start (Emacs's message), insert value, pause / continue / stop,
  countdown typed `0:00:03` ("Heading: time out"), the status bar value ok.
- Attachments: attach with `a`, the `attachment:` link opens the file ok.
- Encryption: not checked (no GnuPG key set up this round).
- Inline images and LaTeX previews: C-c C-x C-v on and off (Emacs's
  messages), as high as a line, whole on hover ok. Not checked: LaTeX (no
  latex / dvipng), remote images, `#+STARTUP: inlineimages`,
  `#+ATTR_ORG: :width`.
