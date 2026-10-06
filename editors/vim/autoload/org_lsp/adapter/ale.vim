" Adapter for dense-analysis/ale.
"
" ALE starts the server when it lints a buffer, or when asked; its
" messages, the answers to requests among them, all come to one callback.
" ALE does not apply the server's edits, so the plugin does.

let s:name = 'org_lsp'
" The callbacks waiting for answers, by connection and request id.
let s:waiting = {}
let s:connections = {}

function! s:linter(bufnr) abort
  for linter in ale#linter#Get(getbufvar(a:bufnr, '&filetype'))
    if linter.name ==# s:name
      return linter
    endif
  endfor
  return {}
endfunction

" One server for all: ALE starts one per project root, and the agenda and
" capture buffers are files in directories of their own.
function! s:root(nr) abort
  if !exists('s:root')
    let s:root = fnamemodify(bufname(a:nr), ':p:h')
  endif
  return s:root
endfunction

function! org_lsp#adapter#ale#register(cmd, options, filetypes) abort
  for filetype in a:filetypes
    call ale#linter#Define(filetype, {
          \ 'name': s:name,
          \ 'lsp': 'stdio',
          \ 'executable': a:cmd[0],
          \ 'command': join(map(copy(a:cmd), 'ale#Escape(v:val)')),
          \ 'project_root': function('s:root'),
          \ 'initialization_options': a:options,
          \ })
  endfor
endfunction

function! s:on_message(conn_id, _, message) abort
  let method = get(a:message, 'method', '')
  if !empty(method) && has_key(a:message, 'id')
    if method ==# 'window/showDocument' || method ==# 'workspace/applyEdit'
      call ale#lsp#SendResponse(a:conn_id, a:message.id, org_lsp#handle_request(method, a:message.params))
    endif
  elseif !empty(method)
    call org_lsp#handle_notification(method, get(a:message, 'params', v:null))
  elseif has_key(a:message, 'id') && has_key(s:waiting, a:conn_id . ':' . a:message.id)
    let Callback = remove(s:waiting, a:conn_id . ':' . a:message.id)
    call Callback(get(a:message, 'error', {}), get(a:message, 'result', v:null))
  endif
endfunction

function! s:started(bufnr, Then, linter, details) abort
  let conn_id = a:details.connection_id
  if !has_key(s:connections, conn_id)
    let s:connections[conn_id] = 1
    call ale#lsp#RegisterCallback(conn_id, function('s:on_message', [conn_id]))
  endif
  let s:connection = conn_id
  call ale#lsp#NotifyForChanges(conn_id, a:bufnr)
  call a:Then(conn_id)
endfunction

" Start the server for the buffer, or find it started, then call `Then`
" with the connection.
function! s:connect(bufnr, Then) abort
  let linter = s:linter(a:bufnr)
  if empty(linter)
    return 0
  endif
  return ale#lsp_linter#StartLSP(a:bufnr, linter, function('s:started', [a:bufnr, a:Then]))
endfunction

function! org_lsp#adapter#ale#ready(bufnr) abort
  if exists('s:connection')
    return 1
  endif
  call s:connect(a:bufnr, {_ -> 0})
  return 0
endfunction

function! org_lsp#adapter#ale#encoding() abort
  return 'utf-16'
endfunction

function! s:send(method, params, Callback, conn_id) abort
  let id = ale#lsp#Send(a:conn_id, [0, a:method, a:params])
  if id > 0
    let s:waiting[a:conn_id . ':' . id] = a:Callback
  else
    call a:Callback({'message': 'the server is not running'}, v:null)
  endif
endfunction

function! org_lsp#adapter#ale#request(bufnr, method, params, Callback) abort
  if !s:connect(a:bufnr, function('s:send', [a:method, a:params, a:Callback]))
    call a:Callback({'message': 'the server is not running'}, v:null)
  endif
endfunction
