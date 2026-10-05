# org-lsp

A language server for [Org mode](https://orgmode.org) documents, so that Org
works in any editor with an LSP client instead of in one editor at a time.

The server owns everything about Org: parsing, highlighting, structure and
edits. An editor needs only a thin shim (start the server, map highlight
groups, bind keys), and editors without a plugin system still get whatever
standard LSP can carry.

**Status: early.** The parser, the agenda, TODO state changes, priorities
and timestamp changes are checked against Emacs (Org 9.8.10) and match it
on the test corpus. Much of Org is
still missing; see [Not done yet](#not-done-yet).

## What works

| Feature | Neovim | VS Code | Helix |
| --- | --- | --- | --- |
| Highlighting (semantic tokens) | yes | yes | no¹ |
| Folding | yes, with `<Tab>` / `<S-Tab>` cycling | yes | no² |
| Outline (document symbols) | yes | yes | yes |
| TODO state changes, as `org-todo` does them, with fast selection by key and notes (`TODO(t)`, `WAIT(w@/!)`) | `cit` / `ciT`, repeatable with `.` | commands, `Ctrl+.` | `Space a` |
| Priorities, as `org-priority` changes them | `ciR` / `cir`, `<Leader>o,` | commands, `Ctrl+.` | `Space a` |
| Timestamps: the part at the cursor, or the date by days | `<C-a>` / `<C-x>`, `<S-Up>` / `<S-Down>`, with counts | commands, `Ctrl+.` | `Space a` |
| Schedule, deadline, insert a timestamp: dates typed as at Org's prompt, or picked in Emacs's calendar | `<Leader>ois`, `<Leader>oid`, `<Leader>oi.`, `<Leader>oi!` | commands, with a preview of the date | no⁴ |
| Completion of TODO keywords and tags in headlines | with an LSP completion plugin | yes | yes |
| Agenda: day, week, fortnight, month and year views, going to a date | yes | yes | no³ |
| Agenda: list of all TODO entries | yes | yes | no³ |
| Refile, as `org-refile` does it, between files too | `<Leader>or` | command | no⁴ |
| Capture, as `org-capture` does it: templates, prompts, a capture buffer | `<Leader>oc`; in the buffer `<C-c>`, `<Leader>ok`, `<Leader>or` | commands; in the buffer `Ctrl+C Ctrl+C`, `Ctrl+C Ctrl+K`, `Ctrl+C Ctrl+W` | no⁴ |
| Agenda: change an entry's state, priority or date, schedule it or set its deadline, go to its entry | yes | yes | no³ |

¹ Helix does not consume semantic tokens; it highlights Org with its own grammar.
² Helix has no folding.
³ Helix cannot open a document on a server's request. `org-lsp agenda FILE...` prints the agenda in a terminal.
⁴ Helix has no way to ask for text on a server's behalf.

Tested with Neovim 0.11.0 and 0.12.5, VSCodium 1.135 and Helix 25.07.1.

**TODO state changes** follow Org: tags return to their column, statistics
cookies in the parent are recounted, `CLOSED:` and state notes are logged
when configured, and an entry with a repeater moves to its next date.

**Priorities and timestamps** change as Org's `S-<up>` and its relatives
change them: the priority of the entry, or the year, month, day, hour or
minute under the cursor, with time ranges, repeaters, warning periods and
`CLOCK:` durations kept in step. A `#+PRIORITIES:` line is honoured.

**The agenda** shows scheduled entries, deadlines with their warnings,
timestamps, date ranges and the time grid, sorted as in Org, and is
rebuilt when its files change. It matches the output of Emacs's
`org-batch-agenda` line for line on the test corpus. A change made from
the agenda edits the entry's file in the editor and leaves saving to you,
as in Emacs.

## Install

The server is a single binary, `org-lsp`. There are no prebuilt binaries yet,
so each route below builds it with [cargo](https://rustup.rs).

### Neovim (0.11 or newer)

Install the repository as a plugin and build the server. With lazy.nvim:

```lua
{
  'kristijanhusak/org-lsp',
  build = 'cargo build --release -p org-lsp',
  init = function()
    vim.g.org_lsp = { settings = { agendaFiles = { '~/org' } } }
  end,
}
```

With any other plugin manager, install the repository and run `:OrgLspBuild`
once. Opening an `.org` file starts the server; `<Leader>oa` opens the
agenda from anywhere. `:checkhealth org-lsp` reports what the plugin found.

The plugin sets buffer-local keys in Org files, so it is meant to be used
instead of nvim-orgmode for those buffers, not next to it.

The plugin binds nvim-orgmode's default keys: `cit` changes the TODO
state, `ciR` / `cir` change the priority, `<C-a>` / `<C-x>` the part of
the timestamp under the cursor, `<Leader>ois` schedules, `<Leader>or`
refiles, `<Leader>oc` captures, `<Tab>` / `<S-Tab>` cycle folds, and in
the agenda `f` / `b`, `t`, `+` / `-` and `<CR>` do what they do in Emacs.
Counts work, and the editing keys repeat with `.`. Dates are asked for
with Emacs's calendar, or typed as at Org's prompt (`fri 10:00`, `+2d`,
`2/15`, `++1w`).

- [docs/neovim/keys.md](docs/neovim/keys.md): every key, what it does,
  and how to change it.
- [docs/neovim/configuration.md](docs/neovim/configuration.md): the
  plugin's options and the server's, with their defaults, and how to
  change them.

### VS Code

```sh
cargo build --release -p org-lsp
cd editors/vscode
npm install
npm run bundle-server
npm run package        # writes org-lsp-0.1.0.vsix
code --install-extension org-lsp-0.1.0.vsix
```

See `editors/vscode/README.md` for the commands, and:

- [docs/vscode/keys.md](docs/vscode/keys.md): every key and command, and
  how to add, change and remove keys.
- [docs/vscode/configuration.md](docs/vscode/configuration.md): the
  settings with their defaults, and how to change them.

### Helix

```sh
cargo install --path server/org-lsp
```

Then add the contents of `editors/helix/languages.toml` to your
`languages.toml`.

- [docs/helix/keys.md](docs/helix/keys.md): what the code action menu
  offers.
- [docs/helix/configuration.md](docs/helix/configuration.md): the
  server's options with their defaults, and how to change them.

### One configuration for every editor

The server reads its options (TODO keywords, agenda files, capture
templates, …) from `~/.config/org-lsp/config.toml` (or `config.json`)
itself, so they can be
set once for every editor. An editor's own settings go on top, and
`org-lsp config` shows what the server makes of them.
[docs/configuration.md](docs/configuration.md) has the details and every
option with its default.

### Other editors

Point the editor's LSP client at `org-lsp` for the `org` file type. The
server talks over standard input and output. `docs/protocol.md` describes
the options, the commands and what an integration has to do.

### Without an editor

```sh
org-lsp agenda ~/org/*.org              # this week
org-lsp agenda --span day --view todo ~/org/*.org
org-lsp agenda --help
```

## Layout

```
plugin/, lua/        Neovim plugin (at the root so plugin managers find it)
server/org-syntax/   the Org parser, a library with no LSP in it
server/org-core/     dates, the outline model, the agenda, TODO state changes
server/org-lsp/      the language server and command line
editors/vscode/      VS Code extension
editors/helix/       Helix configuration
docs/                protocol and parser notes; keys and configuration of each editor
tests/               Org files and what Emacs makes of them
scripts/             reference, fuzzing and test helpers
```

## Development

```sh
cargo test --workspace          # parser, agenda, TODO changes, protocol
scripts/test-nvim.sh            # Neovim plugin against the real server
cd editors/vscode
npm run check && npm run smoke  # types, and the protocol through VS Code's library
xvfb-run -a npm test            # the extension inside VS Code
npm run generate                # after the server's commands or tokens change
node ../../scripts/generate-docs.mjs   # the generated parts of docs/
```

Everything is checked against Emacs. The files under `tests/` come with
what Emacs makes of them: syntax trees from `org-element-parse-buffer`,
agendas from `org-batch-agenda`, and headlines after `org-todo`. The tests
compare against those committed outputs, so they need no Emacs. To
regenerate them (needs Emacs, and `faketime` for the agenda):

```sh
export ORG_LISP_DIR=/path/to/org-mode/lisp   # the reference is Org 9.8.10
scripts/update-fixtures.sh
scripts/update-agenda-fixtures.py
scripts/update-todo-fixtures.py
scripts/update-shift-fixtures.py           # priorities and timestamps
scripts/update-agenda-action-fixtures.py   # the same, from the agenda
scripts/update-fold-fixtures.py            # what folding a subtree hides
scripts/update-read-date-fixtures.py       # answers to the date prompt
scripts/update-planning-fixtures.py        # schedule, deadline, timestamps
scripts/update-calendar-fixtures.sh        # the calendar of the date prompt
```

`scripts/fuzz-against-emacs.py`, `fuzz-agenda.py`, `fuzz-todo.py`,
`fuzz-shift.py`, `fuzz-fold.py`, `fuzz-read-date.py` and
`fuzz-planning.py` generate random documents (or answers) and compare
both programs on them. See
`docs/parser.md`.

## Not done yet

- Agenda: tag and property matches, text search, filters, custom commands,
  the clock report, habits, diary entries and bulk actions.
- Capture, refile, archiving, clocking, tables, completion, following
  links, export.
- Commands that ask for input (a date, a note, a tag): LSP has no prompt
  for free text. Dates are asked for (scheduling, deadlines, timestamps),
  with Emacs's calendar, but not yet from the agenda. State notes are
  written without their text.
- The shifted cursor keys do nothing yet on list items, in tables and on
  property lines.
- Fold cycling is still done by the Neovim plugin, not by the server.
- No prebuilt binaries, and no Vim or Zed integration.
- Source blocks get no highlighting of their own; that needs editor support
  for embedded languages.
