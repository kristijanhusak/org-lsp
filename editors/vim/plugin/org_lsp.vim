" org-lsp for Vim: Org mode through the org-lsp language server, on top of
" yegappan/lsp, vim-lsp, coc.nvim or ALE. See docs/vim.org.
if exists('g:loaded_org_lsp') || !has('patch-9.0.1485')
  finish
endif
let g:loaded_org_lsp = 1

command! -nargs=* -complete=customlist,org_lsp#complete Org call org_lsp#user_command(<f-args>)
command! OrgLspDownload call org_lsp#download()

augroup org_lsp
  autocmd!
  autocmd FileType org,org-agenda,org-date call org_lsp#attach()
  autocmd ColorScheme * call org_lsp#define_highlights()
  " A client maps keys of its own (yegappan/lsp's `<CR>`) when it attaches.
  autocmd User LspAttached,lsp_buffer_enabled call org_lsp#remap_insert_keys(bufnr())
  " The LSP client plugins load after this one: register the server once
  " they all have.
  autocmd VimEnter * ++once call s:start()
  " How wide the agenda's window is, for its tags (`org_agenda_tags_column`
  " `"auto"`), once windows that open, close or change size are set.
  autocmd BufWinEnter,WinNew,WinClosed,WinResized,VimResized * call timer_start(0, {-> org_lsp#tell_agenda_width()})
  autocmd CursorMoved * if &filetype ==# 'org-agenda' | call org_lsp#tell_agenda_cursor() | endif
augroup END

function! s:start() abort
  call org_lsp#register()
  call org_lsp#set_keys('global')
endfunction

if v:vim_did_enter
  call s:start()
endif
