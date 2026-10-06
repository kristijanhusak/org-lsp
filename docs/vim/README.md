# Vim

The Vim plugin (`editors/vim`) runs Org through the org-lsp server on top
of an LSP client: [yegappan/lsp](https://github.com/yegappan/lsp),
[vim-lsp](https://github.com/prabirshrestha/vim-lsp),
[coc.nvim](https://github.com/neoclide/coc.nvim) or
[ALE](https://github.com/dense-analysis/ale). It needs Vim 9.1.

The client starts the server and keeps it up to date with the buffers; the
plugin does the rest the same way for all four: it runs the server's
commands with the cursor position and asks what they ask for, shows the
agenda and capture buffers, highlights with the server's tokens (as text
properties), folds with its folding ranges, and shows the clock.

## Install

With vim-plug:

```vim
Plug 'yegappan/lsp'   " or vim-lsp, coc.nvim, ALE
Plug 'kristijanhusak/org-lsp', { 'rtp': 'editors/vim', 'do': 'cargo build --release -p org-lsp' }
```

With Vim's own packages, clone the repository and add `editors/vim` to
'runtimepath'. The server is found in the checkout (`target/release`), on
PATH, or where `cmd` says.

Leave the client's own semantic highlighting and folding off for Org files
(they are off by default in all four): the plugin does both.

## Options

Set `g:org_lsp` before the plugin loads. Every key may be left out.

```vim
let g:org_lsp = {
      \ 'settings': {'agendaFiles': ['~/org'], 'todoKeywords': ['TODO', 'NEXT', '|', 'DONE']},
      \ 'mappings': {'org': {'org.todo.next': '<Leader>t'}},
      \ }
```

| Option | Default | Meaning |
| --- | --- | --- |
| `cmd` | found | The server command, as a list. |
| `settings` | `{}` | The server's options; see [docs/configuration.md](../configuration.md). The shared `~/.config/org-lsp/config.toml` is read too. |
| `client` | `'auto'` | The LSP client: `'lsp'` (yegappan/lsp), `'vim-lsp'`, `'coc'` or `'ale'`. `'auto'` takes the first of these that is installed. |
| `folding` | `1` | Fold with the server's folding ranges, and cycle with `<Tab>` / `<S-Tab>`. |
| `startup_folded` | `'showeverything'` | `'overview'` opens files folded. |
| `highlight` | `1` | Highlight with the server's tokens. |
| `src_highlight` | `1` | Highlight the code of source blocks (`#+begin_src python`) with the syntax file of its language, as Emacs's `org-src-fontify-natively` does. |
| `win_split_mode` | `'split'` | How the agenda and capture buffers open: `'split'`, `'vsplit'`, `'current'`, or an Ex command that opens a window (`'botright split'`, `'tabnew'`). |
| `mappings` | the server's | Keys by scope (`global`, `org`, `agenda`, `capture`), as for the Neovim plugin: a command maps to a key, a list of keys, or `0`; `0` for a scope (or for `mappings`) sets no keys there. |

The keys are the Neovim plugin's; [docs/neovim/keys.md](../neovim/keys.md)
lists them (the date prompt's calendar aside). `g?` in an Org file or the
agenda shows the keys set there, yours included, in a popup. `:Org agenda`, `:Org
capture`, `:Org todo next` … run any command.

For the clock in the statusline:

```vim
set statusline=%f\ %=%{org_lsp#clock_status()}
```

`User OrgLspClock` fires when the clock starts or stops.

## What differs from Neovim

- Dates are typed, as at Org's prompt (`fri 10:00`, `+2d`); there is no
  calendar to pick them from.
- The agenda is the file the server writes (`Agenda.org-agenda` in a
  temporary directory), read again when it changes.
- Nothing is concealed: Vim cannot hide text that text properties mark,
  so link brackets and emphasis markers show.
- No virtual indentation, and new lines are not indented by the server.
- With coc.nvim, the documents the server asks to show open where
  coc.nvim opens them (`coc.preferences.jumpCommand`), not as
  `win_split_mode` says; and since coc.nvim passes on only an error's
  message, a key that keeps its own meaning (`<C-a>` off a timestamp)
  falls back to it whenever the server refuses.
- With ALE, one server serves the session, started for the directory of
  the first Org file.

## Tests

`editors/vim/test/run.sh` runs the plugin against the real server once per
client, in Vim inside tmux. `CLIENTS` is a directory with the clients
checked out (`lsp`, `vim-lsp`, `coc.nvim` from its `release` branch,
`ale`):

```sh
CLIENTS=~/vim-clients editors/vim/test/run.sh           # all four
CLIENTS=~/vim-clients editors/vim/test/run.sh vim_lsp   # one
```
