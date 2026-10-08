" Vim glue for the org-lsp language server.
"
" The server does the Org work and describes itself (`org-lsp manifest`):
" its commands with their keys, and how its tokens should look. This file
" applies that, on top of whichever LSP client is installed: running
" commands with the cursor position, the agenda and capture buffers the
" server asks to show, folds, highlighting and the clock. What differs
" between clients (sending a request, hearing the server's requests and
" notifications) is in autoload/org_lsp/adapter/.

let s:root = fnamemodify(expand('<sfile>:p'), ':h:h:h:h')

" ----- configuration

let s:defaults = {
      \ 'cmd': [],
      \ 'settings': {},
      \ 'client': 'auto',
      \ 'folding': 1,
      \ 'startup_folded': '',
      \ 'highlight': 1,
      \ 'src_highlight': 1,
      \ 'virtual_indent': 1,
      \ 'win_split_mode': 'split',
      \ 'win_split_size': 0,
      \ 'mappings': {
      \   'global': {
      \     'org.agenda.dispatch': '<Leader>oa',
      \     'org.capture': '<Leader>oc',
      \     'org.clock.goto': '<Leader>oxj',
      \   },
      \   'org': {'cycle': '<Tab>', 'global_cycle': '<S-Tab>', 'help': 'g?'},
      \   'agenda': {'quit': 'q', 'hover': 'K', 'help': 'g?'},
      \ },
      \ }

" The options: `g:org_lsp` over the defaults. `mappings` is merged scope by
" scope, and `0` in place of it (or of a scope) sets no keys.
function! org_lsp#config() abort
  let user = get(g:, 'org_lsp', {})
  let config = extend(deepcopy(s:defaults), user)
  let mappings = get(user, 'mappings', {})
  if type(mappings) == v:t_dict
    let config.mappings = deepcopy(s:defaults.mappings)
    for [scope, keys] in items(mappings)
      if type(keys) == v:t_dict && type(get(config.mappings, scope, {})) == v:t_dict
        let config.mappings[scope] = extend(get(config.mappings, scope, {}), keys)
      else
        let config.mappings[scope] = keys
      endif
    endfor
  endif
  return config
endfunction

" The server command: the configured one, a build inside the plugin's
" checkout, or `org-lsp` on PATH; empty when there is none.
function! org_lsp#server_command() abort
  let cmd = org_lsp#config().cmd
  if !empty(cmd)
    return type(cmd) == v:t_string ? [cmd] : cmd
  endif
  let exe = has('win32') ? 'org-lsp.exe' : 'org-lsp'
  for profile in ['release', 'debug']
    let built = s:root . '/target/' . profile . '/' . exe
    if executable(built)
      return [built]
    endif
  endfor
  return executable('org-lsp') ? ['org-lsp'] : []
endfunction

" What the server says about itself; empty without a server.
function! org_lsp#manifest() abort
  if !exists('s:manifest')
    let cmd = org_lsp#server_command()
    if empty(cmd)
      return {}
    endif
    try
      let s:manifest = json_decode(system(join(map(copy(cmd), 'shellescape(v:val)')) . ' manifest'))
    catch
      return {}
    endtry
  endif
  return s:manifest
endfunction

" The options the server starts with: the user's, and that this plugin
" shows the documents the server asks for, the agenda as the file the
" server writes, whatever the client says.
function! org_lsp#init_options() abort
  return extend(deepcopy(org_lsp#config().settings), {
        \ 'show_document': v:true,
        \ 'virtual_documents': v:false,
        \ 'virtual_indent': org_lsp#config().virtual_indent && s:has_virtual_text() ? v:true : v:false,
        \ 'line_highlights': v:true,
        \ })
endfunction

" Whether text properties can be text of their own: Vim 9.0.0067 on.
function! s:has_virtual_text() abort
  return has('textprop') && has('patch-9.0.0067')
endfunction

function! s:warn(message) abort
  echohl WarningMsg
  echomsg 'org-lsp: ' . a:message
  echohl None
endfunction

" ----- the client

" The adapter for the installed client: `lsp` (yegappan/lsp), `vim_lsp`,
" `coc` or `ale`; empty when there is none.
function! org_lsp#adapter() abort
  if !exists('s:adapter')
    let wanted = org_lsp#config().client
    if wanted ==# 'auto'
      let wanted = exists('g:loaded_lsp') ? 'lsp'
            \ : exists('g:lsp_loaded') ? 'vim_lsp'
            \ : exists('g:did_coc_loaded') ? 'coc'
            \ : exists('g:loaded_ale') ? 'ale' : ''
    endif
    let s:adapter = substitute(wanted, '-', '_', 'g')
  endif
  return s:adapter
endfunction

function! s:call(name, ...) abort
  return call('org_lsp#adapter#' . org_lsp#adapter() . '#' . a:name, a:000)
endfunction

" Register the server with the client. Called once, after the plugins load.
function! org_lsp#register() abort
  if empty(org_lsp#adapter())
    return
  endif
  let cmd = org_lsp#server_command()
  if empty(cmd)
    return
  endif
  let manifest = org_lsp#manifest()
  let filetypes = ['org', get(get(manifest, 'agenda', {}), 'languageId', 'org-agenda')]
  call s:call('register', cmd, org_lsp#init_options(), filetypes)
endfunction

" How the client counts characters in positions: 'utf-16', 'utf-8' or
" 'utf-32'.
function! s:encoding() abort
  return s:call('encoding')
endfunction

" An Org buffer for requests that are not about the current one, such as
" the agenda asked for from another file. With none open, a scratch Org
" file is loaded, and the client starts the server for it.
function! s:org_buffer() abort
  let filetypes = ['org', 'org-agenda']
  if index(filetypes, &filetype) >= 0
    return bufnr()
  endif
  for info in getbufinfo({'bufloaded': 1})
    if index(filetypes, getbufvar(info.bufnr, '&filetype')) >= 0
      return info.bufnr
    endif
  endfor
  let file = tempname() . '.org'
  call writefile([], file)
  let nr = bufadd(file)
  call setbufvar(nr, '&buflisted', 0)
  call bufload(nr)
  if getbufvar(nr, '&filetype') !=# 'org'
    call setbufvar(nr, '&filetype', 'org')
  endif
  return nr
endfunction

" Send a request about buffer `bufnr`; `Callback(error, result)` gets the
" answer, `error` being {} when there is none. Changes not yet sent go
" first.
function! org_lsp#request(bufnr, method, params, Callback) abort
  if exists('*listener_flush')
    call listener_flush(a:bufnr)
  endif
  call s:call('request', a:bufnr, a:method, a:params, a:Callback)
endfunction

" A request answered before this returns, or {'error': …} after `timeout`
" milliseconds.
function! org_lsp#request_sync(bufnr, method, params, timeout) abort
  let state = {'done': 0}
  function! state.answer(error, result) dict abort
    let self.done = 1
    let self.error = a:error
    let self.result = a:result
  endfunction
  call org_lsp#request(a:bufnr, a:method, a:params, state.answer)
  let waited = 0
  while !state.done && waited < a:timeout
    sleep 10m
    let waited += 10
  endwhile
  return state.done ? {'error': state.error, 'result': state.result} : {'error': {'message': 'timed out'}}
endfunction

" ----- positions and URIs

function! org_lsp#uri(bufnr) abort
  let path = fnamemodify(bufname(a:bufnr), ':p')
  let encoded = substitute(path, '\([^A-Za-z0-9_.~/:-]\)', '\=printf("%%%02X", char2nr(submatch(1)))', 'g')
  return 'file://' . (encoded[0] ==# '/' ? '' : '/') . encoded
endfunction

function! org_lsp#uri_to_path(uri) abort
  let path = substitute(a:uri, '^file://', '', '')
  let path = substitute(path, '%\(\x\x\)', '\=nr2char(str2nr(submatch(1), 16))', 'g')
  if has('win32') && path =~# '^/\a:'
    let path = path[1:]
  endif
  return path
endfunction

" Column `character` of `text` as the client counts it, as a byte index.
function! s:byte(text, character) abort
  let encoding = s:encoding()
  if encoding ==# 'utf-8'
    return a:character
  endif
  let index = encoding ==# 'utf-32' ? byteidx(a:text, a:character) : byteidx(a:text, a:character, 1)
  return index < 0 ? len(a:text) : index
endfunction

" Byte index `byte` of `text` as the client counts characters.
function! s:character(text, byte) abort
  let prefix = strpart(a:text, 0, a:byte)
  let encoding = s:encoding()
  return encoding ==# 'utf-8' ? len(prefix) : encoding ==# 'utf-32' ? strchars(prefix) : strutf16len(prefix)
endfunction

" The last Visual selection as a range: from its first character to after
" its last, whole lines in Visual line mode.
function! s:selection_range() abort
  let [first, last] = [getpos("'<"), getpos("'>")]
  let start_line = getline(first[1])
  let start = {'line': first[1] - 1, 'character': s:character(start_line, visualmode() ==# 'V' ? 0 : first[2] - 1)}
  if visualmode() ==# 'V'
    return {'start': start, 'end': {'line': last[1], 'character': 0}}
  endif
  let end_line = getline(last[1])
  let after = min([last[2] - 1, len(end_line)])
  if after < len(end_line)
    let after += len(strcharpart(end_line[after :], 0, 1))
  endif
  return {'start': start, 'end': {'line': last[1] - 1, 'character': s:character(end_line, after)}}
endfunction

" Where the cursor is, as the server's commands take it.
function! s:cursor_context() abort
  let nr = s:org_buffer()
  if nr == bufnr()
    let position = {'line': line('.') - 1, 'character': s:character(getline('.'), col('.') - 1)}
  else
    let position = {'line': 0, 'character': 0}
  endif
  return [nr, {'textDocument': {'uri': org_lsp#uri(nr)}, 'position': position}]
endfunction

" ----- commands

let s:own = {
      \ 'cycle': {'title': 'Org: cycle visibility of the subtree', 'run': function('org_lsp#cycle')},
      \ 'global_cycle': {'title': 'Org: cycle visibility of the whole file', 'run': function('org_lsp#global_cycle')},
      \ 'quit': {'title': 'Agenda: close', 'run': {-> org_lsp#close_buffer(bufnr())}},
      \ 'hover': {'title': 'Agenda: show the entry of the line', 'run': function('org_lsp#preview')},
      \ 'help': {'title': 'Show the keys', 'run': function('org_lsp#help')},
      \ }

function! s:command(name) abort
  for command in get(org_lsp#manifest(), 'commands', [])
    if command.command ==# a:name
      return command
    endif
  endfor
  return {}
endfunction

" Run a command: one of the plugin's own, or one of the server's, which is
" told where the cursor is. `opts.count` is the count typed before the key;
" `opts.key` the key, to give back its own meaning where the command does
" not apply.
function! org_lsp#run(name, ...) abort
  let opts = get(a:, 1, {})
  if has_key(s:own, a:name)
    return s:own[a:name].run()
  endif
  if empty(org_lsp#adapter())
    return s:warn('no LSP client found: install yegappan/lsp, vim-lsp, coc.nvim or ALE')
  endif
  " Rebuilding reads the files again: those changed on disk while open
  " here, and not changed here, are read again first ('autoread'), as the
  " server takes an open file's text from the editor.
  if a:name ==# 'org.agenda.redo'
    let autoread = &autoread
    set autoread
    silent! checktime
    let &autoread = autoread
  endif
  let [nr, context] = s:cursor_context()
  if get(opts, 'count', 0) > 0
    let context.count = opts.count
  endif
  " The Visual selection just left, as Emacs's region, the point at its
  " start.
  if get(opts, 'region', 0) && nr == bufnr()
    let context.range = s:selection_range()
    let context.position = context.range.start
  endif
  " The register the key was typed with (`"a`), as `y` and `p` take it;
  " what the command reads as Emacs's latest kill comes from there.
  let opts = extend(copy(opts), {'register': v:register}, 'keep')
  if get(s:command(a:name), 'clipboard', v:false)
    let context.clipboard = getreg(opts.register)
  endif
  " Below the cursor's line, as nvim-orgmode's `<Leader><CR>` puts it:
  " never above it, and nothing split.
  if get(opts, 'append', 0)
    let context.below = v:true
  endif
  " Filing a capture or a note takes the buffer's text as it is here: some
  " clients tell the server of the last changes only a while later.
  if index(['org.capture.finish', 'org.capture.refile'], a:name) >= 0
    let context.text = join(getline(1, '$'), "\n") . (&eol || &fixeol ? "\n" : '')
  endif
  call s:execute(nr, a:name, context, opts)
endfunction

function! s:execute(bufnr, name, context, opts) abort
  let params = {'command': a:name, 'arguments': [a:context]}
  call org_lsp#request(a:bufnr, 'workspace/executeCommand', params,
        \ function('s:executed', [a:bufnr, a:name, a:context, a:opts]))
endfunction

" Vim's notation of a key (`<C-a>`) as the key itself.
function! s:keycode(key) abort
  " Backslashes and quotes as themselves, `<...>` as the key it names.
  return eval('"' . substitute(escape(a:key, '"\'), '<', '\\<', 'g') . '"')
endfunction

function! s:executed(bufnr, name, context, opts, error, result) abort
  if !empty(a:error)
    let data = get(a:error, 'data', {})
    if has_key(a:opts, 'key') && type(data) == v:t_dict && get(data, 'notApplicable', v:false)
      " Not an Org matter here: the key does what it does elsewhere.
      let times = get(a:opts, 'count', 0)
      call feedkeys((times > 0 ? times : '') . s:keycode(a:opts.key), 'n')
    else
      call s:warn(get(a:error, 'message', 'the command failed'))
    endif
    return
  endif
  let result = type(a:result) == v:t_dict ? a:result : {}
  " The edit and the cursor, carried in the result (`org.meta.return`);
  " from its key, typing goes on there, in insert mode, as with
  " nvim-orgmode.
  if type(get(result, 'edits', 0)) == v:t_list && bufnr() == a:bufnr
    let at_end = s:apply_carried(a:context.textDocument.uri, result)
    if get(a:opts, 'append', 0)
      execute at_end ? 'startinsert!' : 'startinsert'
    endif
  endif
  " What was cut or copied goes to the register, by lines.
  if type(get(result, 'copy', 0)) == v:t_string
    call setreg(get(a:opts, 'register', '"'), result.copy, 'l')
  endif
  " A sparse tree: everything folded but the lines shown.
  if type(get(result, 'sparse', 0)) == v:t_dict && bufnr() == a:bufnr
    call s:show_sparse(a:bufnr, result.sparse)
  endif
  " The agenda's file may have been written anew.
  call s:reload_agenda()
  if type(get(result, 'save', 0)) == v:t_list
    call org_lsp#save(result.save)
  endif
  if type(get(result, 'message', 0)) == v:t_string
    echomsg result.message
  endif
  if type(get(result, 'close', 0)) == v:t_string
    call org_lsp#close(result.close)
  endif
  if type(get(result, 'input', 0)) == v:t_dict
    " Asked from a callback, the prompt waits for the screen to settle.
    call timer_start(0, {-> s:ask(a:bufnr, a:name, a:context, a:opts, result.input)})
  endif
endfunction

" Show a sparse tree (`org-sparse-tree`) in the current window: all folds
" closed, then those around each of `sparse.lines` opened, as `org-occur`
" shows its matches; the matches highlighted (`OrgLspSparseMatch`) until
" the text changes. The cursor stays. With `keep` (`org-reveal`), only
" the folds that start on the lines of `unfold` open, outer ones first;
" the others stay as they are.
function! s:show_sparse(nr, sparse) abort
  if get(a:sparse, 'keep', v:false)
    if &foldenable
      for line in get(a:sparse, 'unfold', [])
        if foldclosed(line + 1) == line + 1
          execute (line + 1) . 'foldopen'
        endif
      endfor
    endif
    return
  endif
  highlight default link OrgLspSparseMatch Search
  if empty(prop_type_get('org_lsp_sparse'))
    call prop_type_add('org_lsp_sparse', {'highlight': 'OrgLspSparseMatch', 'priority': 200, 'combine': 1})
  endif
  call prop_remove({'type': 'org_lsp_sparse', 'bufnr': a:nr, 'all': 1})
  for range in get(a:sparse, 'highlights', [])
    let start = s:byte(getline(range.start.line + 1), range.start.character)
    let end = s:byte(getline(range.end.line + 1), range.end.character)
    call prop_add(range.start.line + 1, start + 1,
          \ {'end_lnum': range.end.line + 1, 'end_col': end + 1, 'type': 'org_lsp_sparse', 'bufnr': a:nr})
  endfor
  if &foldenable
    let view = winsaveview()
    normal! zM
    for line in get(a:sparse, 'lines', [])
      call cursor(line + 1, 1)
      normal! zv
    endfor
    call winrestview(view)
  endif
  " Org takes the highlighting away at the next change.
  execute 'autocmd TextChanged,TextChangedI <buffer=' . a:nr . '> ++once'
        \ 'call prop_remove({"type": "org_lsp_sparse", "bufnr": ' . a:nr . ', "all": 1})'
endfunction

" Ask what the command asks for, and run it again with the answer.
function! s:ask(bufnr, name, context, opts, input) abort
  let answer = org_lsp#prompt(a:input)
  redraw
  if answer is v:null
    return
  endif
  let context = extend(copy(a:context), get(a:input, 'context', {}))
  let context.input = answer
  call s:execute(a:bufnr, a:name, context, a:opts)
endfunction

" The answer to what a command asks for (`{kind, prompt, value, choices}`):
" a key, one of a list, or typed text; v:null when cancelled.
function! org_lsp#prompt(input) abort
  if a:input.kind ==# 'key'
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
    " Typed, completed from a list (Emacs's `completing-read`).
    let s:completions = a:input.completions
    let answer = input({'prompt': a:input.prompt . ': ', 'default': get(a:input, 'value', ''),
          \ 'completion': 'customlist,org_lsp#complete_input', 'cancelreturn': cancelled})
    return answer ==# cancelled ? v:null : answer
  endif
  let answer = inputdialog(a:input.prompt . ': ', get(a:input, 'value', ''), cancelled)
  return answer ==# cancelled ? v:null : answer
endfunction

let s:completions = []

" The completions of what a command asks to be typed, beginning with what
" is typed, without regard to case.
function! org_lsp#complete_input(lead, line, pos) abort
  return filter(copy(s:completions), {_, c -> stridx(tolower(c), tolower(a:lead)) == 0})
endfunction

" Read one key, as Emacs does at its fast selection prompts: the choices
" are listed in a popup at the bottom; <Esc> or CTRL-C cancels.
" How a key of a key input shows: Emacs's names for the blank ones, and
" C-b for a control character.
function! org_lsp#key_name(key) abort
  let names = {' ': 'SPC', "\r": 'RET', "\t": 'TAB'}
  if has_key(names, a:key)
    return names[a:key]
  endif
  let code = strlen(a:key) == 1 ? char2nr(a:key) : -1
  return code > 0 && code < 32 ? 'C-' .. nr2char(code + 96) : a:key
endfunction

function! org_lsp#read_key(prompt, choices) abort
  let popup = 0
  if !empty(a:choices) && exists('*popup_create')
    let lines = map(copy(a:choices), {_, c -> printf('[%s] %s', org_lsp#key_name(c.key), c.label)})
    let popup = popup_create(lines, {
          \ 'line': &lines - &cmdheight - len(lines) - 1, 'col': 1,
          \ 'border': [], 'title': ' ' . a:prompt . ' ', 'zindex': 300,
          \ })
  endif
  echo a:prompt . ': '
  redraw
  try
    let key = getcharstr()
  catch /^Vim:Interrupt$/
    let key = "\<Esc>"
  endtry
  if popup
    call popup_close(popup)
  endif
  echo ''
  return key ==# "\<Esc>" || key ==# "\<C-c>" ? v:null : key
endfunction

" `:Org agenda list` runs `org.agenda.list`; a group stands for its
" dispatcher: `:Org agenda` runs `org.agenda.dispatch`.
function! org_lsp#user_command(...) abort
  let id = 'org.' . join(a:0 > 0 ? a:000 : ['agenda'], '.')
  call org_lsp#run(empty(s:command(id)) && !has_key(s:own, id) ? id . '.dispatch' : id)
endfunction

function! org_lsp#complete(lead, line, pos) abort
  let names = map(copy(get(org_lsp#manifest(), 'commands', [])), {_, c -> substitute(c.command, '^org\.', '', '')})
  return filter(map(names, {_, n -> substitute(n, '\.', ' ', 'g')}), {_, n -> n =~# '^' . escape(a:lead, '.\')})
endfunction

" ----- keys

" The commands whose keys are typed in insert mode (`org.return`).
let s:insert_commands = {}

" The commands whose keys act below the cursor's line and go on in insert
" mode (`org.meta.return`).
let s:append_commands = {}

" Run a command whose result carries edits and a cursor (`org.return`)
" from insert mode, at once. The key keeps its own meaning with no server,
" or where the command does not apply. While the completion menu shows,
" <CR> is the menu's; another key for the command (<S-CR>) closes it and
" runs the command.
function! org_lsp#insert_key(index) abort
  let entry = s:mapped[a:index]
  if s:passing
    " The key mapped again by its own meaning: Vim's key, once.
    let s:passing = 0
    return feedkeys(s:keycode(entry.lhs), 'ni')
  endif
  if empty(org_lsp#adapter()) || !org_lsp#ready(bufnr())
        \ || pumvisible() && s:keycode(entry.lhs) ==# "\<CR>"
    return s:insert_fallback(entry.lhs)
  endif
  if pumvisible()
    " Another key for the command (<S-CR>): the menu takes its entry with
    " <C-y>, then the key again, ahead of what is typed after it.
    call feedkeys(s:keycode(entry.lhs), 'mi')
    return feedkeys("\<C-y>", 'ni')
  endif
  let [nr, context] = s:cursor_context()
  let position = getcurpos()
  let answer = org_lsp#request_sync(nr, 'workspace/executeCommand', {'command': entry.name, 'arguments': [context]}, 1000)
  " While it waits, coc.nvim moves the cursor a column to the left.
  if getcurpos() != position
    call setpos('.', position)
  endif
  let result = get(answer, 'result', v:null)
  if !empty(get(answer, 'error', {})) || type(result) != v:t_dict || type(get(result, 'edits', 0)) != v:t_list
    return s:insert_fallback(entry.lhs)
  endif
  call s:apply_carried(context.textDocument.uri, result)
endfunction

" Apply the edits a command's result carries, and put the cursor where it
" says. Whether the cursor is at the end of its line.
function! s:apply_carried(uri, result) abort
  call org_lsp#apply_edit({'changes': {a:uri: a:result.edits}})
  if type(get(a:result, 'cursor', 0)) != v:t_dict
    return v:false
  endif
  let text = getline(a:result.cursor.line + 1)
  let column = s:byte(text, a:result.cursor.character)
  call cursor(a:result.cursor.line + 1, column + 1)
  return column >= len(text)
endfunction

" Set while the key's own meaning runs, so that a mapping that maps the
" key again (`imap <CR> …<CR>`) reaches Vim's own key and not the
" plugin's again.
let s:passing = 0

function! s:stop_passing(...) abort
  let s:passing = 0
endfunction

" The global insert-mode mapping of `lhs` (an autopairs plugin's `<CR>`),
" which the buffer's mapping hides from maparg().
function! s:global_map(lhs) abort
  if !exists('*maplist')
    return {}
  endif
  let code = s:keycode(a:lhs)
  for map in maplist()
    if !get(map, 'buffer', 0) && get(map, 'mode', '') =~# '[i!]' && s:keycode(map.lhs) ==# code
      return map
    endif
  endfor
  return {}
endfunction

" A key as typed, with the leader as it is set.
function! s:typed(key) abort
  return substitute(s:keycode(a:key), '\c<Leader>', escape(get(g:, 'mapleader', '\'), '\&'), 'g')
endfunction

" Whether `lhs` starts a longer key of `keys` or of the global keys set,
" with the leader as it is set (`,` and `<Leader>ois` with `,` as the
" leader): then it waits for more.
function! s:starts_longer(lhs, keys) abort
  let typed = s:typed(a:lhs)
  for other in a:keys + map(filter(copy(s:mapped), {_, entry -> !entry.buffer}), {_, entry -> entry.lhs})
    let other_typed = s:typed(other)
    if other_typed !=# typed && stridx(other_typed, typed) == 0
      return 1
    endif
  endfor
  return 0
endfunction

" The key's meaning without the plugin: the buffer's mapping that the
" plugin's took the place of (a completion plugin's `<CR>`), else the
" global mapping, else the key.
function! s:insert_fallback(lhs) abort
  let saved = get(get(b:, 'org_lsp_replaced', {}), a:lhs, {})
  if empty(saved)
    let saved = s:global_map(a:lhs)
  endif
  if empty(saved)
    return feedkeys(s:keycode(a:lhs), 'ni')
  endif
  let rhs = saved.expr ? eval(substitute(saved.rhs, '<SID>', '<SNR>' . saved.sid . '_', 'g')) : s:keycode(saved.rhs)
  if !saved.noremap
    let s:passing = 1
    call timer_start(0, function('s:stop_passing'))
  endif
  call feedkeys(rhs, (saved.noremap ? 'n' : 'm') . 'i')
endfunction

" Set the insert-mode keys of buffer `nr` again: an LSP client may have
" mapped one of them (`<CR>`) after the plugin did. The client's mapping is
" kept, for where the plugin's does not apply.
function! org_lsp#remap_insert_keys(nr) abort
  if a:nr != bufnr() || !exists('b:org_lsp_attached')
    return
  endif
  let replaced = get(b:, 'org_lsp_replaced', {})
  for index in range(len(s:mapped))
    let entry = s:mapped[index]
    if !has_key(s:insert_commands, entry.name) || get(entry, 'buffer', 0) != a:nr
      continue
    endif
    let current = maparg(entry.lhs, 'i', 0, 1)
    if get(current, 'buffer', 0) && get(current, 'rhs', '') !~# 'org_lsp#insert_key'
      let replaced[entry.lhs] = current
    endif
    execute printf('inoremap <buffer> <silent> %s <Cmd>call org_lsp#insert_key(%d)<CR>', entry.lhs, index)
  endfor
  let b:org_lsp_replaced = replaced
endfunction

" The editing command that `.` repeats.
let s:last_edit = {}

function! org_lsp#repeat_edit(...) abort
  if !empty(s:last_edit)
    call org_lsp#run(s:last_edit.name, {'count': v:count, 'key': get(s:last_edit, 'key', '')})
  endif
endfunction

" The keys set, by number: what each runs.
let s:mapped = []

function! org_lsp#key(index, count) abort
  let entry = s:mapped[a:index]
  let opts = {'count': a:count}
  if !empty(entry.native)
    let opts.key = entry.native
  endif
  call org_lsp#run(entry.name, opts)
endfunction

" `g@l` runs 'operatorfunc', which `.` repeats; on an empty line `l`
" cannot move, and the command runs directly.
function! org_lsp#repeatable(index) abort
  let entry = s:mapped[a:index]
  let s:last_edit = {'name': entry.name, 'key': entry.native}
  set operatorfunc=org_lsp#repeat_edit
  if getline('.') ==# ''
    call timer_start(0, {-> org_lsp#key(a:index, 0)})
    return ''
  endif
  return 'g@l'
endfunction

" The keys of `scope`: the server's for modal editors, with the
" configuration's in their place, command by command.
function! s:keys_in(scope) abort
  let mappings = org_lsp#config().mappings
  if type(mappings) != v:t_dict || type(get(mappings, a:scope, {})) != v:t_dict
    return [{}, {}]
  endif
  let keys = {}
  let native = {}
  for command in get(org_lsp#manifest(), 'commands', [])
    for key in command.keys
      if key.preset ==# 'vim' && key.in ==# a:scope
        call add(get(extend(keys, {command.command: []}, 'keep'), command.command), key.key)
        if get(key, 'native', v:false)
          let native[key.key] = 1
        endif
        if get(key, 'mode', '') ==# 'insert'
          let s:insert_commands[command.command] = 1
        endif
        if get(key, 'mode', '') ==# 'append'
          let s:append_commands[command.command] = 1
        endif
      endif
    endfor
  endfor
  " The keys of the mappings, the plugin's own (`K` for `hover`) and the
  " user's, are theirs: a command of the server's with the same key (`K`,
  " `org.agenda.habits`) goes without it.
  let mapped = {}
  for [name, key] in items(get(mappings, a:scope, {}))
    let keys[name] = type(key) == v:t_list ? key : (type(key) == v:t_string ? [key] : [])
    for lhs in keys[name]
      let mapped[lhs] = name
    endfor
  endfor
  for name in keys(keys)
    call filter(keys[name], {_, lhs -> get(mapped, lhs, name) ==# name})
  endfor
  return [keys, native]
endfunction

" A key as the left-hand side of a mapping: the keys named in `<...>` as
" they are, and the characters a mapping command reads otherwise (`<`,
" `|`, `\\`, a space) by their names.
function! s:map_lhs(key) abort
  let names = {'<': '<lt>', '|': '<Bar>', '\': '<Bslash>', ' ': '<Space>'}
  let out = ''
  let rest = a:key
  while !empty(rest)
    let named = matchstr(rest, '^<[^<> ]\+>')
    if empty(named)
      let first = strcharpart(rest, 0, 1)
      let named = get(names, first, first)
      let rest = rest[len(first):]
    else
      let rest = rest[len(named):]
    endif
    let out .= named
  endwhile
  return out
endfunction

function! org_lsp#set_keys(scope) abort
  let [keys, native] = s:keys_in(a:scope)
  let buffer = a:scope ==# 'global' ? '' : '<buffer>'
  " In the read-only agenda, one letter runs at once, unless it starts a
  " longer key (`v` of `vd`).
  let all = flatten(values(copy(keys)))
  for [name, list] in items(keys)
    let command = s:command(name)
    for lhs in list
      let mapped = s:map_lhs(lhs)
      call add(s:mapped, {'name': name, 'native': get(native, lhs, 0) ? lhs : '', 'lhs': lhs, 'buffer': empty(buffer) ? 0 : bufnr()})
      let index = len(s:mapped) - 1
      if has_key(s:insert_commands, name)
        " A mapping of the buffer's that this one takes the place of is
        " what the key does where the command does not apply.
        let before = maparg(mapped, 'i', 0, 1)
        if !empty(buffer) && get(before, 'buffer', 0) && get(before, 'rhs', '') !~# 'org_lsp#insert_key'
          let b:org_lsp_replaced = extend(get(b:, 'org_lsp_replaced', {}), {lhs: before})
        endif
        execute printf('inoremap %s <silent> %s <Cmd>call org_lsp#insert_key(%d)<CR>', buffer, mapped, index)
        continue
      endif
      let nowait = a:scope ==# 'agenda' && !s:starts_longer(lhs, all) ? '<nowait>' : ''
      if has_key(s:append_commands, name)
        execute printf('nnoremap %s <silent> %s :<C-u>call org_lsp#run(%s, {''append'': 1})<CR>', buffer, mapped, string(name))
      elseif get(command, 'edits', v:false) && a:scope ==# 'org'
        execute printf('nnoremap %s <silent> <expr> %s org_lsp#repeatable(%d)', buffer, mapped, index)
      else
        execute printf('nnoremap %s %s <silent> %s :<C-u>call org_lsp#key(%d, v:count)<CR>', buffer, nowait, mapped, index)
      endif
      " On a selection, the commands Org gives a region.
      if get(command, 'region', v:false) && a:scope ==# 'org'
        execute printf('xnoremap %s <silent> %s :<C-u>call org_lsp#run(%s, {''region'': 1})<CR>', buffer, mapped, string(name))
      endif
    endfor
  endfor
endfunction

" ----- help

let s:scope_titles = {'org': 'Org file', 'capture': 'Capture buffer', 'agenda': 'Agenda', 'global': 'Anywhere'}

" The keys of `scope` as they are set: `[{keys, title}]`, in the order of
" the server's commands, then the plugin's own.
function! org_lsp#keys(scope) abort
  let bound = s:keys_in(a:scope)[0]
  let out = []
  let names = map(copy(get(org_lsp#manifest(), 'commands', [])), 'v:val.command') + ['cycle', 'global_cycle', 'quit', 'hover', 'help']
  for name in names + sort(keys(bound))
    if !empty(get(bound, name, []))
      let command = s:command(name)
      let title = has_key(s:own, name) ? s:own[name].title : get(command, 'title', name)
      call add(out, {'keys': remove(bound, name), 'title': title})
    endif
  endfor
  return out
endfunction

" The lines of the help for `scopes`: a heading for each, then its keys
" and what they do.
function! org_lsp#help_lines(scopes) abort
  let sections = filter(map(copy(a:scopes), {_, scope -> [scope, org_lsp#keys(scope)]}), '!empty(v:val[1])')
  let width = 0
  for [_, entries] in sections
    for entry in entries
      let width = max([width, strdisplaywidth(join(entry.keys, ', '))])
    endfor
  endfor
  let lines = []
  for [scope, entries] in sections
    if !empty(lines)
      call add(lines, '')
    endif
    call add(lines, get(s:scope_titles, scope, scope))
    for entry in entries
      let keys = join(entry.keys, ', ')
      call add(lines, '  ' . keys . repeat(' ', width - strdisplaywidth(keys)) . '  ' . entry.title)
    endfor
  endfor
  return lines
endfunction

function! s:help_filter(id, key) abort
  if index(['q', "\<Esc>", 'g'], a:key) >= 0
    call popup_close(a:id)
    return 1
  endif
  return popup_filter_menu(a:id, a:key)
endfunction

" Show the keys of the current buffer in a popup; `q` or <Esc> closes it.
function! org_lsp#help() abort
  let manifest = org_lsp#manifest()
  if &filetype ==# get(get(manifest, 'agenda', {}), 'languageId', 'org-agenda')
    let scopes = ['agenda', 'global']
  elseif s:is_capture(bufname())
    let scopes = ['capture', 'org', 'global']
  else
    let scopes = ['org', 'global']
  endif
  let lines = org_lsp#help_lines(scopes)
  if empty(lines)
    echomsg 'org-lsp: no keys are set here'
    return 0
  endif
  return popup_create(lines, {
        \ 'title': ' Org keys ', 'border': [], 'padding': [0, 1, 0, 1],
        \ 'pos': 'center', 'maxheight': &lines - 6, 'scrollbar': 1,
        \ 'filter': function('s:help_filter'), 'mapping': 0, 'zindex': 200,
        \ })
endfunction

" ----- buffers the server shows

" The window for a buffer opened apart (the agenda, a capture buffer), as
" `win_split_mode` says: 'split', 'vsplit', 'current' or an Ex command.
function! s:open_apart(path) abort
  let mode = org_lsp#config().win_split_mode
  if mode !=# 'current'
    let from = win_getid()
    execute mode
    let w:org_lsp_opened = 1
    let w:org_lsp_from = from
    call s:resize_apart(mode, org_lsp#config().win_split_size)
  endif
  execute 'edit' fnameescape(a:path)
endfunction

" `win_split_size`: lines for a split, columns for a vertical one (`vsplit`,
" `vnew`, `vertical`), or below 1 that part of the editor; 0 leaves the
" window as the command made it.
function! s:resize_apart(mode, size) abort
  if type(a:size) != v:t_number && type(a:size) != v:t_float || a:size <= 0
    return
  endif
  let vertical = a:mode =~# '^\s*\S*\s*v' || a:mode =~# 'vert'
  let total = vertical ? &columns : &lines - &cmdheight - 1
  let size = max([1, min([total, float2nr(a:size < 1 ? total * a:size : a:size)])])
  execute (vertical ? 'vertical ' : '') . 'resize ' . size
endfunction

" Tell the server how many columns of text the window that shows the
" agenda has, for `org_agenda_tags_column` `"auto"`: the tags then end at its
" right edge, and the server builds the agenda again when it changes.
let s:told_width = 0
function! org_lsp#tell_agenda_width() abort
  for win in range(1, winnr('$'))
    let nr = winbufnr(win)
    if getbufvar(nr, '&filetype') ==# 'org-agenda'
      let info = getwininfo(win_getid(win))[0]
      let width = info.width - info.textoff
      if width != s:told_width
        let s:told_width = width
        " The server has written the agenda anew by the time it answers;
        " it is read again outside the client's callback.
        call org_lsp#request(nr, 'org-lsp/agendaWidth', {'width': width},
              \ {_error, _result -> timer_start(0, {-> s:reload_agenda()})})
      endif
      return
    endif
  endfor
endfunction

" The line of the agenda the cursor is on, each time it changes, for
" follow mode (`F`), which the server keeps.
function! org_lsp#tell_agenda_cursor() abort
  if &filetype !=# 'org-agenda' || get(b:, 'org_lsp_cursor_line', 0) == line('.')
    return
  endif
  let b:org_lsp_cursor_line = line('.')
  let params = {'textDocument': {'uri': org_lsp#uri(bufnr())},
        \ 'position': {'line': line('.') - 1, 'character': 0}}
  call org_lsp#request(bufnr(), 'org-lsp/agendaCursor', params, {_error, _result -> 0})
endfunction

function! s:is_agenda(path) abort
  return fnamemodify(a:path, ':t') ==# get(get(org_lsp#manifest(), 'agenda', {}), 'fileName', "\n")
endfunction

function! s:is_capture(path) abort
  let prefix = get(get(org_lsp#manifest(), 'capture', {}), 'prefix', "\n")
  return stridx(fnamemodify(a:path, ':t'), prefix) == 0
endfunction

" The agenda is a file the server writes: read it again when it has.
" Reading drops the highlighting; the lines that stayed get theirs back at
" once, from the tokens they had, until the new tokens come.
function! s:reload_agenda() abort
  for info in getbufinfo({'bufloaded': 1})
    if s:is_agenda(info.name)
      let old = getbufline(info.bufnr, 1, '$')
      execute 'checktime' info.bufnr
      call s:repaint_kept(info.bufnr, old)
    endif
  endfor
endfunction

" Paint buffer `nr` with the tokens it had when it had the lines `old`:
" those of the lines before and after the part that changed.
function! s:repaint_kept(nr, old) abort
  let tokens = get(s:painted, a:nr, [])
  let new = getbufline(a:nr, 1, '$')
  if empty(tokens) || new ==# a:old
    return
  endif
  let first = 0
  while first < len(a:old) && first < len(new) && a:old[first] ==# new[first]
    let first += 1
  endwhile
  let [last_old, last_new] = [len(a:old) - 1, len(new) - 1]
  while last_old >= first && last_new >= first && a:old[last_old] ==# new[last_new]
    let [last_old, last_new] = [last_old - 1, last_new - 1]
  endwhile
  let kept = []
  for token in tokens
    if token[0] < first
      call add(kept, token)
    elseif token[0] > last_old
      call add(kept, [token[0] + last_new - last_old] + token[1:])
    endif
  endfor
  call s:paint_absolute(a:nr, new, kept)
endfunction

" `window/showDocument`: the agenda and capture buffers open apart, or in
" the window that shows them; an entry's file in the current window.
function! org_lsp#show_document(params) abort
  if get(a:params, 'external', v:false)
    call s:open_external(a:params.uri)
    return {'success': v:true}
  endif
  let path = org_lsp#uri_to_path(a:params.uri)
  let win = bufwinid(path)
  let from = win_getid()
  " Follow mode shows an entry and leaves the cursor in the agenda.
  let keep_focus = get(a:params, 'takeFocus', v:true) is v:false && get(a:params, 'orgOtherWindow', v:false)
  if s:is_agenda(path) || s:is_capture(path)
    if win != -1
      call win_gotoid(win)
      call s:reload_agenda()
    else
      call s:open_apart(path)
    endif
  elseif get(a:params, 'orgOtherWindow', v:false)
    " An entry's file in another window (TAB in the agenda).
    if !keep_focus
      normal! m'
    endif
    call win_gotoid(s:other_window(win_getid(), path))
    if bufnr() != bufnr(path)
      execute 'edit' fnameescape(path)
    endif
  else
    normal! m'
    if win != -1
      call win_gotoid(win)
    else
      execute 'edit' fnameescape(path)
    endif
  endif
  let start = get(get(a:params, 'selection', {}), 'start', {})
  if !empty(start)
    let text = getline(start.line + 1)
    call cursor(start.line + 1, s:byte(text, start.character) + 1)
    " A capture buffer is for typing in, from where `%?` was.
    if s:is_capture(path)
      call feedkeys(col('.') >= len(text) ? 'A' : 'i', 'n')
    endif
    if get(a:params, 'orgOtherWindow', v:false)
      " The entry unfolded, as `org-agenda-show` shows its context.
      silent! normal! zv
    endif
  endif
  if keep_focus
    call win_gotoid(from)
  endif
  return {'success': v:true}
endfunction

" The window a file opens in from the agenda's window `from` with TAB
" (`org-agenda-goto`): one that shows it, else an empty one, else a new
" one beside the window the agenda was opened from.
function! s:other_window(from, path) abort
  let wins = filter(map(range(1, winnr('$')), 'win_getid(v:val)'),
        \ {_, win -> win != a:from && win_gettype(win) !=# 'popup'})
  for win in wins
    if winbufnr(win) == bufnr(a:path) && bufnr(a:path) != -1
      return win
    endif
  endfor
  for win in wins
    let buf = winbufnr(win)
    if bufname(buf) ==# '' && getbufvar(buf, '&buftype') ==# '' && !getbufvar(buf, '&modified')
          \ && getbufline(buf, 1, '$') == ['']
      return win
    endif
  endfor
  " Split the previous window, or the agenda's when it is the only one:
  " side by side when it is wide, as Emacs's `split-window-sensibly`.
  let previous = win_getid(winnr('#'))
  let base = index(wins, previous) >= 0 ? previous : empty(wins) ? a:from : wins[0]
  call win_gotoid(base)
  execute winwidth(base) >= 160 ? 'belowright vsplit' : 'belowright split'
  return win_getid()
endfunction

" A URL or a file for the system to open (a link of `org-open-at-point`):
" with Vim's own `:Open` where there is one, else the system's opener.
function! s:open_external(uri) abort
  if exists(':Open') == 2
    execute 'Open' fnameescape(a:uri)
    return
  endif
  let opener = has('mac') ? ['open'] : has('win32') ? ['cmd', '/c', 'start', ''] : ['xdg-open']
  call job_start(opener + [a:uri], {'stoponexit': ''})
endfunction

" Write the files at `uris` that have changes.
function! org_lsp#save(uris) abort
  for uri in a:uris
    let nr = bufnr(org_lsp#uri_to_path(uri))
    if nr > 0 && bufloaded(nr) && getbufvar(nr, '&modified')
      let win = bufwinid(nr)
      if win != -1
        call win_execute(win, 'silent update')
      else
        let current = bufnr()
        let hidden = &hidden
        set hidden
        execute 'silent keepalt keepjumps noautocmd buffer' nr
        silent update
        execute 'silent keepalt keepjumps noautocmd buffer' current
        let &hidden = hidden
      endif
    endif
  endfor
endfunction

" Close buffer `nr` and the windows the plugin opened for it; a window that
" was there before goes back to the buffer it showed.
function! org_lsp#close_buffer(nr) abort
  for win in win_findbuf(a:nr)
    if getwinvar(win, 'org_lsp_opened', 0) && winnr('$') > 1
      " Back to the window it was opened from, not the one that takes its
      " place on the screen.
      let from = getwinvar(win, 'org_lsp_from', 0)
      let current = win == win_getid()
      call win_execute(win, 'close!')
      if current && from != win && win_id2win(from) > 0
        call win_gotoid(from)
      endif
    else
      call win_execute(win, 'if bufnr("#") > 0 && bufnr("#") != ' . a:nr . ' | buffer! # | else | enew! | endif')
    endif
  endfor
  if bufexists(a:nr)
    if getbufvar(a:nr, '&modified') && filereadable(bufname(a:nr))
      call win_execute(bufwinid(a:nr), 'silent write')
    endif
    execute 'silent! bwipeout!' a:nr
  endif
endfunction

function! org_lsp#close(uri) abort
  let nr = bufnr(org_lsp#uri_to_path(a:uri))
  if nr > 0
    call org_lsp#close_buffer(nr)
  endif
endfunction

" ----- edits, for clients that leave them to the plugin

" Apply a WorkspaceEdit.
function! org_lsp#apply_edit(edit) abort
  let changes = []
  for [uri, edits] in items(get(a:edit, 'changes', {}))
    call add(changes, [uri, edits])
  endfor
  for change in get(a:edit, 'documentChanges', [])
    if has_key(change, 'textDocument')
      call add(changes, [change.textDocument.uri, change.edits])
    endif
  endfor
  for [uri, edits] in changes
    let path = org_lsp#uri_to_path(uri)
    let nr = bufadd(path)
    call bufload(nr)
    " Last first, so that the positions of the others stay right.
    let sorted = sort(copy(edits), {a, b -> a.range.start.line != b.range.start.line
          \ ? b.range.start.line - a.range.start.line : b.range.start.character - a.range.start.character})
    for edit in sorted
      call s:apply_text_edit(nr, edit)
    endfor
  endfor
  return {'applied': v:true}
endfunction

function! s:apply_text_edit(nr, edit) abort
  let start = a:edit.range.start
  let end = a:edit.range.end
  let lines_count = len(getbufline(a:nr, 1, '$'))
  let first = get(getbufline(a:nr, start.line + 1), 0, '')
  let last = end.line < lines_count ? get(getbufline(a:nr, end.line + 1), 0, '') : ''
  let before = strpart(first, 0, s:byte(first, start.character))
  let after = end.line < lines_count ? strpart(last, s:byte(last, end.character)) : ''
  let lines = split(before . a:edit.newText . after, "\n", 1)
  let removed = min([end.line, lines_count - 1]) - start.line + 1
  if start.line >= lines_count
    call appendbufline(a:nr, lines_count, lines)
    return
  endif
  " Text that ends with a newline at the end of the buffer leaves an empty
  " last piece, which is the buffer's own end of line.
  if end.line >= lines_count && len(lines) > 1 && lines[-1] ==# ''
    call remove(lines, -1)
  endif
  call deletebufline(a:nr, start.line + 1, start.line + removed)
  call appendbufline(a:nr, start.line, lines)
  if lines_count == removed && len(getbufline(a:nr, 1, '$')) > len(lines)
    call deletebufline(a:nr, '$')
  endif
endfunction

" ----- the server's requests and notifications

" A request from the server the client leaves to the plugin. Returns the
" result.
function! org_lsp#handle_request(method, params) abort
  if a:method ==# 'window/showDocument'
    return org_lsp#show_document(a:params)
  elseif a:method ==# 'workspace/applyEdit'
    return org_lsp#apply_edit(a:params.edit)
  endif
  return v:null
endfunction

function! org_lsp#handle_notification(method, params) abort
  if a:method ==# 'org-lsp/save'
    call org_lsp#save(get(a:params, 'uris', []))
  elseif a:method ==# 'org-lsp/clock'
    call s:clock_update(a:params)
  elseif a:method ==# 'org-lsp/timer'
    call s:timer_update(a:params)
  elseif a:method ==# 'org-lsp/virtualIndent'
    call s:virtual_indent(a:params)
  elseif a:method ==# 'org-lsp/lineHighlights'
    call s:line_highlights(a:params)
  endif
endfunction

" Lines of the agenda highlighted from edge to edge, as Emacs's overlays
" cover them (`org-lsp/lineHighlights`): the entry the clock runs on. Signs
" without text, which colour their lines; and the tokens again, which
" moved with them.
function! s:line_highlights(params) abort
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr < 0 || !bufloaded(nr)
    return
  endif
  if empty(s:selectors)
    call org_lsp#define_highlights()
  endif
  call sign_unplace('org_lsp_lines', {'buffer': nr})
  for line in get(a:params, 'lines', [])
    let name = 'org_lsp_line_' . line.highlight
    if empty(sign_getdefined(name))
      call sign_define(name, {'linehl': s:group_name(line.highlight)})
    endif
    call sign_place(0, 'org_lsp_lines', name, nr, {'lnum': line.line + 1})
  endfor
  if org_lsp#config().highlight
    call s:update_tokens(nr)
  endif
endfunction

" ----- org-indent-mode

" Leading stars hidden as Org hides them (`org-hide`): in the color of the
" background.
function! s:hidden_star_highlight() abort
  let normal = get(hlget('Normal', 1), 0, {})
  if has_key(normal, 'guibg') || has_key(normal, 'ctermbg')
    execute 'highlight OrgLspHiddenStar'
          \ . (has_key(normal, 'guibg') ? ' guifg=' . normal.guibg : '')
          \ . (has_key(normal, 'ctermbg') ? ' ctermfg=' . normal.ctermbg : '')
  else
    highlight! default link OrgLspHiddenStar Ignore
  endif
endfunction

function! s:indent_prop_types() abort
  if empty(prop_type_get('org_lsp_indent'))
    call s:hidden_star_highlight()
    call prop_type_add('org_lsp_indent', {})
    call prop_type_add('org_lsp_star', {'highlight': 'OrgLspHiddenStar', 'priority': 1000, 'combine': 0})
    augroup org_lsp_hidden_star
      autocmd!
      autocmd ColorScheme * call s:hidden_star_highlight()
    augroup END
  endif
endfunction

" Show the prefixes the server sent for a file (`org-lsp/virtualIndent`):
" `lines` of `[line, prefix, wrap, hidden stars]`. The prefixes are text of
" their own before the lines; Vim cannot repeat them on the lines a long
" line wraps into, where 'breakindent' lines those up with the line's own
" text instead.
function! s:virtual_indent(params) abort
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr < 0 || !bufloaded(nr) || !s:has_virtual_text()
    return
  endif
  call s:indent_prop_types()
  call prop_remove({'type': 'org_lsp_indent', 'bufnr': nr, 'all': 1})
  call prop_remove({'type': 'org_lsp_star', 'bufnr': nr, 'all': 1})
  let last = getbufinfo(nr)[0].linecount
  for [line, prefix, wrap, stars] in a:params.lines
    " For older text than the buffer has: newer prefixes are on their way.
    if line >= last
      continue
    endif
    if prefix > 0
      call prop_add(line + 1, 1, {'type': 'org_lsp_indent', 'bufnr': nr, 'text': repeat(' ', prefix)})
    endif
    if stars > 0
      call prop_add(line + 1, 1, {'type': 'org_lsp_star', 'bufnr': nr, 'length': stars})
    endif
  endfor
  if !empty(a:params.lines)
    for win in win_findbuf(nr)
      call setwinvar(win, '&breakindent', 1)
      call setwinvar(win, '&breakindentopt', 'list:-1')
    endfor
    call setbufvar(nr, '&formatlistpat', '^\s*\%([-+]\|\s\*\|\d\+[.)]\)\s\+')
  endif
endfunction

" ----- the clock

let s:clock = {}
let s:clock_timer = -1

function! s:duration(minutes) abort
  return printf('%d:%02d', a:minutes / 60, a:minutes % 60)
endfunction

" What Emacs's mode line shows while a clock runs, for 'statusline':
" `[0:25/1:00] (Write the report)`, then the relative timer or the
" countdown while one is on (`<0:05:23>`); or an empty string.
function! org_lsp#clock_status() abort
  let parts = []
  if !empty(s:clock)
    let minutes = s:clock.minutes + float2nr(reltimefloat(reltime(s:clock.received)) / 60)
    let effort = get(s:clock, 'effort', v:null)
    let time = effort is v:null ? s:duration(minutes) : s:duration(minutes) . '/' . s:duration(effort)
    call add(parts, printf('[%s] (%s)', time, s:clock.heading))
  endif
  if !empty(s:timer)
    let seconds = s:timer.seconds
    if !s:timer.paused
      let passed = float2nr(reltimefloat(reltime(s:timer.received)))
      let seconds = s:timer.countdown ? max([0, seconds - passed]) : seconds + passed
    endif
    call add(parts, printf('<%d:%02d:%02d>', seconds / 3600, seconds % 3600 / 60, seconds % 60))
  endif
  return join(parts, ' ')
endfunction

" The relative timer or the countdown (`org-lsp/timer`): the statusline
" shows its seconds as they pass.
let s:timer = {}
let s:timer_ticks = -1

function! s:timer_update(params) abort
  if type(a:params) == v:t_dict && has_key(a:params, 'seconds')
    let s:timer = {'seconds': a:params.seconds, 'countdown': get(a:params, 'countdown', v:false) is v:true,
          \ 'paused': get(a:params, 'paused', v:false) is v:true, 'received': reltime()}
  else
    let s:timer = {}
  endif
  let running = !empty(s:timer) && !s:timer.paused
  if running && s:timer_ticks == -1
    let s:timer_ticks = timer_start(1000, {-> execute('redrawstatus!')}, {'repeat': -1})
  elseif !running && s:timer_ticks != -1
    call timer_stop(s:timer_ticks)
    let s:timer_ticks = -1
  endif
  redrawstatus!
endfunction

function! s:clock_update(params) abort
  if type(a:params) == v:t_dict && has_key(a:params, 'heading')
    let s:clock = extend(copy(a:params), {'received': reltime(), 'minutes': get(a:params, 'minutes', 0)})
  else
    let s:clock = {}
  endif
  if !empty(s:clock) && s:clock_timer == -1
    let s:clock_timer = timer_start(60000, {-> execute('redrawstatus!')}, {'repeat': -1})
  elseif empty(s:clock) && s:clock_timer != -1
    call timer_stop(s:clock_timer)
    let s:clock_timer = -1
  endif
  redrawstatus!
  silent doautocmd <nomodeline> User OrgLspClock
endfunction

" ----- folds

" 'foldexpr' of Org buffers: the levels of the server's folding ranges,
" fetched after each change.
function! org_lsp#foldexpr(lnum) abort
  return get(get(b:, 'org_lsp_folds', []), a:lnum - 1, 0)
endfunction

function! s:fold_ranges(nr) abort
  let ranges = get(b:, 'org_lsp_fold_ranges', [])
  return a:nr == bufnr() ? ranges : getbufvar(a:nr, 'org_lsp_fold_ranges', [])
endfunction

function! s:update_folds(nr) abort
  let params = {'textDocument': {'uri': org_lsp#uri(a:nr)}}
  call org_lsp#request(a:nr, 'textDocument/foldingRange', params, function('s:got_folds', [a:nr]))
endfunction

function! s:got_folds(nr, error, result) abort
  if !empty(a:error) || type(a:result) != v:t_list || !bufexists(a:nr)
    return
  endif
  let lines_count = len(getbufline(a:nr, 1, '$'))
  let depth = repeat([0], lines_count)
  let starts = {}
  let ranges = []
  for range in sort(copy(a:result), {a, b -> a.startLine - b.startLine})
    if range.endLine <= range.startLine
      continue
    endif
    call add(ranges, [range.startLine + 1, range.endLine + 1])
    for line in range(range.startLine, min([range.endLine, lines_count - 1]))
      let depth[line] += 1
    endfor
    let starts[range.startLine] = 1
  endfor
  let levels = map(depth, {i, d -> has_key(starts, i) ? '>' . d : d})
  call setbufvar(a:nr, 'org_lsp_fold_ranges', ranges)
  if levels == getbufvar(a:nr, 'org_lsp_folds', [])
    return
  endif
  call setbufvar(a:nr, 'org_lsp_folds', levels)
  for win in win_findbuf(a:nr)
    call win_execute(win, 'call org_lsp#refold()')
  endfor
endfunction

" Fold the window again with the new levels; closed folds stay closed.
function! org_lsp#refold() abort
  " The first time the window shows the buffer: as it opens.
  let key = 'org_lsp_folded_' . bufnr()
  let first = !has_key(w:, key)
  let w:[key] = 1
  let closed = first ? [] : filter(map(copy(s:fold_ranges(bufnr())), 'v:val[0]'), 'foldclosed(v:val) == v:val')
  setlocal foldmethod=manual
  setlocal foldexpr=org_lsp#foldexpr(v:lnum) foldmethod=expr
  if first
    let &l:foldlevel = org_lsp#config().startup_folded ==# 'overview' ? 0 : 99
    if empty(org_lsp#config().startup_folded)
      let params = {'textDocument': {'uri': org_lsp#uri(bufnr())}}
      call org_lsp#request(bufnr(), 'org-lsp/startupFolds', params, function('s:close_startup_folds', [bufnr(), win_getid()]))
    endif
    return
  endif
  silent! normal! zR
  for line in closed
    execute 'silent! ' . line . 'foldclose'
  endfor
endfunction

" The folds the server says a file opens with closed (`org_startup_folded`,
" `#+STARTUP:`, `VISIBILITY` properties), innermost first.
function! s:close_startup_folds(nr, win, error, result) abort
  if !empty(a:error) || type(a:result) != v:t_dict || winbufnr(a:win) != a:nr
    return
  endif
  for fold in get(a:result, 'folds', [])
    call win_execute(a:win, 'silent! ' . (fold.startLine + 1) . 'foldclose')
  endfor
endfunction

" Cycle the subtree (or block, drawer) at the cursor, as Org's TAB does:
" folded, children visible, everything visible.
" Whether line `line` is a headline with the ARCHIVE tag of its own.
function! s:archived(line) abort
  let tags = matchstr(getline(a:line), '^\*\+\s\%(.*\s\)\=\zs:[[:alnum:]_@#%:]\+:\s*$')
  return stridx(tags, ':ARCHIVE:') >= 0
endfunction

" `org-cycle-hide-archived-subtrees`: close the folds of archived headlines
" from `start` to `finish`, after the cycling opened them. One in a fold
" closed already is left alone: `foldclose` would close one around it.
function! s:hide_archived(all, start, finish) abort
  for range in a:all
    if range[0] >= a:start && range[1] <= a:finish && foldclosed(range[0]) == -1 && s:archived(range[0])
      execute 'silent! ' . range[0] . 'foldclose'
    endif
  endfor
endfunction

function! s:stays_closed(start) abort
  if s:archived(a:start)
    echomsg 'Subtree is archived and stays closed'
  endif
endfunction

function! org_lsp#cycle() abort
  " In a table, TAB goes to the next field (`org-table-next-field`).
  if getline('.') =~# '^\s*|'
    return org_lsp#run('org.table.next.field')
  endif
  let all = s:fold_ranges(bufnr())
  let line = line('.')
  let current = []
  for range in all
    if range[0] == line
      let current = range
      break
    elseif range[0] < line && line <= range[1] && (empty(current) || range[0] > current[0])
      let current = range
    endif
  endfor
  if empty(current)
    return
  endif
  let [start, finish] = current
  let inner = filter(copy(all), {_, r -> r[0] > start && r[1] <= finish})
  let children = filter(copy(inner), {_, r -> empty(filter(copy(inner), {_, o -> o != r && o[0] < r[0] && r[1] <= o[1]}))})
  if foldclosed(start) != -1
    execute printf('silent! %d,%dfoldopen!', start, finish)
    for child in children
      execute 'silent! ' . child[0] . 'foldclose'
    endfor
    call s:hide_archived(all, start, finish)
    return s:stays_closed(start)
  endif
  for range in inner
    if foldclosed(range[0]) != -1
      execute printf('silent! %d,%dfoldopen!', start, finish)
      call s:hide_archived(all, start, finish)
      return s:stays_closed(start)
    endif
  endfor
  execute 'silent! ' . start . 'foldclose'
endfunction

" Cycle the whole file between overview (all folded) and all visible but
" the archived subtrees.
function! org_lsp#global_cycle() abort
  " In a table, S-TAB goes to the previous field.
  if getline('.') =~# '^\s*|'
    return org_lsp#run('org.table.previous.field')
  endif
  if &l:foldlevel == 0
    normal! zR
    call s:hide_archived(s:fold_ranges(bufnr()), 1, line('$'))
  else
    normal! zM
  endif
endfunction

" ----- highlighting

" The prop types of the server's tokens: one per selector of the manifest's
" highlights (`type`, `*.modifier`, `type.modifier`), each with a group
" linked to the first of its groups that exists.
let s:selectors = {}

function! s:group_name(selector) abort
  return 'OrgLsp_' . substitute(substitute(a:selector, '\*', 'Any', ''), '\.', '_', 'g')
endfunction

" The manifest names Neovim's groups (nvim-orgmode's, then treesitter's
" and Neovim's standard ones). Vim has none of the first, and these stand
" for the others.
let s:vim_groups = {
      \ '@comment': 'Comment', '@comment.error': 'Error', '@keyword': 'Keyword',
      \ '@property': 'Identifier', '@tag.attribute': 'Type', '@number': 'Number',
      \ '@markup.list': 'Special', '@markup.list.unchecked': 'Special', '@markup.list.checked': 'Comment',
      \ '@markup.link': 'Underlined', '@markup.link.url': 'Underlined', '@markup.link.label': 'Underlined',
      \ '@markup.raw': 'String', '@markup.math': 'Special', '@function.macro': 'Macro',
      \ '@punctuation.special': 'Delimiter',
      \ '@markup.strong': 'OrgLspBold', '@markup.italic': 'OrgLspItalic',
      \ '@markup.underline': 'OrgLspUnderline', '@markup.strikethrough': 'OrgLspStrikethrough',
      \ 'DiagnosticError': 'WarningMsg', 'DiagnosticWarn': 'Statement', 'DiagnosticOk': 'MoreMsg',
      \ }

function! org_lsp#define_highlights() abort
  highlight default OrgLspBold term=bold cterm=bold gui=bold
  highlight default OrgLspItalic term=italic cterm=italic gui=italic
  highlight default OrgLspUnderline term=underline cterm=underline gui=underline
  highlight default OrgLspStrikethrough term=strikethrough cterm=strikethrough gui=strikethrough
  let s:selectors = {}
  for look in get(org_lsp#manifest(), 'highlights', [])
    let candidates = map(copy(look.vim), {_, g -> get(s:vim_groups, g, g)})
    call filter(candidates, {_, g -> g =~# '^\w\+$'})
    if empty(candidates)
      continue
    endif
    let target = candidates[-1]
    for candidate in candidates
      if hlexists(candidate)
        let target = candidate
        break
      endif
    endfor
    let group = s:group_name(look.token)
    execute 'highlight default link' group target
    let type = 'org_lsp_' . look.token
    let priority = look.token =~# '^\*\.' ? 101 : look.token =~# '\.' ? 102 : 100
    if empty(prop_type_get(type))
      call prop_type_add(type, {'highlight': group, 'combine': 1, 'priority': priority})
    endif
    let s:selectors[look.token] = type
  endfor
endfunction

" `K` in the agenda: the entry of the line in a popup at the cursor, its
" headline and its text with each drawer folded, highlighted as the server
" highlights Org files. It goes when the cursor moves.
function! org_lsp#preview() abort
  let [nr, context] = s:cursor_context()
  let params = {'textDocument': context.textDocument, 'position': context.position}
  call org_lsp#request(nr, 'org-lsp/preview', params, function('s:show_preview', [win_getid(), getcurpos()]))
endfunction

function! s:show_preview(win, position, error, result) abort
  if !empty(a:error)
    return s:warn(get(a:error, 'message', 'no entry to show'))
  endif
  if type(a:result) != v:t_dict
    echomsg 'org-lsp: No entry on this line'
    return
  endif
  " The cursor moved on while the server answered.
  if win_getid() != a:win || getcurpos() != a:position
    return
  endif
  let lines = split(a:result.text, "\n", 1)
  let id = popup_atcursor(lines, {
        \ 'border': [], 'padding': [0, 1, 0, 1], 'wrap': 1, 'maxwidth': max([1, &columns - 6]),
        \ 'moved': 'any', 'close': 'click',
        \ })
  call s:paint(winbufnr(id), lines, get(a:result, 'data', []))
  return id
endfunction

function! s:update_tokens(nr) abort
  let params = {'textDocument': {'uri': org_lsp#uri(a:nr)}}
  call org_lsp#request(a:nr, 'textDocument/semanticTokens/full', params, function('s:got_tokens', [a:nr, getbufvar(a:nr, 'changedtick')]))
endfunction

function! s:got_tokens(nr, tick, error, result) abort
  if !empty(a:error) || type(a:result) != v:t_dict || !bufloaded(a:nr) || getbufvar(a:nr, 'changedtick') != a:tick
    return
  endif
  call s:paint(a:nr, getbufline(a:nr, 1, '$'), get(a:result, 'data', []))
endfunction

" The tokens each buffer was last painted with, as
" `[line, character, length, type, modifiers]`.
let s:painted = {}

" Highlight the `lines` of buffer `nr` with the semantic tokens `data`.
function! s:paint(nr, lines, data) abort
  let tokens = []
  let data = a:data
  let line = 0
  let character = 0
  for i in range(0, len(data) - 1, 5)
    let line += data[i]
    let character = data[i] == 0 ? character + data[i + 1] : data[i + 1]
    call add(tokens, [line, character, data[i + 2], data[i + 3], data[i + 4]])
  endfor
  call s:paint_absolute(a:nr, a:lines, tokens)
endfunction

function! s:paint_absolute(nr, lines, tokens) abort
  if empty(s:selectors)
    call org_lsp#define_highlights()
  endif
  let s:painted[a:nr] = a:tokens
  let manifest = org_lsp#manifest()
  let types = map(copy(manifest.tokenTypes), 'v:val.id')
  let modifiers = map(copy(manifest.tokenModifiers), 'v:val.id')
  let props = {}
  let lines = a:lines
  for [line, character, length, type_index, modifier_bits] in a:tokens
    let type = get(types, type_index, '')
    let text = get(lines, line, '')
    let start = s:byte(text, character)
    let end = s:byte(text, character + length)
    if end <= start
      continue
    endif
    let selectors = [type]
    for bit in range(len(modifiers))
      if and(modifier_bits, 1 << bit)
        call add(selectors, '*.' . modifiers[bit])
        call add(selectors, type . '.' . modifiers[bit])
      endif
    endfor
    for selector in selectors
      if has_key(s:selectors, selector)
        call add(get(extend(props, {selector: []}, 'keep'), selector), [line + 1, start + 1, line + 1, end + 1])
      endif
    endfor
  endfor
  call prop_remove({'types': values(s:selectors), 'bufnr': a:nr, 'all': 1})
  for [selector, positions] in items(props)
    call prop_add_list({'bufnr': a:nr, 'type': s:selectors[selector]}, positions)
  endfor
endfunction

" ----- attaching

let s:pending = {}

" After a change, highlighting and folds are fetched again, once typing
" pauses.
function! s:changed(nr) abort
  if has_key(s:pending, a:nr)
    call timer_stop(s:pending[a:nr])
  endif
  let s:pending[a:nr] = timer_start(200, {-> s:refresh(a:nr)})
endfunction

function! s:refresh(nr) abort
  silent! call remove(s:pending, a:nr)
  if !bufloaded(a:nr)
    return
  endif
  let config = org_lsp#config()
  if config.highlight
    call s:update_tokens(a:nr)
  endif
  if bufnr() == a:nr && &filetype ==# 'org'
    call s:src_syntax()
  endif
  if config.folding && getbufvar(a:nr, '&filetype') ==# 'org'
    call s:update_folds(a:nr)
  endif
endfunction

" Set up an Org buffer, the agenda, or a capture buffer.
function! org_lsp#attach() abort
  if empty(org_lsp#manifest()) || exists('b:org_lsp_attached')
    return
  endif
  let b:org_lsp_attached = 1
  let nr = bufnr()
  if &filetype ==# get(org_lsp#manifest().agenda, 'languageId', 'org-agenda')
    setlocal autoread readonly noswapfile bufhidden=hide nowrap nonumber norelativenumber nofoldenable
    call org_lsp#set_keys('agenda')
  else
    call org_lsp#set_keys('org')
    if s:is_capture(bufname(nr))
      call org_lsp#set_keys('capture')
    endif
    call s:src_syntax()
  endif
  augroup org_lsp_buffer
    execute 'autocmd! * <buffer=' . nr . '>'
    execute 'autocmd TextChanged,InsertLeave,BufReadPost <buffer=' . nr . '> call s:changed(' . nr . ')'
    execute 'autocmd BufWinEnter <buffer=' . nr . '> call s:changed(' . nr . ')'
  augroup END
  " The client may still be starting the server: ask again until it
  " answers.
  call s:first_refresh(nr, 0)
endfunction

" Org's names for languages (`org-src-lang-modes`, Emacs's mode names)
" where Vim's syntax file has another.
let s:src_aliases = {
      \ 'emacs-lisp': 'lisp', 'elisp': 'lisp', 'shell': 'sh', 'bash': 'sh',
      \ 'js': 'javascript', 'ts': 'typescript', 'py': 'python', 'rb': 'ruby',
      \ 'c++': 'cpp', 'yml': 'yaml', 'sqlite': 'sql', 'rs': 'rust',
      \ }

" Source blocks (`#+begin_src python`) in their language's syntax, as Emacs
" shows them with `org-src-fontify-natively`: the language's syntax file
" included once for each language the buffer has, as Vim's Markdown does
" for fenced code. The server leaves a block's code to the editor.
function! s:src_syntax() abort
  if !org_lsp#config().src_highlight
    return
  endif
  let done = get(b:, 'org_lsp_src_languages', {})
  " Source and export blocks (`#+begin_src LANG`, `#+begin_export LANG`),
  " and inline source blocks (`src_LANG{CODE}`), by language.
  let wanted = []
  for line in getline(1, '$')
    let block = matchlist(line, '\c^\s*#+begin_\(src\|export\)\s\+\(\S\+\)')
    if !empty(block)
      call add(wanted, [tolower(block[1]), block[2]])
      continue
    endif
    let at = 0
    while 1
      let [name, start, end] = matchstrpos(line, '\%(^\|[^[:alnum:]_-]\)\@<=src_\zs[^ \t[{]\+', at)
      if start < 0
        break
      endif
      call add(wanted, ['inline', name])
      let at = end
    endwhile
  endfor
  for [kind, name] in wanted
    if has_key(done, kind . ':' . name)
      continue
    endif
    let done[kind . ':' . name] = 1
    let lang = get(s:src_aliases, tolower(name), tolower(name))
    if lang !~# '^\w\+$' || empty(globpath(&runtimepath, 'syntax/' . lang . '.vim'))
      continue
    endif
    let cluster = 'orgLspSrc_' . lang
    if !has_key(done, '@' . lang)
      let done['@' . lang] = 1
      let current = get(b:, 'current_syntax', v:null)
      unlet! b:current_syntax
      execute 'silent! syntax include @' . cluster . ' syntax/' . lang . '.vim'
      if current is v:null
        unlet! b:current_syntax
      else
        let b:current_syntax = current
      endif
    endif
    let pattern = escape(name, '/\.*$^~[]')
    if kind ==# 'inline'
      " To the first `}`: braces in the code end it early.
      execute 'syntax region ' . cluster . ' oneline keepend matchgroup=orgLspSrcDelimiter'
            \ . ' start=/\%(^\|[^[:alnum:]_-]\)\@<=src_' . pattern . '\%(\[[^]]*\]\)\={/'
            \ . ' end=/}/ contains=@' . cluster
    else
      execute 'syntax region ' . cluster . ' keepend matchgroup=orgLspSrcDelimiter'
            \ . ' start=/\c^\s*#+begin_' . kind . '\s\+' . pattern . '\%(\s\|$\).*$/'
            \ . ' end=/\c^\s*#+end_' . kind . '\>.*$/ contains=@' . cluster
    endif
  endfor
  let b:org_lsp_src_languages = done
endfunction

" Whether the server answers for buffer `nr`.
function! org_lsp#ready(nr) abort
  return !empty(org_lsp#adapter()) && s:call('ready', a:nr)
endfunction

function! s:first_refresh(nr, tries) abort
  if !bufloaded(a:nr) || a:tries > 100
    return
  endif
  if !s:call('ready', a:nr)
    call timer_start(100, {-> s:first_refresh(a:nr, a:tries + 1)})
    return
  endif
  call s:refresh(a:nr)
endfunction
