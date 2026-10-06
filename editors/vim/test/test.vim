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
function! org_lsp#read_key(prompt, choices) abort
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

function! s:folds() abort
  call s:until('folds', {-> !empty(get(b:, 'org_lsp_folds', []))})
  call cursor(2, 1)
  call org_lsp#cycle()
  call assert_equal(2, foldclosed(2))
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

function! s:deadline() abort
  let g:answers = ['2026-05-25']
  call cursor(6, 3)
  call org_lsp#run('org.deadline')
  call s:until('the new deadline', {-> getline(7) ==# 'DEADLINE: <2026-05-25 Mon>'})
  silent write
endfunction
call s:test('asks for a date and sets the deadline', function('s:deadline'))

function! s:agenda_window() abort
  for win in range(1, winnr('$'))
    if getbufvar(winbufnr(win), '&filetype') ==# 'org-agenda'
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
  let first = getline(1)
  call feedkeys('f', 'x')
  call s:until('the next week', {-> getline(1) !=# first && getline(1) =~# '^Week-agenda'})
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
  call assert_equal(s:work_buf, bufnr())
endfunction
call s:test('shows the agenda, moves through it, and closes it', function('s:agenda'))

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

function! s:return() abort
  let file = s:work . '/notes/return.org'
  call writefile(['* TODO Write the report :work:', '| a | bb |'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  call cursor(1, 12)
  call feedkeys("a\<CR>x\<Esc>", 'xt')
  call s:until('the split', {-> getline(2) ==# 'xthe report'})
  call assert_equal('* TODO Write' . repeat(' ', 59) . ':work:', getline(1))
  call cursor(3, 7)
  call feedkeys("a\<CR>y\<Esc>", 'xt')
  call s:until('the new row', {-> getline(4) ==# '|   | y   |'})
  silent write
endfunction
call s:test('RET in insert mode is as in Org', function('s:return'))

function! s:meta_return() abort
  let file = s:work . '/notes/meta.org'
  call writefile(['* TODO Write :work:', '- one', '** Sub', '* Next'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  " After the character under the cursor: a new item after the last one.
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
endfunction
call s:test('M-RET and its neighbours insert headings and items', function('s:meta_return'))

function! s:help() abort
  execute 'buffer' s:work_buf
  let popup = org_lsp#help()
  call assert_true(popup > 0, 'a popup')
  let shown = getbufline(winbufnr(popup), 1, '$')
  call assert_equal('Org file', shown[0])
  call assert_true(index(shown, '  cit           Change the TODO state') >= 0, string(shown))
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

for error in v:errors
  call add(s:results, 'not ok - ' . error)
endfor
" Written whole, then renamed: the runner stops Vim as soon as the file is
" there.
call writefile(s:results, $ORG_LSP_TEST_RESULT . '.part')
call rename($ORG_LSP_TEST_RESULT . '.part', $ORG_LSP_TEST_RESULT)
qa!
