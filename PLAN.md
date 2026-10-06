# org-lsp: plan

Written on 2026-10-05, when work was paused in the middle of milestone 3;
last updated the same day, after PR #8 (completion in headlines).
It records where the project stands, what is half done and how to finish
it, and what comes after. The default branch is `main`.

## Goal and rules

Org mode in any editor with an LSP client (Neovim, VS Code, Vim, Helix,
others), as close to Emacs Org mode as it can get.

- **Server in Rust, parser written from scratch.** No tree-sitter.
- **LSP only.** No second server and no protocol of its own. Where LSP has
  no fitting request, the feature waits or uses what LSP has (commands
  with arguments, server-owned documents, semantic tokens).
- **As much as possible in the server.** Editors get glue: a loop over
  tables the server announces (commands, keys, highlight links), not
  logic. A new feature should need no new editor code.
- **Emacs is the reference.** Org logic is ported from the Emacs sources
  (Org 9.8.10) and compared with Emacs by committed reference cases and
  by differential fuzzing. A deliberate difference is written down.
- **One repository**, Neovim files at the root (`plugin/`, `lua/`), other
  editors under `editors/`. The repository is private and stays private.
- **Small milestones.** Each is a branch and a pull request; when CI is
  green it is merged and work continues.

## Where things stand

### Merged into `main`

| | What | PR |
|---|---|---|
| 1 | Parser (`server/org-syntax`), server with highlighting, folding, outline, TODO cycling; Neovim plugin, VS Code extension, docs, CI | first commits |
| 2a | Agenda in the server: dates, repeaters, warning periods, week/day/month views, TODO list, `org-todo` port with logging and repeaters | #1 |
| 2b | Agenda in editors: server-owned document over `workspace/textDocumentContent`, manifest (commands, keys, highlight links) that drives the Neovim plugin and generates the VS Code `package.json`, in-editor tests for both | #2 |
| 3 | Priorities and dates: `org-priority`, `org-timestamp-change`, the agenda's date shift, Org's shifted keys; counts; the agenda's cursor follows an entry | #3 |
| — | Folding takes the blank lines after a subtree as Emacs does (`cycleSeparatorLines`) | #4 |
| 4a–4c | The date prompt: `org-read-date`, schedule, deadline, timestamps that ask for a date, and Emacs's calendar with the prompt | #5 |
| — | The calendar comes first; `i` asks for the answer; Neovim leaves insert mode there | #6 |
| — | `org.agenda.save` (`org-save-all-org-buffers`, `s` and `C-x C-s` in the agenda) | #7 |
| 11 (part) | Completion of TODO keywords and tags in headlines | #8 |
| — | Outline symbols keep their selection inside their range (VS Code refused them while typing) | #9 |
| 4d.1 | Schedule and deadline from an agenda line (`org-agenda-schedule`, `org-agenda-deadline`) | #10 |
| 4d.2 | `j` in the agenda (`org-agenda-goto-date`) | #11 |
| 4d.3 | Set a priority by name (`org-priority`, `org-agenda-priority`) | #12 |
| 4e.1 | Fast TODO selection (`org-fast-todo-selection`) and the text of state notes (`@`) | #13 |
| 4e.2 | Refile (`org-refile`, `org-agenda-refile`) | #14 |
| 4e.3 | Capture (`org-capture`) | #15 |
| — | Editor fixes, prebuilt binaries in CI, `scripts/install-vscode.sh`, tags from all agenda files | #16–#21 |
| 5, 9, 11 | Custom agenda commands and block agendas, `m`/`M`, clocking, log mode, clock report, more completion | #22 |
| 5, 13 | Block agenda cursor, separators, empty blocks; indentation (`org-indent-line`, `org-indent-region`, `adaptIndentation`) and virtual indentation (`org-indent-mode`) | #23 |
| 8 | Emphasis keeps its style across spaces; toggle checkboxes with their cookies | #24 |

### Where to pick up

Asked for on 2026-10-06 and not done yet, in this order (two items per
PR, as agreed):

1. **Zed.** An extension that starts the server for Org files and maps
   the manifest's commands; Zed cannot open server-owned documents, so the
   agenda goes through the file the server writes (as for Vim and Sublime
   Text, with `showDocument` and `virtualDocuments: false`).
2. **Formatting** (milestone 13): document formatting that aligns
   tables and tags (`org-table-align`, `org-align-tags`), on top of the
   range formatting that indents today.

Done from that list in #24: nested emphasis highlighting, checkbox
cookies. In #26: fold cycling in VS Code (`org-lsp.cycle`,
`org-lsp.cycleGlobal` on `Tab` / `Shift+Tab`, in the extension over the
server's folding ranges, since only the editor knows which folds are
closed), and key presets (`org-lsp.keys`: `emacs` binds Emacs's keys in
Org files too, `vim` the Neovim plugin's for VSCodeVim, `custom` none;
keys with their own meaning fall back to it where the server says the
command does not apply). In #27: the Vim plugin (`editors/vim`, shared
code plus adapters for yegappan/lsp, vim-lsp, coc.nvim and ALE, tested
with each against the real server) and the Sublime Text package
(`editors/sublime/LSP-org`, for sublimelsp/LSP, with the emacs, vim and
custom presets; tested against stand-ins, since Sublime Text cannot be
downloaded in the workspace). The server's option `showDocument` lets a
client that does not announce `window/showDocument` say it shows
documents. Concealing emphasis markers already existed
(`hideEmphasisMarkers`, `org-hide-emphasis-markers`, off by default).

Older leftovers, any time:

- The rest of capture (see 4e.3 below): item and checkitem templates,
  date trees, `%^g` tags, `%a` from the agenda.
- Milestones 5 to 12 below.

### Milestone 3, merged: priorities and dates

Kept here as a record of what it covers and how it was checked.

Done and checked:

- `org-core/src/priority.rs`: port of `org-priority` (up, down, set,
  remove; `#+PRIORITIES:`; number priorities; settings).
- `org-core/src/stamp.rs`: port of `org-timestamp-change` (the part under
  the cursor, day shift, active/inactive, end of a time range, repeater
  and warning parts, rounding of minutes, `CLOCK:` duration, where Emacs
  leaves the cursor).
- `org-core/src/edit.rs`: one entry point for every editing command,
  including Org's shifted cursor keys and the agenda's date shift
  (past dates jump to today, date ranges move as a whole).
- The TODO port now moves repeating timestamps with the same code.
- The agenda sorts by each file's own priority range.
- Server: commands `org.priority.up|down`, `org.timestamp.up|down`,
  `org.date.later|earlier`, `org.shift.up|down|left|right`; a `count` in
  the command context; a refusal that tells an editor "this key means
  nothing here" (`notApplicable`), so `<C-a>` still increments a number;
  code actions for all of them; in the agenda the cursor follows an entry
  that moves to another line.
- Server: a command on an agenda line is resolved against the view the
  editor still shows (`OpenView::shown`), or against the line the cursor
  is being moved to (`Server::moving`). A pending cursor move to a line
  lapses when the view changes again before the editor fetched it
  (`Reveal::At`); the move to today in a new view is recomputed
  (`Reveal::Home`).
- Neovim: counts (`3<C-a>`, `2.`), keys that fall through, and reporting
  a server edit back at once (Neovim otherwise waits 150 ms, and a held
  key loses presses).
- VS Code: shifted symbols translate to VS Code's notation (`+` is
  `shift+=`); tests for a timestamp command in a file and for priority
  and date from the agenda. VS Code passes no count; left that way.
- Documentation: `docs/protocol.md`, `README.md`,
  `editors/vscode/README.md`.
- Compared with Emacs:
  - `tests/shift/cases.json`: 337 cases, text and cursor.
  - `tests/agenda-actions/cases.json`: 70 cases run through Emacs's own
    agenda commands; all match.
  - `tests/agenda/priorities`: agenda order with per-file priorities.
  - The committed references regenerate byte for byte.
  - Fuzzing after the last change: `fuzz-shift.py` seeds 2001 and 2002
    (1,400 cases), `fuzz-todo.py`, `fuzz-agenda.py` and
    `fuzz-against-emacs.py` seed 2001: no difference.
- Last full run: 154 Rust tests, clippy and fmt clean; Neovim 0.11.0 and
  0.12.5 (68 + 73 checks); VS Code 12 tests in VSCodium 1.135.

### After milestone 3: folding as Emacs folds

A folded subtree now takes the blank lines before the next headline
along, keeping the last one visible when there are at least
`org-cycle-separator-lines` of them (option `cycleSeparatorLines`), as
`org-cycle-show-empty-lines` does. Blank lines at the end of the file
stay visible. Compared with Emacs by `tests/folding/cases.json` and
`scripts/fuzz-fold.py` (seeds 1 to 4, 1,900 documents, no difference).

Differences from Emacs in this milestone, all deliberate:

- The agenda is rebuilt after a change and the cursor goes with the
  entry. Emacs leaves the line where it is and writes the new date at its
  end until the next rebuild.
- Moved by an hour or more at once, the end of a time range comes out of
  Emacs with minutes like `70`. The server carries them properly.
- "Pressed again" (after the priority went away, the same key goes on
  from the other end) is judged as: same command, same entry, no other
  change in between. The server cannot see cursor movement.
- `org-timestamp-up-day` called by name on a headline sets the first TODO
  keyword in Emacs. No key leads there; the server refuses instead.
- Shifted cursor keys on list items and in tables do nothing yet.
- A repeater timestamp that stands *before* a stray priority cookie in a
  headline is not moved when the entry repeats.

## Next milestones

Each is meant as one or a few small pull requests. The order is by what
the agenda and daily use need first.

### 4. Commands that ask for something

Decided (2026-10-05): typed input goes through the editor's own input box,
and a date can also be picked from a calendar, in every editor that can
show one. In four pull requests:

- **4a, done:** `org-core/src/read_date.rs`, a port of `org-read-date`
  (`org-read-date-analyze`, `org-read-date-get-relative`, Emacs's
  `parse-time-string` and `iso8601-parse`). Checked by
  `tests/read-date/cases.json` (106 answers) and
  `scripts/fuzz-read-date.py` (seeds 1 to 7, 5,400 answers, plus 4,500
  random strings): no difference but those written down below. Uses the
  `regex` crate, so that each of Org's expressions is ported as written.
- **4b, done:** commands that ask for a date: `org.schedule`,
  `org.deadline`, `org.timestamp.insert` and its inactive form, in Org
  files. `org-core/src/planning.rs` ports `org--deadline-or-schedule`,
  `org-add-planning-info`, `org-timestamp` and `org-insert-timestamp`.
  Without an answer a command returns `{ "input": { "kind", "prompt",
  "value" } }`; the editor asks (`vim.ui.input`, `showInputBox`) and runs
  it again with `input`. `org.date.preview` says what an answer means;
  VS Code shows it under the input box. Checked by
  `tests/planning/cases.json` (31 cases, text, cursor and what the prompt
  starts with) and `scripts/fuzz-planning.py` (seeds 1 to 9, 4,700 cases,
  no difference). Not yet: logging a changed date (`org-log-reschedule`,
  `org-log-redeadline`), the prefix arguments (remove a date, set a delay),
  and these commands from the agenda (4d). After scheduling the cursor
  stays where the editor has it; Emacs leaves it where it deleted text.
- **4c, done:** the calendar Emacs shows with the date prompt. A second
  server-owned document, `org-agenda:/Org%20Date` (language `org-date`):
  three months drawn as `calendar-generate` draws them (checked against
  Emacs's output, `tests/calendar/emacs.txt`), and the minibuffer line
  (`SCHEDULED Date+time [2026-05-20]: 10:00    => <…>`, with `(=>F)`).
  Keys in the `calendar` scope are `org-read-date-minibuffer-local-map`'s
  (`S-<arrows>`, `M-S-<arrows>`, `<`, `>`, `.`) and the calendar's `RET`;
  vim adds `h` `j` `k` `l`. The answer is what was typed plus the day moved
  to, as `org-read-date` builds it. Clients opt in with
  `experimental.orgDatePrompt`; a command result `{ "close": uri }` closes
  the document. Neovim shows it in a window at the bottom, VS Code beside
  the file; others keep the plain input box.
- **4d, next:** in this order, each small.
  1. Done: schedule and deadline from an agenda line (`C-c C-s`,
     `C-c C-d`; `<Leader>ois`, `<Leader>oid`). `org-agenda-schedule` runs
     `org-schedule` where the line's marker points (its timestamp, or the
     headline), in the entry's file, so the prompt starts from the
     entry's own date. In the server a `prompt::Target` is a place in a
     file or the entry of an agenda line; `Server::date_source` reads the
     entry's file with `with_source`, which covers files that are not
     open. The agenda is rebuilt and its cursor follows the entry, as
     after the agenda's other commands (Emacs only marks the line with
     ` S => <date>` and rebuilds on `r`). Checked by 34 cases in
     `tests/agenda-actions/cases.json`: the files after the command and
     what the prompt starts with, every line kind, both views.
  2. Done: `j` in the agenda (`org-agenda-goto-date`; nvim-orgmode's
     `J`). The prompt is a plain `org-read-date` ("Date+time [today]"),
     read with the same settings as the others
     (`org-agenda-jump-prefer-future` defaults to them). The agenda then
     runs over as many days as its span has from that day: a month view
     becomes "30 days" starting there, and only a week or a fortnight
     still starts on the first day of the week. The cursor goes to the
     first line of the day, or the last line when the day is not shown.
     In the server a `prompt::Target::Agenda` stands for the view, and
     `Reveal::Day` places the cursor once the client has the new text.
     Checked by 12 cases in `tests/agenda-actions/cases.json`, which now
     also hold the agenda's text and cursor line after the command.
  3. Done: set a priority by name (`C-c ,`, `,` in the agenda;
     `<Leader>o,`). `priority::question` and `priority::read_answer`
     port `org-priority`'s prompt: one key (the first character typed),
     a space removes, letters in either case, and the whole answer only
     when number priorities reach 10. The input has kind `priority`.
     Checked by 108 cases in `tests/shift/cases.json` and 8 in
     `tests/agenda-actions/cases.json`. The text of state notes moved
     to 4e.

### 4e. TODO selection, refile, capture (asked for next)

1. **Done: fast TODO selection and note text.** `org.todo` (`C-c C-t`,
   `t` in the agenda, `cit`) is `org-todo`: with keys on the keywords
   it asks for one (input kind `key` with `choices`; Neovim reads one
   key press, VS Code offers a list) and sets the state it selects,
   `SPC` clears, any other key quits; without keys it cycles.
   `TodoKeywords::fast_keys` keeps the keys and assigns the missing ones
   as `org-assign-fast-keys` does, preferring the current keyword's
   sequence when keys are shared. A change that records a note (`@`,
   `org-log-done` or `org-log-repeat` set to `note`) asks for its text
   (input kind `note`, with `context: { state }`) before it is made;
   `!` and the repeat's time are written at once. The text goes under
   the heading line as `org-store-log-note` writes it (`\\` after the
   heading, lines indented to the item's body). Checked by 133 new
   cases in `tests/todo/cases.json` (`scripts/todo_reference.py` now
   answers the key and writes the note's text). Differences: Emacs
   makes the change first and asks for the note after, so aborting the
   note keeps the change; here cancelling the note cancels the change.
   The note is one line in both editors' input boxes; a multi-line note
   needs a note document like capture's (3).
2. **Done: refile** (`C-c C-w`, also in the agenda; `<Leader>or`).
   `org-core/src/refile.rs` ports `org-refile-get-targets` (the
   selections of `org-refile-targets`, names with links shown by their
   description, outline paths without statistics cookies or `COMMENT`,
   ` (file.org)` after targets from other files) and the move
   (`org-copy-subtree`, `org-paste-subtree`: last child, or first with
   `org-reverse-note-order`; every headline promoted or demoted and its
   tags aligned; the deletion where Emacs's cursor marker ends up, which
   can be the pasted copy). Options `refileTargets`,
   `refileUseOutlinePath`, `reverseNoteOrder`. Input kind `choice` with
   `choices` (Neovim `vim.ui.select`, VS Code a list). Checked by
   `tests/refile/cases.json` (39 cases) and `scripts/fuzz-refile.py`
   (seeds 1 to 4, 1,500 random documents, no difference). Not yet:
   `org-log-refile`, creating parent nodes, refiling a region, the
   prefix arguments (go to the target, copy), `org-refile-target-
   verify-function`; a `regexp` target uses the `regex` crate's syntax.
3. **Done: capture** (`<Leader>oc`, `C-c c` where the editor allows).
   `org-core/src/capture.rs` ports `org-capture-fill-template` (the
   time escapes, `%<format>`, `%f`, `%F`, `%a`, `%i`, `%n`, `%^{prompt
   |default|choices}`, `%^t` `%^T` `%^u` `%^U`, `%\N`, `%\*N`, escaped
   `%`, the tidying of blank lines and tabs) and the placing of `entry`
   and `plain` templates (`org-capture-place-entry`,
   `org-capture-place-plain-text`: targets `file`, `file+headline`,
   `file+olp`; `:prepend`, `:empty-lines*`; `org-blank-before-new-entry`
   as Org sets it). The capture buffer is a file `CAPTURE-<target>` in a
   directory of the server's own; it shows the region Emacs narrows to,
   and finishing puts it back (or places it again if the target changed
   meanwhile). `C-c C-c` / `<C-c>` finishes, `C-c C-k` / `<Leader>ok`
   aborts, `C-c C-w` / `<Leader>or` refiles. Options `captureTemplates`,
   `orgDirectory`, `defaultNotesFile`. Checked by
   `tests/capture/cases.json` and `scripts/fuzz-capture.py` (seeds 1 to
   5, 1,900 cases, no difference). Not yet: `item`, `checkitem` and
   `table-line` templates; `id`, `file+regexp`, `file+olp+datetree`,
   `function` and `clock` targets; `%^g`, `%^G`, `%^p`, `%^C`, `%^L`;
   `%(…)` and `%[file]`; `%c`, `%x`, `%k`, `%K` and `%:keyword` are
   empty; templates' contexts; `:jump-to-captured`, `:clock-in` and the
   like. Differences: the target is not saved (as for every edit here;
   see the autosave question); a `file+headline` headline that is not
   there is added when the capture finishes, not when it starts, so
   aborting leaves no trace; the date prompts are input boxes, without
   the calendar.

Differences from Emacs in 4a, both deliberate:

- A word boundary in Org's expressions (`\<`, `\>`) is one between a
  word character and anything else. `_` counts as a word character, and
  a change of script is no boundary: `日W9` holds no ISO week for the
  server, while Emacs finds `W9`.
- Numbers too large for 64 bits are refused; Emacs reads them as floats
  and usually fails later.

Differences from Emacs in 4c, deliberate or forced by LSP:

- Emacs types in the minibuffer and moves the calendar with `S-<arrows>`
  from there. An editor's input box cannot pass such keys on, so the
  calendar comes first, with its own keys, and `i` asks for the answer.
- Closing the prompt's window does not cancel the prompt on the server
  (the client may report it late, when another prompt is open); the next
  date command replaces it. `q` cancels.
- No mouse selection, holidays or diary marks in the calendar.

### 5. More agenda

- Done: match by tags and properties (`m`, `M`), custom agenda commands
  and block agendas, log mode (`l`), the clock report (`R`). Checked
  against Emacs in `tests/agenda/custom` and `tests/agenda/clock`.
- Done: block agendas keep the cursor in the agenda block that moves
  (`f`, `b`, `.`, `d`/`w`, `j`), on the same day when it is still shown;
  `blockSeparator` and `compactBlocks` (`org-agenda-block-separator`,
  `org-agenda-compact-blocks`, checked against Emacs); `hideEmptyBlocks`
  leaves out blocks with no entries (not in Org).
- Search (`s`), filters (`/`, `<`, `=`, `_`, `|`), stuck projects, group
  tags in match strings, archives mode, entry text, follow mode.
- Bulk marks and actions; hours and minutes on `S-<left>`/`S-<right>`.

### 6. Structure editing

- New headings and items (`M-RET` and relatives), promote and demote,
  move subtrees, cut/copy/paste subtree, sort.
- Refile and archive.
- Fold cycling in the server (Neovim does it in Lua today,
  `lua/org-lsp/folds.lua`), sparse trees.

### 7. Tags and properties

- Set tags with alignment and inheritance, tag groups.
- Properties: set, delete, allowed values (the shifted keys on a property
  line), column view if it can be shown at all.

### 8. Lists and checkboxes

- Done: toggle checkboxes (`org-toggle-checkbox`, on an item or a
  headline), parent boxes from their children, and the cookies that count
  checkboxes (`org-update-checkbox-count`), checked against Emacs in
  `tests/todo`. Cookies that count TODO entries were done with `org-todo`.
- Cycle bullets,
  renumber, indent and outdent, move items. This also completes the
  shifted cursor keys.

### 9. Clocking

- Done: clock in and out, cancel, go to the running clock, from Org files
  and the agenda (`I`, `O`, `X`, `J`); the clock in the editors' status
  line with the effort (`org-lsp/clock`); `org-clock-out-when-done`; the
  clocked entry highlighted in the agenda.
- Left: clock tables in files (`#+BEGIN: clocktable`, `C-c C-x C-r`),
  `org-clock-in-switch-to-state`, idle time and resolving dangling clocks
  with a question, the clock history (`C-u C-c C-x C-i`), effort
  estimates (`C-c C-x e`), `org-clock-display` (`C-c C-x C-d`).

### 10. Capture

- Templates, targets, finishing and aborting; refile targets shared with
  milestone 6.

### 11. Links, completion, navigation

- Follow links (`textDocument/definition`, `documentLink`), store and
  insert links, `id:` links across files, backlinks as references.
- Completion: TODO keywords and tags in headlines are done (server
  only); still to do: properties, link targets, block names, `#+`
  keywords, and tags from the agenda files as well as the file. Hover. Workspace symbols. Rename of tags and headings.

### 12. Tables

- Align and navigate (`TAB`, `RET`), insert and move rows and columns,
  as formatting and commands. Formulas later; they are a project of their
  own.

### 13. Diagnostics and formatting

- Next: document formatting (`textDocument/formatting`): align tables
  and tags, and indent as `org-indent-region` does when asked.
- Done: indentation as Org does it. `org-indent-line` on new lines
  (on-type formatting after a line break: `indentexpr` in Neovim, format
  on type in VS Code), `org-indent-region` on a range (range formatting:
  `=` in Neovim and Helix, Format Selection in VS Code), and
  `adaptIndentation` (`org-adapt-indentation`, `t` or `headline-data`)
  for what commands write: planning lines, drawers, notes, `CLOCK:`
  lines, and the text of entries whose level changes on refile and
  capture (`org-fixup-indentation`). Checked against Emacs in
  `tests/indent` and in the TODO, planning, refile and capture cases.
- Done: virtual indentation (`org-indent-mode`: `startupIndented`,
  `#+STARTUP: indent`) and hidden leading stars (`hideLeadingStars`,
  `#+STARTUP: hidestars`). The server sends each line's prefix
  (`org-lsp/virtualIndent`), checked against Emacs's `line-prefix` and
  `wrap-prefix` in `tests/indent`; Neovim shows them as inline virtual
  text, VS Code as decorations. Neither can indent the lines a long line
  wraps into (`wrap-prefix`); Helix cannot show them at all.
- `org-lint` checks as diagnostics.

### 14. Editors and distribution

- VS Code: keys in Org files (today only the agenda has keys there),
  publishing; the `publisher` field is a placeholder.
- Vim (which client to target is open), Helix (works for highlighting and
  code actions; it cannot open documents on the server's request, so it
  has no agenda), Zed.
- Prebuilt server binaries and a release workflow: done
  (`.github/workflows/release.yml`, a `nightly` prerelease from `main`,
  `scripts/install-vscode.sh`). Left: the Neovim plugin downloading the
  binary instead of building it, and publishing the extension to the
  Marketplace and Open VSX.
- An options schema in the manifest, so editors can offer settings:
  done (`settings` in the manifest, `docs/config.schema.json`).

### Later or out of scope for now

Export, Babel, column view, table formulas, inline images, LaTeX
previews.

## Open questions

1. **Typed input over LSP only.** Decided: the editor's input box, and
   a calendar for dates; see milestone 4.
2. **Licence.** The parser and the Org logic are ports of GPL code from
   Emacs Org mode. The repository has no licence file yet. To be set later.
3. **Saving after edits from the agenda.** Decided: saved by default
   (`autoSave`), unlike Emacs: the files a command changes other than the
   one it was run in (from the agenda, a capture or refile target) are
   saved once the editor has applied the change (the `org-lsp/save`
   notification). Off, they stay unsaved, as in Emacs, and
   `org.agenda.save` (`org-save-all-org-buffers`, `s` in the agenda) saves
   them.
4. **Vim.** Decided: one Vimscript plugin with the logic the Neovim plugin
   has (manifest, keys, the agenda and date prompt buffers, closing them),
   on top of whichever LSP client is installed. Generic LSP (highlighting
   where the client supports semantic tokens, outline, folding, code
   actions) needs nothing. Running commands with the cursor position and
   answering the server's requests differ per client (vim-lsp,
   yegappan/lsp, coc.nvim, ALE), so the plugin has a small adapter per
   client: send a request, receive one. Keep as much as possible in the
   shared code, and write adapters for the most used clients: yegappan/lsp
   (Vim 9's own), vim-lsp, coc.nvim and ALE. Clients without server-owned
   documents get the agenda as a file (the server already writes one) and
   the plain input box instead of the calendar.

## How the work is checked

- Rust: `cargo test --workspace`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo fmt --all`. CI's toolchain is newer than what
  was used locally and has more clippy lints; CI prints them as
  annotations.
- Against Emacs (needs Emacs, `faketime`, and `ORG_LISP_DIR` pointing at
  the `lisp` directory of an Org 9.8.10 checkout):

  | Area | Committed reference | Regenerate | Fuzz |
  |---|---|---|---|
  | Parser | `tests/fixtures/*.elements` | `scripts/update-fixtures.sh` | `scripts/fuzz-against-emacs.py` |
  | Dates | `tests/vectors/*.tsv` | `scripts/gen-vectors.el` | |
  | Agenda | `tests/agenda/*/expected-N.txt` | `scripts/update-agenda-fixtures.py` | `scripts/fuzz-agenda.py` |
  | TODO changes | `tests/todo/cases.json` | `scripts/update-todo-fixtures.py` | `scripts/fuzz-todo.py` |
  | Priorities, timestamps | `tests/shift/cases.json` | `scripts/update-shift-fixtures.py` | `scripts/fuzz-shift.py` |
  | Agenda commands | `tests/agenda-actions/cases.json` | `scripts/update-agenda-action-fixtures.py` | |
  | Folding subtrees | `tests/folding/cases.json` | `scripts/update-fold-fixtures.py` | `scripts/fuzz-fold.py` |
  | Date prompt | `tests/read-date/cases.json` | `scripts/update-read-date-fixtures.py` | `scripts/fuzz-read-date.py` |
  | Commands that ask for a date | `tests/planning/cases.json` | `scripts/update-planning-fixtures.py` | `scripts/fuzz-planning.py` |
  | Calendar of the date prompt | `tests/calendar/emacs.txt` | `scripts/update-calendar-fixtures.sh` | |
  | Indentation | `tests/indent/cases.json` | `scripts/update-indent-fixtures.py` | `scripts/fuzz-indent.py` |
- Neovim: `scripts/test-nvim.sh` (set `NVIM` to the binary), on 0.11.0 and
  stable. Tests run headless against the real server.
- VS Code: in `editors/vscode`, `npm run generate -- --check`, `npm run
  check`, `npm run build`, `xvfb-run -a npm test` (VSCodium works locally
  through `VSCODE_EXECUTABLE`).
- Vim: `CLIENTS=dir editors/vim/test/run.sh`, with the four clients
  checked out in `dir` (`lsp`, `vim-lsp`, `coc.nvim` on its `release`
  branch, `ale`); Vim runs in tmux.
- Sublime Text: `node editors/sublime/generate.mjs --check`,
  `python3 editors/sublime/test/test_plugin.py`.
- CI runs six jobs: server, Neovim 0.11.0, Neovim stable, VS Code, Vim (with each of its four LSP clients), Sublime Text (generated files, the plugin against stand-ins).

## Notes for whoever picks this up

- The work so far was done in a cloud workspace that is thrown away when
  idle. Everything that matters is pushed; the tools (Emacs, an Org
  checkout, Neovim, VSCodium) have to be installed again. What worked
  there: `apt-get install emacs-nox faketime`; Org from the GitHub mirror
  (`git clone --depth 1 --branch release_9.8.10
  https://github.com/bzg/org-mode`, then `make autoloads`), since
  savannah.gnu.org is blocked; Neovim and VSCodium from their GitHub
  release downloads (find VSCodium's latest tag with `git ls-remote
  --tags`), since VS Code's own download host is blocked.
- From that workspace `gh` only works through the REST API (`gh api`),
  CI logs cannot be downloaded (read check-run annotations instead), and
  remote branches cannot be deleted. The merged branches `agenda` and
  `agenda-editors` are still on the remote for that reason.
- `docs/protocol.md` is the description of what an editor integration has
  to do. Keep it in step with the server.
