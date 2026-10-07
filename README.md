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

| Feature | Neovim | VS Code | Vim | Sublime Text | Zed | Helix |
| --- | --- | --- | --- | --- | --- | --- |
| Highlighting (semantic tokens) | yes | yes | yes | yes⁶ | yes⁷ | no¹ |
| Code in its language's colors (`org-src-fontify-natively`): source and export blocks, inline source blocks (`src_python{…}`) | yes, with the language's tree-sitter parser, else its Vim syntax file (Emacs Lisp) | yes, for the languages VS Code ships | yes, with the language's syntax file | yes, for the languages Sublime Text ships | blocks, with `"semantic_tokens": "combined"` | no |
| Folding | yes, with `<Tab>` / `<S-Tab>` cycling | yes, with `Tab` / `Shift+Tab` cycling | yes, with `<Tab>` / `<S-Tab>` cycling | yes, with `Tab` / `Shift+Tab` cycling | Zed's own | no² |
| Concealing: links show their description, emphasis markers can hide (`org-link-descriptive`, `org-hide-emphasis-markers`) | yes | yes | no | no | no | no¹ |
| Outline (document symbols) | yes | yes | through the client | yes | yes | yes |
| Virtual indentation (`org-indent-mode`, `#+STARTUP: indent`) and hidden leading stars, without changing the file | yes, but not on wrapped lines | yes, but not on wrapped lines | yes (Vim 9.0.0067 or later), but not on wrapped lines | the indentation, as phantoms; the stars show | the indentation, as inlay hints (`"inlay_hints": { "enabled": true }`); the stars show | the indentation, as inlay hints (`display-inlay-hints`); the stars show |
| Indentation, as `org-indent-line` and `org-indent-region` do it, under `org_adapt_indentation` (`org-adapt-indentation`); what commands write is indented the same way | new lines (`indentexpr`), `=` | new lines (format on type), Format Selection | no | Format Selection | format selections | `=` |
| Formatting: tables aligned (`org-table-align`) and headlines' tags in their column (`org-align-tags`), in the whole file or in a range (with the range's indentation) | `vim.lsp.buf.format()`, with a range or in Visual mode; `=` | Format Document, Format Selection | the client's format commands | LSP: Format File, LSP: Format Selection | editor: format, format selections | `:format`, `=` |
| RET as Org's `org-return`: the next row in a table, a headline split with its tags kept, the rest of a list item's line kept in the item; indented as Org indents. With `return_uses_meta_return`, or with `<S-CR>` in Vim and Neovim, on an item's line the next item, numbered as M-RET numbers it | `<CR>`, `<S-CR>` in insert mode | Enter | `<CR>`, `<S-CR>` in insert mode | Enter | no | no |
| New headings, items and table rows as Org's M-RET, M-S-RET, C-RET and C-S-RET make them (`org-meta-return`, `org-insert-todo-heading`, and their `respect-content` forms): items with checkboxes, numbers and cookies put right, blank lines kept as around them | `<Leader><CR>`, `<Leader>oiT`, `<Leader>oih`, `<Leader>oit`, below the cursor's line, then typing on in insert mode | `Alt+Enter`, `Alt+Shift+Enter`, `Ctrl+Enter`, `Ctrl+Shift+Enter` | `<Leader><CR>`, `<Leader>oiT`, `<Leader>oih`, `<Leader>oit`, below the cursor's line, then typing on in insert mode | `Alt+Enter`, `Alt+Shift+Enter`, `Ctrl+Enter`, `Ctrl+Shift+Enter` | code actions | no |
| Promoting, demoting and moving subtrees, as `org-do-promote`, `org-promote-subtree` and `org-move-subtree-up` do (tags aligned again, text indented with `org_adapt_indentation`); indenting a drawer or block | `<<`, `>>`, `<s`, `>s`, `<Leader>oK`, `<Leader>oJ` | `Alt+Left`/`Right`, `Alt+Shift+Left`/`Right`, `Alt+Up`/`Down` | `<<`, `>>`, `<s`, `>s`, `<Leader>oK`, `<Leader>oJ` | `Alt+Left`/`Right`, `Alt+Shift+Left`/`Right`, `Alt+Up`/`Down` | code actions | code actions |
| List items: indenting, outdenting and moving them alone or with their children, as `org-indent-item`, `org-outdent-item-tree` and `org-move-item-up` do; cycling the bullets (`org-cycle-list-bullet`); the next and previous item; `C-c C-c` putting the list right (numbers, bullets, indentation) | `>>`, `<<`, `>s`, `<s`, `<Leader>oJ`, `<Leader>oK`, `<S-Left>`/`<S-Right>`, `<C-Space>` | `Alt+arrows`, `Alt+Shift+Left`/`Right`, `Shift+arrows`, `Ctrl+C Ctrl+C` | `>>`, `<<`, `>s`, `<s`, `<Leader>oJ`, `<Leader>oK`, `<S-Left>`/`<S-Right>`, `<C-Space>` | `Alt+arrows`, `Alt+Shift+Left`/`Right`, `Shift+arrows`, `Ctrl+C Ctrl+C` | code actions, but not the bullets | code actions, but not the bullets |
| Cutting, copying and pasting subtrees, as `org-cut-subtree`, `org-copy-subtree` and `org-paste-subtree` do (levels fitted to where they go), through the editor's register or clipboard | `<Leader>osd`, `<Leader>osy`, `<Leader>osp` | `Ctrl+C Ctrl+X Ctrl+W`, `Ctrl+C Ctrl+X Alt+W`, `Ctrl+C Ctrl+X Ctrl+Y` | `<Leader>osd`, `<Leader>osy`, `<Leader>osp` | `Ctrl+C Ctrl+X Ctrl+W`, `Ctrl+C Ctrl+X Alt+W`, `Ctrl+C Ctrl+X Ctrl+Y` | code actions | code actions |
| Sorting entries, as `org-sort-entries` does (alphabetically, by number, priority, property, TODO order, date, scheduled, deadline, creation or clocked time) | `<Leader>o^` | `Ctrl+C ^` | `<Leader>o^` | `Ctrl+C ^` | code actions | code actions |
| Sparse trees, as `org-sparse-tree` makes them (regexp, TODO entries, keywords, tags and properties, deadlines, dates): everything folded but the matches, which are highlighted | `<Leader>o/` | `Ctrl+C /` | `<Leader>o/` | `Ctrl+C /` | no | no |
| TODO state changes, as `org-todo` does them, with fast selection by key and notes (`TODO(t)`, `WAIT(w@/!)`) | `cit` / `ciT`, repeatable with `.` | `Ctrl+C Ctrl+T`, `Shift+Left`/`Right`, `Ctrl+.` | `cit` / `ciT`, repeatable with `.` | `Ctrl+C Ctrl+T`, `Shift+Left`/`Right` | code actions (`ctrl-.`) | `Space a` |
| Checkboxes, as `C-c C-c` toggles them on an item and `org-toggle-checkbox` under a headline, with parent boxes and the cookies that count them (`[1/3]`, `[33%]`) | `<C-Space>` | `Ctrl+C Ctrl+C`, `Ctrl+.` | `<C-Space>` | `Ctrl+C Ctrl+C` | code actions | `Space a` |
| Tags, as `org-set-tags-command` sets them: picked by key from `#+TAGS:` or `org_tag_alist` (fast tag selection, with exclusive groups) or typed, sorted and aligned; also from the agenda | `<Leader>ot` | `Ctrl+C Ctrl+Q` | `<Leader>ot` | `Ctrl+C Ctrl+Q` | no | no |
| Priorities, as `org-priority` changes them | `ciR` / `cir`, `<Leader>o,` | commands, `Ctrl+.` | `ciR` / `cir`, `<Leader>o,` | `Shift+Up`/`Down`, `Ctrl+C ,` | code actions | `Space a` |
| Timestamps: the part at the cursor, or the date by days | `<C-a>` / `<C-x>`, `<S-Up>` / `<S-Down>`, with counts | commands, `Ctrl+.` | `<C-a>` / `<C-x>`, `<S-Up>` / `<S-Down>` | `Shift+arrows` | code actions | `Space a` |
| Schedule, deadline, insert a timestamp: dates typed as at Org's prompt, or picked in Emacs's calendar | `<Leader>ois`, `<Leader>oid`, `<Leader>oi.`, `<Leader>oi!` | commands, with a preview of the date | `<Leader>ois`, `<Leader>oid`, `<Leader>oi.`, `<Leader>oi!`; dates typed | `Ctrl+C Ctrl+S`, `Ctrl+C Ctrl+D`, `Ctrl+C .`, `Ctrl+C !`; dates typed | no⁸ | no⁴ |
| Completion: TODO keywords and tags in headlines, `#+` keywords and `#+STARTUP:` values, drawers and property names, entities, and in links file names, headlines and custom IDs | with an LSP completion plugin | yes | through the client | yes | yes | yes |
| Agenda: tags at the right edge of the window, following it as it is resized (`org_agenda_tags_column` `"auto"`, as Org's default); TAB shows an entry in another window (an empty one, or a new one), RET where the agenda is | yes | tags at column 80; TAB in another group | yes | yes | tags at column 80; TAB in a split | no³ |
| Agenda: day, week, fortnight, month and year views, going to a date | yes | yes | yes, as a file | yes, as a file | code actions, as a file; no going to a date | no³ |
| Agenda: list of all TODO entries, tags and property searches (`m`, `M`), searches for words, phrases and regexps (`s`, `S`, as `org-search-view`; `[`, `]`, `{`, `}` to search again with another word or regexp), custom commands and block agendas (`org-agenda-custom-commands`) | yes | yes | yes | yes | code actions; no searches | no³ |
| Agenda: a hover over a line shows its entry, the headline and its text up to the next heading, each drawer folded to its first line (`:PROPERTIES:...`) | `K`, highlighted | the mouse, or `Ctrl+K Ctrl+I` | `K`, highlighted; the client's hover (`:LspHover`, coc.nvim's `doHover`, `:ALEHover`) | the mouse, or a key bound to LSP's `lsp_hover` | the mouse, or `Ctrl+K Ctrl+I` (`K` and `g h` with vim mode) | no³ |
| Tag and property inheritance (`org_use_tag_inheritance`, `org_tags_exclude_from_inheritance`, `org_use_property_inheritance`): file tags, parents' tags and properties in the agenda's tags and property searches, inherited tags shown in the agenda | yes | yes | yes | yes | yes | no³ |
| Agenda lines formatted as Org formats them, to your taste: `org_agenda_prefix_format` (`org-agenda-prefix-format`, with the category, time, leader, effort, level, last tag and breadcrumbs), the leaders of scheduled entries, deadlines and ranges, `todoKeywordFormat`, `removeTags`, `formatDate`; in custom commands and their blocks too | yes | yes | yes | yes | yes | no³ |
| Tangling, as `org-babel-tangle` does it: `:tangle`, header arguments from properties and `#+HEADER:`, Noweb references, comments, `:shebang`, `:tangle-mode`, `:mkdirp` | `<Leader>obt` | `Ctrl+C Ctrl+V t`, `Ctrl+.` in a block | `<Leader>obt` | `Ctrl+C Ctrl+V t` | code actions in a block | `Space a` in a block |
| Table formulas, as `org-table-recalculate` applies the `#+TBLFM:` line: column, field and range formulas, references by row, column, hline and name, parameters and constants, Calc's arithmetic, vectors, dates and durations, and its modes and formats (`;%.2f`, `;N`, `;T`); iterating until the table is stable | `<Leader>o*`; `<C-Space>` on a `#+TBLFM:` line | `Ctrl+C *` (the cursor's line), `Ctrl+C Ctrl+C` on a `#+TBLFM:` line, `Ctrl+.` in a table | `<Leader>o*`; `<C-Space>` on a `#+TBLFM:` line | `Ctrl+C *`, `Ctrl+C Ctrl+C` on a `#+TBLFM:` line | code actions in a table | `Space a` in a table |
| Refile, as `org-refile` does it, between files too | `<Leader>or` | command | `<Leader>or` | `Ctrl+C Ctrl+W` | code actions | no⁴ |
| Following links, as `org-open-at-point` does: headlines, targets, custom IDs, `file:` links with search options, `id:` links, URLs, timestamps (their day's agenda), tags, footnotes, an entry's links to pick from; in the agenda the line's entry's links | `<Leader>oo`, go to definition | `Ctrl+C Ctrl+O`, go to definition | `<Leader>oo` | `Ctrl+C Ctrl+O` | go to definition (internal and file links) | go to definition |
| Notes, as `org-add-note` takes them, in the entry's log or its drawer; in the agenda `z` | `<Leader>ona`, in a note buffer | `Ctrl+C Ctrl+Z` | `<Leader>ona` | `Ctrl+C Ctrl+Z` | no: Zed cannot ask for text | no⁴ |
| Archiving, as `org-archive-subtree`, `org-archive-to-archive-sibling` and `org-toggle-archive-tag` do it: archive locations (`org_archive_location`, `ARCHIVE` properties, `#+ARCHIVE:`, date trees), the context kept as `ARCHIVE_…` properties, all done subtrees at once | `<Leader>o$`, `<Leader>oxA`, `<Leader>oA`; `$` and `a` in the agenda | `Ctrl+C Ctrl+X Ctrl+S`, `Ctrl+C Ctrl+X A`, `Ctrl+C Ctrl+X a`; `$` and `a` in the agenda | `<Leader>o$`, `<Leader>oxA`, `<Leader>oA`; `$` and `a` in the agenda | `Ctrl+C Ctrl+X Ctrl+S`, `Ctrl+C Ctrl+X A`, `Ctrl+C Ctrl+X a`; `$` and `a` in the agenda | code actions | `Space a` |
| Archived subtrees: left out of the agenda unless asked for (`v a`, `v A` with the archive files, `org-agenda-archives-mode`), closed when cycling opens what is around them (`org-cycle-hide-archived-subtrees`), shaded (`org-archived`) | `va`, `vA` in the agenda; `<Tab>`, `<S-Tab>` | `v a`, `v A` in the agenda; `Tab`, `Shift+Tab` | `va`, `vA` in the agenda; `<Tab>`, `<S-Tab>` | `v a`, `v A` in the agenda; shaded | code actions; shaded | shaded |
| Capture, as `org-capture` does it: templates, prompts, a capture buffer | `<Leader>oc`; in the buffer `<C-c>`, `<Leader>ok`, `<Leader>or` | commands; in the buffer `Ctrl+C Ctrl+C`, `Ctrl+C Ctrl+K`, `Ctrl+C Ctrl+W` | `<Leader>oc`; in the buffer `<C-c>`, `<Leader>ok`, `<Leader>or` | `Ctrl+C c`; in the buffer `Ctrl+C Ctrl+C`, `Ctrl+C Ctrl+K`, `Ctrl+C Ctrl+W` | code actions, templates without text prompts | no⁴ |
| Agenda: change an entry's state, priority or date, schedule it or set its deadline, go to its entry | yes | yes | yes | yes | code actions, without dates asked for | no³ |
| Clocking, as `org-clock-in` and `org-clock-out` do it, with the clock in the status line | `<Leader>oxi`, `<Leader>oxo`, `<Leader>oxq`, `<Leader>oxj`; `clock_status()` | commands; the status bar | `<Leader>oxi`, `<Leader>oxo`, `<Leader>oxq`, `<Leader>oxj`; `org_lsp#clock_status()` | `Ctrl+C Ctrl+X Ctrl+I`, `Ctrl+C Ctrl+X Ctrl+O`; the status bar | code actions | `Space a`⁵ |
| Agenda: the clocked entry highlighted, log mode (what was closed and clocked each day), the clock report | `I`, `O`, `X`, `vl`, `R` | `I`, `O`, `X`, `J`, `l`, `R` | `I`, `O`, `X`, `vl`, `R` | `I`, `O`, `X`, `J`, `l`, `R` | code actions | no³ |

¹ Helix does not consume semantic tokens; it highlights Org with its own grammar.
² Helix has no folding.
³ Helix cannot open a document on a server's request. `org-lsp agenda FILE...` prints the agenda in a terminal.
⁴ Helix has no way to ask for text on a server's behalf.
⁵ Through the commands Helix lists; it has no status line for the clock.
⁶ With LSP's `semantic_highlighting` on. The Sublime Text package is
checked with stand-ins for Sublime Text and its LSP package, not in Sublime
Text itself.
⁷ With `"semantic_tokens": "combined"` for Org in Zed's settings.
⁸ Zed cannot run a server's command from a key or ask for text on its
behalf: the commands are in its code actions, and what they ask for is
asked with buttons (a state, a priority, a target, yes or no), but no date
or text can be.

Tested with Neovim 0.11.0 and 0.12.5, VSCodium 1.135, Vim 9.1 (with yegappan/lsp, vim-lsp, coc.nvim and ALE) and Helix 25.07.1.

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
the agenda edits the entry's file in the editor and saves it (`auto_save`;
off, it is left for you to save, as in Emacs).

## Install

The server is a single binary, `org-lsp`. CI builds it on every push to
`main` for Linux (x86_64, arm64), macOS (arm64, x86_64) and Windows
(x86_64): the [nightly release](https://github.com/kristijanhusak/org-lsp/releases/tag/nightly)
has the binaries (`org-lsp-<platform>.tar.gz`) and a VS Code package for
each platform with the server inside. Version tags make releases of their
own. Each route below can also build it with [cargo](https://rustup.rs).

### Neovim (0.11 or newer)

Install the repository as a plugin and build the server. With lazy.nvim:

```lua
{
  'kristijanhusak/org-lsp',
  build = 'cargo build --release -p org-lsp',
  init = function()
    vim.g.org_lsp = { settings = { org_agenda_files = { '~/org' } } }
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
```

`scripts/fuzz-against-emacs.py`, `fuzz-agenda.py`, `fuzz-todo.py`,
`fuzz-shift.py`, `fuzz-fold.py`, `fuzz-read-date.py`,
`fuzz-planning.py`, `fuzz-indent.py` and `fuzz-format.py` generate random documents (or
answers) and compare
both programs on them. See
`docs/parser.md`.

## Not done yet

- Agenda: stuck projects, filters, habits, diary entries and bulk actions;
  custom commands of those types; files the search view searches besides
  the agenda files (`org-agenda-text-search-extra-files`).
- Table formulas in Emacs Lisp, remote references, units and Calc's
  fractions and symbolic mode; setting tags with a command, export;
  tangling's Emacs Lisp
  (evaluated header arguments, `<<block()>>`), and other languages' `:var`.
- The shifted cursor keys do nothing yet on list items, in tables and on
  property lines.
- Fold cycling is still done by the Neovim plugin, not by the server.

## Licence

MIT, see [LICENSE](LICENSE).
