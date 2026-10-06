" org-lsp for Vim: Org mode through the org-lsp language server, on top of
" yegappan/lsp, vim-lsp, coc.nvim or ALE. See docs/vim/.
if exists('g:loaded_org_lsp') || !has('patch-9.0.1485')
  finish
endif
let g:loaded_org_lsp = 1

command! -nargs=* -complete=customlist,org_lsp#complete Org call org_lsp#user_command(<f-args>)

augroup org_lsp
  autocmd!
  autocmd FileType org,org-agenda call org_lsp#attach()
  autocmd ColorScheme * call org_lsp#define_highlights()
  " The LSP client plugins load after this one: register the server once
  " they all have.
  autocmd VimEnter * ++once call s:start()
augroup END

function! s:start() abort
  call org_lsp#register()
  call org_lsp#set_keys('global')
endfunction

if v:vim_did_enter
  call s:start()
endif
