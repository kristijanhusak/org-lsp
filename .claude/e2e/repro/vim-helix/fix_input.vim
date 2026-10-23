" Harness-only workaround for vim-6: org_lsp#prompt with Vim's input() form.
" Loaded as VH_EXTRA; redefines the function once the autoload file is read.
function! s:fix() abort
  runtime autoload/org_lsp.vim
  function! org_lsp#prompt(input) abort
    if a:input.kind ==# 'key'
      if get(a:input, 'expert', v:false)
        return org_lsp#read_key(a:input.prompt, [], 1)
      endif
      return org_lsp#read_key(a:input.prompt, get(a:input, 'choices', []))
    elseif has_key(a:input, 'choices')
      let lines = [a:input.prompt . ':']
      for i in range(len(a:input.choices))
        call add(lines, (i + 1) . '. ' . a:input.choices[i].label)
      endfor
      let picked = inputlist(lines)
      return picked > 0 && picked <= len(a:input.choices) ? a:input.choices[picked - 1].key : v:null
    endif
    let cancelled = "\x01cancelled"
    if !empty(get(a:input, 'completions', []))
      let g:VH_completions = a:input.completions
      let answer = input(a:input.prompt . ': ', get(a:input, 'value', ''), 'customlist,VHComplete')
      return answer
    endif
    let answer = inputdialog(a:input.prompt . ': ', get(a:input, 'value', ''), cancelled)
    return answer ==# cancelled ? v:null : answer
  endfunction
endfunction
function! VHComplete(lead, line, pos) abort
  return filter(copy(g:VH_completions), {_, c -> stridx(tolower(c), tolower(a:lead)) == 0})
endfunction
call s:fix()
