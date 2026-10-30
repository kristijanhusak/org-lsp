" Adapter for neoclide/coc.nvim.
"
" coc.nvim shows the documents the server asks for on its own: the server
" tells the plugin of them as a notification instead
" (`show_document_notification`).

let s:name = 'org-lsp'

function! org_lsp#adapter#coc#register(cmd, options, filetypes) abort
  call coc#config('languageserver.' . s:name, {
        \ 'command': a:cmd[0],
        \ 'args': a:cmd[1:],
        \ 'filetypes': a:filetypes,
        \ 'initializationOptions': a:options,
        \ })
  for method in ['org-lsp/save', 'org-lsp/clock', 'org-lsp/timer', 'org-lsp/virtualIndent', 'org-lsp/display', 'org-lsp/lineHighlights', 'org-lsp/codeBlocks', 'org-lsp/syntax', 'org-lsp/viewChanged', 'org-lsp/columns', 'org-lsp/folds', 'org-lsp/runCommand', 'org-lsp/message', 'org-lsp/showDocument']
    call CocRegisterNotification(s:name, method, function('org_lsp#handle_notification', [method]))
  endfor
endfunction

function! org_lsp#adapter#coc#ready(bufnr) abort
  if !coc#rpc#ready()
    return 0
  endif
  try
    let services = CocAction('services')
  catch
    return 0
  endtry
  for service in services
    if service.id ==# 'languageserver.' . s:name
      return service.state ==# 'running'
    endif
  endfor
  return 0
endfunction

function! org_lsp#adapter#coc#encoding() abort
  return 'utf-16'
endfunction

" coc.nvim passes on an error's message only: a command that does not
" apply says so in its result (`refusal_results`), taken as the error the
" other clients have.
function! s:answer(Callback, error, result) abort
  if a:error isnot v:null
    return a:Callback(type(a:error) == v:t_dict ? a:error : {'message': a:error}, v:null)
  endif
  if type(a:result) == v:t_dict && get(a:result, 'notApplicable', v:false) is v:true
    return a:Callback({'message': get(a:result, 'message', ''), 'data': {'notApplicable': v:true}}, v:null)
  endif
  return a:Callback({}, a:result)
endfunction

" By buffer: the change count last seen, and when the server will have
" heard of it, in seconds.
let s:synced = {}

function! s:now() abort
  return reltimefloat(reltime())
endfunction

function! s:send(method, params, Callback) abort
  call CocRequestAsync(s:name, a:method, a:params, function('s:answer', [a:Callback]))
endfunction

" When the last request held goes.
let s:last_due = 0.0

" coc.nvim tells the server about a change 150 ms after it hears of it,
" and a request does not wait for that: after a change, requests wait
" until the server has the text they are about. They go in the order they
" were made (a command for the agenda's file, then the agenda's cursor it
" follows): each goes after the one held before it.
function! org_lsp#adapter#coc#request(bufnr, method, params, Callback) abort
  let tick = getbufvar(a:bufnr, 'changedtick')
  let known = get(s:synced, a:bufnr, {'tick': tick, 'at': 0.0})
  if known.tick != tick
    silent! call coc#api#Buf_flush(a:bufnr)
    let known = {'tick': tick, 'at': s:now() + 0.3}
  endif
  let s:synced[a:bufnr] = known
  let now = s:now()
  let last = s:last_due
  if known.at <= now && last <= now
    return s:send(a:method, a:params, a:Callback)
  endif
  let due = known.at > last + 0.002 ? known.at : last + 0.002
  let s:last_due = due
  call timer_start(float2nr(ceil((due - now) * 1000)), {-> s:send(a:method, a:params, a:Callback)})
endfunction

" A notification for the server.
function! org_lsp#adapter#coc#notify(method, params) abort
  if coc#rpc#ready()
    call CocNotify(s:name, a:method, a:params)
  endif
endfunction

" A file no window shows, written: coc.nvim loads the files it edits for
" the server without opening them to it, and the server reads them again.
function! org_lsp#adapter#coc#saved(bufnr) abort
  if coc#rpc#ready()
    call CocNotify(s:name, 'textDocument/didSave', {'textDocument': {'uri': org_lsp#uri(a:bufnr)}})
  endif
endfunction

" Tell the server of the changes to buffer `bufnr` not sent yet: coc.nvim
" syncs a buffer when told it changed, which Vim says of the current one.
function! org_lsp#adapter#coc#flush(bufnr) abort
  if exists('*listener_flush')
    call listener_flush(a:bufnr)
  endif
  silent! call coc#api#Buf_flush(a:bufnr)
  if coc#rpc#ready()
    call coc#rpc#notify('CocAutocmd', ['TextChanged', a:bufnr, getbufvar(a:bufnr, 'changedtick')])
  endif
endfunction
