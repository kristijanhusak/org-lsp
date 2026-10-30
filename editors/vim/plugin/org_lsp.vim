" org-lsp for Vim: Org mode through the org-lsp language server, on top of
" yegappan/lsp, coc.nvim, vim-lsp or ALE. See docs/vim.org.
if exists('g:loaded_org_lsp') || !has('patch-9.0.1485')
  finish
endif
let g:loaded_org_lsp = 1

command! -nargs=* -range -complete=customlist,org_lsp#complete Org call org_lsp#org_command(<range>, <line1>, <line2>, <f-args>)
command! OrgLspDownload call org_lsp#download()

augroup org_lsp
  autocmd!
  autocmd FileType org,org-agenda,org-date call org_lsp#attach()
  " Vim's own syntax for Org, found first when this plugin comes after
  " $VIMRUNTIME in 'runtimepath', gives way to the plugin's.
  autocmd Syntax org call timer_start(0, function('org_lsp#own_syntax', [str2nr(expand('<abuf>'))]))
  " An Org buffer read again from its file (`:e!`, 'autoread').
  autocmd BufReadPost * if getbufvar(str2nr(expand('<abuf>')), 'org_lsp_attached', 0) | call org_lsp#reread(str2nr(expand('<abuf>'))) | endif
  autocmd ColorScheme * call org_lsp#define_highlights() | call org_lsp#reload_syntaxes()
  " A buffer whose syntax changed while it showed in no window.
  autocmd BufWinEnter * if get(b:, 'org_lsp_syntax_stale', 0) | call org_lsp#reload_syntax(bufnr()) | endif
  " A client maps keys of its own (yegappan/lsp's `<CR>`) when it attaches.
  autocmd User LspAttached,lsp_buffer_enabled call org_lsp#remap_insert_keys(bufnr())
  " The LSP client plugins load after this one: register the server once
  " they all have.
  autocmd VimEnter * ++once call s:start()
  " How wide the agenda's window is, for its tags (`org_agenda_tags_column`
  " `"auto"`), once windows that open, close or change size are set.
  autocmd BufWinEnter,WinNew,WinClosed,WinResized,VimResized * call timer_start(0, {-> org_lsp#tell_agenda_width()})
  autocmd CursorMoved * if &filetype ==# 'org-agenda' | call org_lsp#tell_agenda_cursor() | endif
  autocmd CursorMoved * if &filetype ==# 'org' | call org_lsp#tell_roam_cursor() | endif
augroup END

function! s:start() abort
  call org_lsp#register()
  call org_lsp#set_keys('global')
endfunction

if v:vim_did_enter
  call s:start()
endif
