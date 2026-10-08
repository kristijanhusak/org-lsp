# org-lsp

A language server for [Org mode](https://orgmode.org) documents, so that Org
works in any editor with an LSP client instead of in one editor at a time.

The server owns everything about Org: parsing, highlighting, structure and
edits. An editor needs only a thin shim (start the server, map highlight
groups, bind keys), and editors without a plugin system still get whatever
standard LSP can carry.

**Status:** most of Org works in Neovim, Vim, VS Code and Sublime Text;
[What works](#what-works) says how much of each part of Org, and
[docs/parity.md](docs/parity.md) what is missing.

## What works

How much of what Emacs Org does org-lsp does, area by area, and how far
each editor goes with it. Each editor's README has the same table with
what does not work there and why, and its TESTING.md lists what to try by
hand: [Neovim](editors/neovim/README.md), [Vim](editors/vim/README.md),
[VS Code](editors/vscode/README.md),
[Sublime Text](editors/sublime/README.md), [Zed](editors/zed/README.md),
[Helix](editors/helix/README.md).

<!-- BEGIN generated: features -->

| Feature | Parity | Neovim / Vim / VS Code / Sublime Text / Zed / Helix |
| --- | --- | --- |
| [Highlighting](docs/parity.md#highlighting) | 83% | ✅ / ❓ / ✅ / ❓ / ❓ / ❌ |
| [Folding and visibility](docs/parity.md#folding-and-visibility) | 67% | ✅ / ✅ / ✅ / ✅ / ❓ / ❌ |
| [Headings and structure](docs/parity.md#headings-and-structure) | 100% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Lists and checkboxes](docs/parity.md#lists-and-checkboxes) | 91% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [TODO keywords](docs/parity.md#todo-keywords) | 100% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Priorities](docs/parity.md#priorities) | 100% | ✅ / ✅ / ✅ / ✅ / ✅ / ✅ |
| [Tags](docs/parity.md#tags) | 100% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Properties and column view](docs/parity.md#properties-and-column-view) | 67% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Dates and times](docs/parity.md#dates-and-times) | 100% | ✅ / ❓ / ✅ / ❓ / ❓ / ❓ |
| [Agenda](docs/parity.md#agenda) | 81% | ✅ / ✅ / ❓ / ✅ / ❓ / ❌ |
| [Clocking](docs/parity.md#clocking) | 100% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Capture](docs/parity.md#capture) | 82% | ✅ / ✅ / ✅ / ✅ / ❓ / ❌ |
| [Refile and archive](docs/parity.md#refile-and-archive) | 82% | ✅ / ✅ / ✅ / ✅ / ✅ / ❓ |
| [Links](docs/parity.md#links) | 91% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Footnotes](docs/parity.md#footnotes) | 100% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Tables](docs/parity.md#tables) | 65% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Table formulas](docs/parity.md#table-formulas) | 83% | ✅ / ✅ / ✅ / ✅ / ✅ / ✅ |
| [Code blocks (Babel)](docs/parity.md#code-blocks-babel) | 82% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Export and publishing](docs/parity.md#export-and-publishing) | 67% | ✅ / ✅ / ✅ / ✅ / ❌ / ❌ |
| [Citations](docs/parity.md#citations) | 100% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Diagnostics (org-lint)](docs/parity.md#diagnostics-org-lint) | 97% | ✅ / ✅ / ✅ / ✅ / ✅ / ✅ |
| [Indentation and formatting](docs/parity.md#indentation-and-formatting) | 83% | ✅ / ❓ / ✅ / ❓ / ❓ / ❓ |
| [Notes and logging](docs/parity.md#notes-and-logging) | 100% | ✅ / ✅ / ✅ / ✅ / ❌ / ❌ |
| [Completion and editing helpers](docs/parity.md#completion-and-editing-helpers) | 100% | ✅ / ✅ / ✅ / ✅ / ❓ / ❓ |
| [Timers](docs/parity.md#timers) | 60% | ✅ / ✅ / ✅ / ✅ / ❌ / ❌ |
| [Attachments](docs/parity.md#attachments) | 0% | — / — / — / — / — / — |
| [Encryption](docs/parity.md#encryption) | 0% | — / — / — / — / — / — |
| [Inline images and LaTeX previews](docs/parity.md#inline-images-and-latex-previews) | 0% | — / — / — / — / — / — |
| **All of Org** | **78%** |  |

✅ all that org-lsp does works there, ❓ some does not (the editor's
README says what and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](docs/parity.md) lists both.

<!-- END generated -->

Tested with Neovim 0.11.0 and 0.12.5, VSCodium 1.135, Vim 9.1 (with yegappan/lsp, vim-lsp, coc.nvim and ALE) and Helix 25.07.1.

What org-lsp does, it does as Emacs does: each command is checked against
Emacs (Org 9.8.10) on a test corpus and on random documents, and matches
it there. Export and Babel's evaluation are Emacs itself, run by the
server when Emacs is installed.

## Install

The server is a single binary, `org-lsp`. CI builds it on every push to
`main` for Linux (x86_64, arm64), macOS (arm64, x86_64) and Windows
(x86_64): the [nightly release](https://github.com/kristijanhusak/org-lsp/releases/tag/nightly)
has the binaries (`org-lsp-<platform>.tar.gz`) and a VS Code package for
each platform with the server inside. Version tags make releases of their
own. Each route below can also build it with [cargo](https://rustup.rs).

### Neovim (0.11 or newer)

Install the repository as a plugin. Opening an Org file the first time
downloads the server for your platform from the nightly release (needs
`curl` and `tar`); `:OrgLspDownload` updates it, and the plugin manager
can run it on each update. With lazy.nvim:

```lua
{
  'kristijanhusak/org-lsp',
  build = ':OrgLspDownload',
  init = function()
    vim.g.org_lsp = { settings = { org_agenda_files = { '~/org' } } }
  end,
}
```

To build the server instead (needs [cargo](https://rustup.rs)), set
`build = 'cargo build --release -p org-lsp'`, or run `:OrgLspBuild`; a
build in the plugin's directory and `org-lsp` on PATH come before the
download (`download = false` turns it off, `release = 'v0.2.0'` picks a
release). Opening an `.org` file starts the server; `<Leader>oa` opens the
agenda from anywhere. `:checkhealth org-lsp` reports what the plugin found.

The plugin sets buffer-local keys in Org files, so it is meant to be used
instead of nvim-orgmode for those buffers, not next to it.

The plugin binds nvim-orgmode's default keys: `cit` changes the TODO
state, `ciR` / `cir` change the priority, `<C-a>` / `<C-x>` the part of
the timestamp under the cursor, `<Leader>ois` schedules, `<Leader>or`
refiles, `<Leader>oc` captures, `<Tab>` / `<S-Tab>` cycle folds. The
agenda has the keys it has in every editor: Emacs's (`f` / `b`, `t`,
`+` / `-`, `<CR>`, `<C-c><C-s>`), with `J` for a date and the views
under `v` (`vd`, `vw`), so that no motion is taken; `q` closes it.
Counts work, and the editing keys repeat with `.`. `g?` shows the keys
of the buffer at hand. Dates are asked for
with Emacs's calendar, or typed as at Org's prompt (`fri 10:00`, `+2d`,
`2/15`, `++1w`).

- [docs/neovim/keys.md](docs/neovim/keys.md): every key, what it does,
  and how to change it.
- [docs/neovim/configuration.md](docs/neovim/configuration.md): the
  plugin's options and the server's, with their defaults, and how to
  change them.

### VS Code

```sh
scripts/install-vscode.sh            # the nightly build for this machine
scripts/install-vscode.sh v0.2.0     # a release
scripts/install-vscode.sh --source   # built from this checkout (cargo, npm)
```

The script downloads the package for your platform, or builds it, and
installs it with `code --install-extension`. Run it again to update, then
reload the window. `CODE=codium` (or `code-insiders`, `cursor`) installs
into another editor. On Windows without a shell, download
`org-lsp-win32-x64.vsix` from the release and run **Extensions: Install
from VSIX…**.

See `editors/vscode/README.md` for the commands, and:

- [docs/vscode/keys.md](docs/vscode/keys.md): every key and command, and
  how to add, change and remove keys.
- [docs/vscode/configuration.md](docs/vscode/configuration.md): the
  settings with their defaults, and how to change them.

### Vim (9.1 or newer)

The Vim plugin is in `editors/vim`, on top of one of the LSP clients
[yegappan/lsp](https://github.com/yegappan/lsp),
[vim-lsp](https://github.com/prabirshrestha/vim-lsp),
[coc.nvim](https://github.com/neoclide/coc.nvim) or
[ALE](https://github.com/dense-analysis/ale); it finds the one installed
and registers the server with it. With vim-plug:

```vim
Plug 'yegappan/lsp'
Plug 'kristijanhusak/org-lsp', { 'rtp': 'editors/vim', 'do': 'cargo build --release -p org-lsp' }
let g:org_lsp = { 'settings': { 'org_agenda_files': ['~/org'] } }
```

The keys are the Neovim plugin's (`cit`, `<C-a>`, `<Leader>oa`, `<Tab>`,
…), and the agenda opens in a split as the file the server writes.
[docs/vim/README.md](docs/vim/README.md) has the options, what each client
does and what Vim cannot.

### Sublime Text

`editors/sublime/LSP-org` is a package for
[sublimelsp/LSP](https://github.com/sublimelsp/LSP).
`scripts/install-sublime.sh` links it into Sublime Text's `Packages`
folder and has Package Control install LSP; the package finds `org-lsp`
(on PATH, in `~/.cargo/bin`, in the checkout) or downloads the nightly
build. It binds Emacs's keys in Org files (`Ctrl+C Ctrl+T`, `Tab`, …);
`org_keys` in its settings picks `vim` (for Vintage and NeoVintageous) or
`custom` instead. [editors/sublime/README.md](editors/sublime/README.md)
has the details.

### Zed

`editors/zed` is a Zed extension: run **zed: install dev extension** and
pick that folder. It uses `org-lsp` from PATH or downloads the nightly
build. Turn on `"semantic_tokens": "combined"` for Org for the server's
highlighting. The commands, the agenda and capture among
them, are in the code actions, and what they ask for is asked with
buttons; [editors/zed/README.md](editors/zed/README.md) has the settings
and what Zed can and cannot do.

### Helix

Put the `org-lsp` binary from the release on your `PATH`, or build it:

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

### Exporting through Emacs

Export is Org's own: with Emacs installed (Org 9.6 or newer), `C-c C-e`
(`<Leader>oe` in Vim and Neovim) opens Org's export menu, and Emacs makes
the file. The server starts Emacs the first time and keeps it running.
Your export settings go in a file Emacs loads before Org, or inline:

```toml
# ~/.config/org-lsp/config.toml
emacs_config_file = "~/.config/org-lsp/export.el"
emacs_config = '''
(setq org-export-backends '(ascii html icalendar latex md odt))
(setq org-html-validation-link nil)
'''
```

Code blocks are evaluated the same way (`C-c C-c` on a block, `C-c C-v
b` for the file's), with the languages you load there:
`(org-babel-do-load-languages 'org-babel-load-languages '((emacs-lisp . t)
(python . t)))`. Emacs asks before it evaluates, in the editor.

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
editors/vim/         Vim plugin, with adapters for four LSP clients
editors/sublime/     Sublime Text package (for sublimelsp/LSP)
editors/zed/         Zed extension
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
cd ../..
CLIENTS=~/vim-clients editors/vim/test/run.sh   # the Vim plugin, with each LSP client
node editors/sublime/generate.mjs      # the Sublime package's keys, after the server's change
python3 editors/sublime/test/test_plugin.py
node editors/zed/generate.mjs          # the Zed extension's token rules
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
scripts/update-indent-fixtures.py          # org-indent-line and org-indent-region
scripts/update-format-fixtures.py          # org-table-align and org-align-tags
scripts/update-table-fixtures.py           # org-table.el's editing commands
scripts/clock_reference.py                 # clocking in and out
scripts/clocktable_reference.py            # clock tables
```

`scripts/fuzz-against-emacs.py`, `fuzz-agenda.py`, `fuzz-todo.py`,
`fuzz-shift.py`, `fuzz-fold.py`, `fuzz-read-date.py`,
`fuzz-planning.py`, `fuzz-indent.py` and `fuzz-format.py` generate random documents (or
answers) and compare
both programs on them. See
`docs/parser.md`.

## Not done yet

[docs/parity.md](docs/parity.md) lists what Emacs Org does that org-lsp
does not yet, area by area; PLAN.md says in what order it comes.

## Licence

MIT, see [LICENSE](LICENSE).
