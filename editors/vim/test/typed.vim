" Keys typed into Vim through tmux by typist.py, not fed: a prompt waits
" for them as it does for the user's, with 'updatetime' passing and the
" client's autocommands running meanwhile. The agenda dispatcher's
" choices stay shown until a key is typed, and that key picks the view;
" a prompt of two lines waits for its key without Vim's hit-enter prompt.
"
" As the typed scenarios do (scenarios.vim), this writes the keys to type
" to `keys` in the work directory, and typist.py types them; it writes the
" result once both are done or time is up.
set updatetime=100
let s:work = $ORG_LSP_TEST_WORK
call mkdir(s:work . '/notes', 'p')
call writefile(['* TODO Write report', 'SCHEDULED: <2026-05-18 Mon>'], s:work . '/notes/work.org')
execute 'edit' fnameescape(s:work . '/notes/work.org')

let s:results = []
let s:step = 'ready'
let s:since = reltime()
" Whether the choices showed, and whether they went before a key picked
" one.
let s:shown = 0
let s:gone = 0
" What the prompt of two lines read, and whether Vim's hit-enter prompt
" showed while it waited.
let s:read = ''
let s:hit_enter = 0

function! s:type(items) abort
  call writefile([json_encode(a:items)], s:work . '/keys.part')
  call rename(s:work . '/keys.part', s:work . '/keys')
endfunction

function! s:typed() abort
  return !filereadable(s:work . '/keys')
endfunction

function! s:waited() abort
  return reltimefloat(reltime(s:since))
endfunction

function! s:go(step) abort
  let s:step = a:step
  let s:since = reltime()
endfunction

function! s:agenda_shown() abort
  return !empty(filter(range(1, winnr('$')), {_, w -> getbufvar(winbufnr(w), '&filetype') ==# 'org-agenda'}))
endfunction

function! s:finish(timer) abort
  call timer_stop(a:timer)
  call writefile(s:results, s:work . '/result.txt.part')
  call rename(s:work . '/result.txt.part', s:work . '/result.txt')
  qa!
endfunction

function! s:two_lines() abort
  let s:read = org_lsp#read_key("[a-z..]:Set [SPC]:clear\n{[t] TODO [d] DONE }", [], 1)
endfunction

function! s:tick(timer) abort
  if s:step ==# 'ready'
    if org_lsp#ready(bufnr())
      call s:type([['text', '\oa']])
      call s:go('dispatcher')
    endif
  elseif s:step ==# 'dispatcher'
    " The choices, past 'updatetime' and a held request.
    if !empty(popup_list())
      let s:shown = 1
    elseif s:shown
      let s:gone = 1
    endif
    if s:waited() > 1.5 && s:typed()
      call s:type([['text', 'a']])
      call s:go('agenda')
    endif
  elseif s:step ==# 'agenda'
    if s:agenda_shown() || s:waited() > 15
      call add(s:results, (s:shown ? 'ok' : 'not ok') . ' - typed: the agenda dispatcher shows its choices')
      call add(s:results, (s:shown && !s:gone ? 'ok' : 'not ok') . ' - typed: the choices stay until a key is typed')
      call add(s:results, (s:agenda_shown() ? 'ok' : 'not ok') . ' - typed: the key typed opens the agenda'
            \ . (s:agenda_shown() ? '' : printf(' (shown: %d, gone: %d)', s:shown, s:gone)))
      call timer_start(0, {-> s:two_lines()})
      call s:go('two lines')
    endif
  elseif s:step ==# 'two lines'
    let s:hit_enter = s:hit_enter || mode(1) =~# '^r'
    if s:waited() > 1 && s:typed() && empty(s:read)
      call s:type([['text', 'd']])
    endif
    if !empty(s:read) || s:waited() > 10
      call add(s:results, (s:read ==# 'd' && !s:hit_enter ? 'ok' : 'not ok') . ' - typed: a prompt of two lines reads its key'
            \ . (s:read ==# 'd' && !s:hit_enter ? '' : printf(' (read %s, hit-enter prompt: %d)', string(s:read), s:hit_enter)))
      return s:finish(a:timer)
    endif
  endif
endfunction

call timer_start(50, function('s:tick'), {'repeat': -1})
