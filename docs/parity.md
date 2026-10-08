# Parity with Emacs Org

<!-- Written by scripts/generate-docs.mjs from scripts/features.mjs. -->

What Emacs Org (9.8.10) does, area by area, and what org-lsp does of it.
An area's parity is the share of its features done; all of Org is the
mean of the areas. Every feature done is checked against Emacs where Org
decides the outcome (see "How the work is checked" in PLAN.md).

**All of Org: 78%**

| Feature | Parity | Done |
| --- | --- | --- |
| [Highlighting](#highlighting) | 83% | 10 of 12 |
| [Folding and visibility](#folding-and-visibility) | 67% | 6 of 9 |
| [Headings and structure](#headings-and-structure) | 100% | 11 of 11 |
| [Lists and checkboxes](#lists-and-checkboxes) | 91% | 10 of 11 |
| [TODO keywords](#todo-keywords) | 100% | 8 of 8 |
| [Priorities](#priorities) | 100% | 3 of 3 |
| [Tags](#tags) | 100% | 6 of 6 |
| [Properties and column view](#properties-and-column-view) | 67% | 4 of 6 |
| [Dates and times](#dates-and-times) | 100% | 8 of 8 |
| [Agenda](#agenda) | 75% | 15 of 20 |
| [Clocking](#clocking) | 100% | 8 of 8 |
| [Capture](#capture) | 82% | 9 of 11 |
| [Refile and archive](#refile-and-archive) | 82% | 9 of 11 |
| [Links](#links) | 91% | 10 of 11 |
| [Footnotes](#footnotes) | 100% | 4 of 4 |
| [Tables](#tables) | 65% | 11 of 17 |
| [Table formulas](#table-formulas) | 83% | 10 of 12 |
| [Code blocks (Babel)](#code-blocks-babel) | 82% | 9 of 11 |
| [Export and publishing](#export-and-publishing) | 67% | 6 of 9 |
| [Citations](#citations) | 100% | 5 of 5 |
| [Diagnostics (org-lint)](#diagnostics-org-lint) | 97% | 59 of 61 |
| [Indentation and formatting](#indentation-and-formatting) | 83% | 5 of 6 |
| [Notes and logging](#notes-and-logging) | 100% | 4 of 4 |
| [Completion and editing helpers](#completion-and-editing-helpers) | 100% | 4 of 4 |
| [Timers](#timers) | 60% | 3 of 5 |
| [Attachments](#attachments) | 0% | 0 of 2 |
| [Encryption](#encryption) | 0% | 0 of 1 |
| [Inline images and LaTeX previews](#inline-images-and-latex-previews) | 0% | 0 of 2 |

## Highlighting

83% (10 of 12)

- ✅ Headlines by level, TODO keywords, priorities and tags
- ✅ Emphasis, verbatim and code
- ✅ Links shown as their description (`org-link-descriptive`)
- ✅ Emphasis markers hidden (`org-hide-emphasis-markers`)
- ✅ Timestamps, planning and clock lines
- ✅ Blocks, keywords, drawers, properties and comments
- ✅ Code in its language (`org-src-fontify-natively`)
- ✅ Tables
- ✅ Footnotes, targets, macros, entities and LaTeX fragments
- ✅ Archived subtrees shaded (`org-archived`)
- ❌ Radio targets turning their words into links
- ❌ Entities and sub- and superscripts shown as characters (`org-pretty-entities`)

Where editors fall short:

- Vim ❓: Links and emphasis markers are not concealed: Vim cannot hide text that has text properties.
- Sublime Text ❓: Links and emphasis markers are not concealed: Sublime Text cannot hide text in a view. The colors need LSP's `semantic_highlighting` on.
- Zed ❓: Links and emphasis markers are not concealed: Zed lets no server hide text. The colors need `"semantic_tokens": "combined"`; code blocks take their language's colors, inline code does not.
- Helix ❌: Helix does not use semantic tokens: it colors Org with its own tree-sitter grammar.

## Folding and visibility

67% (6 of 9)

- ✅ Cycling a subtree (`TAB`): folded, children, subtree
- ✅ Cycling the file (`S-TAB`): overview, contents, everything
- ✅ Startup visibility `overview` and `showeverything`
- ❌ Other startup visibilities (`content`, `show2levels`…) and the `VISIBILITY` property
- ✅ Blank lines after a subtree kept outside its fold (`org-cycle-separator-lines`)
- ✅ Archived subtrees left folded (`org-cycle-hide-archived-subtrees`)
- ✅ Sparse trees (`C-c /`)
- ❌ Narrowing to a subtree or block (`C-x n s`, `C-x n b`)
- ❌ Showing the context of the cursor (`org-reveal`, `C-c C-r`)

Where editors fall short:

- Zed ❓: Zed folds with its own folding: Org's cycling and sparse trees need folds an extension can set, which Zed does not offer.
- Helix ❌: Helix has no folding.

## Headings and structure

100% (11 of 11)

- ✅ New headings (`M-RET`, `M-S-RET`, `C-RET`, `C-S-RET`)
- ✅ Promoting and demoting a heading or a subtree
- ✅ Moving a subtree up and down
- ✅ Cutting, copying and pasting subtrees, their level fitted
- ✅ Sorting entries (`C-c ^`)
- ✅ The outline of the file (document symbols)
- ✅ Moving between headings (`C-c C-n`, `C-c C-p`, `C-c C-f`, `C-c C-b`, `C-c C-u`)
- ✅ Jumping to a heading (`org-goto`, `C-c C-j`, by outline path)
- ✅ Turning lines into headings and back (`C-c *`, through Emacs)
- ✅ Toggling `COMMENT` (`C-c ;`, through Emacs)
- ✅ Cloning a subtree with its dates shifted (`C-c C-x c`, through Emacs)

Where editors fall short:

- Zed ❓: Moving between headings, `org-goto` and the commands run by Emacs (`C-c *`, `C-c ;`, cloning) are left out: Zed cannot bind a key to a server's command (the commands are code actions), and those are not among them.
- Helix ❓: New headings (`M-RET` and its relatives), moving between headings and `org-goto` are left out: they carry the cursor for the editor to move, which Helix's code actions cannot do; nor are the commands run by Emacs (`C-c *`, `C-c ;`, cloning) among its code actions.

## Lists and checkboxes

91% (10 of 11)

- ✅ New items, numbered and with checkboxes (`M-RET`, `M-S-RET`)
- ✅ `RET` in an item (`org-return`)
- ✅ Indenting, outdenting and moving items, with their children
- ✅ Cycling bullets (`S-<left>`, `S-<right>`)
- ✅ Repairing and renumbering a list (`C-c C-c`)
- ✅ Checkboxes, with their parents and `[1/3]` or `[33%]` cookies
- ✅ Sorting items
- ✅ Moving to the next and previous item
- ✅ Turning lines into items and back (`C-c -`, through Emacs)
- ❌ Radio lists
- ✅ Circular motion between items (`org-list-use-circular-motion`)

Where editors fall short:

- Zed ❓: `RET` stays Zed's own, bullets are not cycled and lines not turned into items: Zed cannot bind a key to a server's command (the commands are code actions), and those are not among them.
- Helix ❓: `RET` stays Helix's own, new items, cycled bullets and lines turned into items are left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and those are not among them.

## TODO keywords

100% (8 of 8)

- ✅ Cycling states, and fast selection by key (`TODO(t)`)
- ✅ Sequences and types from `#+TODO:` and `org_todo_keywords`
- ✅ Logging: `CLOSED:`, the time or a note on a state change (`!`, `@`), `LOG_INTO_DRAWER`
- ✅ Repeating entries moving to their next date
- ✅ The parent's statistics cookies recounted
- ✅ Blocking a state change by children, `ORDERED` siblings and checkboxes (`org-enforce-todo-dependencies`)
- ✅ Dimming blocked tasks in the agenda (`org-agenda-dim-blocked-tasks`)
- ✅ Hiding blocked tasks in the agenda (`org-agenda-dim-blocked-tasks` `invisible`)

Where editors fall short:

- Zed ❓: A note for a state change (`@`) cannot be typed: Zed cannot ask for text on a server's behalf. States are picked with buttons.
- Helix ❓: A note for a state change (`@`) cannot be typed: Helix cannot ask for text on a server's behalf.

## Priorities

100% (3 of 3)

- ✅ Raising, lowering, setting and removing
- ✅ `#+PRIORITIES:` and number priorities
- ✅ From the agenda

## Tags

100% (6 of 6)

- ✅ Setting tags: fast selection by key, exclusive groups, typed
- ✅ Aligning tags (`org_tags_column`)
- ✅ Inheritance and `#+FILETAGS:`
- ✅ Completion
- ✅ Group tags in searches
- ✅ A sparse tree by tags

Where editors fall short:

- Zed ❓: Setting tags is left out: it needs keys pressed in turn, or typed tags, and Zed cannot ask for text on a server's behalf. Completion and renaming work.
- Helix ❓: Setting tags is left out: it needs keys pressed in turn, or typed tags, and Helix cannot ask for text on a server's behalf. Completion and renaming work.

## Properties and column view

67% (4 of 6)

- ✅ Setting and deleting properties, also from the agenda
- ✅ Allowed values (`NAME_ALL`) cycled with `S-<left>` and `S-<right>`
- ✅ Effort (`C-c C-x e`) and `ORDERED`
- ✅ Inheritance, special properties and property searches
- ❌ Column view (`C-c C-x C-c`)
- ❌ Column view dynamic blocks and summaries

Where editors fall short:

- Zed ❓: Setting a property or the effort is left out: they are typed, and Zed cannot ask for text on a server's behalf.
- Helix ❓: Setting a property or the effort is left out: they are typed, and Helix cannot ask for text on a server's behalf.

## Dates and times

100% (8 of 8)

- ✅ Inserting active and inactive timestamps (`C-c .`, `C-c !`)
- ✅ Scheduling and deadlines (`C-c C-s`, `C-c C-d`)
- ✅ Org's date prompt: `+2d`, `fri`, `2/15`, `10:00-11:30`…
- ✅ The calendar with the prompt
- ✅ Changing the part at the cursor (`S-<up>`, `S-<down>`), with ranges, repeaters and warnings
- ✅ Moving a date by days (`S-<left>`, `S-<right>`)
- ✅ Removing a scheduled date or a deadline (`C-u C-c C-s`, `C-u C-c C-d`)
- ✅ Setting a delay or a warning period (`C-u C-u C-c C-s`, `C-u C-u C-c C-d`)

Where editors fall short:

- Vim ❓: Dates are typed, without the calendar: the Vim plugin asks with an input line.
- Sublime Text ❓: Dates are typed, without the calendar: the package asks with an input panel.
- Zed ❓: Scheduling, deadlines and new timestamps are left out: they ask for a date, and Zed cannot ask for text on a server's behalf. Changing a timestamp works.
- Helix ❓: Scheduling, deadlines and new timestamps are left out: they ask for a date, and Helix cannot ask for text on a server's behalf. Changing a timestamp works.

## Agenda

75% (15 of 20)

- ✅ Day, week, fortnight, month and year views, going to a date
- ✅ The list of TODO entries
- ✅ Tag, property and group tag searches
- ✅ Searches for words, phrases and regexps
- ✅ Stuck projects
- ✅ Custom commands and block agendas (`org-agenda-custom-commands`)
- ✅ Commands offered by context (`org-agenda-custom-commands-contexts`)
- ✅ Filters by tag, category, effort, regexp and top headline
- ✅ Bulk marks and actions
- ✅ Follow mode and entry text
- ✅ Log mode and the clock report
- ✅ Archives mode
- ✅ Changing an entry from the agenda: state, priority, tags, dates, notes, refile, archive, clock
- ✅ Lines formatted as Org formats them (`org-agenda-prefix-format` and the rest)
- ✅ The time grid
- ❌ Habits (`org-habit`)
- ❌ Diary entries and sexps (`%%(diary-…)`)
- ❌ Column view in the agenda
- ❌ Writing the agenda to a file (`C-x C-w`)
- ❌ Appointment reminders (`org-agenda-to-appt`)

Where editors fall short:

- VS Code ❓: Tags are at column 80, not at the window's edge (`org_agenda_tags_column` `"auto"`): VS Code does not tell an extension how many columns an editor shows.
- Zed ❓: The agenda is a file, its commands code actions (or keys through Zed's tasks); searches, going to a date and dates asked for are left out (Zed cannot ask for text on a server's behalf), and follow mode (Zed reports no cursor moves to an extension).
- Helix ❌: Helix cannot open a document on a server's request. `org-lsp agenda FILE...` prints the agenda in a terminal.

## Clocking

100% (8 of 8)

- ✅ Clocking in and out, cancelling, going to the clocked entry
- ✅ The clock history and clocking in the last entry
- ✅ The clock in the mode line
- ✅ The time clocked under each headline (`C-c C-x C-d`)
- ✅ Clock tables, with every parameter
- ✅ Resolving dangling clocks and idle time
- ✅ Clocking from the agenda, the clocked entry highlighted
- ✅ A notice when the clock passes the effort (`org-clock-notify-once-if-expired`)

Where editors fall short:

- Zed ❓: No clock in the status line: Zed has no place there for an extension's text.
- Helix ❓: No clock in the status line: Helix has no place there for a server's text.

## Capture

82% (9 of 11)

- ✅ Templates of every type: entries, items, checkboxes, table lines, plain text
- ✅ Targets: files, headlines, outline paths, regexps, date trees, IDs, the clock
- ✅ Prompts and `%`-escapes (`%^{…}`, `%t`, `%a`, `%i`, `%?`…)
- ✅ The capture buffer: finishing, aborting, refiling
- ✅ Clock options (`:clock-in`, `:clock-resume`…)
- ❌ `function` targets
- ✅ `%(…)` (Emacs Lisp, through Emacs)
- ✅ `%[file]`
- ✅ `%c`, `%x` and `%^C` (the editor's clipboard)
- ❌ `%:keyword` (the properties of a stored link)
- ✅ Template contexts (`org-capture-templates-contexts`)

Where editors fall short:

- Zed ❓: Templates with prompts are left out, and there is no capture buffer to type in: Zed cannot ask for text on a server's behalf.
- Helix ❌: Helix cannot open the capture buffer, and Helix cannot ask for text on a server's behalf.

## Refile and archive

82% (9 of 11)

- ✅ Refiling to a heading, between files too (`org-refile-targets`)
- ✅ Archiving a subtree to its archive file
- ✅ Archiving to the archive sibling
- ✅ Toggling the `ARCHIVE` tag
- ✅ Archive locations, `ARCHIVE` properties, date trees, `ARCHIVE_…` context
- ✅ Archiving all done subtrees
- ✅ Logging refiles (`org-log-refile`)
- ✅ Creating parent nodes as the target (`org-refile-allow-creating-parent-nodes`)
- ❌ Refiling a region
- ✅ Copying (`C-c M-w`), going to a target (`C-u C-c C-w`) and to the last refiled entry (`C-u C-u C-c C-w`)
- ❌ `org-refile-target-verify-function`

Where editors fall short:

- Helix ❓: Refiling is left out: it asks for a target from a list, which Helix cannot show for a server. Archiving works as code actions.

## Links

91% (10 of 11)

- ✅ Following links: headlines, targets, custom IDs, files with searches, IDs, URLs, timestamps, tags
- ✅ Storing and inserting links, with descriptions, editing the link at the cursor
- ✅ IDs created (`org-id-get-create`)
- ✅ Completion of file names, headlines and custom IDs in links
- ✅ Backlinks, hovers and clickable links (beyond Emacs: references, document links)
- ✅ Link abbreviations (`#+LINK:`, `org-link-abbrev-alist`)
- ✅ Link abbreviations that call a function (`%(…)`), through Emacs, when the function is safe
- ❌ A region as the search of a stored link
- ✅ Completing `id:` links to entries, which get an ID (`org-id-complete`; Org completes no `help:` links)
- ✅ Default descriptions from the link type: an `id:` link's headline, an `info:` link's command
- ✅ Moving to the next and previous link (`C-c C-x C-n`, `C-c C-x C-p`)

Where editors fall short:

- Zed ❓: Storing and inserting links are left out: they ask for text, and Zed cannot ask for text on a server's behalf; moving between links needs a key. Following works through go to definition, for links inside Org files and to files.
- Helix ❓: Storing and inserting links are left out: they ask for text, and Helix cannot ask for text on a server's behalf; moving between links needs a key. Following works through go to definition.

## Footnotes

100% (4 of 4)

- ✅ Following a reference to its definition and back
- ✅ Highlighting and checking (`org-lint`)
- ✅ Creating a footnote (`C-c C-x f`, through Emacs)
- ✅ Renumbering, sorting, normalizing and deleting footnotes (through Emacs)

Where editors fall short:

- Zed ❓: Making footnotes and their menu are left out: they are not among Zed's code actions. Following them works.
- Helix ❓: Making footnotes and their menu are left out: they are not among Helix's code actions. Following them works.

## Tables

65% (11 of 17)

- ✅ Aligning (`C-c C-c`, formatting)
- ✅ Moving between fields (`TAB`, `S-TAB`), a row made past the last
- ✅ Moving, inserting and deleting rows and columns
- ✅ Rules (`C-c -`, `C-c RET`)
- ✅ Blanking a field
- ✅ Sorting rows
- ✅ The `#+TBLFM:` line following moved columns and rows
- ❌ Rectangles: cutting, copying and pasting
- ✅ Copying down (`S-RET`, through Emacs)
- ❌ Editing a field in its own buffer (`C-c \``)
- ❌ Shrunk columns (`<5>`)
- ✅ Creating an empty table (`C-c |`, through Emacs)
- ❌ Making a table from a region (`C-c |` on a region)
- ✅ Summing (`C-c +`) and field information (`C-c ?`), through Emacs
- ✅ Moving the field with the shifted cursor keys (through Emacs)
- ❌ Importing and exporting tables
- ❌ Plotting (`org-plot`)

Where editors fall short:

- Zed ❓: Moving between fields with `TAB`, copying down, summing, field information and creating a table are left out: Zed cannot bind a key to a server's command (the commands are code actions), and those are not among them. The other edits are code actions in a table.
- Helix ❓: Moving between fields with `TAB`, copying down, summing, field information and creating a table are left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and those are not among them. The other edits are code actions in a table.

## Table formulas

83% (10 of 12)

- ✅ Column, field and range formulas
- ✅ References by row, column, hline and name
- ✅ Parameters and constants
- ✅ Calc's arithmetic, vectors, dates and durations
- ✅ Modes and formats (`;%.2f`, `;N`, `;T`)
- ✅ Recalculating the line, the table, until stable
- ✅ Formulas in Emacs Lisp (through Emacs)
- ✅ Remote references (through Emacs)
- ✅ Units (through Emacs)
- ✅ Calc's fractions, symbolic mode and engineering notation (through Emacs)
- ❌ The formula editor (`C-c '`)
- ❌ The formula debugger

## Code blocks (Babel)

82% (9 of 11)

- ✅ Tangling, with Noweb references, comments and header arguments from properties
- ✅ Evaluating a block, a `#+CALL:` line and inline code (through Emacs)
- ✅ Evaluating the file's and the subtree's blocks
- ✅ Removing results
- ✅ Sessions
- ✅ Asking before evaluating (`org-confirm-babel-evaluate`)
- ❌ Editing a block in its own buffer (`C-c '`)
- ✅ Tangling's Emacs Lisp (evaluated header arguments, `<<block()>>`; through Emacs)
- ❌ Detangling
- ✅ Moving between blocks (`C-c C-v n`, `C-c C-v p`, `C-c C-v g`, `C-c M-f`, `C-c M-b`; through Emacs)
- ✅ The Library of Babel (`C-c C-v i`, through Emacs)

Where editors fall short:

- Zed ❓: Only tangling and the block at the cursor are offered (code actions in a block): the file's and the subtree's evaluation, removing results and moving between blocks need keys, which Zed cannot give a server.
- Helix ❓: Only tangling and the block at the cursor are offered (code actions in a block): the file's and the subtree's evaluation, removing results and moving between blocks need keys, which Helix cannot give a server.

## Export and publishing

67% (6 of 9)

- ✅ The export menu, with the backends Emacs has (through Emacs)
- ✅ Body only and the subtree at the cursor
- ✅ To a buffer, to a file, and opened
- ✅ Publishing a file, a project, all projects
- ✅ Forcing publishing
- ❌ Exporting in the background (`C-a`)
- ❌ Only the visible part (`C-v`)
- ❌ The export stack (`&`)
- ✅ Inserting a template (`#`, through Emacs)

Where editors fall short:

- Zed ❌: Not offered as a code action yet; its menu would come as buttons.
- Helix ❌: Not offered as a code action yet.

## Citations

100% (5 of 5)

- ✅ Bibliographies from `#+BIBLIOGRAPHY:` and `org_cite_global_bibliography` (BibTeX, CSL-JSON)
- ✅ Keys completed
- ✅ A key's entry on hover, following a key
- ✅ Unknown keys reported
- ✅ Inserting, editing and deleting citations (`C-c C-x @`)

Where editors fall short:

- Zed ❓: Inserting citations is not offered as a code action yet. Completion, hover and following work.
- Helix ❓: Inserting citations is not offered as a code action yet. Completion, hover and following work.

## Diagnostics (org-lint)

97% (59 of 61)

- ❓ org-lint's checkers (59 of 61)

## Indentation and formatting

83% (5 of 6)

- ✅ Virtual indentation (`org-indent-mode`, `#+STARTUP: indent`)
- ✅ Hidden leading stars (`org-hide-leading-stars`)
- ✅ Hard indentation (`org-adapt-indentation`)
- ✅ Indenting lines and regions (`org-indent-line`, `org-indent-region`)
- ✅ Aligning tables and tags
- ❌ Lines a long line wraps into indented (`wrap-prefix`)

Where editors fall short:

- Vim ❓: New lines are not indented by the server (no `indentexpr`), and wrapped lines are not.
- Sublime Text ❓: Virtual indentation shows as phantoms, the leading stars still showing; new lines are not indented as they are typed.
- Zed ❓: Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed.
- Helix ❓: Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed.

## Notes and logging

100% (4 of 4)

- ✅ Notes (`org-add-note`), in the log or the `LOGBOOK` drawer
- ✅ Notes and times on state changes
- ✅ `LOG_INTO_DRAWER` and the `LOGGING` property
- ✅ Logging rescheduled dates and deadlines (`org-log-reschedule`, `org-log-redeadline`)

Where editors fall short:

- Zed ❌: A note is typed text: Zed cannot ask for text on a server's behalf.
- Helix ❌: A note is typed text: Helix cannot ask for text on a server's behalf.

## Completion and editing helpers

100% (4 of 4)

- ✅ Completion: TODO keywords, tags, `#+` keywords and `#+STARTUP:` values, blocks, drawers, properties, entities, link targets
- ✅ Structure templates (`C-c C-,`, through Emacs)
- ✅ Emphasis (`C-c C-x C-f`, through Emacs)
- ✅ Inserting a drawer (`C-c C-x d`, through Emacs)

Where editors fall short:

- Zed ❓: Structure templates, emphasis and drawers are not among Zed's code actions.
- Helix ❓: Structure templates, emphasis and drawers are not among Helix's code actions.

## Timers

60% (3 of 5)

- ✅ The relative timer: starting, inserting its value, pausing and stopping (through Emacs)
- ✅ Timer items in lists (`C-c C-x -`, through Emacs)
- ✅ Countdowns (`C-c C-x ;`), the effort taken when there is one (through Emacs)
- ❌ The timer in the mode line
- ❌ Starting with an offset, shifting timer values in a region (prefix arguments)

Where editors fall short:

- Zed ❌: Timers are left out: Zed cannot bind a key to a server's command (the commands are code actions), and they are not among them.
- Helix ❌: Timers are left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and they are not among them.

## Attachments

0% (0 of 2)

- ❌ Attaching files to an entry (`C-c C-a`)
- ❌ `attachment:` links

## Encryption

0% (0 of 1)

- ❌ Encrypting and decrypting entries (`org-crypt`)

## Inline images and LaTeX previews

0% (0 of 2)

- ❌ Inline images (`C-c C-x C-v`)
- ❌ LaTeX previews (`C-c C-x C-l`)
