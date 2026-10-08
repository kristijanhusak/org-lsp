# Testing org-lsp in Neovim by hand

<!-- Written by scripts/generate-docs.mjs from scripts/features.mjs.
     Ticked boxes are kept when it is written again. -->

What to try in Neovim to know that everything org-lsp does works
there. The keys are the defaults. Tick a box once it works; a check that
fails is a bug to report. The [README](README.md) says what is left out
here and why.

## Setup

- [ ] Installed with lazy.nvim (`build = ':OrgLspDownload'`), no server around: opening an Org file downloads the server and starts it
- [ ] `:checkhealth org-lsp` reports the server, where it came from and the agenda files
- [ ] `:OrgLspDownload` downloads the server again and restarts it in the open Org files
- [ ] `g?` in an Org file and in the agenda lists the keys; `<Leader>oa` works in any buffer
- [ ] Options from `~/.config/org-lsp/config.toml` apply, and `setup()`'s `settings` go on top

## Highlighting

- [ ] Headlines are colored by level; TODO and DONE keywords, priorities (`[#A]`) and tags have colors of their own
- [ ] `*bold*`, `/italic/`, `_underline_`, `+strike+`, `=verbatim=` and `~code~` are colored
- [ ] A link shows its description alone; its brackets and target show when the cursor is on its line
- [ ] With `org_hide_emphasis_markers = true`, the markers of `*bold*` are hidden
- [ ] Timestamps, `SCHEDULED:`/`DEADLINE:`, `CLOCK:` lines, drawers, `#+` keywords and `#` comments are colored
- [ ] A `#+begin_src python` block shows its code in Python's colors
- [ ] Inline code (`src_python{x = 1}`) is in Python's colors
- [ ] Table rules and bars, footnotes (`[fn:1]`), `<<targets>>`, macros (`{{{x}}}`) and entities (`\alpha`) are colored
- [ ] A subtree tagged `:ARCHIVE:` is shaded
- [ ] Another color scheme keeps the colors (they follow its groups)

## Folding and visibility

- [ ] On a headline, cycling goes folded → children → subtree: `<Tab>`
- [ ] Cycling the whole file goes overview → contents → everything: `<S-Tab>`
- [ ] A file with `#+STARTUP: overview` opens folded
- [ ] Two blank lines after a subtree stay visible below it when it is folded
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
- [ ] The outline lists the headings nested: `:lua vim.lsp.buf.document_symbol()`

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

## TODO keywords

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
- [ ] Renaming a tag renames it in every headline and `#+FILETAGS:`: `:lua vim.lsp.buf.rename()` on the tag

## Properties and column view

- [ ] Setting a property: the name and the value are completed, `NAME_ALL`'s values offered: `<Leader>oxp`
- [ ] On a property line with `NAME_ALL`, cycling through its values: `<S-Right>`, `<S-Left>`
- [ ] Deleting a property from the entry, and from the whole file: `:Org property delete`, `:Org property delete globally`
- [ ] `C-c C-c` in a property drawer asks what to do with the property: `<C-Space>`
- [ ] Setting the effort: `<Leader>oxe`
- [ ] Toggling `ORDERED`: `:Org ordered toggle`
- [ ] Property names are completed in a drawer

## Dates and times

- [ ] Scheduling with `+2d`: `SCHEDULED: <…>` goes below the headline: `<Leader>ois`
- [ ] A deadline on `fri 10:00`: `<Leader>oid`
- [ ] Inserting an active and an inactive timestamp; a second one right after makes a range: `<Leader>oi.`, `<Leader>oi!`
- [ ] While typing the date, the answer shows what it means (`=> <2026-10-09 Fri>`)
- [ ] In the calendar: moving by day, week and month, scrolling months, today, picking a day: `l` or `<S-Right>`, `j` or `<S-Down>`, `<M-S-Right>`, `>`, `.`, `<CR>`
- [ ] In the calendar: picking a time with the keys, typing the answer instead, cancelling: `t`, `i`, `q` or `<Esc>`
- [ ] Switching to a month of six weeks keeps the whole calendar in view, the cursor on the selected day
- [ ] After the date goes in, the cursor is on the same text and the window shows the same lines, with `scrolloff` set too
- [ ] `calendar_win_mode` `float` and `cursor` open the calendar in a floating window; `settings = { calendar_months = 1 }` shows one month
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
- [ ] The agenda window opens as `win_split_mode` says (`split`, `vsplit`, `float`, `current`)

## Clocking

- [ ] Clocking in: `:LOGBOOK:` gets a `CLOCK:` line and the clock shows (`[0:01] (Task)`): `<Leader>oxi`
- [ ] Clocking out writes the end and the duration: `<Leader>oxo`
- [ ] Cancelling the clock takes its line out: `<Leader>oxq`
- [ ] Going to the clocked entry from another file: `<Leader>oxj`
- [ ] Clocking in the last entry again: `<Leader>oxx`
- [ ] The time clocked under each headline shows next to it: `<Leader>oxd`
- [ ] A clock table: inserted, then updated; shifting its `:block` on the `#+BEGIN:` line: `<Leader>oxr`, `<Leader>oxu`
- [ ] A clock left running when the editor closed is offered for resolving on the next start: `<Leader>oxR`
- [ ] In the agenda, clocking in and out; the clocked line is highlighted from edge to edge: `I` or `<Leader>oxi`, `O` or `<Leader>oxo`
- [ ] The clock in the statusline (`clock_status()`)

## Capture

- [ ] The menu lists the templates by key: `<Leader>oc`
- [ ] An entry template opens the capture buffer with the cursor at `%?`: `<Leader>oc`
- [ ] A `%^{Name}` prompt asks for text: `<Leader>oc`
- [ ] Finishing puts the entry under its target: `<C-c>`
- [ ] Aborting leaves the target unchanged: `<Leader>ok`
- [ ] Refiling from the capture buffer: `<Leader>or`
- [ ] A date tree target files under year, month and day: `<Leader>oc`
- [ ] `%a` links back to where the capture started: `<Leader>oc`

## Refile and archive

- [ ] Refiling a subtree under a heading of the same file, and of another file: `<Leader>or`
- [ ] Archiving a subtree: it goes to `FILE_archive` with its `ARCHIVE_…` properties: `<Leader>o$`
- [ ] Archiving to the archive sibling: `<Leader>oxA`
- [ ] Toggling the `ARCHIVE` tag: `<Leader>oA`
- [ ] Archiving every done subtree after confirming each: `:Org archive done`
- [ ] In the agenda: archiving the line's entry; archives mode shows archived entries: `<C-c><C-x><C-a>`, `va`

## Links

- [ ] Following a link to a headline, a custom ID and a `<<target>>`: `<Leader>oo`
- [ ] Following a `file:` link with a search (`file:notes.org::*Heading`) and an `id:` link: `<Leader>oo`
- [ ] Following a URL opens the browser; a timestamp opens its day's agenda: `<Leader>oo`
- [ ] Go to definition on a link does the same: `gd` or `vim.lsp.buf.definition()`
- [ ] Storing a link to a headline and inserting it elsewhere with a description: `<Leader>ols`, `<Leader>oli`
- [ ] Inserting a link on an existing link edits it: `<Leader>oli`
- [ ] Creating an ID for the entry: `:Org id create`
- [ ] In `[[`, file names, headlines and custom IDs are completed
- [ ] Hovering a link shows where it leads
- [ ] References on a headline list the links to it, in every file
- [ ] Renaming a headline updates the links that search for it
- [ ] Workspace symbols find headlines of every file

## Footnotes

- [ ] Following `[fn:1]` goes to its definition, and from the definition back to the reference: `<Leader>oo`
- [ ] A reference without a definition is reported

## Tables

- [ ] Formatting the file aligns the tables: `vim.lsp.buf.format()`
- [ ] Moving to the next and previous field; past the last field a row is made: `:Org table next field`, `:Org table previous field`
- [ ] Moving a column left and right, a row up and down: `<<`, `>>`, `<Leader>oK`, `<Leader>oJ`
- [ ] Inserting and deleting a column and a row: `>s`, `<s`, `<Leader>oTO`, `<Leader>oTd`
- [ ] A rule below the row; a rule and a move to the row below: `<Leader>o-`, `<Leader>o<CR>`
- [ ] Blanking the field: `<Leader>o<Space>`
- [ ] Sorting the rows by the cursor's column: `<Leader>o^`
- [ ] The `#+TBLFM:` line follows a moved column

## Table formulas

- [ ] `#+TBLFM: $3=$1*$2` fills the third column: `<Leader>o*`
- [ ] Recalculating the cursor's line only: `:Org table recalculate line`
- [ ] `C-c C-c` on the `#+TBLFM:` line applies it: `<C-Space>`
- [ ] A range sum `@>$2=vsum(@2..@-1)` and a format `;%.2f`
- [ ] Date arithmetic (`$2=$1+7`) on timestamps
- [ ] Iterating until the table is stable: `:Org table iterate`

## Code blocks (Babel)

- [ ] Tangling writes each block to its `:tangle` file, `:mkdirp` and `:shebang` applied: `<Leader>obt`
- [ ] Evaluating an `emacs-lisp` block asks first (yes or no), then puts `#+RESULTS:` below it (needs Emacs): `<Leader>obe`
- [ ] `C-c C-c` in the block does the same: `<C-Space>`
- [ ] `src_emacs-lisp{(* 6 7)}` gets `{{{results(=42=)}}}`; a `#+CALL:` line its results: `<Leader>obe`
- [ ] Evaluating the file's and the subtree's blocks: `<Leader>obb`, `<Leader>obs`
- [ ] Removing a block's results: `<Leader>obk`
- [ ] Typing in the file while a slow block runs: the results are dropped with a warning

## Export and publishing

- [ ] The menu shows Emacs's backends, `C-b`, `C-s` and `C-f` toggle the options (needs Emacs): `<Leader>oe`
- [ ] To an HTML file: the file is written next to the Org file and announced: `<Leader>oe`
- [ ] To an HTML file and opened: the browser opens it: `<Leader>oe`
- [ ] The subtree at the cursor as UTF-8 text in a buffer: a file of it is shown: `<Leader>oe`
- [ ] Unsaved changes are in the export: `<Leader>oe`
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

- [ ] A file with `#+STARTUP: indent` shows its text indented by level, the file unchanged
- [ ] With `#+STARTUP: hidestars` only the last star shows
- [ ] With `org_adapt_indentation = true`, a new line below a headline is indented
- [ ] Re-indenting a selection indents it as Org does: `=`
- [ ] Formatting the file aligns tables and puts tags in their column

## Notes and logging

- [ ] Adding a note: a note buffer opens, and finishing writes it in the entry's log: `<Leader>ona`
- [ ] With `LOG_INTO_DRAWER`, the note goes in `:LOGBOOK:`: `<Leader>ona`
- [ ] Adding a note from the agenda: `z` or `<Leader>ona`

## Completion and editing helpers

- [ ] At the start of a headline, TODO keywords are completed
- [ ] `#+` keywords, and the values after `#+STARTUP:`
- [ ] Block names after `#+begin_`, drawer names after `:`
- [ ] Entities after `\` (`\alp` → `\alpha`)
