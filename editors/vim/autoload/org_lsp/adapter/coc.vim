" Adapter for neoclide/coc.nvim.
"
" coc.nvim shows the documents the server asks for on its own.

let s:name = 'org-lsp'

function! org_lsp#adapter#coc#register(cmd, options, filetypes) abort
  call coc#config('languageserver.' . s:name, {
        \ 'command': a:cmd[0],
        \ 'args': a:cmd[1:],
        \ 'filetypes': a:filetypes,
        \ 'initializationOptions': a:options,
        \ })
  for method in ['org-lsp/save', 'org-lsp/clock']
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

" coc.nvim passes on an error's message only. The server refuses a command
" where it does not apply, and the plugin then gives a key its own meaning
" (`<C-a>` off a timestamp); without the error's data, every refusal is
" taken as one.
function! s:error(error) abort
  if a:error is v:null
    return {}
  endif
  return type(a:error) == v:t_dict ? a:error : {'message': a:error, 'data': {'notApplicable': v:true}}
endfunction

function! org_lsp#adapter#coc#request(bufnr, method, params, Callback) abort
  call CocRequestAsync(s:name, a:method, a:params, {error, result -> a:Callback(s:error(error), result)})
endfunction
