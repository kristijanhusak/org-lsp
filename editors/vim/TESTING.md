# Testing org-lsp in Vim by hand

<!-- Written by scripts/generate-docs.mjs from scripts/features.mjs.
     Ticked boxes are kept when it is written again. -->

What to try in Vim to know that everything org-lsp does works
there. The keys are the defaults. Tick a box once it works; a check that
fails is a bug to report. The [README](README.md) says what is left out
here and why.

## Setup

- [ ] Installed with one client (yegappan/lsp, vim-lsp, coc.nvim or ALE): the server starts on an Org file. Go through the list once with each client you use
- [ ] `g?` in an Org file and in the agenda lists the keys; `<Leader>oa` works in any buffer
- [ ] Options from `~/.config/org-lsp/config.toml` apply, and `g:org_lsp`'s `settings` go on top

## Highlighting

**Partly:** Links, emphasis markers and pretty entities are not concealed: Vim cannot hide text that has text properties.

- [ ] Headlines are colored by level; TODO and DONE keywords, priorities (`[#A]`) and tags have colors of their own
- [ ] `*bold*`, `/italic/`, `_underline_`, `+strike+`, `=verbatim=` and `~code~` are colored
- [ ] Timestamps, `SCHEDULED:`/`DEADLINE:`, `CLOCK:` lines, drawers, `#+` keywords and `#` comments are colored
- [ ] A `#+begin_src python` block shows its code in Python's colors
- [ ] Inline code (`src_python{x = 1}`) is in Python's colors
- [ ] Table rules and bars, footnotes (`[fn:1]`), `<<targets>>`, macros (`{{{x}}}`) and entities (`\alpha`) are colored
- [ ] A subtree tagged `:ARCHIVE:` is shaded
- [ ] Another color scheme keeps the colors (they follow its groups)

## Folding and visibility

- [ ] On a headline, cycling goes folded → children → subtree: `<Tab>`
- [ ] Cycling the whole file goes overview → contents → everything: `<S-Tab>`
- [ ] A file with `#+STARTUP: overview` opens folded; with `content` its headlines show, those without children folded, and its drawers folded
- [ ] A subtree with `:VISIBILITY: children` opens with its children folded, one with `all` open
- [ ] Two blank lines after a subtree stay visible below it when it is folded
- [ ] Narrowing to the subtree opens it in a file of its own; saving that file writes it back into the Org file and saves it: `<Leader>ons`
- [ ] Opening the parent of an `:ARCHIVE:` subtree leaves that subtree folded
- [ ] A sparse tree of the TODO entries folds everything else and highlights them: `<Leader>o/`

## Headings and structure

- [ ] On a headline, a new heading of the same level goes below: `<Leader><CR>`
- [ ] A new TODO heading: `<Leader>oiT`
- [ ] A new heading after the subtree, and a TODO one: `<Leader>oih`, `<Leader>oit`
- [ ] Promoting and demoting a heading; its tags stay aligned: `<<`, `>>`
- [ ] Promoting and demoting a subtree: `<s`, `>s`
- [ ] Moving a subtree up and down past its siblings: `<Leader>oK`, `<Leader>oJ`
- [ ] Cutting a subtree and pasting it under another heading: its level fits there: `<Leader>osd`, `<Leader>osp`
- [ ] Copying a subtree and pasting it: `<Leader>osy`, `<Leader>osp`
- [ ] Sorting the children of a heading alphabetically, then by TODO state: `<Leader>o^`
- [ ] Moving to the next and previous heading; with a count, that many: `}`, `{`
- [ ] Moving to the next and previous heading of the same level, stopping at the parent's end: `]]`, `[[`
- [ ] Moving to the parent heading; on a top heading Org says it is at the top level: `g{`
- [ ] Going to a heading picked by its outline path (`A/B/`): `<Leader>oj`
- [ ] Off a table, turning a line into a heading and back (needs Emacs): `:Org table recalculate line`
- [ ] Toggling `COMMENT` on the heading (needs Emacs): `<Leader>o;`
- [ ] Cloning the subtree twice a week apart (`+1w`): the copies follow it with their dates moved (needs Emacs): `<Leader>osc`
- [ ] On a selection of whole lines: two headlines demoted together; lines turned into headings (needs Emacs): `>>`, `:Org table recalculate line`
- [ ] The outline lists the headings nested: the client's document symbols

## Lists and checkboxes

- [ ] On an item, the next item goes below; in a numbered list the numbers go on, after a checkbox item the new one has `[ ]`: `<Leader><CR>`
- [ ] `RET` in the middle of an item's line keeps the rest of the line in the item: `<CR>`
- [ ] The next item from the end of an item's line: `<S-CR>`
- [ ] Indenting and outdenting an item; and with its children: `>>`, `<<`, `>s`, `<s`
- [ ] Moving an item up and down: `<Leader>oK`, `<Leader>oJ`
- [ ] Cycling the bullet `-` → `+` → `1.` → `1)` on an item: `<S-Right>`, `<S-Left>`
- [ ] Toggling a checkbox: its parent box and the `[1/3]` cookie follow: `<C-Space>`
- [ ] On a list numbered 1, 3, 2, the same command renumbers and repairs it: `<C-Space>`
- [ ] Sorting a list's items alphabetically: `<Leader>o^`
- [ ] Off a table, turning lines into items and back (needs Emacs): `<Leader>o-`

## TODO keywords

- [ ] With `org_enforce_todo_dependencies`, a parent with a child not done cannot be done: the change is refused with `blocked (by "TODO Child")`; with `org_enforce_todo_checkbox_dependencies`, an entry with an unchecked box cannot either: `cit`
- [ ] Those tasks are dimmed in the agenda and the TODO list
- [ ] Changing the TODO state of a headline (with `org_todo_keywords` keys, picked by key): `cit`
- [ ] Cycling to the next and previous state: `:Org todo next`, `ciT`
- [ ] Repeating the change with `.`
- [ ] Done with `org_log_done = "time"` writes `CLOSED: [...]` below the headline: `cit`
- [ ] A state with `@` (`WAIT(w@)`) asks for a note, written below the headline: `cit`
- [ ] Done on a repeating entry (`<2026-01-01 Thu +1w>`) moves its date a week on and keeps it TODO: `cit`
- [ ] The parent's `[1/2]` cookie is recounted: `cit`
- [ ] A `#+TODO: A B | C` line in the file is used instead of the settings: `cit`

## Priorities

- [ ] Raising and lowering the priority of a headline: `ciR`, `cir`
- [ ] Setting it by key; `SPC` removes it: `<Leader>o,`
- [ ] `#+PRIORITIES: 1 5 3` makes the priorities numbers: `ciR`

## Tags

- [ ] Setting tags from `#+TAGS: work(w) home(h)` by key; each key turns its tag on and off: `<Leader>ot`
- [ ] An exclusive group (`{ @office(o) @home(m) }`) turns the other tag off: `<Leader>ot`
- [ ] Typing tags when none have keys; they are sorted and aligned to `org_tags_column`: `<Leader>ot`
- [ ] Tags are completed after `:` at the end of a headline
- [ ] Renaming a tag renames it in every headline and `#+FILETAGS:`: the client's rename on the tag

## Properties and column view

**Partly:** Column view is left out: it draws over the headlines, which Vim cannot.

- [ ] Setting a property: the name and the value are completed, `NAME_ALL`'s values offered: `<Leader>oxp`
- [ ] On a property line with `NAME_ALL`, cycling through its values: `<S-Right>`, `<S-Left>`
- [ ] Deleting a property from the entry, and from the whole file: `:Org property delete`, `:Org property delete globally`
- [ ] `C-c C-c` in a property drawer asks what to do with the property: `<C-Space>`
- [ ] Setting the effort: `<Leader>oxe`
- [ ] Toggling `ORDERED`: `:Org ordered toggle`
- [ ] Property names are completed in a drawer

## Dates and times

**Partly:** Dates are typed, without the calendar: the Vim plugin asks with an input line.

- [ ] Scheduling with `+2d`: `SCHEDULED: <…>` goes below the headline: `<Leader>ois`
- [ ] A deadline on `fri 10:00`: `<Leader>oid`
- [ ] Removing the scheduled date and the deadline: `:Org schedule remove`, `:Org deadline remove`
- [ ] Delaying the scheduled date to a day (`-3d`) and setting the deadline's warning period: `:Org schedule delay`, `:Org deadline warning`
- [ ] Inserting an active and an inactive timestamp; a second one right after makes a range: `<Leader>oi.`, `<Leader>oi!`
- [ ] On a timestamp, changing the day, month, year, hour and minute under the cursor; with a count: `<C-a>`, `<C-x>`
- [ ] Moving a timestamp by days: `<S-Up>`, `<S-Down>`
- [ ] Changing the timestamp at the cursor with the date prompt: `cid`
- [ ] A repeater (`+1w`) and a warning (`-2d`) are kept as the date changes

## Agenda

- [ ] The agenda menu lists the views and the custom commands: `<Leader>oa`
- [ ] The week view: scheduled entries, deadlines with their warnings, the time grid: `:Org agenda list`
- [ ] Later, earlier, today; day, week, month views; going to a date: `f`, `b`, `.`, `vd`, `vw`, `vm`, `J`
- [ ] Tags at the right edge of the window, following it as it is resized
- [ ] The TODO list, a tag search (`work+urgent`), a word search: `<Leader>oa`
- [ ] Going to the line's entry in another window, and where the agenda is: `<Tab>`, `<CR>`
- [ ] Changing the line's TODO state and priority; the line follows: `t`, `+` or `<S-Up>`
- [ ] Scheduling the line's entry and setting its deadline: `<C-c><C-s>` or `<Leader>ois`, `<C-c><C-d>` or `<Leader>oid`
- [ ] Setting the line's tags, adding a note, refiling, archiving: `<Leader>ot`, `z` or `<Leader>ona`, `<C-c><C-w>` or `<Leader>or`, `<C-c><C-x><C-a>`
- [ ] Filters by tag, category and effort, and removing them: `\`, `<`, `_`, `|`
- [ ] Marking lines and a bulk action on them: `m`, `B`
- [ ] Follow mode shows the cursor's entry as the cursor moves: `F`
- [ ] Entry text, log mode, the clock report, archives mode: `E` or `vE`, `vl`, `R` or `vR`, `va`
- [ ] Hovering a line shows its entry: `K`
- [ ] Rebuilding the agenda, saving the changed files, quitting: `r`, `s` or `<C-x><C-s>`
- [ ] An entry changed in its file shows in the agenda when it is rebuilt
- [ ] Adding today's appointments: an entry timed a few minutes from now is reminded of every three minutes until it is due (needs Emacs): `:Org agenda appt`

## Clocking

- [ ] Clocking into an entry whose effort its time passes shows `Task ‘…’ should be finished by now. (0:30)`, once: `<Leader>oxi`
- [ ] Clocking in: `:LOGBOOK:` gets a `CLOCK:` line and the clock shows (`[0:01] (Task)`): `<Leader>oxi`
- [ ] Clocking out writes the end and the duration: `<Leader>oxo`
- [ ] Cancelling the clock takes its line out: `<Leader>oxq`
- [ ] Going to the clocked entry from another file: `<Leader>oxj`
- [ ] Clocking in the last entry again: `<Leader>oxx`
- [ ] The time clocked under each headline shows next to it: `<Leader>oxd`
- [ ] A clock table: inserted, then updated; shifting its `:block` on the `#+BEGIN:` line: `<Leader>oxr`, `<Leader>oxu`
- [ ] A clock left running when the editor closed is offered for resolving on the next start: `<Leader>oxR`
- [ ] In the agenda, clocking in and out; the clocked line is highlighted from edge to edge: `I` or `<Leader>oxi`, `O` or `<Leader>oxo`
- [ ] The clock in the statusline (`org_lsp#clock_status()`)

## Capture

- [ ] The menu lists the templates by key: `<Leader>oc`
- [ ] An entry template opens the capture buffer with the cursor at `%?`: `<Leader>oc`
- [ ] A `%^{Name}` prompt asks for text: `<Leader>oc`
- [ ] Finishing puts the entry under its target: `<C-c>`
- [ ] Aborting leaves the target unchanged: `<Leader>ok`
- [ ] Refiling from the capture buffer: `<Leader>or`
- [ ] A date tree target files under year, month and day: `<Leader>oc`
- [ ] `%a` links back to where the capture started: `<Leader>oc`
- [ ] `%c` puts in what was copied last (in Neovim and Vim, the register the key was typed with): `<Leader>oc`
- [ ] `%[snippet.txt]` puts in the file's text, relative to the file captured from: `<Leader>oc`
- [ ] `%(upcase "abc")` puts in `ABC`, what Emacs makes of it (needs Emacs): `<Leader>oc`

## Refile and archive

- [ ] Copying an entry to a target keeps it where it was; going to a target and to the entry refiled last moves the cursor there: `<Leader>oy`, `:Org refile goto`, `:Org refile goto last`
- [ ] With `org_log_refile` `"time"`, the refiled entry gets `Refiled on […]`; with `"note"` a note buffer opens for it: `<Leader>or`
- [ ] Refiling a subtree under a heading of the same file, and of another file: `<Leader>or`
- [ ] Archiving a subtree: it goes to `FILE_archive` with its `ARCHIVE_…` properties: `<Leader>o$`
- [ ] Archiving to the archive sibling: `<Leader>oxA`
- [ ] Toggling the `ARCHIVE` tag: `<Leader>oA`
- [ ] Archiving every done subtree after confirming each: `:Org archive done`
- [ ] In the agenda: archiving the line's entry; archives mode shows archived entries: `<C-c><C-x><C-a>`, `va`

## Links

- [ ] A `#+LINK: gh https://github.com/%s` line makes `[[gh:org-lsp]]` follow `https://github.com/org-lsp`; `org_link_abbrev_alist` does the same from the settings: `<Leader>oo`
- [ ] Following a link to a headline, a custom ID and a `<<target>>`: `<Leader>oo`
- [ ] Following a `file:` link with a search (`file:notes.org::*Heading`) and an `id:` link: `<Leader>oo`
- [ ] Following a URL opens the browser; a timestamp opens its day's agenda: `<Leader>oo`
- [ ] Go to definition on a link does the same: the client's go to definition
- [ ] Storing a link to a headline and inserting it elsewhere with a description: `<Leader>ols`, `<Leader>oli`
- [ ] Inserting a link on an existing link edits it: `<Leader>oli`
- [ ] Creating an ID for the entry: `:Org id create`
- [ ] Moving to the next and previous link; after the last, again from the start of the file: `]l`, `[l`
- [ ] In `[[`, file names, headlines and custom IDs are completed
- [ ] Hovering a link shows where it leads
- [ ] References on a headline list the links to it, in every file
- [ ] Renaming a headline updates the links that search for it
- [ ] Workspace symbols find headlines of every file

## Footnotes

- [ ] Following `[fn:1]` goes to its definition, and from the definition back to the reference: `<Leader>oo`
- [ ] A reference without a definition is reported
- [ ] A new footnote (needs Emacs): `[fn:1]` at the cursor, its definition in the `Footnotes` section, the cursor there: `<Leader>of`
- [ ] On a reference without a definition, Emacs asks whether to make one, in the editor: `<Leader>of`
- [ ] The footnote menu: sorting, renumbering `fn:N`, normalizing, deleting: `<Leader>oF`

## Tables

**Partly:** A shrunk column shows its whole text and `…`: Vim cannot hide text that has text properties.

- [ ] `C-c TAB` in a column with a `<5>` cookie shows five characters of each field and `…`; again, the whole column; a file with `#+STARTUP: shrink` opens so: `<Leader>o<Tab>`
- [ ] Formatting the file aligns the tables: the client's format command
- [ ] Moving to the next and previous field; past the last field a row is made: `:Org table next field`, `:Org table previous field`
- [ ] Moving a column left and right, a row up and down: `<<`, `>>`, `<Leader>oK`, `<Leader>oJ`
- [ ] Inserting and deleting a column and a row: `>s`, `<s`, `<Leader>oTO`, `<Leader>oTd`
- [ ] A rule below the row; a rule and a move to the row below: `<Leader>o-`, `<Leader>o<CR>`
- [ ] Blanking the field: `<Leader>o<Space>`
- [ ] The shifted cursor keys move the field up, down, left and right (needs Emacs); on a timestamp in a field they change it: `:Org shift up`, `:Org shift down`, `<S-Left>`, `<S-Right>`
- [ ] Copying the field down to the next row, a number incremented (needs Emacs); off a table the key keeps its own meaning: `<Leader>oD`
- [ ] The sum of the column and the field's references are shown as messages (needs Emacs): `<Leader>o+`, `<Leader>o?`
- [ ] Creating a table asks for its size (`3x2`; empty for Org's default) (needs Emacs): `<Leader>oi<Bar>`
- [ ] Sorting the rows by the cursor's column: `<Leader>o^`
- [ ] The `#+TBLFM:` line follows a moved column

## Table formulas

- [ ] `#+TBLFM: $3=$1*$2` fills the third column: `<Leader>o*`
- [ ] Recalculating the cursor's line only: `:Org table recalculate line`
- [ ] `C-c C-c` on the `#+TBLFM:` line applies it: `<C-Space>`
- [ ] A range sum `@>$2=vsum(@2..@-1)` and a format `;%.2f`
- [ ] Date arithmetic (`$2=$1+7`) on timestamps
- [ ] Iterating until the table is stable: `:Org table iterate`
- [ ] An Emacs Lisp formula `$3='(+ $1 $2);N` and a remote reference `remote(name,@1$1)` are computed by Emacs (needs Emacs): `<Leader>o*`
- [ ] Editing the formulas in a file of their own: saving it writes them to the `#+TBLFM:` line (needs Emacs): `<Leader>o'`
- [ ] Editing a field in a file of its own: saving it puts the text back on one line and realigns the table (needs Emacs): `<Leader>o``

## Code blocks (Babel)

- [ ] Adding the file's named blocks to the Library of Babel; a `#+CALL:` in another file runs one (needs Emacs): `<Leader>obi`
- [ ] Tangling writes each block to its `:tangle` file, `:mkdirp` and `:shebang` applied: `<Leader>obt`
- [ ] A `:tangle (concat "out" ".el")` block is tangled by Emacs to `out.el` (needs Emacs): `<Leader>obt`
- [ ] Evaluating an `emacs-lisp` block asks first (yes or no), then puts `#+RESULTS:` below it (needs Emacs): `<Leader>obe`
- [ ] `C-c C-c` in the block does the same: `<C-Space>`
- [ ] `src_emacs-lisp{(* 6 7)}` gets `{{{results(=42=)}}}`; a `#+CALL:` line its results: `<Leader>obe`
- [ ] Evaluating the file's and the subtree's blocks: `<Leader>obb`, `<Leader>obs`
- [ ] Removing a block's results: `<Leader>obk`
- [ ] Moving to the next and previous code block, and to any block (needs Emacs): `<Leader>obn`, `<Leader>obp`, `:Org block next`, `:Org block previous`
- [ ] Going to a named code block, picked from the names (needs Emacs): `<Leader>obg`
- [ ] Typing in the file while a slow block runs: the results are dropped with a warning
- [ ] Editing a code block in a file of its own: the file has the language's extension, its lines unindented and `,*` unescaped; saving it writes the block back and saves the Org file; the same block again shows the same file (needs Emacs): `<Leader>o'`
- [ ] The same at a footnote reference (its definition, the label kept), a LaTeX fragment and an export block; at `#+INCLUDE:` the file included is shown (needs Emacs): `<Leader>o'`

## Export and publishing

- [ ] `#` in the export menu inserts the options template of the backend picked (`default` for all): `<Leader>oe`
- [ ] The menu shows Emacs's backends, `C-b`, `C-s` and `C-f` toggle the options (needs Emacs): `<Leader>oe`
- [ ] To an HTML file: the file is written next to the Org file and announced: `<Leader>oe`
- [ ] To an HTML file and opened: the browser opens it: `<Leader>oe`
- [ ] The subtree at the cursor as UTF-8 text in a buffer: a file of it is shown: `<Leader>oe`
- [ ] Unsaved changes are in the export: `<Leader>oe`
- [ ] `&` in the export menu lists what the exports made, the last first; picking one shows it again: `<Leader>oe`
- [ ] `emacs_config` adds Markdown (`org-export-backends`): it is in the menu after a restart

## Citations

- [ ] Keys are completed after `[cite:@`
- [ ] Hovering a key shows its entry; following it opens the bibliography there
- [ ] An unknown key is reported, with close keys to put instead
- [ ] Inserting a citation, picking keys; on a citation, adding a key; with the prefix, deleting one: `<Leader>oic`, `<Leader>oiC`

## Diagnostics (org-lint)

- [ ] A heading in a paragraph (`text ** heading`), a link to nowhere and a block left open are reported
- [ ] An unknown header argument (`:foo 1`) on a source block is reported, as information
- [ ] `org_lint_ignore = ["link-to-local-file"]` silences that checker

## Indentation and formatting

**Partly:** New lines are not indented by the server (no `indentexpr`), and wrapped lines are not.

- [ ] A file with `#+STARTUP: indent` shows its text indented by level, the file unchanged
- [ ] With `#+STARTUP: hidestars` only the last star shows
- [ ] Formatting the file aligns tables and puts tags in their column

## Notes and logging

- [ ] Adding a note: a note buffer opens, and finishing writes it in the entry's log: `<Leader>ona`
- [ ] With `LOG_INTO_DRAWER`, the note goes in `:LOGBOOK:`: `<Leader>ona`
- [ ] Adding a note from the agenda: `z` or `<Leader>ona`
- [ ] With `org_log_reschedule` `"time"` (or `#+STARTUP: logreschedule`), rescheduling logs `Rescheduled from "[…]" on […]`; with `"note"` a note buffer opens for it: `<Leader>ois`

## Completion and editing helpers

- [ ] At the start of a headline, TODO keywords are completed
- [ ] `#+` keywords, and the values after `#+STARTUP:`
- [ ] Block names after `#+begin_`, drawer names after `:`
- [ ] Entities after `\` (`\alp` → `\alpha`)
- [ ] A block from the structure templates (`s` for a source block) (needs Emacs): `<Leader>oib`
- [ ] Emphasis markers around the cursor, `*` for bold (needs Emacs): `<Leader>oie`
- [ ] A drawer, its name completed from the file's (needs Emacs): `<Leader>oiD`

## Timers

- [ ] Starting the timer says when it started (needs Emacs): `<Leader>ox0`
- [ ] Inserting the timer's value (`0:00:12 `) and a timer item (`- 0:00:15 :: `): `<Leader>ox.`, `<Leader>ox-`
- [ ] Pausing and continuing; stopping: `<Leader>ox,`, `<Leader>ox_`
- [ ] A countdown asks for its length (`1` for a minute); when it ends, `Heading: time out` is shown: `<Leader>ox;`
- [ ] On an entry with an effort the countdown takes it, without asking: `<Leader>ox;`
- [ ] While the timer or a countdown is on, the status line shows its value, counting each second; paused, it stays; stopped, it goes

## Attachments

- [ ] Attaching a file by copying it: the entry gets an ID and the `ATTACH` tag, the file goes in its directory (needs Emacs): `<Leader>oxa`
- [ ] An `attachment:` link opens the attached file (needs Emacs): `<Leader>oo`

## Encryption

- [ ] With `org-crypt-key` set in the Emacs configuration, encrypting an entry replaces its text with a PGP message; decrypting brings it back (needs Emacs and GnuPG): `:Org crypt encrypt`, `:Org crypt decrypt`
- [ ] Encrypting and decrypting every entry tagged `crypt` (needs Emacs and GnuPG): `:Org crypt encrypt all`, `:Org crypt decrypt all`
- [ ] With `(org-crypt-use-before-save-magic)` in the Emacs configuration, saving a file encrypts its entries tagged `crypt` (needs Emacs and GnuPG)
