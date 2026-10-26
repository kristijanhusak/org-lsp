" The plugin against the real server, through one LSP client. Sourced by
" vimrc once Vim has started; writes TAP-like lines to $ORG_LSP_TEST_RESULT.
let s:work = $ORG_LSP_TEST_WORK
let s:results = []

function! s:until(what, Probe, ...) abort
  let timeout = get(a:, 1, 15000)
  let waited = 0
  while !a:Probe()
    if waited >= timeout
      throw 'timed out waiting for ' . a:what
    endif
    sleep 20m
    let waited += 20
    " Keys the plugin feeds (a key's own meaning) run as they would.
    call feedkeys('', 'x')
  endwhile
endfunction

function! s:test(name, Fn) abort
  try
    call a:Fn()
    call add(s:results, 'ok - ' . a:name)
  catch
    call add(s:results, 'not ok - ' . a:name . ': ' . v:exception . ' (' . v:throwpoint . ')')
  endtry
endfunction

function! s:types(line) abort
  return map(prop_list(a:line), 'v:val.type')
endfunction

" What the commands ask, answered by the tests.
let g:answers = []
call org_lsp#config()
function! org_lsp#prompt(input) abort
  return empty(g:answers) ? v:null : remove(g:answers, 0)
endfunction
function! org_lsp#read_key(prompt, choices, ...) abort
  return empty(g:answers) ? v:null : remove(g:answers, 0)
endfunction

call mkdir(s:work . '/notes', 'p')
call writefile([
      \ '#+TITLE: Work',
      \ '* TODO [#A] Write report :office:',
      \ 'SCHEDULED: <2026-05-18 Mon 09:00>',
      \ 'Some *bold* text. Count 5',
      \ '** Outline of the report',
      \ '* TODO Call the client',
      \ 'DEADLINE: <2026-05-20 Wed>',
      \ ], s:work . '/notes/work.org')
execute 'edit' fnameescape(s:work . '/notes/work.org')
let s:work_buf = bufnr()

function! s:starts() abort
  call s:until('the server', {-> org_lsp#ready(s:work_buf)})
  call s:until('highlighting', {-> index(s:types(2), 'org_lsp_stars.level1') >= 0})
  call assert_true(index(s:types(2), 'org_lsp_headline.level1') >= 0, 'headline level 1: ' . string(s:types(2)))
endfunction
call s:test('starts the server and highlights with its tokens', function('s:starts'))

" `org_fontify_whole_heading_line`: each headline's line coloured from
" edge to edge in the look of its level, with a sign without text.
function! s:whole_heading_lines() abort
  let Placed = {-> map(sign_getplaced(s:work_buf, {'group': 'org_lsp_lines'})[0].signs, '[v:val.lnum, v:val.name]')}
  call s:until('the headlines coloured', {-> len(Placed()) == 3})
  call assert_equal([[2, 'org_lsp_line_headline.level1'], [5, 'org_lsp_line_headline.level2'], [6, 'org_lsp_line_headline.level1']], Placed())
  call assert_equal('OrgLsp_headline_level1', sign_getdefined('org_lsp_line_headline.level1')[0].linehl)
endfunction
call s:test('colours the lines of headlines from edge to edge', function('s:whole_heading_lines'))

" A keyword with a face of its own (`org_todo_keyword_faces`) shows in it.
function! s:keyword_faces() abort
  call s:until('the face on TODO', {-> index(s:types(2), 'org_lsp_face_todo_TODO') >= 0})
  call assert_equal('#ff8800', synIDattr(hlID('OrgLspTodoFace_TODO'), 'fg#', 'gui'))
  call assert_equal('1', synIDattr(hlID('OrgLspTodoFace_TODO'), 'bold', 'gui'))
  " A tag with a face of its own (`org_tag_faces`): its name, not the colons.
  let tag = filter(prop_list(2), 'v:val.type ==# "org_lsp_face_tag_office"')
  call assert_equal([[27, 6]], map(tag, '[v:val.col, v:val.length]'))
  call assert_equal('#0088ff', synIDattr(hlID('OrgLspTagFace_office'), 'fg#', 'gui'))
endfunction
call s:test('gives keywords their faces', function('s:keyword_faces'))

" The TODO keywords offered in their faces, as `org-fast-todo-selection`
" shows them.
function! s:choice_faces() abort
  let line = org_lsp#choice_line({'key': 'w', 'label': 'WAIT', 'highlight': 'todoKeyword', 'face': {'foreground': '#00aa00'}})
  call assert_equal('[w] WAIT', line.text)
  call assert_equal([{'col': 5, 'length': 4, 'type': 'org_lsp_face_todo_WAIT'}], line.props)
  call assert_equal('#00aa00', synIDattr(hlID('OrgLspTodoFace_WAIT'), 'fg#', 'gui'))
  let done = org_lsp#choice_line({'key': 'd', 'label': 'DONE', 'highlight': 'todoKeyword.done'})
  call assert_equal(1, len(done.props))
  call assert_false(empty(prop_type_get(done.props[0].type).highlight), 'a highlight: ' . string(done.props))
  call assert_equal([], org_lsp#choice_line({'key': ' ', 'label': 'clear'}).props)
endfunction
call s:test('offers TODO keywords in their faces', function('s:choice_faces'))

function! s:folds() abort
  call s:until('folds', {-> !empty(get(b:, 'org_lsp_folds', []))})
  call cursor(2, 1)
  call org_lsp#cycle()
  call assert_equal(2, foldclosed(2))
  " The closed fold shows its headline, then `org-ellipsis`.
  call assert_equal('org_lsp#foldtext()', &l:foldtext)
  call s:until('the ellipsis', {-> foldtextresult(2) ==# getline(2) . '...'})
  call org_lsp#cycle()
  call assert_equal(-1, foldclosed(2))
  call assert_equal(-1, foldclosed(3))
endfunction
call s:test('folds and cycles as TAB does', function('s:folds'))

function! s:archived_folds() abort
  let file = s:work . '/notes/archived.org'
  call writefile(['* Project', 'Body', '** Old :ARCHIVE:', 'old body', '** New', 'new body'], file)
  execute 'edit' fnameescape(file)
  call s:until('folds', {-> len(get(b:, 'org_lsp_fold_ranges', [])) == 3})
  call cursor(1, 1)
  call org_lsp#cycle()
  call assert_equal(1, foldclosed(1))
  call org_lsp#cycle()
  call org_lsp#cycle()
  call assert_equal([-1, 3, -1], [foldclosed(1), foldclosed(3), foldclosed(5)])
  call org_lsp#global_cycle()
  call org_lsp#global_cycle()
  call assert_equal([-1, 3, -1], [foldclosed(1), foldclosed(3), foldclosed(5)])
  execute 'buffer' s:work_buf
endfunction
call s:test('archived subtrees stay closed when cycling', function('s:archived_folds'))

function! s:todo() abort
  call cursor(6, 3)
  call feedkeys('cit', 'x')
  call s:until('DONE', {-> getline(6) ==# '* DONE Call the client'})
  call feedkeys('.', 'x')
  call s:until('repeated', {-> getline(6) ==# '* Call the client'})
  call feedkeys('cit', 'x')
  call s:until('TODO again', {-> getline(6) ==# '* TODO Call the client'})
endfunction
call s:test('changes the TODO state with cit, and . repeats it', function('s:todo'))

function! s:increment() abort
  call cursor(3, 21)
  call feedkeys("\<C-a>", 'x')
  call s:until('the day after', {-> getline(3) ==# 'SCHEDULED: <2026-05-19 Tue 09:00>'})
  call cursor(4, 25)
  call feedkeys("\<C-a>", 'x')
  call s:until('the number incremented', {-> getline(4) ==# 'Some *bold* text. Count 6'})
endfunction
call s:test('<C-a> changes a timestamp, and increments a number elsewhere', function('s:increment'))

function! s:calendar_window() abort
  for win in range(1, winnr('$'))
    if getbufvar(winbufnr(win), '&filetype') ==# 'org-date'
      return win_getid(win)
    endif
  endfor
  return 0
endfunction

function! s:deadline() abort
  let file = win_getid()
  call cursor(6, 3)
  call org_lsp#run('org.deadline')
  " The calendar at the bottom, the cursor on the day of the deadline
  " Org proposes (that of the entry's timestamp).
  call s:until('the calendar', {-> s:calendar_window() == win_getid()})
  call s:until('the prompt line', {-> getline('$') =~# '^DEADLINE'})
  call s:until('the calendar as high as its text', {-> s:calendar_window() == win_getid() && winheight(0) == line('$')})
  call s:until('the cursor on the day', {-> expand('<cword>') ==# '20'})
  " Not to be typed into: `x` there changes nothing.
  silent! normal! x
  call assert_equal(0, &l:modifiable)
  call assert_equal(0, &l:modified)
  " `j` a week later, `<CR>` takes that day.
  call feedkeys('j', 'x')
  call s:until('the selected day a week later', {-> getline('$') =~# '=> <2026-05-27 Wed>'})
  call s:until('the cursor on it', {-> expand('<cword>') ==# '27'})
  call feedkeys("\<CR>", 'x')
  call s:until('the calendar closed', {-> s:calendar_window() == 0})
  call assert_equal(file, win_getid())
  call s:until('the new deadline', {-> getline(7) ==# 'DEADLINE: <2026-05-27 Wed>'})
  " `i` there asks for the date as text.
  let g:answers = ['2026-05-25']
  call org_lsp#run('org.deadline')
  call s:until('the calendar again', {-> s:calendar_window() == win_getid()})
  call feedkeys('i', 'x')
  call s:until('the calendar closed again', {-> s:calendar_window() == 0})
  call s:until('the typed deadline', {-> getline(7) ==# 'DEADLINE: <2026-05-25 Mon>'})
  silent write
endfunction
call s:test('picks a date in the calendar and sets the deadline', function('s:deadline'))

" The window the plugin opened for the agenda.
function! s:agenda_window() abort
  for win in range(1, winnr('$'))
    if getbufvar(winbufnr(win), '&filetype') ==# 'org-agenda' && !empty(getwinvar(win, 'org_lsp_opened', ''))
      return win_getid(win)
    endif
  endfor
  return 0
endfunction

function! s:agenda() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:agenda_window() != 0})
  call win_gotoid(s:agenda_window())
  call s:until('its text', {-> getline(1) =~# '^Week-agenda'})
  call assert_true(search('Write report', 'n') > 0, 'an entry')
  " Alt-m as a terminal sends it, <Esc> then `m`.
  call assert_equal('org.agenda.bulk.toggle', org_lsp#mapped_command('<Esc>m'))
  " Not to be typed into: `x` there changes nothing.
  silent! normal! x
  call assert_equal(0, &l:modifiable)
  call assert_equal(0, &l:modified)
  " `org_agenda_tags_column` `"auto"`: the tags end at the window's right edge,
  " and follow it when it changes size.
  let agenda = win_getid()
  let Report = {-> getbufline(winbufnr(agenda), search('Write report', 'n'))[0]}
  let Width = {-> getwininfo(agenda)[0].width - getwininfo(agenda)[0].textoff}
  call s:until('the tags at the edge', {-> Report() =~# ':office:$' && strchars(Report()) == Width()})
  leftabove 20vnew
  let narrow = win_getid()
  call win_gotoid(agenda)
  call s:until('the tags at the narrower edge', {-> Width() < &columns && strchars(Report()) == Width()})
  call win_execute(narrow, 'close')
  call s:until('the tags at the edge again', {-> Width() == &columns && strchars(Report()) == Width()})
  let first = getline(1)
  call feedkeys('f', 'x')
  call s:until('the next week', {-> getline(1) !=# first && getline(1) =~# '^Week-agenda'})
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
  call assert_equal(s:work_buf, bufnr())
  " `win_split_size`: lines for a split.
  let g:org_lsp.win_split_size = 6
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda of 6 lines', {-> s:agenda_window() != 0 && winheight(s:agenda_window()) == 6})
  call win_gotoid(s:agenda_window())
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
  call remove(g:org_lsp, 'win_split_size')
endfunction
call s:test('shows the agenda, moves through it, and closes it', function('s:agenda'))

" A change made from the agenda shows in the agenda once the server has
" written it anew, whichever client: the server says so
" (`org-lsp/viewChanged`).
function! s:agenda_refresh() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:agenda_window() != 0})
  call win_gotoid(s:agenda_window())
  call s:until('its text', {-> search('TODO \[#A\] Write report', 'n') > 0})
  for state in ['DONE', '', 'TODO']
    call cursor(search('Write report', 'n'), 1)
    call feedkeys('t', 'x')
    let Shown = {-> getline(search('Write report', 'n'))}
    call s:until('the agenda showing ' . (empty(state) ? 'no state' : state),
          \ {-> state ==# '' ? Shown() !~# '\(TODO\|DONE\) \[#A\]' : Shown() =~# state . ' \[#A\] Write report'})
  endfor
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
  " The file as it was.
  call s:until('the file as it was', {-> getbufline(s:work_buf, 2)[0] =~# '^\* TODO \[#A\] Write report'})
endfunction
call s:test('a change from the agenda shows in it', function('s:agenda_refresh'))

" Put the cursor on the line with `text`, and whether it is still there a
" moment later.
function! s:stays_on(text) abort
  if getline('.') !~# a:text
    call cursor(search(a:text, 'n'), 30)
  endif
  sleep 200m
  return getline('.') =~# a:text
endfunction

" A hover over an entry, as the LSP client shows one, has its headline and
" its text.
function! s:agenda_hover() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:agenda_window() != 0})
  call win_gotoid(s:agenda_window())
  call s:until('its text', {-> search('Write report', 'n') > 0})
  " The agenda shows today first, which moves the cursor there, and again
  " once it knows the window's width, its tags then at the window's edge:
  " read again, it moves the cursor, which would close the popups. The
  " cursor goes to the entry once that is done.
  let agenda = win_getid()
  let Report = {-> getbufline(winbufnr(agenda), search('Write report', 'n'))[0]}
  let Width = {-> getwininfo(agenda)[0].width - getwininfo(agenda)[0].textoff}
  call s:until('the agenda as wide as its window', {-> Report() =~# ':office:$' && strchars(Report()) == Width()})
  call s:until('the cursor to stay on the entry', function('s:stays_on', ['Write report']))
  let client = $ORG_LSP_TEST_CLIENT
  if client ==# 'coc'
    call CocActionAsync('doHover')
  elseif client ==# 'ale'
    let g:ale_hover_to_preview = 1
    ALEHover
  else
    LspHover
  endif
  let Shown = {-> join(flatten(map(popup_list(), {_, id -> getbufline(winbufnr(id), 1, '$')}))
        \ + flatten(map(filter(range(1, winnr('$')), {_, w -> getwinvar(w, '&previewwindow')}), {_, w -> getbufline(winbufnr(w), 1, '$')})), "\n")}
  call s:until('the entry in a popup', {-> Shown() =~# 'TODO \[#A\] Write report :office:'})
  call assert_match('SCHEDULED: <2026-05-\d\d', Shown())
  call popup_clear(1)
  pclose
  " `K`: the plugin's own popup, highlighted by the server's tokens.
  call feedkeys('K', 'x')
  call s:until('the entry in the popup of K', {-> !empty(popup_list())
        \ && getbufline(winbufnr(popup_list()[0]), 1)[0] =~# '^\* TODO \[#A\] Write report :office:$'})
  let popup = winbufnr(popup_list()[0])
  call assert_match('^SCHEDULED: <2026-05-', getbufline(popup, 2)[0])
  call assert_notequal([], prop_list(1, {'bufnr': popup}), 'highlighted')
  call popup_clear(1)
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
endfunction
call s:test('a hover shows the entry of an agenda line', function('s:agenda_hover'))

function! s:agenda_tab() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:agenda_window() != 0})
  let agenda = s:agenda_window()
  call win_gotoid(agenda)
  call s:until('its text', {-> search('Write report', 'n') > 0})
  " TAB: the entry in the window that shows its file.
  let shown = win_findbuf(s:work_buf)[0]
  call cursor(search('Write report', 'n'), 1)
  call feedkeys("\<Tab>", 'x')
  call s:until('the file in its window', {-> win_getid() == shown})
  call assert_true(getline('.') =~# 'Write report', 'on the headline')
  call assert_equal(agenda, s:agenda_window(), 'the agenda stays')
  " Alone, the agenda makes a window for it.
  call win_gotoid(agenda)
  only
  call cursor(search('Write report', 'n'), 1)
  call feedkeys("\<Tab>", 'x')
  call s:until('the file in a new window', {-> bufnr() == s:work_buf && win_getid() != agenda})
  call assert_equal(2, winnr('$'), 'two windows')
  call win_gotoid(agenda)
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
endfunction
call s:test('TAB shows the entry in another window', function('s:agenda_tab'))

" F (follow mode): the entry of the cursor's line in another window as the
" cursor moves, the cursor staying in the agenda.
function! s:agenda_follow() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:agenda_window() != 0})
  let agenda = s:agenda_window()
  call win_gotoid(agenda)
  call s:until('its text', {-> search('Write report', 'n') > 0})
  call cursor(search('Write report', 'n'), 1)
  call feedkeys('F', 'x')
  let Shown = {-> filter(win_findbuf(s:work_buf), {_, win -> win != agenda})}
  call s:until('the entry in another window', {-> !empty(Shown())
        \ && getbufline(s:work_buf, line('.', Shown()[0]))[0] =~# 'Write report'})
  call assert_equal(agenda, win_getid(), 'the cursor stays in the agenda')
  " `CursorMoved` comes from the main loop, which a test does not run.
  call cursor(search('Call', 'n'), 1)
  call org_lsp#tell_agenda_cursor()
  call s:until('the next entry', {-> getbufline(s:work_buf, line('.', Shown()[0]))[0] =~# 'Call'})
  " The outline path of its entry, echoed (`org-agenda-show-outline-path`).
  call s:until('the outline path', {-> org_lsp#echoed() ==# 'work.org'})
  call assert_equal(agenda, win_getid(), 'still in the agenda')
  " Off: the other window stays where it was.
  call feedkeys('F', 'x')
  sleep 300m
  call cursor(search('Write report', 'n'), 1)
  call org_lsp#tell_agenda_cursor()
  sleep 300m
  call assert_match('Call', getbufline(s:work_buf, line('.', Shown()[0]))[0])
  call win_gotoid(agenda)
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
endfunction
call s:test('F shows the entry of the line in another window', function('s:agenda_follow'))

" `\ot` sets tags a key at a time, until RET.
function! s:set_tags() abort
  let file = s:work . '/notes/tags.org'
  call writefile(['#+TAGS: work(w) home(h)', '* Task'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(2, 1)
  let g:answers = ['w', 'h', "\r"]
  call feedkeys("\\ot", 'xt')
  call s:until('the tags set', {-> getline(2) ==# '* Task' . repeat(' ', 77 - 6 - 11) . ':work:home:'})
  silent write
endfunction
call s:test('sets tags by key', function('s:set_tags'))

function! s:capture() abort
  let g:answers = ['t']
  call org_lsp#run('org.capture')
  call s:until('the capture buffer', {-> fnamemodify(bufname(), ':t') =~# '^CAPTURE-'})
  " The cursor is where `%?` was, for typing.
  call assert_equal([1, len('* TODO ')], [line('.'), col('.')])
  call setline(1, '* TODO Call mom')
  call feedkeys("\<C-c>", 'x')
  call s:until('the entry filed', {-> filereadable(s:work . '/notes/inbox.org') && index(readfile(s:work . '/notes/inbox.org'), '* TODO Call mom') >= 0})
  call s:until('the capture buffer closed', {-> fnamemodify(bufname(), ':t') !~# '^CAPTURE-'})
endfunction
call s:test('captures through a capture buffer', function('s:capture'))

" `org-lsp protocol URL`, as the system runs it for a bookmarklet: the
" server asks Vim to start the capture (`org-lsp/runCommand`).
function! s:protocol_capture() abort
  call s:until('the server to listen', {-> !empty(glob(s:work . '/.local/state/org-lsp/protocol/*.json', 0, 1))})
  let url = 'org-protocol://capture?template=w&url=https%3A%2F%2Forgmode.org%2F&title=Org+mode&body=Selected'
  call system(shellescape($ORG_LSP_BIN) . ' protocol ' . shellescape(url))
  call assert_equal(0, v:shell_error)
  call s:until('the capture buffer', {-> fnamemodify(bufname(), ':t') =~# '^CAPTURE-'})
  call assert_equal(['* Org mode', 'https://orgmode.org/', 'Selected'], getline(1, '$'))
  call org_lsp#run('org.capture.abort')
  call s:until('the capture buffer closed', {-> fnamemodify(bufname(), ':t') !~# '^CAPTURE-'})
endfunction
call s:test('captures from an org-protocol URL', function('s:protocol_capture'))

function! s:clock() abort
  execute 'buffer' s:work_buf
  call cursor(2, 3)
  call org_lsp#run('org.clock.in')
  call s:until('the clock', {-> org_lsp#clock_status() ==# '[0:00] (Write report)'})
  call s:until('the CLOCK line', {-> search('CLOCK: \[2026-05-18 Mon 10:30\]', 'n') > 0})
  call org_lsp#run('org.clock.out')
  call s:until('the clock stopped', {-> org_lsp#clock_status() ==# ''})
endfunction
call s:test('clocks in and out, for the statusline', function('s:clock'))

" Clocking in from the agenda colours the entry's line from edge to edge,
" with a sign without text; canceling the clock takes it away.
function! s:agenda_clocking() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:agenda_window() != 0})
  call win_gotoid(s:agenda_window())
  call s:until('its text', {-> getline(1) =~# '^Week-agenda'})
  let line = search('Write report', 'n')
  call cursor(line, 1)
  let Placed = {-> map(sign_getplaced(bufnr(), {'group': 'org_lsp_lines'})[0].signs, '[v:val.lnum, v:val.name]')}
  call org_lsp#run('org.clock.in')
  call s:until('the clocked line', {-> !empty(Placed())})
  call assert_equal([[line, 'org_lsp_line_agendaClocking']], Placed())
  call assert_equal('OrgLsp_agendaClocking', sign_getdefined('org_lsp_line_agendaClocking')[0].linehl)
  call org_lsp#run('org.clock.cancel')
  call s:until('the line plain again', {-> empty(Placed())})
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
endfunction
call s:test('highlights the clocked line of the agenda', function('s:agenda_clocking'))

" A shrunk column has `…` after its fields, an inlay hint, gone when it is
" shrunk no more (ALE shows no inlay hints).
function! s:shrink() abort
  if org_lsp#adapter() ==# 'ale'
    return
  endif
  let file = s:work . '/notes/shrink.org'
  call writefile(['| <3>    |', '| abcdef |'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  let nr = bufnr()
  let Hints = {-> len(filter(prop_list(1, {'bufnr': nr, 'end_lnum': 2}), {_, p -> get(p, 'text', '') =~# '…'}))}
  call cursor(2, 3)
  call org_lsp#run('org.table.toggle.column.width')
  call s:until('the column shrunk', {-> Hints() > 0})
  call org_lsp#run('org.table.toggle.column.width')
  call s:until('the column whole again', {-> Hints() == 0})
  execute 'buffer' s:work_buf
endfunction
call s:test('a shrunk column has … after it until it is whole again', function('s:shrink'))

" With `org_return_follows_link`, RET on a link in Insert mode follows it
" and types nothing, the target in the same file.
function! s:return_follows_link() abort
  let file = s:work . '/notes/follow.org'
  let lines = ['* Links', 'Go to [[*Target][the target]] end', '* Target']
  call writefile(lines, file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(2, 20)
  call feedkeys("i\<CR>\<Esc>", 'xt')
  call s:until('the target', {-> line('.') == 3})
  call assert_equal(lines, getline(1, '$'))
  execute 'buffer' s:work_buf
endfunction
call s:test('RET on a link follows it', function('s:return_follows_link'))

function! s:return() abort
  let file = s:work . '/notes/return.org'
  call writefile(['* TODO Write the report :work:', '| a | bb |', '#+begin_src sh', 'echo', '#+end_src'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(1, 12)
  call feedkeys("a\<CR>x\<Esc>", 'xt')
  call s:until('the split', {-> getline(2) ==# 'xthe report'})
  call assert_equal('* TODO Write' . repeat(' ', 59) . ':work:', getline(1))
  call cursor(3, 7)
  call feedkeys("a\<CR>y\<Esc>", 'xt')
  call s:until('the new row', {-> getline(4) ==# '|   | y   |'})
  " Where RET does not apply, another plugin's global mapping of <CR> (as
  " autopairs plugins make) does what it does, and one that maps <CR>
  " again does not loop; <S-CR> is RET too, but for making the next item
  " on a list item.
  " (A client's own buffer mapping of <CR>, which hides the global one,
  " comes first: yegappan/lsp and coc.nvim make one.)
  if empty(get(b:, 'org_lsp_replaced', {}))
    inoremap <CR> <CR>#
    call cursor(6, 4)
    call feedkeys("a\<CR>w\<Esc>", 'xt')
    call s:until('the global mapping in code', {-> getline(7) =~# '#w$'})
    iunmap <CR>
    imap <CR> <CR>%
    call cursor(7, col('$'))
    call feedkeys("a\<CR>u\<Esc>", 'xt')
    call s:until('a remapping global mapping in code', {-> getline(8) =~# '%u$'})
    iunmap <CR>
  endif
  call cursor(2, 5)
  call feedkeys("a\<S-CR>v\<Esc>", 'xt')
  call s:until('<S-CR> as RET', {-> getline(3) ==# 'vreport'})
  call assert_equal('xthe', getline(2))
  silent write
  " With the completion menu open, <CR> is the menu's, and <S-CR> still
  " RET: the menu closed, then the next row.
  let completed = s:work . '/notes/completed.org'
  call writefile(['| alpha | alps |', '| al |'], completed)
  execute 'edit' fnameescape(completed)
  call s:until('the server for RET with the menu', {-> org_lsp#ready(bufnr())})
  call cursor(2, 5)
  " The menu takes nothing in, so that the server has the text even from
  " a client that tells it of a change only once Vim waits for a key.
  let completeopt = &completeopt
  set completeopt=menu,menuone,noinsert,noselect
  call feedkeys("i\<C-n>\<S-CR>z\<Esc>", 'xt')
  let &completeopt = completeopt
  call s:until('<S-CR> with the menu open', {-> getline(3) ==# '| z      |      |'})
  call assert_equal('| al    |      |', getline(2))
  silent write
  " On a list item, <S-CR> makes the next item, numbered; <CR> is Org's
  " RET, a new line in the item.
  let listed = s:work . '/notes/listed.org'
  call writefile(['1. one', '2. two'], listed)
  execute 'edit' fnameescape(listed)
  call s:until('the server for <S-CR> on a list', {-> org_lsp#ready(bufnr())})
  call cursor(1, 1)
  call feedkeys("A\<S-CR>next\<Esc>", 'xt')
  call s:until('the next item', {-> getline(2) ==# '2. next'})
  call assert_equal(['1. one', '2. next', '3. two'], getline(1, '$'))
  call feedkeys("A\<CR>more\<Esc>", 'xt')
  call s:until('a new line in the item', {-> getline(3) ==# '   more'})
  silent write
endfunction
call s:test('RET in insert mode is as in Org', function('s:return'))

function! s:meta_return() abort
  let file = s:work . '/notes/meta.org'
  call writefile(['* TODO Write :work:', '- one', '** Sub', '* Next'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  " Below the cursor's line: a new item after the last one.
  call cursor(2, 5)
  call feedkeys("\\\<CR>", 'xt')
  call s:until('the new item', {-> getline(3) ==# '- '})
  call assert_equal(['* TODO Write :work:', '- one', '- ', '** Sub', '* Next'], getline(1, '$'))
  call assert_equal(3, line('.'))
  call feedkeys("\<Esc>", 'xt')
  " A TODO heading after the subtree.
  call cursor(1, 1)
  call feedkeys("\\oit", 'xt')
  call s:until('the TODO heading', {-> getline(5) ==# '* TODO '})
  call assert_equal(5, line('.'))
  call feedkeys("\<Esc>", 'xt')
  silent write
  " On a numbered item's bullet, the next item goes after the item, its
  " text left whole.
  let numbered = s:work . '/notes/numbered.org'
  call writefile(['1. one', '   more', '2. two'], numbered)
  execute 'edit' fnameescape(numbered)
  call s:until('the server for the numbered list', {-> org_lsp#ready(bufnr())})
  call cursor(1, 1)
  call feedkeys("\\\<CR>", 'xt')
  call s:until('the new numbered item', {-> getline(3) ==# '2. '})
  call assert_equal(['1. one', '   more', '2. ', '3. two'], getline(1, '$'))
  call assert_equal(3, line('.'))
  call feedkeys("\<Esc>", 'xt')
  silent write
endfunction
call s:test('M-RET and its neighbours insert headings and items', function('s:meta_return'))

function! s:structure() abort
  let file = s:work . '/notes/structure.org'
  call writefile(['* A', '** B', 'text', '*** C', '** D'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  " `>>` and `<<` demote and promote the headline, `>s` its subtree,
  " `\oJ` moves the subtree down.
  call cursor(2, 1)
  call feedkeys('>>', 'xt')
  call s:until('the headline demoted', {-> getline(2) ==# '*** B'})
  call feedkeys('<<', 'xt')
  call s:until('and promoted back', {-> getline(2) ==# '** B'})
  call feedkeys('>s', 'xt')
  call s:until('the subtree demoted', {-> getline(2) ==# '*** B' && getline(4) ==# '**** C'})
  call feedkeys('<s', 'xt')
  call s:until('and promoted back', {-> getline(2) ==# '** B' && getline(4) ==# '*** C'})
  call feedkeys("\\oJ", 'xt')
  call s:until('the subtree moved down', {-> getline(1, '$') == ['* A', '** D', '** B', 'text', '*** C']})
  call assert_equal(3, line('.'))
  " Off a headline `>>` shifts the line as Vim does.
  setlocal shiftwidth=2
  call cursor(4, 1)
  call feedkeys('>>', 'xt')
  call s:until('the text shifted', {-> getline(4) ==# '  text'})
  silent write
endfunction
call s:test('<< and >> promote and demote headlines, and move subtrees', function('s:structure'))

" On a Visual selection the keys of the commands Org gives a region work
" on it: `>>` demotes the headlines selected, `\o*` makes the lines
" selected headings (through Emacs).
function! s:region() abort
  let file = s:work . '/notes/region.org'
  call writefile(['* A', '* B', 'one', 'two'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(1, 1)
  call feedkeys('Vj>>', 'xt')
  call s:until('both headlines demoted', {-> getline(1, '$') == ['** A', '** B', 'one', 'two']})
  if executable('emacs')
    call cursor(3, 1)
    call feedkeys("Vj\\o*", 'xt')
    " A level below the headline above, as Emacs makes them.
    call s:until('the lines made headings', {-> getline(1, '$') == ['** A', '** B', '*** one', '*** two']}, 60000)
    " The cursor goes where Emacs left the point, after the edit.
    sleep 1500m
    call feedkeys('', 'x')
  endif
  silent write
endfunction
call s:test('a selection is the region of the commands Org gives one', function('s:region'))

" On list items `>>` and `<<` indent and outdent the item, `\oJ` moves
" it with the cursor, `<S-Right>` cycles the bullets and `<C-Space>`
" (`C-c C-c`) renumbers the list.
function! s:list_items() abort
  let file = s:work . '/notes/items.org'
  call writefile(['- a', '- b', '  - c', '- d'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(2, 3)
  call feedkeys('>>', 'xt')
  call s:until('the item indented', {-> getline(2) ==# '  - b'})
  call feedkeys('<<', 'xt')
  call s:until('and outdented back', {-> getline(2) ==# '- b'})
  call feedkeys("\\oJ", 'xt')
  call s:until('the item moved down', {-> getline(1, '$') == ['- a', '- d', '- b', '  - c']})
  call assert_equal([3, 3], [line('.'), col('.')])
  call feedkeys("\<S-Right>", 'xt')
  call s:until('the bullets cycled', {-> getline(1, '$') == ['+ a', '+ d', '+ b', '  - c']})
  silent %delete _
  call setline(1, ['1. a', '3. b', '7. c'])
  call cursor(2, 2)
  call feedkeys("\<C-Space>", 'xt')
  call s:until('the list renumbered', {-> getline(1, '$') == ['1. a', '2. b', '3. c']})
  silent write
endfunction
call s:test('the meta and shifted keys on list items', function('s:list_items'))

" `\osd` cuts the subtree into the register, `\osy` copies it, and `\osp`
" pastes the register as a subtree, at the level of the headlines around.
function! s:subtree_clipboard() abort
  let file = s:work . '/notes/clipboard.org'
  call writefile(['* A', '** B', 'text', '** C'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(2, 1)
  call feedkeys('"a\osy', 'xt')
  call s:until('the subtree in register a', {-> getreg('a') ==# "** B\ntext\n"})
  call assert_equal('V', getregtype('a'))
  call feedkeys("\\osd", 'xt')
  call s:until('the subtree cut', {-> getline(1, '$') == ['* A', '** C']})
  call assert_equal("** B\ntext\n", getreg('"'))
  call cursor(1, 1)
  call setreg('"', "* X\n", 'l')
  call feedkeys("\\osp", 'xt')
  call s:until('the register pasted', {-> getline(1, '$') == ['* X', '* A', '** C']})
  silent write
endfunction
call s:test('cuts, copies and pastes subtrees through registers', function('s:subtree_clipboard'))

" `\o^` sorts the children of the entry by the key typed.
function! s:sort() abort
  let file = s:work . '/notes/sorting.org'
  call writefile(['* Parent', '** b', '** C', 'text', '** a'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(1, 3)
  let g:answers = ['a']
  call feedkeys("\\o^", 'xt')
  call s:until('the children sorted', {-> getline(1, '$') == ['* Parent', '** a', '** b', '** C', 'text']})
  call assert_equal(1, line('.'))
  silent write
endfunction
call s:test('sorts the children of an entry', function('s:sort'))

" `\o/` makes a sparse tree: everything folded but the matches, which are
" highlighted.
function! s:sparse() abort
  let file = s:work . '/notes/sparse.org'
  call writefile(['* A', 'an apple', '* B :work:', 'nothing', '** C', 'Apple pie', '* D', 'none'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server and the folds', {-> org_lsp#ready(bufnr()) && foldlevel(2) > 0})
  let g:answers = ['r', 'apple']
  call feedkeys("\\o/", 'xt')
  let Marked = {line -> empty(prop_type_get('org_lsp_sparse')) ? 0 : len(prop_list(line, {'types': ['org_lsp_sparse']}))}
  call s:until('the matches highlighted', {-> Marked(2) == 1 && Marked(6) == 1})
  call assert_equal([-1, -1, 7], [foldclosed(2), foldclosed(6), foldclosed(8)])
  call setline(8, 'changed')
  doautocmd TextChanged
  call s:until('the highlights gone with the change', {-> Marked(2) == 0 && Marked(6) == 0})
  " A sparse tree by tags keeps its highlights through changes, as
  " `org-scan-tags` does, until `C-c C-c` takes them away.
  let g:answers = ['m', 'work']
  call feedkeys("\\o/", 'xt')
  call s:until('the tagged headlines highlighted', {-> Marked(3) == 1 && Marked(5) == 1})
  call setline(8, 'changed again')
  doautocmd TextChanged
  sleep 300m
  call assert_equal([1, 1], [Marked(3), Marked(5)])
  call feedkeys("\<C-Space>", 'xt')
  call s:until('the highlights taken away by C-c C-c', {-> Marked(3) == 0 && Marked(5) == 0})
  call assert_equal('* B :work:', getline(3))
  normal! zR
  " `\oR` (`org-reveal`): the cursor's context shown, the other folds
  " left as they are.
  normal! zM
  call cursor(6, 1)
  call feedkeys("\\oR", 'xt')
  call s:until('the context of the cursor shown', {-> foldclosed(6) == -1})
  call assert_equal([-1, 1, 7], [foldclosed(5), foldclosed(2), foldclosed(8)])
  normal! zR
endfunction
call s:test('makes a sparse tree', function('s:sparse'))

" A subtree yanked at the start of a line is folded once the buffer has
" it (`org-yank-folded-subtrees`); C-k on its folded headline kills what
" the fold hides (`kill-line`).
function! s:yank_folded() abort
  let file = s:work . '/notes/yank.org'
  call writefile(['* A', 'a body', '* B'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server and the folds', {-> org_lsp#ready(bufnr()) && foldlevel(2) > 0})
  call setreg('"', ['* N', 'new body', '** M', 'more'], 'l')
  " coc.nvim follows a buffer's changes a moment after it opens.
  sleep 300m
  call cursor(3, 1)
  call feedkeys(":call org_lsp#run('org.yank')\<CR>", 'xt')
  call s:until('the yanked subtree folded', {-> foldclosed(3) == 3 && foldclosedend(3) == 6})
  call assert_equal(['* A', 'a body', '* N', 'new body', '** M', 'more', '* B'], getline(1, '$'))
  call assert_equal(7, line('.'))
  5foldopen
  call assert_equal([-1, 5], [foldclosed(3), foldclosed(5)])
  normal! zM
  call cursor(3, 1)
  call feedkeys(":call org_lsp#run('org.kill.line')\<CR>", 'xt')
  call s:until('the folded subtree killed', {-> line('$') == 4})
  call assert_equal(['* A', 'a body', '', '* B'], getline(1, '$'))
  call assert_equal("* N\nnew body\n** M\nmore", getreg('"'))
  silent! write
endfunction
call s:test('folds a yanked subtree and kills it folded', function('s:yank_folded'))

" A file opens as its `#+STARTUP:` says: with `content` the headlines
" show, those without children folded, and drawers folded.
function! s:startup() abort
  let file = s:work . '/notes/startup.org'
  call writefile(['#+STARTUP: content', '* A', ':PROPERTIES:', ':ID: 1', ':END:', 'text', '** B', 'b text', '* C', 'c text'], file)
  execute 'edit' fnameescape(file)
  call s:until('the folds it opens with', {-> foldclosed(10) == 9})
  call assert_equal([3, -1, 7], [foldclosed(3), foldclosed(6), foldclosed(8)])
  normal! zR
endfunction
call s:test('opens a file as its startup says', function('s:startup'))

function! s:help() abort
  execute 'buffer' s:work_buf
  let popup = org_lsp#help()
  call assert_true(popup > 0, 'a popup')
  let shown = getbufline(winbufnr(popup), 1, '$')
  call assert_equal('Org file', shown[0])
  call assert_true(match(shown, '^  cit\s\+Change the TODO state$') >= 0, string(shown))
  call assert_true(match(shown, '^  g?\s\+Show the keys$') >= 0, 'g? is listed')
  call popup_close(popup)
endfunction
call s:test('g? lists the keys of the buffer', function('s:help'))

function! s:archive() abort
  let file = s:work . '/notes/archive.org'
  call writefile(['Intro', '* DONE Old', 'Notes.', '* DONE Older', '* TODO New'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(2, 1)
  call feedkeys("\\o$", 'xt')
  call s:until('the subtree archived', {-> getline(2) ==# '* DONE Older'})
  call s:until('the archive file', {-> filereadable(file . '_archive')})
  call assert_true(index(readfile(file . '_archive'), '* DONE Old') >= 0, string(readfile(file . '_archive')))
  let g:answers = ['y']
  call cursor(1, 1)
  call org_lsp#run('org.archive.done')
  call s:until('the subtree without open TODO items archived', {-> getline(1, '$') ==# ['Intro', '* TODO New']})
  silent write
endfunction
call s:test('archives a subtree, and asks about each done one', function('s:archive'))

function! s:code_link_note() abort
  let file = s:work . '/notes/code.org'
  call writefile(['* Code', '#+begin_src python :results output', 'def greet():', '#+end_src', 'See [[*Code]].',
        \ '#+begin_export python', 'def bye():', '#+end_export', 'Then src_python[:exports code]{def hi(): pass} too.',
        \ '#+begin_src emacs-lisp', '(defun f () 1)', '#+end_src',
        \ '#+begin_example', '#+begin_src python', 'def no():', '#+end_src', '#+end_example'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  let Syntax = {line, col -> join(map(synstack(line, col), 'synIDattr(v:val, "name")'))}
  " The code of a source block in its language's syntax, in a region where
  " the server says the code is (`org-lsp/codeBlocks`).
  call s:until('the code in its syntax', {-> Syntax(3, 1) =~# 'orgLspSrcCode_python python'})
  call assert_notmatch('python', Syntax(2, 3))
  call assert_notmatch('python', Syntax(5, 1))
  " Export blocks, inline source blocks and Emacs Lisp too.
  call assert_match('python', Syntax(7, 1))
  call assert_match('python', Syntax(9, 32))
  call assert_notmatch('python', Syntax(9, 30))
  call assert_notmatch('python', Syntax(9, 50))
  call assert_match('lispDecl', Syntax(11, 2))
  " Within the blocks of a syntax for Org whose regions take the whole
  " block, as Vim's own `syntax/org.vim` has them.
  syntax region orgCodeBlock start="\c^\s*#+BEGIN_SRC.*" end="\c^\s*#+END_SRC" keepend
  doautocmd TextChanged
  call s:until('the code within the block of the syntax for Org', {-> Syntax(3, 1) =~# 'orgCodeBlock orgLspSrcCode_python python'})
  syntax clear orgCodeBlock
  " What an example block holds is no block.
  call assert_notmatch('python', Syntax(15, 1))
  " The regions move with the code, and go with it.
  call append(0, 'Moved down.')
  doautocmd TextChanged
  call s:until('the region moved', {-> Syntax(4, 1) =~# 'python' && Syntax(3, 1) !~# 'python'})
  call deletebufline('%', 3, 5)
  doautocmd TextChanged
  call s:until('the region gone with its block', {-> Syntax(3, 1) !~# 'python' && Syntax(5, 1) =~# 'python'})
  call append(2, ['#+begin_src python :results output', 'def greet():', '#+end_src'])
  call deletebufline('%', 1)
  doautocmd TextChanged
  call s:until('the block back', {-> Syntax(3, 1) =~# 'python' && Syntax(7, 1) =~# 'python'})
  " No regions with `src_highlight` off.
  let g:org_lsp = extend(get(g:, 'org_lsp', {}), {'src_highlight': 0})
  call setline(5, 'See [[*Code]]. ')
  doautocmd TextChanged
  call s:until('the regions gone', {-> Syntax(3, 1) !~# 'python' && Syntax(7, 1) !~# 'python'})
  let g:org_lsp.src_highlight = 1
  call setline(5, 'See [[*Code]].')
  doautocmd TextChanged
  call s:until('the regions back', {-> Syntax(3, 1) =~# 'python'})
  " `<Leader>oo` follows the link at the cursor.
  call cursor(5, 8)
  call feedkeys("\\oo", 'xt')
  call s:until('the cursor on the headline', {-> line('.') == 1})
  " `<Leader>ona` opens a buffer for the note, and writes it in the entry.
  call feedkeys("\\ona", 'xt')
  call s:until('the note buffer', {-> fnamemodify(bufname(), ':t') =~# '^CAPTURE-'})
  call setline(3, 'Looked at it')
  call feedkeys("\<C-c>", 'x')
  call s:until('the note in the entry', {-> index(getbufline(file, 1, '$'), '  Looked at it') >= 0})
  silent! execute 'buffer' bufnr(file)
  silent write
endfunction
call s:test('source blocks in their syntax, links followed, notes added', function('s:code_link_note'))

" A file name that is not ASCII: its URI has the bytes of its UTF-8 name,
" as the clients and the server write it, and comes back as the name.
function! s:non_ascii_name() abort
  call mkdir(s:work . '/names', 'p')
  let file = s:work . '/names/café 日記.org'
  call writefile(['* TODO Café'], file)
  execute 'edit' fnameescape(file)
  let uri = org_lsp#uri(bufnr())
  let expected = 'file://' . s:work . '/names/caf%C3%A9%20%E6%97%A5%E8%A8%98.org'
  " ALE writes the hex digits in lower case.
  call assert_equal($ORG_LSP_TEST_CLIENT ==# 'ale' ? substitute(expected, '%\x\x', '\L&', 'g') : expected, uri)
  call assert_equal(file, org_lsp#uri_to_path(uri))
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call s:until('highlighting', {-> index(s:types(1), 'org_lsp_stars.level1') >= 0})
  call cursor(1, 3)
  call feedkeys('cit', 'x')
  call s:until('DONE', {-> getline(1) ==# '* DONE Café'})
  silent write
  execute 'buffer' s:work_buf
endfunction
call s:test('a file whose name is not ASCII', function('s:non_ascii_name'))

function! s:virtual_indent() abort
  let file = s:work . '/notes/indented.org'
  call writefile(['#+STARTUP: indent', '* A', 'text', '** B and a title that wraps', '- item'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  " `org-indent-mode`: text before the lines, the file as it is.
  call s:until('the prefixes', {-> !empty(prop_type_get('org_lsp_indent')) && !empty(prop_list(3, {'types': ['org_lsp_indent']}))})
  call assert_equal('  ', prop_list(3, {'types': ['org_lsp_indent']})[0].text)
  call assert_equal('    ', prop_list(5, {'types': ['org_lsp_indent']})[0].text)
  call assert_equal('text', getline(3))
  " The leading star of `** B` hidden.
  call assert_equal(1, prop_list(4, {'types': ['org_lsp_star']})[0].length)
  call assert_true(&breakindent)
  " A headline that wraps goes on under its title, after its stars and a
  " space (`wrap-prefix`, without the prefix).
  vsplit
  setlocal wrap linebreak
  let off = getwininfo(win_getid())[0].textoff
  execute 'vertical resize' 24 + off
  call cursor(1, 1)
  redraw
  let row = screenpos(win_getid(), 4, 1).row
  let left = win_screenpos(0)[1] + off
  let Shown = {r -> join(map(range(left, left + 23), {_, c -> screenstring(r, c)}), '')}
  call assert_equal(['  * B and a title that  ', '   wraps                '], [Shown(row), Shown(row + 1)])
  close
endfunction
call s:test('org-indent-mode shows the file indented, unchanged', function('s:virtual_indent'))

" The first 14 columns of `line` as the screen shows them, redrawn.
function! s:shown(line) abort
  doautocmd <nomodeline> CursorMoved
  redraw
  let row = screenpos(win_getid(), a:line, 1).row
  return join(map(range(1, 14), {_, c -> screenstring(row, c)}), '')
endfunction

" What Org hides is concealed: a link's brackets and target, and an
" entity shows as its character (`org-lsp/display`). The cursor's line
" shows as it is.
function! s:conceals() abort
  let file = s:work . '/notes/concealed.org'
  call writefile(['#+STARTUP: entitiespretty', 'See [[https://orgmode.org][Org]] and \alpha.', 'last'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(3, 1)
  call s:until('the link and the entity concealed', {-> s:shown(2) ==# 'See Org and α.'})
  call cursor(2, 1)
  call assert_equal('See [[https://', s:shown(2))
  call cursor(3, 1)
  call assert_equal('See Org and α.', s:shown(2))
  " Concealed on the cursor's line too in Normal mode ('concealcursor').
  setlocal concealcursor=n
  call cursor(2, 1)
  call assert_equal('See Org and α.', s:shown(2))
  setlocal concealcursor=
  bwipeout!
  execute 'buffer' s:work_buf
endfunction
call s:test('conceals what Org hides, but on the cursor line', function('s:conceals'))

" A command's edit (the TODO keyword cycled) is highlighted as soon as
" the server has it, not when Vim next sees the change ('updatetime').
function! s:highlights_edits() abort
  let file = s:work . '/notes/cycled.org'
  call writefile(['* TODO Cycle me'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call s:until('highlighting', {-> index(s:types(1), 'org_lsp_todoKeyword') >= 0})
  call cursor(1, 3)
  let start = reltime()
  call org_lsp#run('org.todo.next')
  call s:until('DONE highlighted', {-> getline(1) =~# 'DONE' && index(s:types(1), 'org_lsp_todoKeyword.done') >= 0}, 2000)
  call assert_true(reltimefloat(reltime(start)) < 2.0)
  bwipeout!
  execute 'buffer' s:work_buf
endfunction
call s:test('highlights the edit of a command at once', function('s:highlights_edits'))

" Indentation by the server: a line opened below a list item starts at
" the item's text, as Org's RET has it, and `==` re-indents a line as
" `org-indent-region` does.
function! s:indent() abort
  let file = s:work . '/notes/indent.org'
  call writefile(['* Head', '- item', '     stray'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call assert_equal('org_lsp#indent_expr()', &l:indentexpr)
  " coc.nvim follows a buffer's changes a moment after it opens.
  sleep 300m
  call cursor(2, 1)
  call feedkeys("obody\<Esc>", 'xt')
  call assert_equal('  body', getline(3), 'a new line in a list item starts at its text')
  call cursor(4, 1)
  call feedkeys('==', 'xt')
  call s:until('the stray line re-indented', {-> getline(4) ==# '  stray'})
  silent write
endfunction
call s:test('indents new lines and = as Org does', function('s:indent'))

" Column view (`org-columns`, through Emacs): the headlines covered by
" their columns, the header in the status line; `l` picks the next column,
" `n` its next allowed value; `q` takes the view away.
function! s:columns() abort
  let file = s:work . '/notes/columned.org'
  call writefile(['#+COLUMNS: %25ITEM %TODO', '* Project', '** Task'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(1, 1)
  call org_lsp#run('org.columns')
  call s:until('column view', {-> org_lsp#columns_on(bufnr())})
  call s:until('the headline covered by its columns', {-> trim(org_lsp#columns_shown(bufnr(), 2)) ==# '** Task                   |      |'})
  call assert_match('ITEM                      | TODO |', &l:statusline)
  call cursor(3, 1)
  call feedkeys('l', 'x')
  call assert_equal(1, b:org_lsp_column, 'l picks the next column')
  call feedkeys('n', 'x')
  call s:until('the next TODO state', {-> getline(3) ==# '** TODO Task'})
  call s:until('the view following it', {-> trim(org_lsp#columns_shown(bufnr(), 2)) ==# '** Task                   | TODO |'})
  call feedkeys('q', 'x')
  call s:until('column view off', {-> !org_lsp#columns_on(bufnr())})
  call assert_equal([], prop_list(1, {'end_lnum': -1, 'types': ['org_lsp_columns']}), 'nothing covers the headlines')
  call assert_equal('', maparg('q', 'n'), 'q as Vim has it')
  silent write
  " In the agenda (`org-agenda-columns`): each entry's line covered by its
  " columns; `q` gives the agenda its own keys back.
  call org_lsp#run('org.agenda.todos')
  call s:until('the TODO list', {-> &filetype ==# 'org-agenda' && search('TODO Task$', 'nw') > 0})
  let agenda = bufnr()
  let task = search('TODO Task$', 'nw')
  call cursor(task, 1)
  call org_lsp#run('org.agenda.columns')
  call s:until('column view in the agenda', {-> org_lsp#columns_on(agenda)})
  call s:until('the entry covered by its columns', {-> trim(org_lsp#columns_shown(agenda, task - 1)) =~# '^Task\s\+| TODO |'})
  call feedkeys('q', 'x')
  call s:until('column view off in the agenda', {-> !org_lsp#columns_on(agenda)})
  " `q` closes the agenda again.
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> &filetype !=# 'org-agenda'})
endfunction
if executable('emacs')
  call s:test('column view covers the headlines with their columns', function('s:columns'))
endif

" The org-roam buffer opens beside the file, the cursor staying there, with
" the node's backlinks; it follows the cursor to another node; the command
" again hides it.
function! s:roam_buffer() abort
  let dir = s:work . '/roam'
  call mkdir(dir, 'p')
  call writefile([':PROPERTIES:', ':ID:       a-id', ':END:', '#+title: A note', '', 'Text.'], dir . '/a.org')
  call writefile([':PROPERTIES:', ':ID:       b-id', ':END:', '#+title: B note', '', 'See [[id:a-id][A]].'], dir . '/b.org')
  execute 'edit' fnameescape(dir . '/b.org')
  execute 'edit' fnameescape(dir . '/a.org')
  let here = win_getid()
  call s:until('the server', {-> !empty(org_lsp#adapter()) && org_lsp#ready(bufnr())})
  call org_lsp#run('org.roam.buffer.toggle')
  call s:until('the org-roam buffer', {-> org_lsp#roam_buffer_shown()})
  call assert_equal(here, win_getid(), 'the cursor stays in the file')
  let roam = bufnr('org-roam-buffer.org')
  call s:until('its backlinks', {-> index(getbufline(roam, 1, '$'), '* Backlinks (1)') >= 0})
  " `CursorMoved` comes from the main loop, which a test does not run.
  execute 'buffer' fnameescape(dir . '/b.org')
  call cursor(6, 1)
  call org_lsp#tell_roam_cursor()
  call s:until('the other node', {-> getbufline(roam, 1)[0] ==# '#+title: B note'})
  call org_lsp#run('org.roam.buffer.toggle')
  call s:until('the buffer hidden', {-> !org_lsp#roam_buffer_shown()})
endfunction
call s:test('the org-roam buffer shows the backlinks of the node', function('s:roam_buffer'))

" org-journal: today's entry in a file whose name has no extension, an Org
" file all the same; the TODO of the day before comes along, its text
" folded as org-journal folds it.
function! s:journal() abort
  let dir = s:work . '/journal'
  call mkdir(dir, 'p')
  call writefile(['* Sunday, 05/17/26', '** 09:00 standup', '** TODO carried', 'a note'], dir . '/20260517')
  execute 'edit' fnameescape(s:work . '/notes/work.org')
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call s:until('the journal directory', {-> org_lsp#journal_dir() ==# dir . '/'})
  call org_lsp#run('org.journal.new.entry')
  call s:until("today's file", {-> fnamemodify(bufname(), ':p') ==# dir . '/20260518'})
  call assert_equal('org', &filetype, 'an Org file')
  call s:until('the server on it', {-> org_lsp#ready(bufnr())})
  call assert_equal(['* Monday, 05/18/26', '** TODO carried', 'a note', '** 10:30 '], getline(1, '$'))
  call s:until('the TODO folded', {-> foldclosed(2) == 2})
  call assert_equal(-1, foldclosed(1), "the date's headline open")
endfunction
call s:test('org-journal makes an entry in a file without the extension', function('s:journal'))

" Text a command asks for is typed with `input()` (the tests answer the
" others): completed from the list, the default there, <Esc> no answer.
" Prompts that are Emacs's own words show as they are, titles with `: `.
function! s:typed_answers() abort
  call assert_equal('Owner: ', org_lsp#prompt_text('Owner'))
  call assert_equal('Execute this? ', org_lsp#prompt_text('Execute this? '))
  call assert_equal('Tags :work: ', org_lsp#prompt_text('Tags :work:'))
  call feedkeys("al\<Tab>\<CR>", 't')
  call assert_equal('alpha', org_lsp#read_text({'prompt': 'Owner', 'value': '', 'completions': ['alpha', 'beta']}))
  call feedkeys("\<CR>", 't')
  call assert_equal('1:00', org_lsp#read_text({'prompt': 'Effort', 'value': '1:00'}))
  call feedkeys("x\<Esc>", 't')
  call assert_equal(v:null, org_lsp#read_text({'prompt': 'Owner', 'value': ''}))
  call assert_equal('', maparg('<Esc>', 'c'), 'the mapping for <Esc> gone')
endfunction
call s:test('reads typed answers with completion, and <Esc> cancels', function('s:typed_answers'))

" The server's messages come as `org-lsp/message`, which the plugin shows
" as they are, whatever the client; Emacs's questions are asked by the
" plugin too.
function! s:messages() abort
  call assert_true(org_lsp#init_options().message_notifications, 'messages as notifications')
  call assert_false(org_lsp#init_options().message_requests, 'no message requests')
  " (The tests run silent, which keeps messages out of the history.)
  unsilent call org_lsp#handle_notification('org-lsp/message', {'type': 3, 'message': 'Sum of 2 items: 5'})
  call assert_equal('Sum of 2 items: 5', split(execute('messages'), "\n")[-1])
endfunction
call s:test("shows the server's messages as they are", function('s:messages'))

" TAB at the end of a new, empty headline cycles its level, again and again
" (`org-cycle-level`), from the blank the cursor is on in Normal mode.
function! s:cycle_level() abort
  let file = s:work . '/notes/level.org'
  call writefile(['* A', '** '], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(2, 3)
  call org_lsp#cycle()
  call s:until('a level up', {-> getline(2) ==# '* '})
  call org_lsp#cycle()
  call s:until('a level down again', {-> getline(2) ==# '** '})
  silent! write
  execute 'buffer' s:work_buf
endfunction
call s:test('TAB on a new headline cycles its level', function('s:cycle_level'))

" `:Org` takes a range, as a selection of whole lines: `:2,3Org meta right`
" demotes both headlines. `:Org agenda <Tab>` completes the next word.
function! s:org_range() abort
  let file = s:work . '/notes/range.org'
  call writefile(['* C', '* D', '* E'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  2,3Org meta right
  call s:until('both demoted', {-> getline(1, '$') == ['* C', '** D', '** E']})
  let words = org_lsp#complete('', 'Org agenda ', 11)
  call assert_true(index(words, 'list') >= 0 && index(words, 'dispatch') >= 0, string(words))
  call assert_equal(['list'], org_lsp#complete('li', 'Org agenda li', 13))
  silent! write
  execute 'buffer' s:work_buf
endfunction
call s:test(':Org takes a range and completes word by word', function('s:org_range'))

" A closed fold's line has blanks after its text, not Vim's dashes, and
" CTRL-Space works in a terminal, which sends it as NUL.
function! s:terminal_keys() abort
  call assert_match('fold: ', &l:fillchars)
  call assert_notmatch('fold:-', &l:fillchars)
  call assert_match('^org_lsp#repeatable', maparg('<C-@>', 'n'))
  call assert_notequal('', maparg('<C-@>', 'n'))
endfunction
call s:test('fills folds with blanks and binds CTRL-Space as NUL', function('s:terminal_keys'))

" A buffer read again from its file (`:e!`) is what the server has after.
function! s:reread() abort
  let file = s:work . '/notes/reread.org'
  call writefile(['* One'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call writefile(['* One', '* Two'], file)
  edit!
  let nr = bufnr()
  let Names = {-> map(copy(get(org_lsp#request_sync(nr, 'textDocument/documentSymbol',
        \ {'textDocument': {'uri': org_lsp#uri(nr)}}, 2000), 'result', [])), 'v:val.name')}
  call s:until('the new heading in the server', {-> Names() == ['One', 'Two']})
  execute 'buffer' s:work_buf
endfunction
call s:test('a buffer read again is told to the server', function('s:reread'))

for error in v:errors
  call add(s:results, 'not ok - ' . error)
endfor
" Written whole, then renamed: the runner stops Vim as soon as the file is
" there.
call writefile(s:results, $ORG_LSP_TEST_RESULT . '.part')
call rename($ORG_LSP_TEST_RESULT . '.part', $ORG_LSP_TEST_RESULT)
qa!
