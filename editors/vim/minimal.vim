" A minimal vimrc for trying the Vim plugin from a checkout of this
" repository, with vim-plug and yegappan/lsp in a temporary folder:
"
"   cargo build --release -p org-lsp
"   vim -N -u editors/vim/minimal.vim ~/org/todo.org
"
" It sets no Org options: the server reads your own configuration
" (`~/.config/org-lsp/config.toml`).
set nocompatible

let s:root = '/tmp/org-lsp-vim'
let s:org_lsp = expand('~/github/org-lsp')   " the checkout

" vim-plug, fetched once into the temporary folder.
if !filereadable(s:root . '/autoload/plug.vim')
  call system('curl -fsSLo ' . shellescape(s:root . '/autoload/plug.vim') . ' --create-dirs '
        \ . 'https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim')
endif
let &runtimepath = s:root . ',' . &runtimepath

call plug#begin(s:root . '/plugged')
Plug 'yegappan/lsp'
Plug s:org_lsp, {'rtp': 'editors/vim'}
call plug#end()

" Install the client on the first start, before the plugins load.
if !isdirectory(s:root . '/plugged/lsp')
  PlugInstall --sync | close
endif

let g:org_lsp = {'cmd': [s:org_lsp . '/target/release/org-lsp']}

set hidden
set mouse=a
filetype plugin indent on
syntax on
