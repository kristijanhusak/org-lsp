" Adapter for prabirshrestha/vim-lsp.
"
" vim-lsp passes the server's requests it does not answer itself, and its
" notifications, through its stream. It has no function to answer a
" request, so its own script function is used for that.

let s:name = 'org-lsp'

function! org_lsp#adapter#vim_lsp#register(cmd, options, filetypes) abort
  call lsp#register_server({
        \ 'name': s:name,
        \ 'cmd': {_ -> a:cmd},
        \ 'allowlist': a:filetypes,
        \ 'initialization_options': a:options,
        \ })
  call lsp#callbag#pipe(
        \ lsp#stream(),
        \ lsp#callbag#filter({message -> get(message, 'server', '') ==# s:name}),
        \ lsp#callbag#subscribe({'next': function('s:on_message')}),
        \ )
endfunction

function! s:respond(id, result) abort
  if !exists('s:send_response')
    let scripts = getscriptinfo({'name': 'autoload/lsp\.vim$'})
    let s:send_response = empty(scripts) ? v:null : function('<SNR>' . scripts[0].sid . '_send_response')
  endif
  if s:send_response isnot v:null
    call s:send_response(s:name, {'id': a:id, 'result': a:result})
  endif
endfunction

function! s:on_message(message) abort
  if has_key(a:message, 'request') && !has_key(a:message, 'response')
    let request = a:message.request
    " vim-lsp answers the others itself.
    if get(request, 'method', '') ==# 'window/showDocument'
      call s:respond(request.id, org_lsp#handle_request(request.method, get(request, 'params', {})))
    endif
  elseif has_key(a:message, 'response') && !has_key(a:message, 'request')
    let notification = a:message.response
    if has_key(notification, 'method') && !has_key(notification, 'id')
      call org_lsp#handle_notification(notification.method, get(notification, 'params', v:null))
    endif
  endif
endfunction

function! org_lsp#adapter#vim_lsp#ready(bufnr) abort
  if lsp#get_server_status(s:name) !=# 'running'
    " A request starts the server for the buffer.
    call org_lsp#adapter#vim_lsp#request(a:bufnr, 'workspace/symbol', {'query': ''}, {...->0})
    return 0
  endif
  return 1
endfunction

function! org_lsp#adapter#vim_lsp#encoding() abort
  return 'utf-16'
endfunction

" A file's URI as vim-lsp opens it.
function! org_lsp#adapter#vim_lsp#uri(path) abort
  return lsp#utils#path_to_uri(a:path)
endfunction

function! org_lsp#adapter#vim_lsp#request(bufnr, method, params, Callback) abort
  call lsp#send_request(s:name, {
        \ 'method': a:method,
        \ 'params': a:params,
        \ 'bufnr': a:bufnr,
        \ 'on_notification': {data -> a:Callback(get(data.response, 'error', {}), get(data.response, 'result', v:null))},
        \ })
endfunction

" Tell the server of the changes to buffer `bufnr` not sent yet: vim-lsp
" sends them for the current buffer, and for another only with a request
" about it.
function! org_lsp#adapter#vim_lsp#flush(bufnr) abort
  if lsp#get_server_status(s:name) ==# 'running'
    call lsp#ensure_flush_all(a:bufnr, [s:name])
  endif
endfunction
