# Testing org-lsp in Sublime Text by hand

<!-- Written by scripts/generate-docs.mjs from scripts/features.mjs.
     Ticked boxes are kept when it is written again. -->

What to try in Sublime Text to know that everything org-lsp does works
there. The keys are the defaults. Tick a box once it works; a check that
fails is a bug to report. The [README](README.md) says what is left out
here and why.

## Setup

- [ ] Installed with `scripts/install-sublime.sh`: LSP comes with it, and the server is found or downloaded
- [ ] LSP's `semantic_highlighting` is on
- [ ] The `Org:` commands are in the Command Palette; *Org: Download the latest server* replaces the downloaded server

## Highlighting

**Partly:** Links and emphasis markers are not concealed: Sublime Text cannot hide text in a view. The colors need LSP's `semantic_highlighting` on.

- [ ] Headlines are colored by level; TODO and DONE keywords, priorities (`[#A]`) and tags have colors of their own
- [ ] `*bold*`, `/italic/`, `_underline_`, `+strike+`, `=verbatim=` and `~code~` are colored
- [ ] Timestamps, `SCHEDULED:`/`DEADLINE:`, `CLOCK:` lines, drawers, `#+` keywords and `#` comments are colored
- [ ] A `#+begin_src python` block shows its code in Python's colors
- [ ] Inline code (`src_python{x = 1}`) is in Python's colors
- [ ] Table rules and bars, footnotes (`[fn:1]`), `<<targets>>`, macros (`{{{x}}}`) and entities (`\alpha`) are colored
- [ ] A subtree tagged `:ARCHIVE:` is shaded

## Folding and visibility

- [ ] On a headline, cycling goes folded → children → subtree: `Tab`
- [ ] Cycling the whole file goes overview → contents → everything: `Shift+Tab`
- [ ] A file with `#+STARTUP: overview` opens folded
- [ ] Two blank lines after a subtree stay visible below it when it is folded
- [ ] Opening the parent of an `:ARCHIVE:` subtree leaves that subtree folded
- [ ] A sparse tree of the TODO entries folds everything else and highlights them: `Ctrl+C /`

## Headings and structure

- [ ] On a headline, a new heading of the same level goes below: `Alt+Enter`
- [ ] A new TODO heading: `Alt+Shift+Enter`
- [ ] A new heading after the subtree, and a TODO one: `Ctrl+Enter`, `Ctrl+Shift+Enter`
- [ ] Promoting and demoting a heading; its tags stay aligned: `Alt+Left`, `Alt+Right`
- [ ] Promoting and demoting a subtree: `Alt+Shift+Left`, `Alt+Shift+Right`
- [ ] Moving a subtree up and down past its siblings: `Alt+Up`, `Alt+Down`
- [ ] Cutting a subtree and pasting it under another heading: its level fits there: `Ctrl+C Ctrl+X Ctrl+W`, `Ctrl+C Ctrl+X Ctrl+Y`
- [ ] Copying a subtree and pasting it: `Ctrl+C Ctrl+X Alt+W`, `Ctrl+C Ctrl+X Ctrl+Y`
- [ ] Sorting the children of a heading alphabetically, then by TODO state: `Ctrl+C ^`
- [ ] Moving to the next and previous heading; with a count, that many: `Ctrl+C Ctrl+N`, `Ctrl+C Ctrl+P`
- [ ] Moving to the next and previous heading of the same level, stopping at the parent's end: `Ctrl+C Ctrl+F`, `Ctrl+C Ctrl+B`
- [ ] Moving to the parent heading; on a top heading Org says it is at the top level: `Ctrl+C Ctrl+U`
- [ ] Going to a heading picked by its outline path (`A/B/`): `Ctrl+C Ctrl+J`
- [ ] The outline lists the headings nested: *Goto Symbol*

## Lists and checkboxes

- [ ] On an item, the next item goes below; in a numbered list the numbers go on, after a checkbox item the new one has `[ ]`: `Alt+Enter`
- [ ] `RET` in the middle of an item's line keeps the rest of the line in the item: `Enter`
- [ ] Indenting and outdenting an item; and with its children: `Alt+Right`, `Alt+Left`, `Alt+Shift+Right`, `Alt+Shift+Left`
- [ ] Moving an item up and down: `Alt+Up`, `Alt+Down`
- [ ] Cycling the bullet `-` → `+` → `1.` → `1)` on an item: `Shift+Right`, `Shift+Left`
- [ ] Toggling a checkbox: its parent box and the `[1/3]` cookie follow: `Ctrl+C Ctrl+C`
- [ ] On a list numbered 1, 3, 2, the same command renumbers and repairs it: `Ctrl+C Ctrl+C`
- [ ] Sorting a list's items alphabetically: `Ctrl+C ^`

## TODO keywords

- [ ] Changing the TODO state of a headline (with `org_todo_keywords` keys, picked by key): `Ctrl+C Ctrl+T`
- [ ] Cycling to the next and previous state: *Org: Cycle TODO state forward*, *Org: Cycle TODO state backward*
- [ ] Done with `org_log_done = "time"` writes `CLOSED: [...]` below the headline: `Ctrl+C Ctrl+T`
- [ ] A state with `@` (`WAIT(w@)`) asks for a note, written below the headline: `Ctrl+C Ctrl+T`
- [ ] Done on a repeating entry (`<2026-01-01 Thu +1w>`) moves its date a week on and keeps it TODO: `Ctrl+C Ctrl+T`
- [ ] The parent's `[1/2]` cookie is recounted: `Ctrl+C Ctrl+T`
- [ ] A `#+TODO: A B | C` line in the file is used instead of the settings: `Ctrl+C Ctrl+T`

## Priorities

- [ ] Raising and lowering the priority of a headline: *Org: Raise the priority*, *Org: Lower the priority*
- [ ] Setting it by key; `SPC` removes it: `Ctrl+C ,`
- [ ] `#+PRIORITIES: 1 5 3` makes the priorities numbers: *Org: Raise the priority*

## Tags

- [ ] Setting tags from `#+TAGS: work(w) home(h)` by key; each key turns its tag on and off: `Ctrl+C Ctrl+Q`
- [ ] An exclusive group (`{ @office(o) @home(m) }`) turns the other tag off: `Ctrl+C Ctrl+Q`
- [ ] Typing tags when none have keys; they are sorted and aligned to `org_tags_column`: `Ctrl+C Ctrl+Q`
- [ ] Tags are completed after `:` at the end of a headline
- [ ] Renaming a tag renames it in every headline and `#+FILETAGS:`: *LSP: Rename* on the tag

## Properties and column view

- [ ] Setting a property: the name and the value are completed, `NAME_ALL`'s values offered: `Ctrl+C Ctrl+X p`
- [ ] On a property line with `NAME_ALL`, cycling through its values: `Shift+Right`, `Shift+Left`
- [ ] Deleting a property from the entry, and from the whole file: *Org: Delete a property of the entry*, *Org: Delete a property from every entry of the file*
- [ ] `C-c C-c` in a property drawer asks what to do with the property: `Ctrl+C Ctrl+C`
- [ ] Setting the effort: `Ctrl+C Ctrl+X e`
- [ ] Toggling `ORDERED`: `Ctrl+C Ctrl+X o`
- [ ] Property names are completed in a drawer

## Dates and times

**Partly:** Dates are typed, without the calendar: the package asks with an input panel.

- [ ] Scheduling with `+2d`: `SCHEDULED: <…>` goes below the headline: `Ctrl+C Ctrl+S`
- [ ] A deadline on `fri 10:00`: `Ctrl+C Ctrl+D`
- [ ] Inserting an active and an inactive timestamp; a second one right after makes a range: `Ctrl+C .`, `Ctrl+C !`
- [ ] On a timestamp, changing the day, month, year, hour and minute under the cursor; with a count: *Org: Increase the part of the timestamp at the cursor*, *Org: Decrease the part of the timestamp at the cursor*
- [ ] Moving a timestamp by days: *Org: Move the date one day later*, *Org: Move the date one day earlier*
- [ ] Changing the timestamp at the cursor with the date prompt: *Org: Change the timestamp at the cursor*
- [ ] A repeater (`+1w`) and a warning (`-2d`) are kept as the date changes

## Agenda

- [ ] The agenda menu lists the views and the custom commands: `Ctrl+C a`
- [ ] The week view: scheduled entries, deadlines with their warnings, the time grid: *Org: Agenda for the current week or day*
- [ ] Later, earlier, today; day, week, month views; going to a date: `f`, `b`, `.`, `v d`, `v w`, `v m`, `J`
- [ ] Tags at the right edge of the window, following it as it is resized
- [ ] The TODO list, a tag search (`work+urgent`), a word search: `Ctrl+C a`
- [ ] Going to the line's entry in another window, and where the agenda is: `Tab`, `Enter`
- [ ] Changing the line's TODO state and priority; the line follows: `t`, `+` or `Shift+Up`
- [ ] Scheduling the line's entry and setting its deadline: `Ctrl+C Ctrl+S`, `Ctrl+C Ctrl+D`
- [ ] Setting the line's tags, adding a note, refiling, archiving: *Org: Set the tags of the headline*, `z`, `Ctrl+C Ctrl+W`, `Ctrl+C Ctrl+X Ctrl+A`
- [ ] Filters by tag, category and effort, and removing them: `\`, `<`, `_`, `|`
- [ ] Marking lines and a bulk action on them: `m`, `B`
- [ ] Follow mode shows the cursor's entry as the cursor moves: `F`
- [ ] Entry text, log mode, the clock report, archives mode: `E` or `v E`, `v l`, `R` or `v R`, `v a`
- [ ] Hovering a line shows its entry: the mouse
- [ ] Rebuilding the agenda, saving the changed files, quitting: `r`, `s` or `Ctrl+X Ctrl+S`
- [ ] An entry changed in its file shows in the agenda when it is rebuilt

## Clocking

- [ ] Clocking in: `:LOGBOOK:` gets a `CLOCK:` line and the clock shows (`[0:01] (Task)`): `Ctrl+C Ctrl+X Ctrl+I`
- [ ] Clocking out writes the end and the duration: `Ctrl+C Ctrl+X Ctrl+O`
- [ ] Cancelling the clock takes its line out: `Ctrl+C Ctrl+X Ctrl+Q`
- [ ] Going to the clocked entry from another file: *Org: Go to the clocked entry*
- [ ] Clocking in the last entry again: `Ctrl+C Ctrl+X Ctrl+X`
- [ ] The time clocked under each headline shows next to it: `Ctrl+C Ctrl+X Ctrl+D`
- [ ] A clock table: inserted, then updated; shifting its `:block` on the `#+BEGIN:` line: `Ctrl+C Ctrl+X Ctrl+R`, `Ctrl+C Ctrl+X Ctrl+U`
- [ ] A clock left running when the editor closed is offered for resolving on the next start: *Org: Resolve a dangling clock*
- [ ] In the agenda, clocking in and out; the clocked line is highlighted from edge to edge: `I`, `O`
- [ ] The clock in the status bar

## Capture

- [ ] The menu lists the templates by key: `Ctrl+C c`
- [ ] An entry template opens the capture buffer with the cursor at `%?`: `Ctrl+C c`
- [ ] A `%^{Name}` prompt asks for text: `Ctrl+C c`
- [ ] Finishing puts the entry under its target: `Ctrl+C Ctrl+C`
- [ ] Aborting leaves the target unchanged: `Ctrl+C Ctrl+K`
- [ ] Refiling from the capture buffer: `Ctrl+C Ctrl+W`
- [ ] A date tree target files under year, month and day: `Ctrl+C c`
- [ ] `%a` links back to where the capture started: `Ctrl+C c`

## Refile and archive

- [ ] Refiling a subtree under a heading of the same file, and of another file: `Ctrl+C Ctrl+W`
- [ ] Archiving a subtree: it goes to `FILE_archive` with its `ARCHIVE_…` properties: `Ctrl+C Ctrl+X Ctrl+S` or `Ctrl+C $`
- [ ] Archiving to the archive sibling: `Ctrl+C Ctrl+X A`
- [ ] Toggling the `ARCHIVE` tag: `Ctrl+C Ctrl+X a`
- [ ] Archiving every done subtree after confirming each: *Org: Archive the subtrees without open TODO items*
- [ ] In the agenda: archiving the line's entry; archives mode shows archived entries: `Ctrl+C Ctrl+X Ctrl+A`, `v a`

## Links

- [ ] Following a link to a headline, a custom ID and a `<<target>>`: `Ctrl+C Ctrl+O`
- [ ] Following a `file:` link with a search (`file:notes.org::*Heading`) and an `id:` link: `Ctrl+C Ctrl+O`
- [ ] Following a URL opens the browser; a timestamp opens its day's agenda: `Ctrl+C Ctrl+O`
- [ ] Go to definition on a link does the same: *LSP: Goto Definition*
- [ ] Storing a link to a headline and inserting it elsewhere with a description: `Ctrl+C l`, `Ctrl+C Ctrl+L`
- [ ] Inserting a link on an existing link edits it: `Ctrl+C Ctrl+L`
- [ ] Creating an ID for the entry: *Org: Create an ID for the entry*
- [ ] Moving to the next and previous link; after the last, again from the start of the file: `Ctrl+C Ctrl+X Ctrl+N`, `Ctrl+C Ctrl+X Ctrl+P`
- [ ] In `[[`, file names, headlines and custom IDs are completed
- [ ] Hovering a link shows where it leads
- [ ] References on a headline list the links to it, in every file
- [ ] Renaming a headline updates the links that search for it
- [ ] Workspace symbols find headlines of every file

## Footnotes

- [ ] Following `[fn:1]` goes to its definition, and from the definition back to the reference: `Ctrl+C Ctrl+O`
- [ ] A reference without a definition is reported

## Tables

- [ ] Formatting the file aligns the tables: *LSP: Format File*
- [ ] Moving to the next and previous field; past the last field a row is made: *Org: Go to the next table field, making a row after the last*, *Org: Go to the previous table field*
- [ ] Moving a column left and right, a row up and down: `Alt+Left`, `Alt+Right`, `Alt+Up`, `Alt+Down`
- [ ] Inserting and deleting a column and a row: `Alt+Shift+Right`, `Alt+Shift+Left`, `Alt+Shift+Down`, `Alt+Shift+Up`
- [ ] A rule below the row; a rule and a move to the row below: `Ctrl+C -`, `Ctrl+C Enter`
- [ ] Blanking the field: *Org: Blank the table field*
- [ ] Sorting the rows by the cursor's column: `Ctrl+C ^`
- [ ] The `#+TBLFM:` line follows a moved column

## Table formulas

- [ ] `#+TBLFM: $3=$1*$2` fills the third column: *Org: Recalculate the table*
- [ ] Recalculating the cursor's line only: `Ctrl+C *`
- [ ] `C-c C-c` on the `#+TBLFM:` line applies it: `Ctrl+C Ctrl+C`
- [ ] A range sum `@>$2=vsum(@2..@-1)` and a format `;%.2f`
- [ ] Date arithmetic (`$2=$1+7`) on timestamps
- [ ] Iterating until the table is stable: *Org: Recalculate the table until it does not change*

## Code blocks (Babel)

- [ ] Tangling writes each block to its `:tangle` file, `:mkdirp` and `:shebang` applied: `Ctrl+C Ctrl+V t` or `Ctrl+C Ctrl+V Ctrl+T`
- [ ] Evaluating an `emacs-lisp` block asks first (yes or no), then puts `#+RESULTS:` below it (needs Emacs): `Ctrl+C Ctrl+V e` or `Ctrl+C Ctrl+V Ctrl+E`
- [ ] `C-c C-c` in the block does the same: `Ctrl+C Ctrl+C`
- [ ] `src_emacs-lisp{(* 6 7)}` gets `{{{results(=42=)}}}`; a `#+CALL:` line its results: `Ctrl+C Ctrl+V e` or `Ctrl+C Ctrl+V Ctrl+E`
- [ ] Evaluating the file's and the subtree's blocks: `Ctrl+C Ctrl+V b` or `Ctrl+C Ctrl+V Ctrl+B`, `Ctrl+C Ctrl+V s` or `Ctrl+C Ctrl+V Ctrl+S`
- [ ] Removing a block's results: `Ctrl+C Ctrl+V k`
- [ ] Typing in the file while a slow block runs: the results are dropped with a warning

## Export and publishing

- [ ] The menu shows Emacs's backends, `C-b`, `C-s` and `C-f` toggle the options (needs Emacs): `Ctrl+C Ctrl+E`
- [ ] To an HTML file: the file is written next to the Org file and announced: `Ctrl+C Ctrl+E`
- [ ] To an HTML file and opened: the browser opens it: `Ctrl+C Ctrl+E`
- [ ] The subtree at the cursor as UTF-8 text in a buffer: a file of it is shown: `Ctrl+C Ctrl+E`
- [ ] Unsaved changes are in the export: `Ctrl+C Ctrl+E`
- [ ] `emacs_config` adds Markdown (`org-export-backends`): it is in the menu after a restart

## Citations

- [ ] Keys are completed after `[cite:@`
- [ ] Hovering a key shows its entry; following it opens the bibliography there
- [ ] An unknown key is reported, with close keys to put instead
- [ ] Inserting a citation, picking keys; on a citation, adding a key; with the prefix, deleting one: `Ctrl+C Ctrl+X @`, *Org: Delete the citation or reference at the cursor, or insert one with a style*

## Diagnostics (org-lint)

- [ ] A heading in a paragraph (`text ** heading`), a link to nowhere and a block left open are reported
- [ ] An unknown header argument (`:foo 1`) on a source block is reported, as information
- [ ] `org_lint_ignore = ["link-to-local-file"]` silences that checker

## Indentation and formatting

**Partly:** Virtual indentation shows as phantoms, the leading stars still showing; new lines are not indented as they are typed.

- [ ] A file with `#+STARTUP: indent` shows its text indented by level, the file unchanged
- [ ] Re-indenting a selection indents it as Org does: *LSP: Format Selection*
- [ ] Formatting the file aligns tables and puts tags in their column

## Notes and logging

- [ ] Adding a note: a note buffer opens, and finishing writes it in the entry's log: `Ctrl+C Ctrl+Z`
- [ ] With `LOG_INTO_DRAWER`, the note goes in `:LOGBOOK:`: `Ctrl+C Ctrl+Z`
- [ ] Adding a note from the agenda: `z`

## Completion and editing helpers

- [ ] At the start of a headline, TODO keywords are completed
- [ ] `#+` keywords, and the values after `#+STARTUP:`
- [ ] Block names after `#+begin_`, drawer names after `:`
- [ ] Entities after `\` (`\alp` → `\alpha`)
