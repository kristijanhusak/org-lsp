" The shared end-to-end scenarios (tests/e2e/scenarios/*.json) in Vim, by
" pressing the keys the plugin maps for each step's command, through one
" LSP client ($ORG_LSP_TEST_CLIENT).
"
" vimrc sources this file when $ORG_LSP_TEST_SCENARIO names a scenario,
" before the plugins load, so that the server starts with the scenario's
" options; the steps run once Vim has started. run.sh runs each scenario
" in a Vim of its own and reads the line written to $ORG_LSP_TEST_RESULT.
" What a scenario expects is what the server's own runner saw
" (server/org-lsp/tests/e2e_scenarios.rs, tests/e2e/expected/NAME.txt), as
" in Neovim (tests/nvim/scenarios.lua). A command with no key in Vim
" fails, unless tests/e2e/exceptions.json says why under `editors.vim`.

let s:here = expand('<sfile>:p:h')
let s:e2e = fnamemodify(s:here, ':h:h:h') . '/tests/e2e'
let s:name = $ORG_LSP_TEST_SCENARIO
let s:client = $ORG_LSP_TEST_CLIENT

function! s:json(path) abort
  return json_decode(join(readfile(a:path), "\n"))
endfunction

let s:spec = s:json(s:e2e . '/scenarios/' . s:name . '.json')
let s:excepted = get(get(s:json(s:e2e . '/exceptions.json'), 'editors', {}), 'vim', {})

" Steps a client cannot take as the others do, by client, scenario and
" step (from 1), with the reason: the scenario stops before them.
let s:skips = {}

" coc.nvim opens the files it edits that are not loaded
" (`workspace.openResourceCommand`), and Vim tells of each: with the
" command's own message, more than the command line holds, which waits for
" a key.
if s:client ==# 'coc'
  set shortmess+=F
endif

" The scenario's files, in a directory of their own.
let s:dir = resolve($ORG_LSP_TEST_WORK) . '/files'
call mkdir(s:dir, 'p')
let s:files = {}
if has_key(s:spec, 'files_from')
  let s:from = fnamemodify(s:here, ':h:h:h') . '/tests/' . s:spec.files_from
  for s:path in glob(s:from . '/*.org', 0, 1) + glob(s:from . '/*.org_archive', 0, 1)
    let s:files[fnamemodify(s:path, ':t')] = readfile(s:path, 'b')
  endfor
endif
for [s:file, s:text] in items(get(s:spec, 'files', {}))
  let s:files[s:file] = type(s:text) == v:t_list ? s:text + [''] : split(s:text, "\n", 1)
endfor
for [s:file, s:lines] in items(s:files)
  call mkdir(fnamemodify(s:dir . '/' . s:file, ':h'), 'p')
  call writefile(s:lines, s:dir . '/' . s:file, 'b')
endfor

" `value` with `$DIR` in its strings replaced by the directory.
function! s:substitute(value) abort
  if type(a:value) == v:t_string
    return substitute(a:value, '\$DIR', escape(s:dir, '\&'), 'g')
  elseif type(a:value) == v:t_list
    return map(copy(a:value), {_, item -> s:substitute(item)})
  elseif type(a:value) == v:t_dict
    return map(copy(a:value), {_, item -> s:substitute(item)})
  endif
  return a:value
endfunction

" The options of the server's runner: the clock and the agenda files, then
" the scenario's.
let s:settings = {
      \ 'clock': get(s:spec, 'clock', {'today': '2026-05-18', 'now': '10:30'}),
      \ 'org_agenda_files': [s:dir],
      \ 'org_directory': s:dir,
      \ 'org_default_notes_file': s:dir . '/notes.org',
      \ }
call extend(s:settings, s:substitute(get(s:spec, 'options', {})))
if get(s:spec, 'emacs', v:false) && !empty($ORG_LISP_DIR)
  let s:settings.emacs_config = printf('(add-to-list ''load-path "%s") ', escape($ORG_LISP_DIR, '\"'))
        \ . get(s:settings, 'emacs_config', '')
endif
let g:org_lsp = {'cmd': [$ORG_LSP_BIN], 'settings': s:settings}

" ----- running the steps

function! s:fail(message) abort
  throw 'scenario: ' . a:message
endfunction

function! s:until(what, Probe, ...) abort
  let timeout = get(a:, 1, 15000)
  let waited = 0
  while !a:Probe()
    if waited >= timeout
      call s:fail('timed out waiting for ' . a:what)
    endif
    sleep 20m
    let waited += 20
    call feedkeys('', 'x')
  endwhile
endfunction

" What the server said, as the user sees it: the messages since they were
" last looked at.
function! s:messages() abort
  " Not Vim's own about the files it reads (`"Agenda.org-agenda" 3L, 70B`)
  " and the buffers it shows again (`… line 3 of 7 --42%-- col 1`, as
  " vim-lsp edits a file in the agenda's window and goes back), nor the
  " blank one it keeps when insert mode ends, nor coc.nvim's about the
  " files it edits.
  return filter(split(execute('messages'), "\n"), {_, line -> line !~# '^".*" .*\d\+L, \d\+B$'
        \ && line !~# '^".*" .*line \d\+ of \d\+ --\d\+%-- col ' && line !~# '^\s*$' && line !~# '^\[coc\.nvim\] '})
endfunction

" A buffer the server knows, to ask it through.
function! s:server_buffer() abort
  for nr in [bufnr()] + map(getbufinfo({'bufloaded': 1}), 'v:val.bufnr')
    if index(['org', 'org-agenda', 'org-date'], getbufvar(nr, '&filetype')) >= 0 && org_lsp#ready(nr)
      return nr
    endif
  endfor
  return -1
endfunction

" A request answered: the server has handled everything sent before it.
function! s:round_trip() abort
  let nr = s:server_buffer()
  if nr < 0
    return
  endif
  let answered = {'done': 0}
  call org_lsp#request(nr, 'org-lsp/test-sync', v:null, {... -> extend(answered, {'done': 1})})
  let waited = 0
  while !answered.done && waited < 5000
    sleep 5m
    let waited += 5
  endwhile
endfunction

let s:cursor = []

" Wait until the server has handled everything, and the plugin has done
" what it asked: nothing changes any more between two round trips. Keys
" the plugin feeds run meanwhile.
function! s:settle() abort
  let last = v:null
  for _ in range(300)
    call s:round_trip()
    call s:round_trip()
    sleep 20m
    call feedkeys('', 'x')
    " As the main loop tells of the cursor moved (coc.nvim follows it).
    let cursor = [bufnr(), getcurpos()]
    if cursor !=# s:cursor
      let s:cursor = cursor
      silent doautocmd <nomodeline> CursorMoved
    endif
    let state = [bufnr(), getcurpos(), len(s:messages()), mode(), winnr('$')]
          \ + map(getbufinfo({'bufloaded': 1}), '[v:val.bufnr, v:val.changedtick]')
    if state ==# last
      return
    endif
    let last = state
  endfor
endfunction

" A key as typed: the leader as it is set, `<...>` as the key it names.
function! s:typed(key) abort
  let key = substitute(a:key, '\c<Leader>', escape(get(g:, 'mapleader', '\'), '\&'), 'g')
  return eval('"' . substitute(escape(key, '"\'), '<', '\\<', 'g') . '"')
endfunction

function! s:press(keys) abort
  call feedkeys(s:typed(a:keys), 'mx')
endfunction

let s:agenda_name = ''
let s:prompt_name = ''

" How a buffer is named in the snapshots, as the server's runner names
" documents.
function! s:name_of(nr) abort
  let path = fnamemodify(bufname(a:nr), ':p')
  let tail = fnamemodify(path, ':t')
  if tail ==# s:agenda_name
    return 'agenda'
  elseif tail ==# s:prompt_name
    return 'date prompt'
  elseif stridx(path, s:dir . '/') == 0
    return path[len(s:dir) + 1 :]
  endif
  return tail
endfunction

function! s:agenda_buffer() abort
  for info in getbufinfo({'bufloaded': 1})
    if fnamemodify(info.name, ':t') ==# s:agenda_name
      return info.bufnr
    endif
  endfor
  return -1
endfunction

function! s:file_buffer(file) abort
  let nr = bufnr(fnameescape(s:dir . '/' . a:file))
  return nr > 0 && bufloaded(nr) ? nr : -1
endfunction

function! s:text_of(lines) abort
  return substitute(join(a:lines, "\n"), '\n*$', '', '') . "\n"
endfunction

" The scope of the current buffer's keys, and the scopes it falls back to:
" a capture buffer has the keys of Org files too, and the global keys work
" everywhere.
function! s:scopes() abort
  if org_lsp#columns_on(bufnr())
    return ['columns', &filetype ==# 'org-agenda' ? 'agenda' : 'org', 'global']
  elseif &filetype ==# 'org-agenda'
    return ['agenda', 'global']
  elseif &filetype ==# 'org-date'
    return ['calendar', 'global']
  elseif &filetype ==# 'org' && stridx(fnamemodify(bufname(), ':t'), s:capture_prefix) == 0
    return ['capture', 'org', 'global']
  elseif &filetype ==# 'org'
    return ['org', 'global']
  endif
  return ['global']
endfunction

" The key the plugin set for `command` where the cursor is, the first of
" its keys; each of them is checked to run it in the buffer.
function! s:key_of(command) abort
  for scope in s:scopes()
    for entry in org_lsp#keys(scope)
      if entry.command ==# a:command && !empty(entry.keys)
        for lhs in scope ==# 'global' ? [] : entry.keys
          let mapped = org_lsp#mapped_command(lhs)
          if mapped !=# a:command
            call s:fail(printf('`%s` is not mapped to %s in %s (%s)', lhs, a:command, scope, empty(mapped) ? 'unmapped' : mapped))
          endif
        endfor
        return entry.keys[0]
      endif
    endfor
  endfor
  return ''
endfunction

" What is asked is answered from the step's answers, in turn: keys, text
" typed in the input box, a choice from a list by its label. What goes
" wrong is said once the step is done: the prompt is asked from a timer.
let s:answers = []
let s:problem = ''

function! s:answer(input) abort
  if empty(s:answers)
    let s:problem = printf('`%s` is asked and the step has no answer left', a:input.prompt)
    return v:null
  endif
  let answer = remove(s:answers, 0)
  let label = type(answer) == v:t_dict ? answer.choice : answer
  if a:input.kind ==# 'key' && type(answer) != v:t_dict
    return answer
  endif
  let choices = get(a:input, 'choices', [])
  if !empty(choices)
    for exact in [1, 0]
      for choice in choices
        if exact ? choice.label ==# label : stridx(choice.label, label) >= 0
          return choice.key
        endif
      endfor
    endfor
    let s:problem = printf('no choice `%s` in %s', label, string(map(copy(choices), 'v:val.label')))
    return v:null
  endif
  return label
endfunction

" Column view, as the server last said it, in the buffer it shows in.
let s:columns_seen = 0
let s:columns_buffer = -1

function! s:columns_text() abort
  if !s:columns_seen
    return "(no column view)\n"
  endif
  let view = org_lsp#columns_view(s:columns_buffer)
  if empty(view)
    return "(column view off)\n"
  endif
  let out = [substitute(view.header, '\s\+$', '', '')]
  for row in view.lines
    call add(out, printf('%d: %s', row[0] + 1, substitute(join(row[2], ' | '), '\s\+$', '', '')))
  endfor
  return join(out, "\n") . "\n"
endfunction

function! s:snapshot(kind) abort
  if a:kind ==# 'agenda'
    let nr = s:agenda_buffer()
    return nr > 0 ? s:text_of(getbufline(nr, 1, '$')) : "(no agenda)\n"
  elseif a:kind ==# 'buffer'
    return printf("[%s]\n", s:name_of(bufnr())) . s:text_of(getline(1, '$'))
  elseif a:kind ==# 'cursor'
    return printf("%s:%d: %s\n", s:name_of(bufnr()), line('.'), getline('.'))
  elseif a:kind ==# 'messages'
    let lines = s:messages()
    return empty(lines) ? '' : join(lines, "\n") . "\n"
  elseif a:kind ==# 'columns'
    return s:columns_text()
  elseif a:kind =~# '^file:'
    let file = a:kind[5:]
    let nr = s:file_buffer(file)
    if nr > 0
      return s:text_of(getbufline(nr, 1, '$'))
    elseif filereadable(s:dir . '/' . file)
      return s:text_of(readfile(s:dir . '/' . file, 'b'))
    endif
    return "(no file)\n"
  endif
  call s:fail('unknown snapshot kind ' . a:kind)
endfunction

" The expected snapshots, by step and kind.
function! s:expected() abort
  let expected = {}
  let current = ''
  for line in readfile(s:e2e . '/expected/' . s:name . '.txt')
    let header = matchlist(line, '^=== \(\d\+\) \(.\+\)$')
    if !empty(header)
      let current = header[1] . ' ' . header[2]
      let expected[current] = []
    elseif !empty(current)
      call add(expected[current], line)
    endif
  endfor
  return expected
endfunction

" Put the cursor where the step says, if it says.
function! s:place_cursor(step) abort
  if has_key(a:step, 'line')
    let [row, column] = [a:step.line, get(a:step, 'column', 0)]
  elseif has_key(a:step, 'at')
    let row = 0
    for number in range(1, line('$'))
      let at = stridx(getline(number), a:step.at)
      if at >= 0
        let [row, column] = [number, at + get(a:step, 'column', 0)]
        break
      endif
    endfor
    if row == 0
      call s:fail(printf('`%s` is not in %s', a:step.at, s:name_of(bufnr())))
    endif
  else
    return 0
  endif
  call cursor(row, column + 1)
  return 1
endfunction

" Put the cursor where the step says, as a user moves it there.
function! s:place(step) abort
  if !s:place_cursor(a:step)
    return
  endif
  " `CursorMoved` comes from the main loop, which a script does not run.
  silent doautocmd <nomodeline> CursorMoved
  call s:settle()
endfunction

function! s:open(file) abort
  silent execute 'edit' fnameescape(s:dir . '/' . a:file)
  call s:until('the server', {-> exists('b:org_lsp_attached') && org_lsp#ready(bufnr())}, 30000)
  " coc.nvim follows a buffer's changes a moment after it opens.
  if s:client ==# 'coc'
    sleep 500m
  endif
  call s:settle()
endfunction

" Where typing goes: where insert mode last started since the key was
" pressed (`A` in a capture buffer, after `<Leader><CR>`), else the
" cursor. A script never returns to the main loop, where insert mode would
" go on: the keys the plugin feeds run in Normal mode until they end, and
" what is typed goes in where insert mode started.
let s:insert_at = []
augroup org_lsp_scenarios
  autocmd!
  autocmd InsertEnter * let s:insert_at = [line('.'), col('.')]
augroup END

" Type `text` where insert mode is, as text inserted there.
function! s:type(text) abort
  let [lnum, column] = empty(s:insert_at) ? [line('.'), col('.')] : s:insert_at
  let s:insert_at = []
  let line = getline(lnum)
  let lines = split(a:text, "\n", 1)
  let after_typed = len(lines) == 1 ? column - 1 + len(lines[0]) : len(lines[-1])
  let lines[0] = strpart(line, 0, column - 1) . lines[0]
  let lines[-1] .= strpart(line, column - 1)
  call setline(lnum, lines[0])
  call append(lnum, lines[1:])
  call cursor(lnum + len(lines) - 1, after_typed + 1)
  silent doautocmd <nomodeline> TextChanged
  call s:settle()
endfunction

function! s:run_command(index, step) abort
  let s:answers = deepcopy(get(a:step, 'answers', []))
  let s:problem = ''
  let s:insert_at = []
  let key = s:key_of(a:step.command)
  let times = get(a:step, 'count', 0)
  let columns_before = org_lsp#columns_on(bufnr())
  let s:columns_buffer = get(a:step, 'wait', '') ==# 'columns' ? bufnr() : s:columns_buffer
  if !empty(key)
    call s:press((times > 0 ? times : '') . key)
  elseif has_key(s:excepted, a:step.command)
    " Run as the plugin runs a command without a key.
    call org_lsp#run(a:step.command, {'count': times})
  else
    call s:fail(printf('%s has no key in Vim here (%s)', a:step.command, join(s:scopes(), ', ')))
  endif
  call s:settle()
  " Dates are typed in the calendar's minibuffer.
  while !empty(s:answers) && empty(s:problem) && &filetype ==# 'org-date'
    let type = s:key_of('org.calendar.type')
    if empty(type)
      call s:fail('no key for org.calendar.type')
    endif
    call s:press(type)
    call s:settle()
  endwhile
  if get(a:step, 'wait', '') ==# 'columns'
    call s:until('column view', {-> org_lsp#columns_on(s:columns_buffer) != columns_before}, 30000)
    let s:columns_seen = 1
    call s:settle()
  endif
  if !empty(s:problem)
    call s:fail(s:problem)
  endif
  if !empty(s:answers)
    call s:fail(printf('%s did not ask for %s', a:step.command, string(s:answers)))
  endif
endfunction

function! s:run_step(index, step, expected) abort
  if has_key(a:step, 'open')
    call s:open(a:step.open)
  endif
  if has_key(a:step, 'focus')
    let nr = a:step.focus ==# 'agenda' ? s:agenda_buffer() : s:file_buffer(a:step.focus)
    if nr < 0
      call s:fail(printf('no buffer %s to go to', a:step.focus))
    endif
    let win = bufwinid(nr)
    if win != -1
      call win_gotoid(win)
    else
      execute 'buffer' nr
    endif
    call s:settle()
  endif
  if get(a:step, 'close', v:false)
    " The agenda's own key.
    let quit = s:key_of('quit')
    if empty(quit)
      call s:fail(printf('no key to close %s', s:name_of(bufnr())))
    endif
    call s:press(quit)
    call s:settle()
  endif
  call s:place(a:step)
  if has_key(a:step, 'type')
    call s:type(a:step.type)
  endif
  if has_key(a:step, 'command')
    call s:run_command(a:index, a:step)
  endif
  for kind in get(a:step, 'expect', [])
    if !has_key(a:expected, a:index . ' ' . kind)
      call s:fail(printf("no expected %s; run the server's runner with ORG_LSP_UPDATE_E2E=1", kind))
    endif
    let want = a:expected[a:index . ' ' . kind]
    let want_text = empty(want) ? '' : join(want, "\n") . "\n"
    " What comes later than the server's last answer (a window that
    " closes) is given a moment more.
    let waited = 0
    while 1
      let got = substitute(s:snapshot(kind), '\V' . escape(s:dir, '\'), '$DIR', 'g')
      if got ==# want_text || waited >= 3000
        break
      endif
      call s:settle()
      sleep 50m
      let waited += 50
    endwhile
    if got !=# want_text
      call s:fail(printf("%s:\n--- expected\n%s--- actual\n%s", kind, want_text, got))
    endif
  endfor
  if index(get(a:step, 'expect', []), 'messages') >= 0
    messages clear
  endif
endfunction

" ----- the steps typed
"
" With $ORG_LSP_TEST_TYPED, nothing stands in for the plugin's prompts:
" the keys of each step, and the answers to what it asks (a key at a
" menu, text at a prompt, a number from a list, a date in the calendar),
" are typed into the terminal by typist.py through tmux, as the user types
" them. The steps run from a timer, which gives Vim back to its main loop
" between ticks: 'updatetime' passes, CursorHold and the client's
" autocommands run, and a prompt waits for its key as it does for the
" user's. A step is a list of actions, each asked at every tick until it
" is done.

let s:typed = !empty($ORG_LSP_TEST_TYPED)
let s:actions = []

" An action: `s:do_{name}` called with its state and `args`, until it
" says it is done (1), or gives the actions that take its place (a list).
function! s:action(name, ...) abort
  return {'name': a:name, 'args': a:000, 'state': {}}
endfunction

" The keys of `keys` (Vim's notation, the leader as `<Leader>`) as
" typist.py types them: tmux's names for the keys named, the rest as text.
function! s:tmux_keys(keys) abort
  let names = {'cr': 'Enter', 'enter': 'Enter', 'return': 'Enter', 'tab': 'Tab', 's-tab': 'BTab',
        \ 'esc': 'Escape', 'space': 'Space', 'bs': 'BSpace', 'del': 'DC', 'nul': 'C-Space',
        \ 'c-@': 'C-Space', 'c-space': 'C-Space', 'pageup': 'PageUp', 'pagedown': 'PageDown'}
  let texts = {'bar': '|', 'lt': '<', 'bslash': '\'}
  let rest = substitute(a:keys, '\c<Leader>', escape(get(g:, 'mapleader', '\'), '\&'), 'g')
  let out = []
  while !empty(rest)
    let named = matchstr(rest, '^<\%([CSMA]-\)*[^<> -]\+>\|^<\%([CSMA]-\)\+.>')
    if !empty(named)
      let rest = rest[len(named):]
      let name = tolower(named[1:-2])
      if has_key(texts, name)
        call add(out, ['text', texts[name]])
        continue
      endif
      let modifiers = toupper(substitute(matchstr(name, '^\%([csma]-\)*'), 'a-', 'm-', 'g'))
      let base = name[len(modifiers):]
      let key = has_key(names, name) ? names[name]
            \ : modifiers . get(names, base, len(base) == 1 ? named[-2:-2] : toupper(base[0]) . base[1:])
      call add(out, ['key', key])
      continue
    endif
    let char = strcharpart(rest, 0, 1)
    let rest = rest[len(char):]
    if !empty(out) && out[-1][0] ==# 'text'
      let out[-1][1] .= char
    else
      call add(out, ['text', char])
    endif
  endwhile
  return out
endfunction

" A key of an answer as tmux names it: the blank ones and the control
" characters by their names, the others as text.
function! s:tmux_key(key) abort
  let names = {"\r": 'Enter', "\t": 'Tab', ' ': 'Space', "\<Esc>": 'Escape'}
  if has_key(names, a:key)
    return ['key', names[a:key]]
  elseif strlen(a:key) == 1 && char2nr(a:key) < 32
    return ['key', 'C-' . nr2char(char2nr(a:key) + 96)]
  endif
  return ['text', a:key]
endfunction

" Type `items` (from s:tmux_keys): done once typist.py has typed them.
function! s:do_keys(state, items) abort
  let path = $ORG_LSP_TEST_WORK . '/keys'
  if !get(a:state, 'written', 0)
    call writefile([json_encode(a:items)], path . '.part')
    call rename(path . '.part', path)
    let a:state.written = 1
    let a:state.since = reltime()
    return 0
  endif
  if reltimefloat(reltime(a:state.since)) > 10
    call s:fail('the keys were not typed: ' . string(a:items))
  endif
  return !filereadable(path)
endfunction

function! s:do_call(state, Function, args) abort
  call call(a:Function, a:args)
  return 1
endfunction

function! s:do_pause(state, milliseconds) abort
  let a:state.since = get(a:state, 'since', reltime())
  return reltimefloat(reltime(a:state.since)) * 1000 >= a:milliseconds
endfunction

function! s:do_until(state, what, Probe, timeout) abort
  let a:state.since = get(a:state, 'since', reltime())
  if a:Probe()
    return 1
  elseif reltimefloat(reltime(a:state.since)) * 1000 >= a:timeout
    call s:fail('timed out waiting for ' . a:what)
  endif
  return 0
endfunction

" The text of a line of the screen.
function! s:screen_line(row) abort
  return substitute(join(map(range(1, &columns), {_, column -> screenstring(a:row, column)}), ''), '\s\+$', '', '')
endfunction

" What waits for the user: `text` typed at input(), a number at
" inputlist(), a `key` read by getcharstr() (in the plugin's prompt,
" which runs from a timer: this tick runs inside it), Vim's `more` or
" hit-enter prompt, or '' when nothing does.
function! s:waiting() abort
  let mode = mode(1)
  if mode ==# 'c'
    return getcmdtype() ==# '@' ? 'text' : getcmdtype() ==# '' ? 'list' : 'command line'
  elseif mode =~# '^r'
    return 'more'
  elseif state() =~# 'c.*c' && (!empty(popup_list()) || !empty(s:screen_line(&lines)))
    return 'key'
  endif
  return ''
endfunction

" What the screen shows of a prompt: its last lines.
function! s:prompt_shown() abort
  return join(map(range(max([1, &lines - 2]), &lines), {_, row -> s:screen_line(row)}), ' | ')
endfunction

" Wait until nothing changes: the server has answered two requests in a
" row, the screen, the buffers and what waits for the user stay the same
" for longer than 'updatetime' a few times over, and no key is pending.
function! s:do_settle(state) abort
  let st = a:state
  let st.since = get(st, 'since', reltime())
  if get(st, 'asked', 0)
    if !st.answered
      return reltimefloat(reltime(st.since)) > 20
    endif
    let st.asked = 0
    let signature = [bufnr(), getcurpos(), len(s:messages()), mode(1), s:waiting(), getcmdline(), winnr('$'), popup_list(), s:screen_line(&lines)]
          \ + map(getbufinfo({'bufloaded': 1}), '[v:val.bufnr, v:val.changedtick]')
    if signature !=# get(st, 'last', v:null)
      let st.last = signature
      let st.changed = reltime()
      let st.rounds = 0
    endif
    let st.rounds = get(st, 'rounds', 0) + 1
    if st.rounds >= 2 && reltimefloat(reltime(st.changed)) * 1000 >= 4 * &updatetime && state() !~# 'm'
      return 1
    endif
    " As the scripted steps do, the steps go on past what never settles.
    return reltimefloat(reltime(st.since)) > 20
  endif
  let nr = s:server_buffer()
  if nr < 0
    return reltimefloat(reltime(st.since)) * 1000 >= 4 * &updatetime
  endif
  let st.asked = 1
  let st.answered = 0
  call org_lsp#request(nr, 'org-lsp/test-sync', v:null, {... -> extend(st, {'answered': 1})})
  return 0
endfunction

function! s:settle_typed() abort
  return s:action('settle')
endfunction

" Open `file` as the user does, with `:edit`.
function! s:typed_open(file) abort
  return [s:action('keys', s:tmux_keys(':edit ' . fnameescape(s:dir . '/' . a:file) . '<CR>')),
        \ s:action('until', 'the server', {-> exists('b:org_lsp_attached') && org_lsp#ready(bufnr())}, 30000)]
        \ + (s:client ==# 'coc' ? [s:action('pause', 500)] : [])
        \ + [s:settle_typed()]
endfunction

" Type `text` where the cursor is: in insert mode if it is (a capture
" buffer starts there), else from `i`; <Esc> after.
function! s:do_type(state, text) abort
  let items = mode() =~# '^i' ? [] : [['text', 'i']]
  let lines = split(a:text, "\n", 1)
  for i in range(len(lines))
    if i > 0
      call add(items, ['key', 'Enter'])
    endif
    if !empty(lines[i])
      call add(items, ['text', lines[i]])
    endif
  endfor
  call add(items, ['key', 'Escape'])
  return [s:action('keys', items), s:settle_typed()]
endfunction

" Press the key of `command` with `times` before it, as found where the
" cursor is then.
function! s:do_press(state, command, times) abort
  let key = s:key_of(a:command)
  if empty(key)
    if a:command ==# 'quit'
      call s:fail(printf('no key to close %s', s:name_of(bufnr())))
    elseif !has_key(s:excepted, a:command)
      call s:fail(printf('%s has no key in Vim here (%s)', a:command, join(s:scopes(), ', ')))
    endif
    " Run as the plugin runs a command without a key.
    call org_lsp#run(a:command, {'count': a:times})
    return 1
  endif
  return [s:action('keys', s:tmux_keys((a:times > 0 ? a:times : '') . key))]
endfunction

" The next answer typed, once what the step asks waits for it; the date
" prompt's calendar is first told that a date is typed (`i`).
function! s:do_answer(state) abort
  let waiting = s:waiting()
  " coc.nvim tells of the files it edits that are not loaded (their names,
  " `Use ':wa' to save changes`): with the command's message, more than
  " the command line holds, and Vim waits for Enter, which the user types.
  if waiting ==# 'more' && s:client ==# 'coc' && s:prompt_shown() =~# '\V[coc.nvim] \|' . escape(s:dir, '\')
    return [s:action('keys', [['key', 'Enter']]), s:settle_typed(), s:action('answer')]
  endif
  if empty(s:answers)
    if !empty(waiting)
      call s:fail(printf('`%s` is asked and the step has no answer left', s:prompt_shown()))
    endif
    return 1
  endif
  let answer = s:answers[0]
  let label = type(answer) == v:t_dict ? answer.choice : answer
  if waiting ==# 'key'
    if type(answer) == v:t_dict
      let item = ['text', s:choice_key(label)]
    else
      let item = s:tmux_key(answer)
    endif
    let items = [item]
  elseif waiting ==# 'text'
    " What the prompt has typed already (the time of the date changed)
    " replaced: an answer is the whole text.
    let items = [['key', 'C-u'], ['text', label], ['key', 'Enter']]
  elseif waiting ==# 'list'
    let items = [['text', s:choice_number(label)], ['key', 'Enter']]
  elseif empty(waiting) && &filetype ==# 'org-date'
    let type = s:key_of('org.calendar.type')
    if empty(type)
      call s:fail('no key for org.calendar.type')
    endif
    return [s:action('keys', s:tmux_keys(type)), s:settle_typed(), s:action('answer')]
  elseif empty(waiting)
    call s:fail(printf('%s did not ask for %s', s:command_name, string(s:answers)))
  else
    call s:fail(printf('Vim waits at its %s prompt: %s', waiting, s:prompt_shown()))
  endif
  call remove(s:answers, 0)
  return [s:action('keys', items), s:settle_typed(), s:action('answer')]
endfunction

" The key of the choice labelled `label` in the popup of choices
" (`[k] label`), the exact label first.
function! s:choice_key(label) abort
  let lines = []
  for id in popup_list()
    let lines += getbufline(winbufnr(id), 1, '$')
  endfor
  for exact in [1, 0]
    for line in lines
      let m = matchlist(line, '^\[\(.\{-}\)\] \(.*\)$')
      if !empty(m) && (exact ? m[2] ==# a:label : stridx(m[2], a:label) >= 0)
        return m[1] ==# 'SPC' ? ' ' : m[1]
      endif
    endfor
  endfor
  call s:fail(printf('no choice `%s` in %s', a:label, string(lines)))
endfunction

" The number of the choice labelled `label` in the list shown
" (`2. label`), the exact label first.
function! s:choice_number(label) abort
  let lines = map(range(1, &lines), {_, row -> s:screen_line(row)})
  for exact in [1, 0]
    for line in lines
      let m = matchlist(line, '^\s*\(\d\+\)\. \(.*\)$')
      if !empty(m) && (exact ? m[2] ==# a:label : stridx(m[2], a:label) >= 0)
        return m[1]
      endif
    endfor
  endfor
  call s:fail(printf('no choice `%s` in %s', a:label, string(filter(lines, '!empty(v:val)'))))
endfunction

" Compare what `kind` shows with what is expected, given 3 s more for
" what comes later than the server's last answer.
function! s:do_expect(state, kind, want) abort
  let a:state.since = get(a:state, 'since', reltime())
  let got = substitute(s:snapshot(a:kind), '\V' . escape(s:dir, '\'), '$DIR', 'g')
  if got ==# a:want
    return 1
  elseif reltimefloat(reltime(a:state.since)) >= 3
    call s:fail(printf("%s:\n--- expected\n%s--- actual\n%s", a:kind, a:want, got))
  endif
  return 0
endfunction

" Go to the buffer of `focus` by a command typed: Vim waits for keys with
" what the current buffer maps (whether CTRL-C is mapped), and a timer
" that changed it would leave that as it was.
function! s:do_focus(state, focus) abort
  let nr = a:focus ==# 'agenda' ? s:agenda_buffer() : s:file_buffer(a:focus)
  if nr < 0
    call s:fail(printf('no buffer %s to go to', a:focus))
  endif
  let win = bufwinid(nr)
  let command = win != -1 ? 'call win_gotoid(' . win . ')' : 'buffer ' . nr
  return [s:action('keys', s:tmux_keys(':' . command . '<CR>'))]
endfunction

function! s:typed_command(step) abort
  let s:answers = deepcopy(get(a:step, 'answers', []))
  let s:command_name = a:step.command
  let s:columns_before = org_lsp#columns_on(bufnr())
  let s:columns_buffer = get(a:step, 'wait', '') ==# 'columns' ? bufnr() : s:columns_buffer
endfunction

function! s:typed_answered(step) abort
  if !empty(s:answers)
    call s:fail(printf('%s did not ask for %s', a:step.command, string(s:answers)))
  endif
endfunction

" The actions of step `index`.
function! s:typed_step(index, step, expected) abort
  let actions = []
  if has_key(a:step, 'open')
    let actions += s:typed_open(a:step.open)
  endif
  if has_key(a:step, 'focus')
    let actions += [s:action('focus', a:step.focus), s:settle_typed()]
  endif
  if get(a:step, 'close', v:false)
    let actions += [s:action('press', 'quit', 0), s:settle_typed()]
  endif
  if has_key(a:step, 'line') || has_key(a:step, 'at')
    let actions += [s:action('call', function('s:place_cursor'), [a:step]), s:settle_typed()]
  endif
  if has_key(a:step, 'type')
    let actions += [s:action('type', a:step.type)]
  endif
  if has_key(a:step, 'command')
    let actions += [s:action('call', function('s:typed_command'), [a:step]),
          \ s:action('press', a:step.command, get(a:step, 'count', 0)), s:settle_typed(), s:action('answer')]
    if get(a:step, 'wait', '') ==# 'columns'
      let actions += [s:action('until', 'column view', {-> org_lsp#columns_on(s:columns_buffer) != s:columns_before}, 30000),
            \ s:action('call', {-> extend(s:, {'columns_seen': 1})}, []), s:settle_typed()]
    endif
    let actions += [s:action('call', function('s:typed_answered'), [a:step])]
  endif
  for kind in get(a:step, 'expect', [])
    if !has_key(a:expected, a:index . ' ' . kind)
      call s:fail(printf("no expected %s; run the server's runner with ORG_LSP_UPDATE_E2E=1", kind))
    endif
    let want = a:expected[a:index . ' ' . kind]
    call add(actions, s:action('expect', kind, empty(want) ? '' : join(want, "\n") . "\n"))
  endfor
  if index(get(a:step, 'expect', []), 'messages') >= 0
    call add(actions, s:action('call', {-> execute('messages clear')}, []))
  endif
  return actions
endfunction

let s:step = 0

" A tick: the next action asked, the next step's actions once they are
" all done.
function! s:tick(timer) abort
  try
    while empty(s:actions)
      " $ORG_LSP_TEST_STOP: Vim left as it is after that step, to go on
      " by hand.
      if !empty($ORG_LSP_TEST_STOP) && s:step == str2nr($ORG_LSP_TEST_STOP)
        return timer_stop(a:timer)
      endif
      if s:step == len(s:spec.steps)
        call timer_stop(a:timer)
        return s:result([printf('ok - %s (%d steps, typed)', s:name, len(s:spec.steps))])
      endif
      let s:step += 1
      let s:actions = s:typed_step(s:step, s:spec.steps[s:step - 1], s:expected_snapshots)
    endwhile
    let action = s:actions[0]
    let done = call('s:do_' . action.name, [action.state] + action.args)
    if type(done) == v:t_list
      call remove(s:actions, 0)
      call extend(s:actions, done, 0)
    elseif done
      call remove(s:actions, 0)
    endif
  catch
    call timer_stop(a:timer)
    let message = v:exception =~# '^scenario: ' ? v:exception[10:] : v:exception . ' (' . v:throwpoint . ')'
    return s:result(split(printf('not ok - %s: step %d (typed): %s', s:name, s:step, message), "\n"))
  endtry
endfunction

function! s:result(lines) abort
  " Written whole, then renamed: the runner stops Vim as soon as the file
  " is there.
  call writefile(a:lines, $ORG_LSP_TEST_RESULT . '.part')
  call rename($ORG_LSP_TEST_RESULT . '.part', $ORG_LSP_TEST_RESULT)
  qa!
endfunction

function! s:run(...) abort
  if get(s:spec, 'emacs', v:false) && !executable('emacs')
    if !empty($ORG_LSP_REQUIRE_EMACS)
      return s:result([printf('not ok - %s: Emacs is required but not found', s:name)])
    endif
    return s:result([printf('ok - %s # skip no Emacs', s:name)])
  endif
  call org_lsp#config()
  let manifest = org_lsp#manifest()
  let s:agenda_name = manifest.agenda.fileName
  let s:prompt_name = manifest.datePrompt.fileName
  let s:capture_prefix = manifest.capture.prefix
  if s:typed
    " As a user's vimrc sets them (defaults.vim for 'ttimeout'): the
    " client's autocommands on CursorHold run while a prompt waits, and
    " <Esc> typed is <Esc>, not the start of a key.
    set updatetime=100 ttimeout ttimeoutlen=100
    let s:expected_snapshots = s:expected()
    messages clear
    call timer_start(20, function('s:tick'), {'repeat': -1})
    return
  endif
  " What the commands ask, answered by the steps.
  function! org_lsp#prompt(input) abort
    return s:answer(a:input)
  endfunction
  function! org_lsp#read_key(prompt, choices, ...) abort
    return s:answer({'kind': 'key', 'prompt': a:prompt, 'choices': a:choices})
  endfunction
  let skips = get(get(s:skips, s:client, {}), s:name, {})
  let expected = s:expected()
  messages clear
  let steps = s:spec.steps
  let index = 0
  try
    for step in steps
      let index += 1
      if has_key(skips, string(index))
        return s:result([printf('ok - %s (steps 1 to %d of %d) # skip step %d: %s',
              \ s:name, index - 1, len(steps), index, skips[string(index)])])
      endif
      call s:run_step(index, step, expected)
    endfor
  catch
    let message = v:exception =~# '^scenario: ' ? v:exception[10:] : v:exception . ' (' . v:throwpoint . ')'
    return s:result(split(printf('not ok - %s: step %d: %s', s:name, index, message), "\n"))
  endtry
  call s:result([printf('ok - %s (%d steps)', s:name, len(steps))])
endfunction

autocmd VimEnter * ++once call timer_start(100, function('s:run'))
