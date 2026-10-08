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

To try it without touching your own configuration,
[editors/vim/minimal.vim](../../editors/vim/minimal.vim) is a vimrc that
puts vim-plug and yegappan/lsp in a temporary folder and loads the plugin
from a checkout (`~/github/org-lsp`; change both places it is named
for another):

```sh
cargo build --release -p org-lsp
vim -N -u editors/vim/minimal.vim ~/org/todo.org
```

Leave the client's own semantic highlighting and folding off for Org files
(they are off by default in all four): the plugin does both.

## Keys and options

- [keys.md](keys.md): every key, what it does, and how to change it.
- [configuration.md](configuration.md): the plugin's options and the
  server's, with their defaults.

`K` in the agenda shows the entry of the line in a popup at the cursor,
highlighted as Org files are; moving the cursor closes it. `:Org agenda`,
`:Org capture`, `:Org todo next` … run any command.

## What differs from Neovim

- The agenda and the date prompt's calendar are files the server writes
  (`Agenda.org-agenda`, `Date.org-date` in a temporary directory), read
  again when it says they changed.
- Nothing is concealed: Vim cannot hide text that text properties mark,
  so link brackets and emphasis markers show.
- New lines are not indented by the server.
- With coc.nvim, the documents the server asks to show open where
  coc.nvim opens them (`coc.preferences.jumpCommand`), not as
  `win_split_mode` and `win_split_size` say; and since coc.nvim passes on only an error's
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
