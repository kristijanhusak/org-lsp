# Vim typed round (2026-10-25)

Harness: `editors/vim/test/run.sh` typed scenarios (`ORG_LSP_VIM_TESTS=typed`,
`ORG_LSP_TEST_STOP=N` to stop a typed scenario after step N and go on by hand);
for the checklist, a Python driver typing into Vim in tmux (socket `vtyped`):
the worktree's plugin, one client, `updatetime=300`, `ttimeoutlen=50`, leader
`\`, 100x40 unless said. Server: the prebuilt debug binary of main.

## Typed scenarios: what they found

- Alt with a character (`<M-m>`, `<M-*>` in the agenda) could not be typed in
  tmux: it comes as `<Esc>m`. Fixed: the plugin binds `<Esc>m` too (test.vim
  asserts it; typed `agenda-bulk` fails without it).
- coc.nvim: the command's message (`Acted on 2 entries`, refile's) was lost
  when the answer came while coc.nvim opened an unloaded file it edits with
  `:silent` (the plugin's `attach` waits on coc.nvim there). Fixed with
  `:unsilent` (typed `agenda-bulk` with coc fails without it).
- coc.nvim: `[coc.nvim] Use ':wa' to save changes …` after an edit of a file
  not shown, with the command's message, gives Vim's hit-enter prompt on an
  80-column screen (refile from the agenda, refile from capture). coc.nvim's
  own; documented in docs/vim.org (`coc.preferences.messageLevel`). The typed
  harness types Enter there.
- ALE: `g:ale_hover_cursor` (on by default) echoes the hover where the cursor
  rests, into `:messages`; in the agenda that is the entry's headline, which
  replaces the command's message after 'updatetime'. ALE's own; documented;
  the test vimrc turns it off.
- vim-lsp: applying an edit to another file, it switches the window's buffer
  and back with `:buffer!`, and Vim's line about the buffer
  (`"Agenda.org-agenda" [readonly] line 3 of 7 --42%-- col 1`) goes into the
  messages. vim-lsp's own; filtered in the harness.
- Vim: CTRL-C typed while a callback has changed the current buffer (the
  harness first went to the agenda with win_gotoid() from a timer) is an
  interrupt, not the start of `<C-c>l`: Vim decides whether CTRL-C is mapped
  when it starts waiting for a key (ui.c `ctrl_c_interrupts`). Not seen when the
  plugin opens the agenda from a callback (tried `\oa a` then `C-c C-s` with
  'updatetime' 4000: works). Harness types `:call win_gotoid()` instead.
- Vim keeps a blank line in `:messages` when insert mode ends (`-- INSERT --`
  cleared), even without plugins; filtered.

## Checklist (editors/vim/TESTING.org), coc.nvim first

- Setup (coc, lsp, vim_lsp, ale): ok. Server starts; `g?` in an Org file and the
  agenda; `\oa` from `:enew`; `~/.config/org-lsp/config.toml` keywords apply
  and `g:org_lsp` settings go on top (`org_log_done` false over toml's
  `"time"`); inlay hints of `\oxd` show with coc, lsp (`showInlayHints`),
  vim-lsp (`g:lsp_inlay_hints_enabled`), none with ALE (documented).
  download.sh: see the end.
- Highlighting (coc): ok after a fix. Levels, `org_n_level_faces`,
  `org_cycle_level_faces`, TODO headline, whole heading line (signs), emphasis,
  `org_emphasis_alist`, `org_fontify_emphasized_text = false`, ARCHIVE shading,
  Python in src blocks and `src_python{}`, table, footnote, target, macro,
  entity, `:colorscheme desert` keeps them. Bug (fixed, ab36328): with
  `org_fontify_quote_and_verse_blocks` the text of quote and verse blocks had no
  look in Vim: their groups `@markup.quote` had no stand-in, so no text
  property type; highlight.vim missed it as it only checked looks with a type
  (now it fails on a look with groups and no type).
- Folding (coc): ok. TAB folded → children → subtree; `org_ellipsis " ▾"`;
  S-TAB overview ↔ all (archived subtrees stay closed in all, as Emacs's
  `org-cycle-hide-archived-subtrees`); `#+STARTUP: overview`, `content`;
  `:VISIBILITY:` (applied only with a `#+STARTUP` other than showeverything, as
  Emacs); one blank line of two stays visible after a folded subtree
  (`org-cycle-separator-lines`); TAB on new `** ` → `* ` → `** `, on new `- `
  indents; lists open in children, folded with `integrate`;
  `org_cycle_global_at_bob`; narrow `\ons`, `:w` writes back and saves,
  `\onw` and `\o'` widen; sparse tree `\o/ t` (2 TODO entries found,
  highlights, typing removes them; kept with
  `org_remove_highlights_with_change = false` until `C-c C-c`, with Emacs's
  message); `\oR` reveal.
- Headings and structure (coc): ok. `\<CR>`, `\oiT`, `\oih`, `\oit`, `<<`/`>>`
  (tags realigned), `<s`/`>s`, `\oK`/`\oJ`, inline task (15 stars + END, `>>`
  16, folds hiding text and END, left out of documentSymbols), cut/copy/paste
  subtree (level fits), folded copy pasted folded, `:Org yank` before `* B`
  folded and on `** C` with Emacs's message, `:Org kill line` on a folded
  headline and both `org_ctrl_k_protect_subtree` values, promoting top level to
  `# A`, sort `\o^` a/o (Emacs's todo order), `}`/`2}`/`{`, `]]`/`[[` stop at the
  parent's end, `g{` and its message, `\oj` (a numbered inputlist of outline
  paths: typed number), special C-a/C-e/C-k, `:Org toggle heading` and
  `\o;` and `\osc` (Emacs), Visual `>>` on two headlines, documentSymbols
  nested.
- Lists and checkboxes (coc): ok. `\<CR>` on items (a mixed list's bullets
  made alike, as Emacs's `org-list-struct-fix-bul`; no box after a box item),
  RET mid item, `>>`/`<<`/`>s`/`<s` (parent box follows), `\oJ`/`\oK`,
  S-Right/S-Left bullets `-` `+` `1.` `1)`, C-Space box + cookie, renumber 1 3
  2, sort `\o^ a`, `\o-` on a selection (Emacs) and in a table. `<S-CR>`: not
  typable in tmux (documented).
- TODO keywords (coc): ok after a fix. Blocked by child / by checkboxes with
  Emacs's messages; dimmed in the TODO list; keys menu with WAIT in its face;
  `.` repeats (asks again); CLOSED; WAIT(w@) note buffer, `C-c` files it;
  repeater +1w with LAST_REPEAT; cookie; `#+TODO: A B | C`; two sets cycle
  BUG FIXED none REPORT with `cit` (`:Org todo next` goes none → TODO, as
  Emacs's `org-todo 'right`); nextset/previousset messages; keyword face in
  the file and agenda; `org_use_fast_todo_selection` false. Bug (fixed,
  04b5cf3): `"expert"`'s prompt is two lines, and Vim showed its hit-enter
  prompt over it; typed.vim now types such a prompt's key and fails without
  the fix.
- Priorities (coc): ok. `ciR` on [#A] removes it, `cir` twice from none → [#C]
  (Emacs), `\o,` b / SPC with Emacs's messages, `#+PRIORITIES: 1 5 3` starts at
  [#3] then [#2].
- Tags (coc): ok. Fast tags menu from `#+TAGS`, keys toggle, exclusive group,
  typed tags with completion after `:` (kept in the order typed: Emacs sorts
  only with `org-tags-sort-function`; TESTING.org says "sorted"), aligned to
  `org_tags_column`; completion after `:` at a headline's end (coc, lsp;
  vim-lsp with `omnifunc=lsp#complete`, ALE with `g:ale_completion_enabled`);
  `org_tag_faces` on `work` only; coc rename of a tag renames it in headlines
  and `#+FILETAGS`.
- Properties and column view (coc, Emacs from ORG_LISP_DIR): ok. Column view
  with `#+COLUMNS`, header in the status line, effort sum; `h`/`l` pick a
  column, `n`/`p` allowed values (the key becomes `:OWNER:`, as Emacs's
  `org-entry-put` from column view), `e` (an inputlist of the allowed values),
  `v`; M-Left, `<`/`>`, M-Down/M-Up (rows moved), M-S-Left asks (y/n) and
  deletes; `q`. `\oxp` name completed, `_ALL` values as an inputlist; S-Right
  /S-Left on a property line; `:Org property delete` (an inputlist of
  CATEGORY, COLOR, OWNER, as Emacs's list) and `delete globally` (`Property
  "Color" removed from 2 entries`); C-c C-c in a drawer offers set/delete/
  delete globally/compute; `\oxe`; `:Org ordered toggle` (Emacs's message).
  Server: completing a property name at `:Co` in a drawer gives `color:` and
  `end:` only (lower case); Emacs's `pcomplete/org-mode/prop` offers the
  buffer's and the default property names in both cases (`COLUMNS:`,
  `COOKIE_DATA:` …) and no `END`. Minor, not fixed (server).
- Dates and times (coc): ok but one. `\ois` `i +2d`, `\oid` `i fri 10:00`,
  delay (`-3d` → `-5d` from the scheduled day, as Emacs computes it), warning,
  remove both (Emacs's messages), `\oi.`, `\oi!`, the calendar's keys (also
  typed in the `calendar` scenario), `t` from 12:44 → 12:45, `l` then `k`
  12:50, `j` down to 12:40, `calendar_week_start_day 1`, a six-week month
  (window as high, cursor on the day), `\o>` with no prompt line, C-a/C-x on
  year/day (count 3)/minute/hour/month, S-Up/S-Down, `cid`, repeater and
  warning kept. A prompt with the entry's time already typed (`09:35`)
  needs it cleared first, as Emacs's minibuffer does.
  **Server bug**: `\oi.` RET then `\oi.` RET gives
  `<2026-10-25 Sun--<2026-10-25 Sun>>`: after the first, Vim's cursor is on
  the stamp's `>` (Normal mode cannot be past the end), the server takes that
  position as inside the stamp and inserts `--<…>` there (`planning.rs`
  `timestamp`, `repeated`: `out.insert_str(offset, …)`). Emacs's point is
  after `>`. Repro: empty line in an Org file, `\oi.` Enter, `\oi.` Enter.
  Expected `<2026-10-25 Sun>--<2026-10-25 Sun>`.
- Agenda (coc; the 16 typed scenarios type most of it with every client: the
  dispatcher, f b . vd vw vm vy J, Tab/RET, t + , C-c C-s/C-d, C-c C-q, z,
  C-c C-w, C-c C-x C-a, a (confirm), filters / \ < = _ ^ |, bulk m u U M-m * M-*
  % B, F, E vl R va vA v[ [ ] { }, K habits, r s C-x C-w, columns, q): ok.
  By hand: the menu lists `org_agenda_custom_commands` (`[w] Work tasks`) and
  runs it; `m work+urgent`; `s report`; week view with time grid and
  `In 3 d.:` deadline; S-Up priority; `K` hover popup of the entry;
  `\ois`/`\oid` in the agenda (`S => Scheduled to`, `D => Deadline on`); the
  entry's path in the echo area as the cursor moves; `org_super_agenda_mode`
  groups (Today with the time grid, `Category: calls`) in the week and the
  TODO list; `org_agenda_prefix_format` with `(org-entry-get nil "EFFORT")`
  (Emacs: `1:00`, `nil`); agenda column view with the sum on the header, `e`
  on the effort writes `:EFFORT: 2:00`; `:Org agenda appt` (`Added 1 event for
  today`, then `Appointment in 3 minutes: 14:59 Meeting`); `:Org agenda list`
  with no file open. Habit colors: highlight.vim checks the backgrounds.
  Bug (fixed, d2fe44f): the agenda and the calendar were 'readonly' but
  modifiable: `x`, `dd` there changed them after Vim's W10 warning, the client
  sent that to the server (whose lines of the view no longer matched) and the
  highlighting failed with E964. Now 'nomodifiable'; test.vim checks `x` in
  both.
- Clocking (coc): ok. Over-effort `Task ‘Long task’ should be finished by now.
  (0:30)` once, statusline `[0:45/0:30] (Long task)`, out, cancel (`Clock
  canceled`), `\oxx` (`Clocking back: …`), `\oxj` from another file, `\oxd`,
  `\oxr` clock table, S-Right on `:block today` → `today+1` (and Emacs's
  message without `:block`), `\oxu`; a clock left running: statusline at once,
  `\oxz` asks `Dangling clock started 358 mins ago [jkKtTgGSscCiq]? ` with the
  choices, `K` Enter stops it; clocking in elsewhere asks first. Agenda I/O
  and its line highlight: scenario + highlight.vim.
- Capture (coc): ok. Menu, `%?` in insert mode, `%^{…}`, finish, abort, refile
  (typed `capture` scenario), `file+olp+datetree` (2026 / 2026-10 October /
  2026-10-25 Sunday), `%a`, `%c` from `"a`, `%[snippet.txt]`, `%(upcase "abc")`
  (Emacs). coc.nvim opens the unloaded target in a tab of its own
  (documented, `workspace.openResourceCommand`).
- org-protocol (coc): ok as far as it goes without a browser. `org-lsp protocol`
  run by hand: store-link (Emacs's message in Vim, `\oli` offers the URL),
  capture (title, link, text), open-source with `org_protocol_project_alist`
  (underscored keys), and with no Vim running, `org_protocol_editor` (a tmux
  command starting Vim) opens the capture there. Not done: `scripts/install.sh
  protocol` (the system's handler) and the bookmarklets (no browser here).
- Notes, org-roam (coc): ok. `\nf` completes an alias (`Fir` → First) and
  visits; `\ni` inserts `[[id:…][Gamma]]`; a new title opens a capture with
  ID and `#+title:`, `C-c` writes it and puts the link in; `:Org roam node
  random`; alias add (`ROAM_ALIASES`), remove asks; tag add writes
  `#+filetags: :idea:`; `[[roam:Beta]]` followed visits Beta and becomes an
  id link, `:w` converts the others; `[[roam:` completes titles; `\nl`
  beside with `Backlinks (1)` and `Unlinked References (1)`, follows the
  cursor into another node, toggles off; `\oo` on an item's headline offers
  its links (as Emacs at a headline); `:Org roam buffer dedicated` asks the
  node and opens `org-roam-buffer-alpha.org`; dailies `\ndd` (ID, title),
  `\ndb`/`\ndf`, `\ndn` captures at its end, `\ndc` (`Find daily-note:`, the
  days with a note marked, RET visits), `\nd.` (an inputlist of the notes).
  The org-roam buffer can be typed into (Emacs's is read-only); left as it
  is: the server changes it with edits, which a 'nomodifiable' buffer would
  refuse.
- A journal, org-journal (coc): ok. `\jj` today's file (Org without an
  extension), the date header and `** 10:30 `, cursor after it, yesterday's
  TODO carried over; `\jb` back to the first and `No journal entry before
  this one`; new date entry for a future date gives `** TODO` and the date's
  stamp, as recorded from org-journal (tests/journal); scheduled entry asks;
  agenda integration (a journal TODO with today's stamp in the week view);
  `\js` asks the string and both days, results link to the lines; future
  scheduled, schedule view, reschedule (moved to 20261030); the calendar marks
  journal days; with `org_journal_follow_mode`, `]` shows the next day's
  entry and the cursor stays in the calendar. The calendar's j keys: typed
  `journal-calendar` scenario.
- Refile and archive (coc): ok. Targets as an inputlist (`Refile subtree "Task
  one" to:`, other file's with `(other.org)`), `org_log_refile` time and note,
  to another file, `\oy` copy, `:Org refile goto` and `goto last`, `\o$` to
  `work.org_archive` with ARCHIVE_* properties, `\oxA` sibling, `\oA` tag on
  and off, `:Org archive done` asks for each. Agenda archiving: scenario.
- Links (coc): ok after a fix. `#+LINK` and `org_link_abbrev_alist` (opened
  with a stand-in `xdg-open`), headline, custom id, `<<target>>`,
  `file:…::*Heading`, `id:`, `[[No such heading]]` asks and adds it,
  `org_file_apps` `cp` (`Running cp …done`, the copy made) and `4\oo` opens
  the file in Vim, URL, timestamp (day agenda), `shell:` yes (`hi`; coc.nvim
  shows the server's showMessage as `[coc.nvim] hi`, known) / no (`Abort`),
  `info:` on the web, coc definition/hover/references/workspace symbols,
  rename of a headline updates the links in other files, `\ols`/`\oli` with
  the stored link as default, `\oli` on a link edits it, `:Org id create`,
  `]l`/`[l` (`No further link found`, then from the start), `[[` completion
  of link types, `[[*` headlines, `[[#` custom ids, `[[file:` files (coc and
  lsp). Bug (fixed, 356378c): with `org_return_follows_link`, RET on a link in
  Insert mode did nothing: the plugin put the cursor back after the server
  had gone to the target (it undoes coc.nvim's move a column left); test.vim
  types RET on a link. Not checkable here: `man:` (no man pages in this
  container), org-ctags (`ctags` is Emacs's, not Universal Ctags).
  Server: with `org_modules ["ol-man"]`, `[[man:ls::-l]]` is still reported
  as `Unknown fuzzy location "man:ls::-l" (org-lint invalid-fuzzy-link)`;
  Emacs's org-lint knows the link types of loaded modules. Minor.
  With `org_file_apps` set to only `[["txt", …]]`, `file:other.org::*H` went to
  `xdg-open` (Emacs's default list falls back to Emacs for an Org file);
  without the setting it opens in Vim. Not pursued.
- Footnotes (coc, Emacs): ok. `[fn:1]` to its definition after the label and
  back, `No link found` right after the label, the undefined reference as
  information, `\of` inserts `[fn:2]` and its definition and Emacs's
  message, on an undefined reference asks `No definition for 9.  Create one?
  (y or n)`, `\oF` renumber and sort.
- Tables (coc; Emacs where needed): ok after a fix. `\o<Tab>` in a column
  shrinks (`…` after each field) and back, `#+STARTUP: shrink`; coc format
  aligns; next/previous field, past the last makes a row; `<<`/`>>` move a
  column (`#+TBLFM` follows: `$1=$1`), `\oJ`/`\oK`, `>s`/`<s`, `\oTO`/`\oTd`,
  `\o-`, `\o<CR>`, `\o<Space>`; `:Org shift up/down` swap fields (Emacs);
  S-Right on a stamp in a field; `\oD` copies down incremented (and Emacs's
  `Not in table data field` off a table); `\o+` (`Sum of 2 items: 12`),
  `\o?`; `\o^` sort; `\oi|` 3x2. `The formulas in #+TBLFM have been updated`
  shows now. Bug (fixed, 13840f8): with yegappan/lsp the `…` stayed after the
  column was made whole again (yegappan keeps its hints when the server
  sends none); test.vim checks it with every client but ALE (inlay hints are
  on in the test vimrc now). **Server bug**: a file with `#+STARTUP: shrink`
  opened with `:e` shows no `…` with yegappan/lsp or vim-lsp: those send a
  full didChange with the same text right after didOpen, and the server
  forgets the shrunk columns. Repro without an editor:
  `.claude/e2e/repro/vim-typed/shrink_didchange.py` (didOpen, inlayHint → 4
  hints; didOpen, didChange with the same text, inlayHint → `[]`). Expected:
  a change that changes nothing keeps them (`shrunk_after_change` treats the
  whole text as changed).
- Table formulas (coc, Emacs): ok after a fix. `\o*` `$3=$1*$2`, `:Org table
  recalculate line`, C-c C-c on `#+TBLFM`, `vsum(@2..@-1);%.2f`, `$2=$1+7` on a
  stamp (`[2026-11-01 Sun]`, inactive as Emacs gives it), `:Org table
  iterate` (`Table was already stable`), an Elisp formula and `remote()`
  (Emacs), `\o'` formulas in a file of their own written back on `:w`, `\o``
  field editor: lines joined on `:w`. Bug (fixed, 78d8e1c): with the formula
  debugger on (`\o{`), the substitution history and `Continue to next? (y or
  n)` were all the title of the popup of choices, over lines the command
  line did not have yet; now the history is in the command line, the question
  the title; typed.vim types such a question. Server (minor): in the field
  editor, text typed on the first line, above its `#` lines, came back with a
  newline in the field (`| new value spread |` and the row's rest on the next
  line, the table broken); Emacs removes the `#` lines, then joins all lines
  with spaces (`org-table-finish-edit-field`). Repro: `\o`` on a field, replace
  the first line `#` by `new`, `:w`.
- Code blocks, Babel (coc, Emacs; Emacs's questions also with lsp, vim_lsp,
  ale): ok. `\obe` asks `Evaluate this emacs-lisp code block (answer) on your
  system? (y or n)` and writes `#+RESULTS:`, inline `{{{results(=42=)}}}`,
  `#+CALL:`, `\obk`, C-c C-c in a block, `\obi` (1 source block added to
  Library of Babel) and `#+CALL: double(x=5)` in another file, `\obt` with
  `:mkdirp` and `:shebang` (mode 755) and a `(concat …)` target, `\obd` after
  editing `out.el` in Vim (`Detangled 1 code blocks`), `\obs`, `\obb`, typing
  while a slow block runs (`The file changed while Emacs ran the code;
  nothing was changed. Run it again.`), `\obn`/`\obp`, `:Org block next`,
  `\obg` (an inputlist of names), `\o'` on a block (`.el`, unindented, written
  back indented), a footnote, a LaTeX fragment (`.tex`), an export block
  (`.html`), `#+INCLUDE:` opens the file.
- Export (coc, Emacs): ok but one. The menu with Emacs's backends; C-b, C-s,
  C-v toggle (`Body only: On`, `Export scope: Subtree`); `h h` writes
  work.html (`Exported to …`); `h o` opens it (stand-in `xdg-open`); C-v with
  a folded subtree leaves its text out; `t U` shows the export in a buffer;
  the subtree's export; unsaved changes are in it; `#` with `1` (default)
  inserts the options template; `&` lists the exports, last first;
  `org-export-backends` with `md` in `emacs_config` puts Markdown in the menu.
  Server (minor): the first `\oe` of a session lists no `[#] Insert template`
  and `[&] Export stack`; from the second on they are there (Emacs's
  dispatcher always has them). Repro: start Vim on an Org file, `\oe`.
- Citations (coc, Emacs): ok. `[cite:@k` completes keys with their entries,
  hover shows the entry, `\oo` opens the .bib at the entry, an unknown key is
  information with `(suggestions: knuth1984)`, `\oic` picks a key (inputlist
  of entries), on the citation's style part asks the style, `\oiC` on a key
  deletes it; `:Org bibtex create` asks type, title, author, journal, year,
  id and writes the headline; copy (register), export (`Successfully
  exported 1 BibTeX entries`), yank, import (`Parsed 1 entries`).
- Diagnostics (coc): ok. Misplaced heading, link to a missing file and the
  block left open are hints, the unknown header argument information;
  `org_lint_ignore ["link-to-local-file"]` silences that one.
- Indentation (coc): ok. `#+STARTUP: indent` virtual indentation with the
  file unchanged, `hidestars`, wrapped lines as documented (item text's
  indentation, no virtual prefix), `org_adapt_indentation`, `=`, format aligns
  tables and tags, `#+STARTUP: num` and `:Org num mode` (Emacs's messages).
- Notes and logging (coc): ok. `\ona` note, `LOG_INTO_DRAWER`,
  `org_log_reschedule` time (`- Rescheduled from "[2026-10-26 Mon]" on …`)
  and note (a note buffer), `:Org todo yesterday` (`CLOSED: [2026-10-24 Sat
  23:59]`). Agenda `z`: scenario.
- Completion and editing helpers (coc, Emacs): ok. TODO at a headline's start,
  `#+STARTUP:` values, drawer names, entities; `\oib` structure templates
  (`s`), `\oie` (`*` at a line's start gives `** word` as Emacs does),
  `\oiD` asks the drawer.
- Timers (coc, Emacs): ok. Start (`Timer start time set to …`), `\ox.`,
  `\ox-`, pause/continue/stop with their messages, the status line counting,
  paused staying, gone when stopped; countdown asks and ends with
  `Heading: time out`; with an effort it starts without asking.
- Attachments (coc, Emacs): ok. `\oxa c` copies into `data/93/…`, ID and
  `:ATTACH:`, `File "file.txt" is now an attachment`; `[[attachment:file.txt]]`
  opens it.
- Encryption: not checked. gpg-agent cannot start here (the socket would be in
  GNUPGHOME, whose path in the scratch directory is too long, and there is no
  /run/user), so no key could be made; the user's own ~/.gnupg was left alone.
- Inline images and LaTeX previews: not in Vim.

## The other clients (lsp, vim_lsp, ale), for what differs by client

- Windows: capture to a file not loaded, refile to another file not loaded,
  `t` in the agenda on an entry of a file not loaded: all four write the right
  file; coc.nvim opens those files in a tab of its own (documented). The
  calendar is as high as its text with all four.
- Prompts: the plugin's own (popup keys, input(), inputlist()), the same with
  all four; Emacs's questions (Babel's yes/no) shown and answered with all
  four. The typed scenarios type all of them with each client.
- Messages: a command's message with all four (coc.nvim: after the fix).
  **Server bug**: the output of a `shell:` link (`hi`) still goes out as
  `window/showMessage` though the plugin asks for `message_notifications`:
  coc.nvim shows `[coc.nvim] hi`, yegappan/lsp `Info: Lsp(org-lsp) hi`,
  vim-lsp and ALE nothing. `server.rs` `run_shell_link` builds a
  `ShowMessage` notification itself instead of going through `show_message`
  (which picks `org-lsp/message`). Repro: `[[shell:echo hi]]`, `\oo`, `y`.
- Inlay hints: see Setup and Tables.
- download.sh: 16 ok (4 clients × 4).

## Server-side issues (not fixed here)

1. `\oi.` twice makes `<…Sun--<…Sun>>` (Dates and times).
2. `#+STARTUP: shrink` lost after a didChange that changes nothing
   (Tables; `.claude/e2e/repro/vim-typed/shrink_didchange.py`).
3. `shell:` link output sent as `window/showMessage` (above).
4. Minor: property name completion in a drawer (Properties), `man:` links
   linted as fuzzy with `ol-man` (Links), the field editor with text above
   its `#` lines (Table formulas), the first export menu without `#`/`&`
   (Export).

## Not checked, and why

- Encryption (3 items): no gpg-agent possible here.
- `man:` links: no man pages in this container; org-ctags: no Universal
  Ctags (`ctags` is Emacs's).
- `#+PLOT` with gnuplot: gnuplot not installed (the ASCII bars of `\opa`
  checked).
- org-protocol: the system handler (`scripts/install.sh protocol`) and the
  bookmarklets in a browser (no browser); `org-lsp protocol` by hand was.
- vim-plug itself (download.sh checks the download and `:OrgLspDownload`).
- `<S-CR>`: tmux cannot type it (documented).
- Colors as seen: checked through text property types and highlight groups,
  not by eye.
