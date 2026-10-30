" Adapter for yegappan/lsp.
"
" Requests go straight to the server's channel, so that a command the
" server refuses (where a key keeps its own meaning) is not reported as an
" error by the client.

function! org_lsp#adapter#lsp#register(cmd, options, filetypes) abort
  call LspAddServer([{
        \ 'name': 'org-lsp',
        \ 'filetype': a:filetypes,
        \ 'path': a:cmd[0],
        \ 'args': a:cmd[1:],
        \ 'initializationOptions': a:options,
        \ 'customRequestHandlers': {'window/showDocument': function('s:request')},
        \ 'customNotificationHandlers': {
        \   'org-lsp/save': function('s:notification'),
        \   'org-lsp/clock': function('s:notification'),
        \   'org-lsp/timer': function('s:notification'),
        \   'org-lsp/virtualIndent': function('s:notification'),
        \   'org-lsp/display': function('s:notification'),
        \   'org-lsp/lineHighlights': function('s:notification'),
        \   'org-lsp/codeBlocks': function('s:notification'),
        \   'org-lsp/syntax': function('s:notification'),
        \   'org-lsp/viewChanged': function('s:notification'),
        \   'org-lsp/columns': function('s:notification'),
        \   'org-lsp/folds': function('s:notification'),
        \   'org-lsp/runCommand': function('s:notification'),
        \   'org-lsp/message': function('s:notification'),
        \ },
        \ }])
endfunction

function! s:request(server, request) abort
  let result = org_lsp#handle_request(a:request.method, get(a:request, 'params', {}))
  call a:server.sendResponse(a:request, result, {})
endfunction

function! s:notification(server, notification) abort
  call org_lsp#handle_notification(a:notification.method, get(a:notification, 'params', v:null))
endfunction

function! s:server(bufnr) abort
  let server = lsp#buffer#BufLspServerGet(a:bufnr)
  return !empty(server) && get(server, 'name', '') ==# 'org-lsp' ? server : {}
endfunction

function! org_lsp#adapter#lsp#ready(bufnr) abort
  let server = s:server(a:bufnr)
  return !empty(server) && get(server, 'ready', v:false)
endfunction

function! org_lsp#adapter#lsp#encoding() abort
  for info in getbufinfo({'bufloaded': 1})
    let server = s:server(info.bufnr)
    if !empty(server)
      return get(server, 'posEncoding', 16) == 8 ? 'utf-8' : 'utf-16'
    endif
  endfor
  return 'utf-16'
endfunction

function! org_lsp#adapter#lsp#request(bufnr, method, params, Callback) abort
  let server = s:server(a:bufnr)
  if empty(server) || !get(server, 'ready', v:false)
    return a:Callback({'message': 'the server is not running yet'}, v:null)
  endif
  call ch_sendexpr(job_getchannel(server.job), {'method': a:method, 'params': a:params},
        \ {'callback': {_, reply -> a:Callback(get(reply, 'error', {}), get(reply, 'result', v:null))}})
endfunction

" Tell the server of the changes to buffer `bufnr` not sent yet.
function! org_lsp#adapter#lsp#flush(bufnr) abort
  if exists('*listener_flush')
    call listener_flush(a:bufnr)
  endif
endfunction

" Show the inlay hints of buffer `bufnr` again: yegappan/lsp asks for them
" after a change of the text, not when the server has new ones (the
" clock's times, shrunk columns), as it announces no `refreshSupport`.
" Those shown go first: it keeps them when the server has none left (a
" column no longer shrunk).
function! org_lsp#adapter#lsp#inlay_hints(bufnr) abort
  if get(g:LspOptionsGet(), 'showInlayHints', v:false) && a:bufnr == bufnr()
    for type in ['LspInlayHintsType', 'LspInlayHintsParam']
      if !empty(prop_type_get(type))
        call prop_remove({'type': type, 'bufnr': a:bufnr, 'all': v:true})
      endif
    endfor
    call lsp#inlayhints#LspInlayHintsUpdateNow(a:bufnr)
  endif
endfunction
