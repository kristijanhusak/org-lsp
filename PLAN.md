# org-lsp: plan

Written on 2026-10-05, when work was paused in the middle of milestone 3.
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

### Milestone 3, in progress: priorities and dates

Branch `shift-commands`, three commits, **no pull request yet**.

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
- Compared with Emacs:
  - `tests/shift/cases.json`: 337 cases, text and cursor.
  - `scripts/fuzz-shift.py`: last six seeds, 4,200 cases, no difference.
  - `tests/agenda-actions/cases.json`: 70 cases run through Emacs's own
    agenda commands (`org-agenda-todo`, `-priority-up/down`,
    `-do-date-later/earlier`); all match.
  - `tests/agenda/priorities`: agenda order with per-file priorities.
- The last full run of the Rust tests was green with 151 tests, at commit
  `989f0ee`. Tests added after that were run on their own.

Saved unfinished in the last commit (`1c55605`, marked WIP):

- Neovim glue: counts (`3<C-a>`, `2.`), keys that fall through, and
  reporting a server edit back at once (Neovim otherwise waits 150 ms,
  and a held key loses presses). `tests/nvim/e2e.lua` passed with 68
  checks. `tests/nvim/agenda.lua` passed with 70 checks before the
  held-key test was added to it.
- Server: a command on an agenda line is resolved against the view the
  editor still shows (`OpenView::shown`), or against the line the cursor
  is being moved to (`Server::moving`). Without this, a key pressed
  before the agenda is redrawn can change a different entry than the one
  under the cursor. Rust test
  `a_command_from_a_stale_agenda_reaches_its_entry` passes.

To finish milestone 3, in this order:

1. Fix the one failing Rust test: in `changes_dates_from_the_agenda`
   (`server/org-lsp/tests/agenda_lsp.rs`) add `client.refetch_view();`
   after `client.command("org.agenda.todos", VIEW, 0);`. The test acted on
   a view the client had not fetched, which now rightly means the old one.
2. Run everything again: `cargo test --workspace`, `cargo clippy
   --workspace --all-targets -- -D warnings`, `scripts/test-nvim.sh`
   (Neovim 0.11.0 and stable). The Neovim tests have not run since the
   server change in the WIP commit.
3. VS Code, not started:
   - `editors/vscode/scripts/generate.mjs`: translate the new agenda keys
     (`+` is `shift+=` in VS Code's notation; `S-<up>` and the like already
     work), then `npm run generate`. CI checks that `package.json` is up
     to date and fails until this is done.
   - Tests in `editors/vscode/test/extension.test.cjs` for priority and
     date from the agenda and for a timestamp command in a file.
   - VS Code passes no count. Leave it, or add it to the middleware.
4. Documentation: `docs/protocol.md` (new commands; `count`;
   `notApplicable`; `native` keys; the options `priorityHighest`,
   `priorityLowest`, `priorityDefault`, `priorityStartCycleWithDefault`,
   `timestampRoundingMinutes`; cursor following; lines of a stale view),
   `README.md`, `editors/vscode/README.md`.
5. Fuzz once more after the last changes: `scripts/fuzz-shift.py`,
   `scripts/fuzz-todo.py`, `scripts/fuzz-agenda.py`,
   `scripts/fuzz-against-emacs.py`.
6. Open PR #3 from `shift-commands`, wait for the four CI jobs, merge.

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

Needs one decision first, see "Open questions": how a command gets typed
input over LSP only.

- Port `org-read-date` (`+2d`, `fri`, `2/15`, `+1w 10:00`, …), checked
  against Emacs like the other ports.
- `org-schedule`, `org-deadline` (in files and from the agenda),
  `org-time-stamp` and its inactive form, set a priority by name, go to a
  date in the agenda (`j`), text for state notes (now written empty).

### 5. More agenda

- Match by tags and properties (`m`, `M`), search (`s`), filters
  (`/`, `<`, `=`, `_`, `|`), stuck projects, custom agenda commands.
- Log mode, archives mode, clock report, entry text, follow mode.
- Bulk marks and actions; hours and minutes on `S-<left>`/`S-<right>`.
- A calendar or date picker as a second server-owned document. For this
  the manifest's single `agenda` entry becomes a list of views (scheme,
  language, key scope, how to present it).

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

- Toggle checkboxes with their statistics cookies, cycle bullets,
  renumber, indent and outdent, move items. This also completes the
  shifted cursor keys.

### 9. Clocking

- Clock in and out, cancel, go to the running clock, effort, clock
  tables, clocking from the agenda (`I`, `O`).

### 10. Capture

- Templates, targets, finishing and aborting; refile targets shared with
  milestone 6.

### 11. Links, completion, navigation

- Follow links (`textDocument/definition`, `documentLink`), store and
  insert links, `id:` links across files, backlinks as references.
- Completion: tags, TODO keywords, properties, link targets, block names,
  `#+` keywords. Hover. Workspace symbols. Rename of tags and headings.

### 12. Tables

- Align and navigate (`TAB`, `RET`), insert and move rows and columns,
  as formatting and commands. Formulas later; they are a project of their
  own.

### 13. Diagnostics and formatting

- `org-lint` checks as diagnostics. Document formatting (tags, tables,
  indentation).

### 14. Editors and distribution

- VS Code: keys in Org files (today only the agenda has keys there),
  publishing; the `publisher` field is a placeholder.
- Vim (which client to target is open), Helix (works for highlighting and
  code actions; it cannot open documents on the server's request, so it
  has no agenda), Zed.
- Prebuilt server binaries and a release workflow.
- An options schema in the manifest, so editors can offer settings.

### Later or out of scope for now

Export, Babel, column view, table formulas, inline images, LaTeX
previews.

## Open questions

1. **Typed input over LSP only.** LSP has no free-text prompt. The way
   that stays inside LSP: the manifest marks a command as taking input
   (`"input": { "prompt": "Schedule: " }`), the editor glue asks with its
   own input box and passes the answer as a command argument. One generic
   code path per editor, about ten lines. Editors without glue (Helix)
   would get a few fixed choices as code actions ("Schedule: today",
   "tomorrow", "+1w"). Not started; needs a yes.
2. **Licence.** The parser and the Org logic are ports of GPL code from
   Emacs Org mode. The repository has no licence file yet.
3. **Saving after edits from the agenda.** Edited files stay unsaved, as
   in Emacs. nvim-orgmode saves them. Keep, or make it an option.
4. **Which Vim client** to write the Vim integration for.

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

  The last two rows exist on `shift-commands` only, until PR #3 is merged.
- Neovim: `scripts/test-nvim.sh` (set `NVIM` to the binary), on 0.11.0 and
  stable. Tests run headless against the real server.
- VS Code: in `editors/vscode`, `npm run generate -- --check`, `npm run
  check`, `npm run build`, `xvfb-run -a npm test` (VSCodium works locally
  through `VSCODE_EXECUTABLE`).
- CI runs four jobs: server, Neovim 0.11.0, Neovim stable, VS Code.

## Notes for whoever picks this up

- The work so far was done in a cloud workspace that is thrown away when
  idle. Everything that matters is pushed; the tools (Emacs, an Org
  checkout, Neovim, VSCodium) have to be installed again.
- From that workspace `gh` only works through the REST API (`gh api`),
  CI logs cannot be downloaded (read check-run annotations instead), and
  remote branches cannot be deleted. The merged branches `agenda` and
  `agenda-editors` are still on the remote for that reason.
- `docs/protocol.md` is the description of what an editor integration has
  to do. Keep it in step with the server.
