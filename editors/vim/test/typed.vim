" Keys typed into Vim through tmux by run.sh, not fed: a prompt waits
" for them as it does for the user's, with 'updatetime' passing and the
" client's autocommands running meanwhile. The agenda dispatcher's
" choices stay shown until a key is typed, and that key picks the view.
"
" run.sh waits for `ready` in the work directory, types `\oa`, waits, then
" types `a`; this writes the result once the agenda shows or time is up.
set updatetime=100
let s:work = $ORG_LSP_TEST_WORK
call mkdir(s:work . '/notes', 'p')
call writefile(['* TODO Write report', 'SCHEDULED: <2026-05-18 Mon>'], s:work . '/notes/work.org')
execute 'edit' fnameescape(s:work . '/notes/work.org')

" Whether the choices showed, and whether they went before a key picked
" one.
let s:shown = 0
let s:gone = 0
let s:ticks = 0

function! s:agenda_shown() abort
  return !empty(filter(range(1, winnr('$')), {_, w -> getbufvar(winbufnr(w), '&filetype') ==# 'org-agenda'}))
endfunction

function! s:finish(results) abort
  call writefile(a:results, s:work . '/result.txt.part')
  call rename(s:work . '/result.txt.part', s:work . '/result.txt')
  qa!
endfunction

function! s:tick(timer) abort
  let s:ticks += 1
  if s:agenda_shown()
    call timer_stop(a:timer)
    let results = []
    call add(results, (s:shown ? 'ok' : 'not ok') . ' - typed: the agenda dispatcher shows its choices')
    call add(results, (s:shown && !s:gone ? 'ok' : 'not ok') . ' - typed: the choices stay until a key is typed')
    call add(results, 'ok - typed: the key typed opens the agenda')
    return s:finish(results)
  endif
  if !empty(popup_list())
    let s:shown = 1
  elseif s:shown && !filereadable(s:work . '/picked')
    let s:gone = 1
  endif
  if s:ticks > 300
    call timer_stop(a:timer)
    return s:finish(['not ok - typed: the key typed opens the agenda (shown: ' . s:shown . ', gone: ' . s:gone . ')'])
  endif
endfunction

function! s:wait_ready(timer) abort
  if org_lsp#ready(bufnr())
    call writefile([], s:work . '/ready')
    call timer_start(50, function('s:tick'), {'repeat': -1})
  else
    call timer_start(100, function('s:wait_ready'))
  endif
endfunction
call timer_start(100, function('s:wait_ready'))
