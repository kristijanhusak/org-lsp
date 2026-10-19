" Highlighting on screen, through one LSP client: for each look of the
" manifest's `highlights` table, a text property of its type in the shared
" sample (tests/e2e/highlight: an Org file, the week's agenda, the TODO list
" and the date prompt) where the server put a token it applies to, its
" group linked to a group that the default colorscheme defines, or for a
" habit's days with Org's background, which shows there. run.sh
" sources it as vimrc sources test.vim (ORG_LSP_TEST_FILE); it writes
" TAP-like lines to $ORG_LSP_TEST_RESULT.
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

" The fixture, which vimrc copied there and took the server's options from.
let s:dir = s:work . '/highlight'

" The looks still to find, and what was wrong with those found.
let s:remaining = []
let s:failures = []
let s:checked = 0

" The group a highlight group shows as, after its links.
function! s:resolved(group) abort
  return synIDattr(synIDtrans(hlID(a:group)), 'name')
endfunction

" Whether a group shows anything with the default colorscheme.
function! s:defined(group) abort
  let found = hlget(a:group, v:true)
  if empty(found) || get(found[0], 'cleared', v:false)
    return v:false
  endif
  return !empty(filter(copy(found[0]), {key -> index(['term', 'cterm', 'ctermfg', 'ctermbg', 'gui', 'guifg', 'guibg'], key) >= 0}))
endfunction

" Check the looks still to check on what buffer `nr` shows.
function! s:check_looks(nr, where) abort
  for look in copy(s:remaining)
    let type = 'org_lsp_' . look.token
    let prop = {}
    for lnum in range(1, len(getbufline(a:nr, 1, '$')))
      let found = prop_list(lnum, {'bufnr': a:nr, 'types': [type]})
      " Of a look with a background, a place where it shows.
      if has_key(look, 'background')
        call filter(found, {_, p -> s:background_at(a:nr, lnum, p.col) ==# type})
      endif
      if !empty(found)
        let prop = extend(found[0], {'lnum': lnum})
        break
      endif
    endfor
    if empty(prop)
      continue
    endif
    call filter(s:remaining, {_, other -> other.token !=# look.token})
    let s:checked += 1
    let text = getbufline(a:nr, prop.lnum)[0][prop.col - 1 : prop.col + prop.length - 2]
    let place = printf('%s line %d %s', a:where, prop.lnum, string(text))
    let group = prop_type_get(type).highlight
    let target = s:resolved(group)
    if has_key(look, 'background') && target ==# group
      " Org's background (a habit's days), over the other properties there.
      let wanted = get(look.background, &background)
      let shown = s:background_at(a:nr, prop.lnum, prop.col)
      if get(get(hlget(group), 0, {}), 'guibg', '') !=? wanted
        call add(s:failures, printf('%s: %s has not the background of Org, %s, at %s', look.token, group, wanted, place))
      elseif shown !=# type
        call add(s:failures, printf('%s: %s shows the background of %s, at %s', look.token, group, shown, place))
      endif
    elseif target ==# group || empty(target)
      call add(s:failures, printf('%s: %s links to nothing, at %s', look.token, group, place))
    elseif !s:defined(target)
      call add(s:failures, printf('%s: %s (%s) has no definition, at %s', look.token, group, target, place))
    endif
  endfor
endfunction

" The property type whose background shows at a place: of those with one
" there, the one of the highest priority.
function! s:background_at(nr, lnum, col) abort
  let best = ['', -1]
  for found in prop_list(a:lnum, {'bufnr': a:nr})
    let definition = prop_type_get(found.type)
    let group = s:resolved(get(definition, 'highlight', ''))
    if found.col <= a:col && a:col < found.col + found.length && !empty(get(get(hlget(group), 0, {}), 'guibg', ''))
          \ && get(definition, 'priority', 0) > best[1]
      let best = [found.type, get(definition, 'priority', 0)]
    endif
  endfor
  return best[0]
endfunction

function! s:window(filetype) abort
  for win in range(1, winnr('$'))
    if getbufvar(winbufnr(win), '&filetype') ==# a:filetype
      return win_getid(win)
    endif
  endfor
  return 0
endfunction

" The looks Vim has a group for: those with a text property type.
function! s:sample() abort
  execute 'edit' fnameescape(s:dir . '/sample.org')
  let s:sample_buf = bufnr()
  call s:until('the server', {-> org_lsp#ready(s:sample_buf)})
  call s:until('highlighting', {-> !empty(prop_type_get('org_lsp_headline.level1')) && !empty(prop_list(6, {'types': ['org_lsp_headline.level1']}))})
  for look in org_lsp#manifest().highlights
    if !empty(prop_type_get('org_lsp_' . look.token))
      call add(s:remaining, look)
    endif
  endfor
  let s:looks = len(s:remaining)
  call assert_true(s:looks > 70, 'the looks with a group: ' . s:looks)
  call s:check_looks(s:sample_buf, 'sample.org')
endfunction
call s:test('highlights the Org file', function('s:sample'))

function! s:agenda() abort
  call org_lsp#run('org.agenda.list')
  call s:until('the agenda', {-> s:window('org-agenda') != 0})
  call win_gotoid(s:window('org-agenda'))
  call s:until('its text', {-> getline(1) =~# '^Week-agenda'})
  let nr = bufnr()
  call s:until('its highlighting', {-> !empty(prop_list(2, {'bufnr': nr, 'types': ['org_lsp_agendaDate.today']}))})
  call s:check_looks(nr, 'the agenda')
  " The line of the clocked entry, highlighted from edge to edge.
  let line = search('Clocked in', 'n')
  let Placed = {-> map(sign_getplaced(nr, {'group': 'org_lsp_lines'})[0].signs, '[v:val.lnum, v:val.name]')}
  call s:until('the clocked line', {-> Placed() == [[line, 'org_lsp_line_agendaClocking']]})
  call assert_true(s:defined(s:resolved(sign_getdefined('org_lsp_line_agendaClocking')[0].linehl)), 'the clocked line has a color')
  call org_lsp#run('org.agenda.todos')
  call s:until('the TODO list', {-> getline(1) =~# '^Global list'})
  call s:until('its highlighting', {-> !empty(prop_list(search('Clocked in', 'n'), {'bufnr': nr, 'types': ['org_lsp_agendaClocking']}))})
  call s:check_looks(nr, 'the TODO list')
endfunction
call s:test('highlights the agenda and the TODO list', function('s:agenda'))

function! s:calendar() abort
  only
  execute 'buffer' s:sample_buf
  call cursor(search('^\* Headline level one', 'n'), 1)
  call org_lsp#run('org.schedule')
  call s:until('the calendar', {-> s:window('org-date') != 0})
  let nr = winbufnr(s:window('org-date'))
  call s:until('its highlighting', {-> !empty(prop_list(1, {'bufnr': nr, 'types': ['org_lsp_calendarHeader']}))})
  call s:check_looks(nr, 'the date prompt')
endfunction
call s:test('highlights the date prompt', function('s:calendar'))

function! s:all() abort
  for look in s:remaining
    call add(s:failures, look.token . ': no text property of its type')
  endfor
  if !empty(s:failures)
    throw printf('%d of %d looks: %s', len(s:failures), s:looks, join(s:failures, '; '))
  endif
  call assert_equal(s:looks, s:checked)
endfunction
call s:test('every look shows in a defined group', function('s:all'))

for error in v:errors
  call add(s:results, 'not ok - ' . error)
endfor
call writefile(s:results, $ORG_LSP_TEST_RESULT . '.part')
call rename($ORG_LSP_TEST_RESULT . '.part', $ORG_LSP_TEST_RESULT)
qa!
