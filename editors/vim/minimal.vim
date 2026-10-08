" A minimal vimrc for trying the Vim plugin, everything in a temporary
" folder: vim-plug, yegappan/lsp, this repository (the server built with
" cargo on the first start), and an Org file to try it on.
"
"   curl -fLo /tmp/minimal.vim https://raw.githubusercontent.com/kristijanhusak/org-lsp/main/editors/vim/minimal.vim
"   vim -N -u /tmp/minimal.vim
"
" The first start clones and builds, which takes a few minutes; later ones
" start at once. Remove the folder (`/tmp/org-lsp-vim`) to start over.
set nocompatible

let s:root = (empty($TMPDIR) ? '/tmp' : substitute($TMPDIR, '/$', '', '')) . '/org-lsp-vim'
let s:org_files = s:root . '/org'

" vim-plug, fetched once.
if !filereadable(s:root . '/autoload/plug.vim')
  call system('curl -fsSLo ' . shellescape(s:root . '/autoload/plug.vim') . ' --create-dirs '
        \ . 'https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim')
endif
let &runtimepath = s:root . ',' . &runtimepath

call plug#begin(s:root . '/plugged')
Plug 'yegappan/lsp'
Plug 'kristijanhusak/org-lsp', {'rtp': 'editors/vim', 'do': 'cargo build --release -p org-lsp'}
call plug#end()

" Installed before the plugins load, which comes after this file.
if !isdirectory(s:root . '/plugged/lsp') || !isdirectory(s:root . '/plugged/org-lsp')
  PlugInstall --sync | close
endif

let g:org_lsp = {
      \ 'cmd': [s:root . '/plugged/org-lsp/target/release/org-lsp'],
      \ 'settings': {'org_agenda_files': [s:org_files]},
      \ }

" An Org file to try it on.
if !isdirectory(s:org_files)
  call mkdir(s:org_files, 'p')
  call writefile([
        \ '* TODO Write the report',
        \ '  SCHEDULED: ' . strftime('<%Y-%m-%d %a>'),
        \ '* TODO Call the client',
        \ '  DEADLINE: ' . strftime('<%Y-%m-%d %a>', localtime() + 3 * 86400),
        \ '  - [ ] Prepare the questions',
        \ ], s:org_files . '/todo.org')
endif
if argc() == 0
  execute 'autocmd VimEnter * ++once edit ' . fnameescape(s:org_files . '/todo.org')
endif

set hidden
set mouse=a
filetype plugin indent on
syntax on
