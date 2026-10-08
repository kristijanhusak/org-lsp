# Testing org-lsp in Helix by hand

<!-- Written by scripts/generate-docs.mjs from scripts/features.mjs.
     Ticked boxes are kept when it is written again. -->

What to try in Helix to know that everything org-lsp does works
there. The keys are the defaults. Tick a box once it works; a check that
fails is a bug to report. The [README](README.md) says what is left out
here and why.

## Setup

- [ ] `editors/helix/languages.toml` added to `~/.config/helix/languages.toml`, `org-lsp` on PATH: `:lsp-restart` starts it
- [ ] Code actions (`Space a`) list the Org commands where they apply
- [ ] Inlay hints show with `display-inlay-hints = true`

## Highlighting

**Not in Helix:** Helix does not use semantic tokens: it colors Org with its own tree-sitter grammar.

## Folding and visibility

**Not in Helix:** Helix has no folding.

## Headings and structure

**Partly:** New headings (`M-RET` and its relatives), moving between headings and `org-goto` are left out: they carry the cursor for the editor to move, which Helix's code actions cannot do; nor are the commands run by Emacs (`C-c *`, `C-c ;`, cloning) among its code actions.

- [ ] Promoting and demoting a heading; its tags stay aligned: the code action *Promote the heading; outdent the item; move the table column left*, the code action *Demote the heading; indent the item, the drawer or the block; move the table column right*
- [ ] Promoting and demoting a subtree: the code action *Promote the subtree; outdent the item with its children; delete the table column*, the code action *Demote the subtree; indent the item with its children; insert a table column*
- [ ] Moving a subtree up and down past its siblings: the code action *Move the subtree, the item or the table row up*, the code action *Move the subtree, the item or the table row down*
- [ ] Cutting a subtree and pasting it under another heading: its level fits there: the code action *Cut the subtree*, the code action *Paste the subtree*
- [ ] Copying a subtree and pasting it: the code action *Copy the subtree*, the code action *Paste the subtree*
- [ ] Sorting the children of a heading alphabetically, then by TODO state: the code action *Sort the entries, the items or the table rows*
- [ ] The outline lists the headings nested: `Space s`

## Lists and checkboxes

**Partly:** `RET` stays Helix's own, new items, cycled bullets and lines turned into items are left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and those are not among them.

- [ ] Indenting and outdenting an item; and with its children: the code action *Demote the heading; indent the item, the drawer or the block; move the table column right*, the code action *Promote the heading; outdent the item; move the table column left*, the code action *Demote the subtree; indent the item with its children; insert a table column*, the code action *Promote the subtree; outdent the item with its children; delete the table column*
- [ ] Moving an item up and down: the code action *Move the subtree, the item or the table row up*, the code action *Move the subtree, the item or the table row down*
- [ ] Toggling a checkbox: its parent box and the `[1/3]` cookie follow: the code action *Toggle the checkbox*
- [ ] On a list numbered 1, 3, 2, the same command renumbers and repairs it: the code action *Toggle the checkbox*
- [ ] Sorting a list's items alphabetically: the code action *Sort the entries, the items or the table rows*

## TODO keywords

**Partly:** A note for a state change (`@`) cannot be typed: Helix cannot ask for text on a server's behalf.

- [ ] Changing the TODO state of a headline (with `org_todo_keywords` keys, picked by key): the code action *Change the TODO state*
- [ ] Done with `org_log_done = "time"` writes `CLOSED: [...]` below the headline: the code action *Change the TODO state*
- [ ] Done on a repeating entry (`<2026-01-01 Thu +1w>`) moves its date a week on and keeps it TODO: the code action *Change the TODO state*
- [ ] The parent's `[1/2]` cookie is recounted: the code action *Change the TODO state*
- [ ] A `#+TODO: A B | C` line in the file is used instead of the settings: the code action *Change the TODO state*

## Priorities

- [ ] Raising and lowering the priority of a headline: the code action *Raise the priority*, the code action *Lower the priority*
- [ ] `#+PRIORITIES: 1 5 3` makes the priorities numbers: the code action *Raise the priority*

## Tags

**Partly:** Setting tags is left out: it needs keys pressed in turn, or typed tags, and Helix cannot ask for text on a server's behalf. Completion and renaming work.

- [ ] Tags are completed after `:` at the end of a headline
- [ ] Renaming a tag renames it in every headline and `#+FILETAGS:`: `Space r` on the tag

## Properties and column view

**Partly:** Setting a property or the effort is left out: they are typed, and Helix cannot ask for text on a server's behalf.

- [ ] On a property line with `NAME_ALL`, cycling through its values: the code action *Move the date a day later, or cycle the TODO state or the list's bullets forward*, the code action *Move the date a day earlier, or cycle the TODO state or the list's bullets backward*
- [ ] Deleting a property from the entry, and from the whole file: the code action *Delete a property of the entry*, the code action *Delete a property from every entry of the file*
- [ ] Toggling `ORDERED`: the code action *Toggle whether subtasks must be done in order (ORDERED)*
- [ ] Property names are completed in a drawer

## Dates and times

**Partly:** Scheduling, deadlines and new timestamps are left out: they ask for a date, and Helix cannot ask for text on a server's behalf. Changing a timestamp works.

- [ ] On a timestamp, changing the day, month, year, hour and minute under the cursor; with a count: the code action *Increase the part of the timestamp at the cursor*, the code action *Decrease the part of the timestamp at the cursor*
- [ ] Moving a timestamp by days: the code action *Move the date one day later*, the code action *Move the date one day earlier*
- [ ] A repeater (`+1w`) and a warning (`-2d`) are kept as the date changes

## Agenda

**Not in Helix:** Helix cannot open a document on a server's request. `org-lsp agenda FILE...` prints the agenda in a terminal.

## Clocking

**Partly:** No clock in the status line: Helix has no place there for a server's text.

- [ ] Clocking in: `:LOGBOOK:` gets a `CLOCK:` line and the clock shows (`[0:01] (Task)`): the code action *Clock in*
- [ ] Clocking out writes the end and the duration: the code action *Clock out*
- [ ] Cancelling the clock takes its line out: the code action *Cancel the clock*
- [ ] Going to the clocked entry from another file: the code action *Go to the clocked entry*
- [ ] Clocking in the last entry again: the code action *Clock in the last clocked task*
- [ ] The time clocked under each headline shows next to it: the code action *Show the time clocked under each headline*
- [ ] A clock table: inserted, then updated; shifting its `:block` on the `#+BEGIN:` line: the code action *Insert or update a clock table*, the code action *Update the dynamic block*
- [ ] A clock left running when the editor closed is offered for resolving on the next start: the code action *Resolve a dangling clock*
- [ ] In the agenda, clocking in and out; the clocked line is highlighted from edge to edge: the code action *Clock in*, the code action *Clock out*

## Capture

**Not in Helix:** Helix cannot open the capture buffer, and Helix cannot ask for text on a server's behalf.

## Refile and archive

**Partly:** Refiling is left out: it asks for a target from a list, which Helix cannot show for a server. Archiving works as code actions.

- [ ] Archiving a subtree: it goes to `FILE_archive` with its `ARCHIVE_…` properties: the code action *Archive the subtree*
- [ ] Archiving to the archive sibling: the code action *Archive the subtree under its Archive sibling*
- [ ] Toggling the `ARCHIVE` tag: the code action *Toggle the ARCHIVE tag*
- [ ] Archiving every done subtree after confirming each: the code action *Archive the subtrees without open TODO items*

## Links

**Partly:** Storing and inserting links are left out: they ask for text, and Helix cannot ask for text on a server's behalf; moving between links needs a key. Following works through go to definition.

- [ ] Following a link to a headline, a custom ID and a `<<target>>`: the code action *Open the link or thing at the cursor*
- [ ] Following a `file:` link with a search (`file:notes.org::*Heading`) and an `id:` link: the code action *Open the link or thing at the cursor*
- [ ] Go to definition on a link does the same: `gd`
- [ ] Creating an ID for the entry: the code action *Create an ID for the entry*
- [ ] In `[[`, file names, headlines and custom IDs are completed
- [ ] Hovering a link shows where it leads
- [ ] References on a headline list the links to it, in every file
- [ ] Renaming a headline updates the links that search for it
- [ ] Workspace symbols find headlines of every file

## Footnotes

**Partly:** Making footnotes and their menu are left out: they are not among Helix's code actions. Following them works.

- [ ] Following `[fn:1]` goes to its definition, and from the definition back to the reference: the code action *Open the link or thing at the cursor*
- [ ] A reference without a definition is reported

## Tables

**Partly:** Moving between fields with `TAB`, copying down, summing, field information and creating a table are left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and those are not among them. The other edits are code actions in a table.

- [ ] Formatting the file aligns the tables: `:format`
- [ ] Moving a column left and right, a row up and down: the code action *Promote the heading; outdent the item; move the table column left*, the code action *Demote the heading; indent the item, the drawer or the block; move the table column right*, the code action *Move the subtree, the item or the table row up*, the code action *Move the subtree, the item or the table row down*
- [ ] Inserting and deleting a column and a row: the code action *Demote the subtree; indent the item with its children; insert a table column*, the code action *Promote the subtree; outdent the item with its children; delete the table column*, the code action *Insert a table row above*, the code action *Delete the table row*
- [ ] A rule below the row; a rule and a move to the row below: the code action *Insert a rule below the table row*, the code action *Insert a rule below the table row and go below it*
- [ ] Blanking the field: the code action *Blank the table field*
- [ ] Sorting the rows by the cursor's column: the code action *Sort the entries, the items or the table rows*
- [ ] The `#+TBLFM:` line follows a moved column

## Table formulas

- [ ] `#+TBLFM: $3=$1*$2` fills the third column: the code action *Recalculate the table*
- [ ] Recalculating the cursor's line only: the code action *Recalculate the table's current line*
- [ ] `C-c C-c` on the `#+TBLFM:` line applies it: the code action *Toggle the checkbox*
- [ ] A range sum `@>$2=vsum(@2..@-1)` and a format `;%.2f`
- [ ] Date arithmetic (`$2=$1+7`) on timestamps
- [ ] Iterating until the table is stable: the code action *Recalculate the table until it does not change*
- [ ] An Emacs Lisp formula `$3='(+ $1 $2);N` and a remote reference `remote(name,@1$1)` are computed by Emacs (needs Emacs): the code action *Recalculate the table*

## Code blocks (Babel)

**Partly:** Only tangling and the block at the cursor are offered (code actions in a block): the file's and the subtree's evaluation, removing results and moving between blocks need keys, which Helix cannot give a server.

- [ ] Tangling writes each block to its `:tangle` file, `:mkdirp` and `:shebang` applied: the code action *Tangle the file's code blocks*
- [ ] A `:tangle (concat "out" ".el")` block is tangled by Emacs to `out.el` (needs Emacs): the code action *Tangle the file's code blocks*
- [ ] Evaluating an `emacs-lisp` block asks first (yes or no), then puts `#+RESULTS:` below it (needs Emacs): the code action *Evaluate the code block or call at the cursor through Emacs*
- [ ] `src_emacs-lisp{(* 6 7)}` gets `{{{results(=42=)}}}`; a `#+CALL:` line its results: the code action *Evaluate the code block or call at the cursor through Emacs*
- [ ] Typing in the file while a slow block runs: the results are dropped with a warning

## Export and publishing

**Not in Helix:** Not offered as a code action yet.

## Citations

**Partly:** Inserting citations is not offered as a code action yet. Completion, hover and following work.

- [ ] Keys are completed after `[cite:@`
- [ ] Hovering a key shows its entry; following it opens the bibliography there
- [ ] An unknown key is reported, with close keys to put instead

## Diagnostics (org-lint)

- [ ] A heading in a paragraph (`text ** heading`), a link to nowhere and a block left open are reported
- [ ] An unknown header argument (`:foo 1`) on a source block is reported, as information
- [ ] `org_lint_ignore = ["link-to-local-file"]` silences that checker

## Indentation and formatting

**Partly:** Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed.

- [ ] A file with `#+STARTUP: indent` shows its text indented by level, the file unchanged
- [ ] Re-indenting a selection indents it as Org does: `=`
- [ ] Formatting the file aligns tables and puts tags in their column

## Notes and logging

**Not in Helix:** A note is typed text: Helix cannot ask for text on a server's behalf.

## Completion and editing helpers

**Partly:** Structure templates, emphasis and drawers are not among Helix's code actions.

- [ ] At the start of a headline, TODO keywords are completed
- [ ] `#+` keywords, and the values after `#+STARTUP:`
- [ ] Block names after `#+begin_`, drawer names after `:`
- [ ] Entities after `\` (`\alp` → `\alpha`)

## Timers

**Not in Helix:** Timers are left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and they are not among them.
