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
  " `win_split_size`: lines for a split. (coc.nvim opens documents where it
  " opens them.)
  if $ORG_LSP_TEST_CLIENT ==# 'coc'
    return
  endif
  let g:org_lsp.win_split_size = 6
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda of 6 lines', {-> s:agenda_window() != 0 && winheight(s:agenda_window()) == 6})
  call win_gotoid(s:agenda_window())
  call feedkeys('q', 'x')
  call s:until('the agenda closed', {-> s:agenda_window() == 0})
  call remove(g:org_lsp, 'win_split_size')
endfunction
call s:test('shows the agenda, moves through it, and closes it', function('s:agenda'))

" A hover over an entry, as the LSP client shows one, has its headline and
" its text.
function! s:agenda_hover() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:agenda_window() != 0})
  call win_gotoid(s:agenda_window())
  call s:until('its text', {-> search('Write report', 'n') > 0})
  call cursor(search('Write report', 'n'), 30)
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
  " coc.nvim opens documents where it opens them.
  if $ORG_LSP_TEST_CLIENT ==# 'coc'
    return
  endif
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
  call writefile(['* A', 'an apple', '* B', 'nothing', '** C', 'Apple pie', '* D', 'none'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server and the folds', {-> org_lsp#ready(bufnr()) && foldlevel(2) > 0})
  let g:answers = ['r', 'apple']
  call feedkeys("\\o/", 'xt')
  let Marked = {line -> empty(prop_type_get('org_lsp_sparse')) ? 0 : len(prop_list(line, {'types': ['org_lsp_sparse']}))}
  call s:until('the matches highlighted', {-> Marked(2) == 1 && Marked(6) == 1})
  call assert_equal([-1, -1, 7], [foldclosed(2), foldclosed(6), foldclosed(8)])
  normal! zR
endfunction
call s:test('makes a sparse tree', function('s:sparse'))

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

function! s:code_link_note() abort
  let file = s:work . '/notes/code.org'
  call writefile(['* Code', '#+begin_src python :results output', 'def greet():', '#+end_src', 'See [[*Code]].',
        \ '#+begin_export python', 'def bye():', '#+end_export', 'Then src_python[:exports code]{def hi(): pass} too.',
        \ '#+begin_src emacs-lisp', '(defun f () 1)', '#+end_src'], file)
  execute 'edit' fnameescape(file)
  call s:until('the server', {-> org_lsp#ready(bufnr())})
  " The code of a source block in its language's syntax.
  call assert_match('^python', synIDattr(synID(3, 1, 1), 'name'))
  call assert_notmatch('^python', synIDattr(synID(5, 1, 1), 'name'))
  " Export blocks, inline source blocks and Emacs Lisp too.
  call assert_match('^python', synIDattr(synID(7, 1, 1), 'name'))
  call assert_match('^python', synIDattr(synID(9, 32, 1), 'name'))
  call assert_notmatch('^python', synIDattr(synID(9, 50, 1), 'name'))
  call assert_match('^lisp', synIDattr(synID(11, 2, 1), 'name'))
  " `<Leader>oo` follows the link at the cursor.
  call cursor(5, 8)
  call feedkeys("\\oo", 'xt')
  call s:until('the cursor on the headline', {-> line('.') == 1})
  " `<Leader>ona` asks for a note and writes it in the entry.
  let g:answers = ['Looked at it']
  call feedkeys("\\ona", 'xt')
  call s:until('the note in the entry', {-> index(getbufline(file, 1, '$'), '  Looked at it') >= 0})
  silent! execute 'buffer' bufnr(file)
  silent write
endfunction
call s:test('source blocks in their syntax, links followed, notes added', function('s:code_link_note'))

function! s:virtual_indent() abort
  let file = s:work . '/notes/indented.org'
  call writefile(['#+STARTUP: indent', '* A', 'text', '** B', '- item'], file)
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
endfunction
call s:test('org-indent-mode shows the file indented, unchanged', function('s:virtual_indent'))

for error in v:errors
  call add(s:results, 'not ok - ' . error)
endfor
" Written whole, then renamed: the runner stops Vim as soon as the file is
" there.
call writefile(s:results, $ORG_LSP_TEST_RESULT . '.part')
call rename($ORG_LSP_TEST_RESULT . '.part', $ORG_LSP_TEST_RESULT)
qa!
