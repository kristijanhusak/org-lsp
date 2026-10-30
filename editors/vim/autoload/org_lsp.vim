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
      \ 'download': 1,
      \ 'release': 'nightly',
      \ 'settings': {},
      \ 'client': 'auto',
      \ 'folding': 1,
      \ 'startup_folded': '',
      \ 'highlight': 1,
      \ 'src_highlight': 1,
      \ 'virtual_indent': 1,
      \ 'conceal': 1,
      \ 'indent': 1,
      \ 'win_split_mode': 'split',
      \ 'win_split_size': 0,
      \ 'calendar': 1,
      \ 'calendar_win_mode': 'botright split',
      \ 'mappings': {
      \   'global': {
      \     'org.agenda.dispatch': '<Leader>oa',
      \     'org.capture': '<Leader>oc',
      \     'org.clock.goto': '<Leader>oxj',
      \     'org.journal.new.entry': '<Leader>jj',
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
" checkout, `org-lsp` on PATH, or the downloaded one; empty when there is
" none.
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
  if executable('org-lsp')
    return ['org-lsp']
  endif
  let downloaded = org_lsp#download#path()
  return executable(downloaded) ? [downloaded] : []
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
" server writes, whatever the client says; that the server writes the
" files it makes, which coc.nvim would have as an empty line; and that
" this plugin shows the server's messages and asks Emacs's questions, as
" vim-lsp and ALE show neither, and the others put their name before the
" messages. coc.nvim shows the documents the server asks for on its own,
" and passes on only an error's message: it is told the documents to show
" as a notification, and that a command does not apply as a result.
function! org_lsp#init_options() abort
  let coc = org_lsp#adapter() ==# 'coc'
  let integration = {
        \ 'org_client': v:true,
        \ 'show_document': v:true,
        \ 'show_document_notification': coc ? v:true : v:false,
        \ 'refusal_results': coc ? v:true : v:false,
        \ 'create_files': v:false,
        \ 'view_changed': v:true,
        \ 'date_prompt': org_lsp#config().calendar ? v:true : v:false,
        \ 'virtual_documents': v:false,
        \ 'virtual_indent': org_lsp#config().virtual_indent && s:has_virtual_text() ? v:true : v:false,
        \ 'display': s:has_virtual_text() ? v:true : v:false,
        \ 'line_highlights': v:true,
        \ 'code_blocks': org_lsp#config().src_highlight ? v:true : v:false,
        \ 'syntax': v:true,
        \ 'run_command': v:true,
        \ 'message_requests': v:false,
        \ 'message_notifications': v:true,
        \ }
  " Those that are off by default are left out when off: a server older
  " than the plugin does not know the newer ones.
  let off = ['show_document_notification', 'refusal_results', 'date_prompt', 'virtual_indent', 'display', 'code_blocks']
  call filter(integration, {key, value -> value isnot v:false || index(off, key) < 0})
  return extend(deepcopy(org_lsp#config().settings), integration)
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

" What the server says (Emacs's text, as its echo area shows it), in the
" look of its `type`: 1 an error, 2 a warning, else a message. Shown and
" kept in the messages even when it comes while a client does something
" silently.
function! s:say(message, type) abort
  execute 'echohl' get({1: 'ErrorMsg', 2: 'WarningMsg'}, a:type, 'None')
  for line in split(a:message, "\n")
    unsilent echomsg line
  endfor
  echohl None
endfunction

" ----- the client

" The adapter for the installed client: `lsp` (yegappan/lsp), `coc`,
" `vim_lsp` or `ale`; empty when there is none.
function! org_lsp#adapter() abort
  if !exists('s:adapter')
    let wanted = org_lsp#config().client
    if wanted ==# 'auto'
      let wanted = exists('g:loaded_lsp') ? 'lsp'
            \ : exists('g:did_coc_loaded') ? 'coc'
            \ : exists('g:lsp_loaded') ? 'vim_lsp'
            \ : exists('g:loaded_ale') ? 'ale' : ''
    endif
    let s:adapter = substitute(wanted, '-', '_', 'g')
  endif
  return s:adapter
endfunction

function! s:call(name, ...) abort
  return call('org_lsp#adapter#' . org_lsp#adapter() . '#' . a:name, a:000)
endfunction

" Register the server with the client. Called once, after the plugins
" load, or once the server is downloaded.
function! org_lsp#register() abort
  if empty(org_lsp#adapter()) || get(s:, 'registered', 0)
    return
  endif
  let cmd = org_lsp#server_command()
  if empty(cmd)
    return
  endif
  let s:registered = 1
  let manifest = org_lsp#manifest()
  let filetypes = ['org', get(get(manifest, 'agenda', {}), 'languageId', 'org-agenda'),
        \ get(get(manifest, 'datePrompt', {}), 'languageId', 'org-date')]
  call s:call('register', cmd, org_lsp#init_options(), filetypes)
endfunction

" `:OrgLspDownload`, and the first Org file without a server: download the
" server of the `release` option. The server is registered with the
" client, and starts for the Org buffers open, when there was none before;
" a server that runs already is replaced the next time Vim starts.
function! org_lsp#download() abort
  call org_lsp#download#run(org_lsp#config().release, {path -> s:downloaded()})
endfunction

function! s:downloaded() abort
  if get(s:, 'registered', 0)
    echomsg 'org-lsp: the new server runs the next time Vim starts'
    return
  endif
  unlet! s:manifest
  call org_lsp#register()
  call org_lsp#set_keys('global')
  " The FileType event again, for the client to start the server for them
  " and for this plugin to attach.
  let manifest = org_lsp#manifest()
  let filetypes = ['org', get(get(manifest, 'agenda', {}), 'languageId', 'org-agenda')]
  for info in getbufinfo({'bufloaded': 1})
    let filetype = getbufvar(info.bufnr, '&filetype')
    if index(filetypes, filetype) >= 0
      call setbufvar(info.bufnr, '&filetype', filetype)
    endif
  endfor
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

" An Org buffer read again from its file: its text properties are gone,
" so its tokens are painted again on every line; and vim-lsp does not tell
" the server, whose agenda and edits would go on from the old text.
function! org_lsp#reread(nr) abort
  silent! call remove(s:painted, a:nr)
  if org_lsp#adapter() ==# 'vim_lsp' && getbufvar(a:nr, '&filetype') ==# 'org'
    call org_lsp#adapter#vim_lsp#reread(a:nr)
  endif
endfunction

" Send a request about buffer `bufnr`; `Callback(error, result)` gets the
" answer, `error` being {} when there is none. Changes not yet sent go
" first.
function! org_lsp#request(bufnr, method, params, Callback) abort
  call s:flush_changes(a:bufnr)
  call s:call('request', a:bufnr, a:method, a:params, a:Callback)
endfunction

" The changes of the Org buffers the server has not heard of yet, sent
" first: an edit the server asked for, made in a buffer other than the
" current one (from the agenda), is sent by some clients only when that
" buffer is the current one again, and the next command would act on the
" text before it.
let s:flushed = {}
function! s:flush_changes(bufnr) abort
  if s:is_view_type(getbufvar(a:bufnr, '&filetype'))
    call s:give_view(a:bufnr)
  endif
  for info in getbufinfo({'bufloaded': 1})
    let nr = info.bufnr
    if nr != a:bufnr && (getbufvar(nr, '&filetype') !=# 'org' || get(s:flushed, nr, -1) == info.changedtick)
      continue
    endif
    let s:flushed[nr] = info.changedtick
    call s:call('flush', nr)
  endfor
endfunction

" How many answers `org_lsp#request_sync()` waits for: the timers that
" fire meanwhile leave the buffer's colors and folds for later, as the
" answer is to come at once (the indentation of a line `o` opened).
let s:waiting = 0

" A request answered before this returns, or {'error': …} after `timeout`
" milliseconds.
function! org_lsp#request_sync(bufnr, method, params, timeout) abort
  let state = {'done': 0}
  function! state.answer(error, result) dict abort
    let self.done = 1
    let self.error = a:error
    let self.result = a:result
  endfunction
  let start = reltime()
  let s:waiting += 1
  try
    call org_lsp#request(a:bufnr, a:method, a:params, state.answer)
    " Vim writes to the server a pipe's worth (64 KB) each time it waits:
    " the whole text of a big file goes in pieces, each after a wait.
    while !state.done && reltimefloat(reltime(start)) * 1000 < a:timeout
      sleep 1m
    endwhile
  finally
    let s:waiting -= 1
  endtry
  return state.done ? {'error': state.error, 'result': state.result} : {'error': {'message': 'timed out'}}
endfunction

" ----- positions and URIs

" The URI of buffer `bufnr`'s file, as the LSP client writes it: the
" server knows a document by the URI the client opened it with. Each byte
" of the UTF-8 name is percent-encoded (`file:///tmp/caf%C3%A9.org`), and
" on Windows the drive follows with forward slashes
" (`file:///C:/notes/a.org`). A client that spells it otherwise (ALE
" writes `%c3`, vim-lsp leaves `@` as it is) is asked for its own.
function! org_lsp#uri(bufnr) abort
  let path = fnamemodify(bufname(a:bufnr), ':p')
  if !empty(org_lsp#adapter()) && exists('*org_lsp#adapter#' . org_lsp#adapter() . '#uri')
    return s:call('uri', path)
  endif
  if has('win32')
    let path = tr(path, '\', '/')
  endif
  let encoded = substitute(path, '[^A-Za-z0-9_.~/:-]', '\=s:percent(submatch(0))', 'g')
  return 'file://' . (encoded[0] ==# '/' ? '' : '/') . encoded
endfunction

" Character `char` as the percent-encoding of its bytes.
function! s:percent(char) abort
  return join(map(range(len(a:char)), {_, i -> printf('%%%02X', char2nr(a:char[i]))}), '')
endfunction

" The file of `uri`: its bytes decoded one by one (`%C3%A9` is the two
" bytes of `é`), so that the name comes back as it was.
function! org_lsp#uri_to_path(uri) abort
  let path = substitute(a:uri, '^file://', '', '')
  let path = substitute(path, '%\(\x\x\)', '\=printf("%c", str2nr(submatch(1), 16))', 'g')
  if has('win32') && path =~# '^/\a:'
    let path = path[1:]
  endif
  return path
endfunction

" Column `character` of `text` as the client counts it, as a byte index.
function! s:byte(text, character) abort
  return s:byte_in(s:encoding(), a:text, a:character)
endfunction

" `s:byte()` for the server's position encoding `encoding`.
function! s:byte_in(encoding, text, character) abort
  if a:encoding ==# 'utf-8'
    return a:character
  endif
  let index = a:encoding ==# 'utf-32' ? byteidx(a:text, a:character) : byteidx(a:text, a:character, 1)
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
  " In the date prompt's calendar, the day under the cursor: org-journal's
  " keys there act on it.
  if s:is_prompt(expand('%:p'))
    let position = {'line': line('.') - 1, 'character': col('.') - 1}
    return [nr, {'textDocument': {'uri': org_lsp#uri(bufnr())}, 'position': position}]
  endif
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
    return s:warn('no LSP client found: install yegappan/lsp, coc.nvim, vim-lsp or ALE')
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
  " Column view's commands act on the column picked.
  if a:name =~# '^org\.columns\.' && org_lsp#columns_on(nr)
    let context.column = getbufvar(nr, 'org_lsp_column', 0)
  endif
  if get(opts, 'count', 0) > 0
    let context.count = opts.count
  endif
  " The Visual selection just left, as Emacs's region, the point at its
  " start.
  if get(opts, 'region', 0) && nr == bufnr()
    let context.range = s:selection_range()
    let context.position = context.range.start
  endif
  " The lines of a range given to `:Org`.
  if has_key(opts, 'lines') && nr == bufnr()
    let context.range = {'start': {'line': opts.lines[0] - 1, 'character': 0}, 'end': {'line': opts.lines[1], 'character': 0}}
    let context.position = context.range.start
  endif
  " The register the key was typed with (`"a`), as `y` and `p` take it;
  " what the command reads as Emacs's latest kill comes from there.
  let opts = extend(copy(opts), {'register': v:register}, 'keep')
  if get(s:command(a:name), 'clipboard', v:false)
    let context.clipboard = getreg(opts.register)
  endif
  " The closed folds, for what is visible.
  if get(s:command(a:name), 'folds', v:false)
    let context.folded = s:closed_folds()
  endif
  " Below the cursor's line, as nvim-orgmode's `<Leader><CR>` puts it:
  " never above it, and nothing split.
  if get(opts, 'append', 0)
    let context.below = v:true
  endif
  " Filing a capture or a note takes the buffer's text as it is here: some
  " clients tell the server of the last changes only a while later. A
  " newline ends it when the file has one ('eol'): the server writes the
  " buffer without, and one more would be a blank line after the entry.
  if index(['org.capture.finish', 'org.capture.refile'], a:name) >= 0
    let context.text = join(getline(1, '$'), "\n") . (&eol ? "\n" : '')
  endif
  " Whether the org-roam buffer shows, which the command hides then.
  if a:name ==# 'org.roam.buffer.toggle'
    let context.visible = org_lsp#roam_buffer_shown() ? v:true : v:false
  endif
  " What the server asked the command to be run with (a capture an
  " org-protocol URL started).
  if type(get(opts, 'argument', 0)) == v:t_dict
    call extend(context, opts.argument)
  endif
  call s:execute(nr, a:name, context, opts)
endfunction

" A server still starting (the agenda asked for with no Org file open, the
" scratch file just loaded) gets the command once it answers, for up to
" ten seconds.
function! s:execute(bufnr, name, context, opts, ...) abort
  let tries = get(a:, 1, 0)
  if tries < 100 && !org_lsp#ready(a:bufnr)
    call timer_start(100, {-> s:execute(a:bufnr, a:name, a:context, a:opts, tries + 1)})
    return
  endif
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
      call s:say(get(a:error, 'message', 'the command failed'), 2)
    endif
    return
  endif
  let result = type(a:result) == v:t_dict ? a:result : {}
  " The edit and the cursor, carried in the result (`org.meta.return`);
  " from its key, typing goes on there, in insert mode, as with
  " nvim-orgmode: by keys ahead of those typed since, as `:startinsert`
  " in a callback waits for the next command.
  if type(get(result, 'edits', 0)) == v:t_list && bufnr() == a:bufnr
    let at_end = s:apply_carried(a:context.textDocument.uri, result)
    if get(a:opts, 'append', 0)
      call feedkeys(at_end ? 'A' : 'i', 'ni')
    endif
  endif
  " What was cut or copied goes to the register, by lines, or by
  " characters for a kill of part of a line (`org.kill.line`).
  if type(get(result, 'copy', 0)) == v:t_string
    call setreg(get(a:opts, 'register', '"'), result.copy, get(result, 'copyCharacters', v:false) is v:true ? 'c' : 'l')
  endif
  " A sparse tree: everything folded but the lines shown.
  if type(get(result, 'sparse', 0)) == v:t_dict && bufnr() == a:bufnr
    call s:show_sparse(a:bufnr, result.sparse)
  endif
  " The agenda's file may have been written anew.
  call s:reload_views()
  if type(get(result, 'save', 0)) == v:t_list
    call org_lsp#save(result.save)
  endif
  if type(get(result, 'message', 0)) == v:t_string
    " Shown and kept in the messages, though the answer comes while a
    " client does something silently (coc.nvim opening a file it edits).
    unsilent echomsg result.message
  endif
  if type(get(result, 'close', 0)) == v:t_string
    call org_lsp#close(result.close)
  endif
  if type(get(result, 'input', 0)) == v:t_dict
    " Asked from a callback, the prompt waits for the screen to settle.
    call timer_start(0, {-> s:ask(a:bufnr, a:name, a:context, a:opts, result.input)})
  endif
  " What the command showed as inlay hints (the clock's times, a shrunk
  " column), for clients that ask for them again only after a change.
  if index(['lsp', 'vim_lsp'], org_lsp#adapter()) >= 0
    call s:call('inlay_hints', a:bufnr)
  endif
endfunction

" The closed folds of the current window, as their first and last lines,
" from 0.
function! s:closed_folds() abort
  let folds = []
  let lnum = 1
  while lnum <= line('$')
    if foldclosed(lnum) == lnum
      call add(folds, [lnum - 1, foldclosedend(lnum) - 1])
      let lnum = foldclosedend(lnum) + 1
    else
      let lnum += 1
    endif
  endwhile
  return folds
endfunction

" Show a sparse tree (`org-sparse-tree`) in the current window: all folds
" closed, then those around each of `sparse.lines` opened, as `org-occur`
" shows its matches; the matches highlighted (`OrgLspSparseMatch`) until
" the text changes, or with `keepHighlights` until they are replaced. The
" cursor stays. With `keep` (`org-reveal`), only the folds that start on
" the lines of `unfold` open, outer ones first; the others stay as they
" are, and `highlights`, when given (`C-c C-c`), replace those shown.
function! s:show_sparse(nr, sparse) abort
  highlight default link OrgLspSparseMatch Search
  if empty(prop_type_get('org_lsp_sparse'))
    call prop_type_add('org_lsp_sparse', {'highlight': 'OrgLspSparseMatch', 'priority': 200, 'combine': 1})
  endif
  let keep = get(a:sparse, 'keep', v:false)
  if !keep || has_key(a:sparse, 'highlights')
    call prop_remove({'type': 'org_lsp_sparse', 'bufnr': a:nr, 'all': 1})
    call setbufvar(a:nr, 'org_lsp_sparse_shown', getbufvar(a:nr, 'org_lsp_sparse_shown', 0) + 1)
  endif
  for range in get(a:sparse, 'highlights', [])
    let start = s:byte(getline(range.start.line + 1), range.start.character)
    let end = s:byte(getline(range.end.line + 1), range.end.character)
    call prop_add(range.start.line + 1, start + 1,
          \ {'end_lnum': range.end.line + 1, 'end_col': end + 1, 'type': 'org_lsp_sparse', 'bufnr': a:nr})
  endfor
  if keep
    if &foldenable
      for line in get(a:sparse, 'unfold', [])
        if foldclosed(line + 1) == line + 1
          execute (line + 1) . 'foldopen'
        endif
      endfor
    endif
    return
  endif
  if &foldenable
    let view = winsaveview()
    normal! zM
    for line in get(a:sparse, 'lines', [])
      call cursor(line + 1, 1)
      normal! zv
    endfor
    call winrestview(view)
  endif
  " Org takes the highlighting away at the next change, unless the server
  " says it stays (`org-remove-highlights-with-change`); a change clears
  " only the highlights it was waiting for.
  if !get(a:sparse, 'keepHighlights', v:false)
    execute 'autocmd TextChanged,TextChangedI <buffer=' . a:nr . '> ++once'
          \ 'call org_lsp#clear_sparse(' . a:nr . ', ' . getbufvar(a:nr, 'org_lsp_sparse_shown') . ')'
  endif
endfunction

" The highlights of a sparse tree taken away, if they are still those
" shown when the change was waited for.
function! org_lsp#clear_sparse(nr, shown) abort
  if getbufvar(a:nr, 'org_lsp_sparse_shown', 0) == a:shown
    call prop_remove({'type': 'org_lsp_sparse', 'bufnr': a:nr, 'all': 1})
  endif
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
    " `expert` (`org-use-fast-todo-selection`): the keys are in the prompt,
    " which shows alone, as Emacs's message does.
    if get(a:input, 'expert', v:false)
      return org_lsp#read_key(a:input.prompt, [], 1)
    endif
    return org_lsp#read_key(a:input.prompt, get(a:input, 'choices', []))
  elseif a:input.kind ==# 'priority'
    " One key, as `org-priority` reads it: a letter, or SPC to remove it.
    return org_lsp#read_key(a:input.prompt, [])
  elseif has_key(a:input, 'choices')
    let lines = [org_lsp#prompt_text(a:input.prompt)]
    for i in range(len(a:input.choices))
      call add(lines, (i + 1) . '. ' . a:input.choices[i].label)
    endfor
    let picked = inputlist(lines)
    return picked > 0 && picked <= len(a:input.choices) ? a:input.choices[picked - 1].key : v:null
  endif
  return org_lsp#read_text(a:input)
endfunction

" A prompt as it is shown: Emacs's own words (`Execute this? `, ending in
" a space) as they are, a title (`Select a TODO state`) followed by `: `.
function! org_lsp#prompt_text(prompt) abort
  if a:prompt =~# '\s$'
    return a:prompt
  endif
  return a:prompt . (a:prompt =~# '[:?]$' ? ' ' : ': ')
endfunction

let s:cancelled = 0

" <Esc> while text is typed for a command: the answer is no answer.
function! org_lsp#cancel_input() abort
  let s:cancelled = 1
  return ''
endfunction

" Text typed for a command (`{prompt, value, completions, complete}`),
" completed from the list when there is one (Emacs's `completing-read`), or
" as a file name; v:null when cancelled with <Esc> or CTRL-C.
function! org_lsp#read_text(input) abort
  let s:completions = get(a:input, 'completions', [])
  let s:separator = get(a:input, 'separator', '')
  " `input()` gives <Esc> the meaning of an empty answer: a mapping of
  " its own says it is none, the user's own put back after.
  let kept = maparg('<Esc>', 'c', 0, 1)
  cnoremap <Esc> <C-\>eorg_lsp#cancel_input()<CR><CR>
  let s:cancelled = 0
  try
    " A file's or a directory's name (`read-file-name`): Vim completes it.
    let complete = get({'file': 'file', 'directory': 'dir'}, get(a:input, 'complete', ''),
          \ 'customlist,org_lsp#complete_input')
    let answer = input(org_lsp#prompt_text(a:input.prompt), get(a:input, 'value', ''), complete)
  catch /^Vim:Interrupt$/
    let s:cancelled = 1
  finally
    cunmap <Esc>
    if !empty(kept)
      call mapset('c', 0, kept)
    endif
  endtry
  return s:cancelled ? v:null : answer
endfunction

let s:completions = []
let s:separator = ''

" The completions of what a command asks to be typed, beginning with what
" is typed, without regard to case. With a separator (the `:` between
" tags), what follows the last one is completed, what comes before kept.
function! org_lsp#complete_input(lead, line, pos) abort
  let before = ''
  let lead = a:lead
  if !empty(s:separator)
    let last = strridx(lead, s:separator)
    if last >= 0
      let before = lead[: last + len(s:separator) - 1]
      let lead = lead[last + len(s:separator) :]
    endif
  endif
  let found = filter(copy(s:completions), {_, c -> stridx(tolower(c), tolower(lead)) == 0})
  return map(found, {_, c -> before . c})
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

" A line of the choices `read_key` shows: the key and the label, a TODO
" keyword in its face as `org-fast-todo-selection` shows it (`face`, its
" own; else `highlight`, the selector of its token).
function! org_lsp#choice_line(choice) abort
  let key = printf('[%s] ', org_lsp#key_name(a:choice.key))
  let line = {'text': key . a:choice.label, 'props': []}
  if empty(s:selectors)
    call org_lsp#define_highlights()
  endif
  let type = has_key(a:choice, 'face') ? s:define_face('todo', a:choice.label, a:choice.face)
        \ : get(s:selectors, get(a:choice, 'highlight', ''), '')
  if !empty(type)
    call add(line.props, {'col': len(key) + 1, 'length': len(a:choice.label), 'type': type})
  endif
  return line
endfunction

" What `getcharstr()` gives while it waits that no key was typed for:
" `<CursorHold>` when it waits in a callback (coc.nvim's, after
" 'updatetime'), and the like.
let s:not_keys = ["\<CursorHold>", "\<Ignore>", "\<FocusGained>", "\<FocusLost>"]

" The next key typed.
function! org_lsp#getkey() abort
  let key = getcharstr()
  while index(s:not_keys, key) >= 0
    let key = getcharstr()
  endwhile
  return key
endfunction

function! org_lsp#read_key(prompt, choices, ...) abort
  " With a third argument, the prompt as it is.
  let text = a:0 > 0 && a:1 ? a:prompt : org_lsp#prompt_text(a:prompt)
  " A prompt of more lines (`expert`'s keys under it, the formula
  " debugger's history above its question) has a command line as high while
  " it waits, as Emacs's echo area grows, not Vim's hit-enter prompt.
  let height = &cmdheight
  let &cmdheight = max([height, len(split(text, "\n", 1))])
  let popup = 0
  if !empty(a:choices) && exists('*popup_create')
    let lines = map(copy(a:choices), {_, c -> org_lsp#choice_line(c)})
    " Over the command line, the question its title.
    let popup = popup_create(lines, {
          \ 'line': &lines - &cmdheight - len(lines) - 1, 'col': 1,
          \ 'border': [], 'title': ' ' . get(split(a:prompt, "\n"), -1, '') . ' ', 'zindex': 300,
          \ })
  endif
  echo text
  redraw
  try
    let key = org_lsp#getkey()
  catch /^Vim:Interrupt$/
    let key = "\<Esc>"
  finally
    let &cmdheight = height
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
  call call('org_lsp#org_command', [0, 0, 0] + a:000)
endfunction

" `:Org` with its range: `:'<,'>Org toggle heading` works on those lines,
" as on a selection of whole lines.
function! org_lsp#org_command(range, first, last, ...) abort
  let id = 'org.' . join(a:0 > 0 ? a:000 : ['agenda'], '.')
  if empty(s:command(id)) && !has_key(s:own, id) && !empty(s:command(id . '.dispatch'))
    let id .= '.dispatch'
  endif
  call org_lsp#run(id, a:range > 0 ? {'lines': [a:first, a:last]} : {})
endfunction

" The next word of `:Org`'s command after those typed: `:Org agenda <Tab>`
" offers `list`, `todos`, `dispatch`, …, and `:Org agenda.<Tab>` the same
" after the dot.
function! org_lsp#complete(lead, line, pos) abort
  let typed = split(a:line[: a:pos - 1 - len(a:lead)])[1:]
  " The words of `lead` up to its last dot, kept as they are.
  let done = matchstr(a:lead, '^.*\.')
  let partial = a:lead[len(done):]
  let prefix = join(['org'] + typed, '.') . '.' . done
  let words = []
  for command in get(org_lsp#manifest(), 'commands', [])
    if stridx(command.command, prefix) == 0
      let word = done . matchstr(command.command[len(prefix):], '^[^.]\+')
      if word !=# done && stridx(word, a:lead) == 0 && index(words, word) < 0
        call add(words, word)
      endif
    endif
  endfor
  return sort(words)
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
  " While it waits, coc.nvim moves the cursor a column to the left; a link
  " followed meanwhile (`org_return_follows_link`) takes it to another line,
  " where it stays.
  if getcurpos() != position && getcurpos()[1] == position[1]
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
  " The server hears of the edit at once, and the folds follow it: those
  " it asks for once the document has it (`org-lsp/folds`) close then.
  let nr = bufnr(org_lsp#uri_to_path(a:uri))
  if !empty(a:result.edits) && nr > 0
    call s:call('flush', nr)
    call s:changed(nr)
  endif
  if type(get(a:result, 'cursor', 0)) != v:t_dict
    return v:false
  endif
  let text = getline(a:result.cursor.line + 1)
  let column = s:byte(text, a:result.cursor.character)
  call cursor(a:result.cursor.line + 1, column + 1)
  redrawstatus
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
  " The keys of the mappings, the plugin's own (`q` for `quit`) and the
  " user's, are theirs: a command of the server's with the same key goes
  " without it.
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

" The keys as a terminal sends them too: CTRL-Space comes as NUL, which
" Vim reads as `<C-@>` (Neovim as `<C-Space>`), and Alt with a character
" (`<M-m>`) as <Esc> then the character, as tmux and most terminals send
" it, which Vim reads as two keys.
function! s:with_terminal_keys(list) abort
  if has('gui_running')
    return a:list
  endif
  let terminal = map(filter(copy(a:list), {_, lhs -> lhs =~? '<C-Space>'}),
        \ {_, lhs -> substitute(lhs, '\c<C-Space>', '<C-@>', 'g')})
  let terminal += map(filter(copy(a:list), {_, lhs -> lhs =~? '^<[MA]-[^<>]>$'}),
        \ {_, lhs -> '<Esc>' . lhs[3]})
  return a:list + terminal
endfunction

function! org_lsp#set_keys(scope) abort
  let [keys, native] = s:keys_in(a:scope)
  let buffer = a:scope ==# 'global' ? '' : '<buffer>'
  " In the read-only agenda, one letter runs at once, unless it starts a
  " longer key (`v` of `vd`).
  let all = flatten(values(copy(keys)))
  for [name, list] in items(keys)
    let command = s:command(name)
    for lhs in s:with_terminal_keys(list)
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
      let nowait = index(['agenda', 'calendar', 'columns'], a:scope) >= 0 && !s:starts_longer(lhs, all) ? '<nowait>' : ''
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

" ----- indentation

" Indentation as Org does it, computed by the server: new lines get the
" column `org-indent-line` gives them ('indentexpr', from the server's
" on-type formatting), and `=` re-indents lines as `org-indent-region`
" does (range formatting). `org_adapt_indentation` decides how far text
" goes.
function! s:indent_attach() abort
  setlocal indentexpr=org_lsp#indent_expr()
  " Re-indent only lines that are opened (`o`, `O`, Enter) or asked for
  " (CTRL-F), as Org does on RET and TAB, not as text is typed.
  setlocal indentkeys=o,O,!^F
  nnoremap <buffer> <expr> = org_lsp#indent_operator_keys('')
  xnoremap <buffer> <expr> = org_lsp#indent_operator_keys('')
  nnoremap <buffer> <expr> == org_lsp#indent_operator_keys('_')
endfunction

" 'indentexpr' of Org buffers: the column of line `v:lnum`, or -1 to keep
" its indentation when the server does not answer in time.
function! org_lsp#indent_expr() abort
  let nr = bufnr()
  if !org_lsp#ready(nr)
    return -1
  endif
  let params = {
        \ 'textDocument': {'uri': org_lsp#uri(nr)},
        \ 'position': {'line': v:lnum - 1, 'character': 0},
        \ 'ch': "\n",
        \ 'options': {'tabSize': &tabstop, 'insertSpaces': v:true},
        \ }
  let response = org_lsp#request_sync(nr, 'textDocument/onTypeFormatting', params, 1000)
  if !empty(get(response, 'error', {}))
    return -1
  endif
  let edits = get(response, 'result', v:null)
  " No edit: the line is indented as it should be.
  if type(edits) != v:t_list || empty(edits)
    return indent(v:lnum)
  endif
  return strdisplaywidth(edits[0].newText)
endfunction

function! org_lsp#indent_operator_keys(motion) abort
  set operatorfunc=org_lsp#indent_operator
  return 'g@' . a:motion
endfunction

" The operator of `=`: `org-indent-region` over the lines moved over.
function! org_lsp#indent_operator(...) abort
  let [first, last] = [line("'["), line("']")]
  let nr = bufnr()
  if !org_lsp#ready(nr)
    return
  endif
  let finish = last < line('$') ? {'line': last, 'character': 0}
        \ : {'line': last - 1, 'character': s:character(getline(last), len(getline(last)))}
  let params = {
        \ 'textDocument': {'uri': org_lsp#uri(nr)},
        \ 'range': {'start': {'line': first - 1, 'character': 0}, 'end': finish},
        \ 'options': {'tabSize': &tabstop, 'insertSpaces': v:true},
        \ }
  let response = org_lsp#request_sync(nr, 'textDocument/rangeFormatting', params, 2000)
  let edits = get(response, 'result', v:null)
  if empty(get(response, 'error', {})) && type(edits) == v:t_list && !empty(edits)
    call org_lsp#apply_edit({'changes': {org_lsp#uri(nr): edits}})
  endif
endfunction

" ----- column view

" Column view (`org-columns`), as the server says Emacs shows it
" (`org-lsp/columns`): each headline covered by its columns, a popup over
" its line that follows it, the header in the window's status line, and the
" column picked highlighted on the cursor's line. Emacs's cursor moves from
" column to column there; `h` and `l` do here. By buffer: `{header,
" columns, lines (the server's), rows: {line: text}, popups: {line: id},
" last}`.
let s:columns = {}

highlight default link OrgLspColumn CursorColumn
highlight default link OrgLspColumnCurrent Visual
highlight default link OrgLspColumnTitle StatusLine

function! s:columns_types() abort
  if empty(prop_type_get('org_lsp_columns'))
    call prop_type_add('org_lsp_columns', {})
  endif
  if empty(prop_type_get('org_lsp_column_current'))
    call prop_type_add('org_lsp_column_current', {'highlight': 'OrgLspColumnCurrent'})
  endif
endfunction

" What the server says column view shows (`org-lsp/columns`).
function! s:columns_show(params) abort
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr <= 0 || !bufloaded(nr)
    return
  endif
  if get(a:params, 'lines', v:null) is v:null
    return s:columns_off(nr)
  endif
  call s:columns_types()
  let first = !has_key(s:columns, nr)
  let view = first ? {'popups': {}, 'last': -1} : s:columns[nr]
  let view.header = a:params.header
  let view.columns = a:params.columns
  let view.lines = a:params.lines
  let view.rows = {}
  for [line, text, _] in a:params.lines
    let view.rows[line] = text
  endfor
  let s:columns[nr] = view
  " Where Emacs's cursor went, when the server says (a column moved).
  let column = type(get(a:params, 'column', v:null)) == v:t_number ? a:params.column : getbufvar(nr, 'org_lsp_column', 0)
  call setbufvar(nr, 'org_lsp_column', max([0, min([len(view.columns) - 1, column])]))
  if first
    call s:columns_attach(nr)
  endif
  call s:columns_draw(nr)
endfunction

" The characters of column `index` in a line of the view, from 0: the
" column, then ` | ` after it (`|` after the last one).
function! s:columns_cell(view, index) abort
  let start = 0
  for i in range(a:index)
    let start += a:view.columns[i].width + 3
  endfor
  let length = a:view.columns[a:index].width + (a:index + 1 == len(a:view.columns) ? 2 : 3)
  return [start, length]
endfunction

" The popup's text for line `row` (from 0) of buffer `nr`: the view's,
" as wide as the line it covers, with the column picked highlighted on
" the cursor's line.
function! s:columns_text(nr, row, win) abort
  let view = s:columns[a:nr]
  let line = get(getbufline(a:nr, a:row + 1), 0, '')
  let text = view.rows[a:row]
  let text .= repeat(' ', max([0, strdisplaywidth(line) - strdisplaywidth(text)]))
  let props = []
  if a:win != -1 && getcurpos(a:win)[1] - 1 == a:row
    let [start, length] = s:columns_cell(view, getbufvar(a:nr, 'org_lsp_column', 0))
    let from = byteidx(text, start)
    let to = byteidx(text, start + length)
    let to = to < 0 ? len(text) : to
    if from >= 0 && to > from
      call add(props, {'col': from + 1, 'length': to - from, 'type': 'org_lsp_column_current'})
    endif
  endif
  return [{'text': text, 'props': props}]
endfunction

function! s:columns_draw(nr) abort
  let view = get(s:columns, a:nr, {})
  if empty(view)
    return
  endif
  call s:columns_clear(a:nr)
  let win = bufwinid(a:nr)
  if win == -1
    return
  endif
  for [row, _] in items(view.rows)
    let row = str2nr(row)
    if row >= getbufinfo(a:nr)[0].linecount
      continue
    endif
    call prop_add(row + 1, 1, {'type': 'org_lsp_columns', 'id': row + 1, 'bufnr': a:nr})
    let view.popups[row] = popup_create(s:columns_text(a:nr, row, win), {
          \ 'textprop': 'org_lsp_columns', 'textpropid': row + 1, 'textpropwin': win,
          \ 'line': -1, 'col': 0, 'pos': 'topleft', 'fixed': 1, 'wrap': 0,
          \ 'highlight': 'OrgLspColumn', 'zindex': 40,
          \ })
  endfor
  call s:columns_header(a:nr)
endfunction

function! s:columns_clear(nr) abort
  let view = get(s:columns, a:nr, {})
  for id in values(get(view, 'popups', {}))
    call popup_close(id)
  endfor
  if !empty(view)
    let view.popups = {}
  endif
  if bufloaded(a:nr) && !empty(prop_type_get('org_lsp_columns'))
    call prop_remove({'type': 'org_lsp_columns', 'bufnr': a:nr, 'all': 1})
  endif
endfunction

" Draw line `row` again, for the cursor that came or went.
function! s:columns_line(nr, row) abort
  let view = get(s:columns, a:nr, {})
  if empty(view) || !has_key(view.popups, a:row)
    return
  endif
  call popup_settext(view.popups[a:row], s:columns_text(a:nr, a:row, bufwinid(a:nr)))
endfunction

" The header in the status line of the windows that show buffer `nr`,
" lined up with the text (Emacs puts it in `header-line-format`).
function! s:columns_header(nr) abort
  let view = s:columns[a:nr]
  for win in win_findbuf(a:nr)
    if type(getwinvar(win, 'org_lsp_statusline', v:null)) == v:t_none
      call setwinvar(win, 'org_lsp_statusline', getwinvar(win, '&statusline'))
    endif
    let offset = get(get(getwininfo(win), 0, {}), 'textoff', 0)
    call setwinvar(win, '&statusline', '%#OrgLspColumnTitle#' . repeat(' ', offset) . substitute(view.header, '%', '%%', 'g'))
  endfor
endfunction

" Pick the column `delta` columns away, as Emacs's `left` and `right` move
" between them.
function! org_lsp#columns_move(delta) abort
  let view = get(s:columns, bufnr(), {})
  if empty(view)
    return
  endif
  let b:org_lsp_column = max([0, min([len(view.columns) - 1, get(b:, 'org_lsp_column', 0) + a:delta])])
  call s:columns_line(bufnr(), line('.') - 1)
endfunction

function! s:columns_cursor() abort
  let nr = bufnr()
  let view = get(s:columns, nr, {})
  if empty(view)
    return
  endif
  let row = line('.') - 1
  if view.last != row
    call s:columns_line(nr, view.last)
  endif
  call s:columns_line(nr, row)
  let view.last = row
endfunction

" The keys of column view in buffer `nr`, `h` and `l` among them.
function! s:columns_attach(nr) abort
  let win = bufwinid(a:nr)
  if win == -1
    return
  endif
  call win_execute(win, 'call org_lsp#set_keys("columns")')
  for [lhs, delta] in [['h', -1], ['<Left>', -1], ['l', 1], ['<Right>', 1]]
    call win_execute(win, printf('nnoremap <buffer> <silent> %s <Cmd>call org_lsp#columns_move(%d)<CR>', lhs, delta))
  endfor
  execute 'augroup org_lsp_columns_' . a:nr
  autocmd!
  execute 'autocmd CursorMoved <buffer=' . a:nr . '> call s:columns_cursor()'
  execute 'autocmd BufWinEnter <buffer=' . a:nr . '> call timer_start(0, {-> s:columns_draw(' . a:nr . ')})'
  execute 'autocmd BufWinLeave <buffer=' . a:nr . '> call s:columns_clear(' . a:nr . ')'
  augroup END
endfunction

" Column view off in buffer `nr`: the text as it is, the keys of Org files
" (or of the agenda) again.
function! s:columns_off(nr) abort
  if !has_key(s:columns, a:nr)
    return
  endif
  call s:columns_clear(a:nr)
  call remove(s:columns, a:nr)
  execute 'augroup org_lsp_columns_' . a:nr
  autocmd!
  augroup END
  execute 'augroup! org_lsp_columns_' . a:nr
  for win in win_findbuf(a:nr)
    let saved = getwinvar(win, 'org_lsp_statusline', v:null)
    if type(saved) != v:t_none
      call setwinvar(win, '&statusline', saved)
      call win_execute(win, 'unlet! w:org_lsp_statusline')
    endif
  endfor
  let win = bufwinid(a:nr)
  if win == -1
    return
  endif
  let keys = ['h', '<Left>', 'l', '<Right>']
  for entry in org_lsp#keys('columns')
    let keys += entry.keys
  endfor
  for lhs in keys
    call win_execute(win, 'silent! nunmap <buffer> ' . s:map_lhs(lhs))
  endfor
  let agenda = s:is_view_type(getbufvar(a:nr, '&filetype'))
  call win_execute(win, printf('call org_lsp#set_keys(%s)', string(agenda ? 'agenda' : 'org')))
endfunction

" Whether column view shows in buffer `nr`, and the text it shows over
" line `row` (from 0): for the tests.
function! org_lsp#columns_on(nr) abort
  return has_key(s:columns, a:nr)
endfunction

" What column view shows in buffer `nr`, as the server said it: `{header,
" lines: [[line, text, values]]}`, or {} when it is off.
function! org_lsp#columns_view(nr) abort
  let view = get(s:columns, a:nr, {})
  return empty(view) ? {} : {'header': view.header, 'lines': view.lines}
endfunction

function! org_lsp#columns_shown(nr, row) abort
  let id = get(get(get(s:columns, a:nr, {}), 'popups', {}), a:row, 0)
  return id ? get(getbufline(winbufnr(id), 1), 0, '') : ''
endfunction

" ----- help

let s:scope_titles = {'org': 'Org file', 'capture': 'Capture buffer', 'agenda': 'Agenda', 'calendar': 'Date prompt', 'columns': 'Column view', 'global': 'Anywhere'}

" The keys of `scope` as they are set: `[{keys, title, command}]`, in the order of
" the server's commands, then the plugin's own.
function! org_lsp#keys(scope) abort
  let bound = s:keys_in(a:scope)[0]
  let out = []
  let names = map(copy(get(org_lsp#manifest(), 'commands', [])), 'v:val.command') + ['cycle', 'global_cycle', 'quit', 'hover', 'help']
  for name in names + sort(keys(bound))
    if !empty(get(bound, name, []))
      let command = s:command(name)
      let title = has_key(s:own, name) ? s:own[name].title : get(command, 'title', name)
      call add(out, {'keys': remove(bound, name), 'title': title, 'command': name})
    endif
  endfor
  return out
endfunction

" The lines of the help for `scopes`: a heading for each, then its keys
" and what they do.
" The command key `lhs` runs in Normal mode in the current buffer, one of
" the server's or the plugin's own; '' when it runs none: for the tests.
function! org_lsp#mapped_command(lhs) abort
  let map = maparg(s:map_lhs(a:lhs), 'n', 0, 1)
  if !get(map, 'buffer', 0)
    return ''
  endif
  let index = matchstr(map.rhs, 'org_lsp#\%(key\|repeatable\)(\zs\d\+')
  if !empty(index)
    return s:mapped[str2nr(index)].name
  endif
  return matchstr(map.rhs, "org_lsp#run('\\zs[^']\\+")
endfunction

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
    let w:org_lsp_from = from
    call s:resize_apart(mode, org_lsp#config().win_split_size)
  endif
  execute 'edit' fnameescape(a:path)
  if mode !=# 'current'
    let w:org_lsp_opened = expand('%:p')
  endif
endfunction

" The date prompt, at the bottom (`calendar_win_mode`), as Emacs shows
" its calendar, as high as its text.
function! s:open_prompt(path, win) abort
  if a:win != -1
    call win_gotoid(a:win)
    call s:reload_views()
  else
    let from = win_getid()
    execute org_lsp#config().calendar_win_mode
    let w:org_lsp_from = from
    execute 'edit' fnameescape(a:path)
    let w:org_lsp_opened = expand('%:p')
  endif
  if org_lsp#config().calendar_win_mode !~# 'v'
    execute 'resize' line('$')
  endif
  call winrestview({'topline': 1, 'leftcol': 0})
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
              \ {_error, _result -> timer_start(0, {-> s:reload_views()})})
      endif
      return
    endif
  endfor
endfunction

" The line of the agenda the cursor is on, each time it changes, for
" follow mode (`F`), which the server keeps, and the outline path of its
" entry (`org-agenda-show-outline-path`), echoed and not kept in the
" message history, as Emacs shows it.
function! org_lsp#tell_agenda_cursor() abort
  " Not on the first line of the agenda read empty while the server wrote
  " it: the line it was on is where it goes back to.
  if &filetype !=# 'org-agenda' || get(b:, 'org_lsp_cursor_line', 0) == line('.') || (line('$') == 1 && getline(1) ==# '')
    return
  endif
  let b:org_lsp_cursor_line = line('.')
  let params = {'textDocument': {'uri': org_lsp#uri(bufnr())},
        \ 'position': {'line': line('.') - 1, 'character': 0}, 'width': &columns}
  let buf = bufnr()
  call org_lsp#request(buf, 'org-lsp/agendaCursor', params, {_error, result -> s:echo_path(buf, result)})
endfunction

let s:echoed = ''

" The answer comes later: echoed only if the user is still in the agenda
" it was asked for, and not typing.
function! s:echo_path(buf, result) abort
  if bufnr() != a:buf || mode() !=# 'n'
    return
  endif
  if type(a:result) == v:t_dict && type(get(a:result, 'echo', 0)) == v:t_string
    let s:echoed = a:result.echo
    echo a:result.echo
  endif
endfunction

" The outline path echoed last, for tests.
function! org_lsp#echoed() abort
  return s:echoed
endfunction

" The cursor on another line of an Org file while the org-roam buffer
" shows: the buffer follows the node it is in.
function! org_lsp#tell_roam_cursor() abort
  if s:is_roam_buffer(expand('%:p')) || get(b:, 'org_lsp_roam_line', 0) == line('.') || !org_lsp#roam_buffer_shown()
    return
  endif
  let b:org_lsp_roam_line = line('.')
  let params = {'textDocument': {'uri': org_lsp#uri(bufnr())},
        \ 'position': {'line': line('.') - 1, 'character': 0}}
  call org_lsp#request(bufnr(), 'org-lsp/cursor', params, {_error, _result -> 0})
endfunction

" Whether `path` is a file of the org-roam buffer.
function! s:is_roam_buffer(path) abort
  let prefix = get(get(org_lsp#manifest(), 'roamBuffer', {}), 'prefix', "\n")
  return stridx(fnamemodify(a:path, ':t'), prefix) == 0
endfunction

" Whether a window shows the persistent org-roam buffer (not a dedicated
" one).
function! org_lsp#roam_buffer_shown() abort
  let prefix = get(get(org_lsp#manifest(), 'roamBuffer', {}), 'prefix', "\n")
  for win in range(1, winnr('$'))
    if fnamemodify(bufname(winbufnr(win)), ':t') ==# prefix . '.org'
      return 1
    endif
  endfor
  return 0
endfunction

function! s:is_agenda(path) abort
  return fnamemodify(a:path, ':t') ==# get(get(org_lsp#manifest(), 'agenda', {}), 'fileName', "\n")
endfunction

function! s:is_prompt(path) abort
  return fnamemodify(a:path, ':t') ==# get(get(org_lsp#manifest(), 'datePrompt', {}), 'fileName', "\n")
endfunction

" Whether `filetype` is that of a file the server writes: the agenda or
" the date prompt.
function! s:is_view_type(filetype) abort
  let manifest = org_lsp#manifest()
  return a:filetype ==# get(get(manifest, 'agenda', {}), 'languageId', 'org-agenda')
        \ || a:filetype ==# get(get(manifest, 'datePrompt', {}), 'languageId', 'org-date')
endfunction

function! s:is_capture(path) abort
  let prefix = get(get(org_lsp#manifest(), 'capture', {}), 'prefix', "\n")
  return stridx(fnamemodify(a:path, ':t'), prefix) == 0
endfunction

" The agenda and the date prompt are files the server writes: read them
" again when it has. Reading drops the highlighting; the lines that stayed
" get theirs back at once, from the tokens they had, until the new tokens
" come.
function! s:reload_views() abort
  for info in getbufinfo({'bufloaded': 1})
    if s:is_agenda(info.name) || s:is_prompt(info.name)
      let old = getbufline(info.bufnr, 1, '$')
      let views = s:window_views(info.bufnr)
      " Read empty already, by Vim on its own while the server wrote it and
      " with no autocommands (`org_lsp#save` coming back to it): its windows
      " went to the first line. They go back where they were, as a read
      " waiting to be done again has them, else to the line last told to the
      " server.
      let lost = old ==# ['']
      if lost
        let pending = getbufvar(info.bufnr, 'org_lsp_reread', v:null)
        let line = getbufvar(info.bufnr, 'org_lsp_cursor_line', 1)
        let views = type(pending) == v:t_dict ? pending : map(views, {_, _view -> {'lnum': line}})
      endif
      " Without the file's name and size, which would take the place of
      " what the command said.
      silent execute 'checktime' info.bufnr
      " Read while the server wrote it (in place): never empty otherwise.
      if getbufline(info.bufnr, 1, '$') ==# ['']
        call s:read_empty_view(info.bufnr, views)
        continue
      endif
      call s:give_view(info.bufnr)
      if lost
        for [win, view] in items(views)
          call win_execute(str2nr(win), 'call winrestview(' . string(view) . ')')
        endfor
      endif
      call s:repaint_kept(info.bufnr, old)
      call s:columns_draw(info.bufnr)
      " The calendar stays as high as its text: a month of six weeks.
      if s:is_prompt(info.name) && org_lsp#config().calendar_win_mode !~# 'v'
        for win in win_findbuf(info.bufnr)
          call win_execute(win, 'resize ' . len(getbufline(info.bufnr, 1, '$')) . ' | call winrestview({"topline": 1})')
        endfor
      endif
    endif
  endfor
endfunction

" The view of each window that shows buffer `nr`, by window.
function! s:window_views(nr) abort
  let views = {}
  for win in win_findbuf(a:nr)
    call win_execute(win, 'let s:view = winsaveview()')
    let views[win] = s:view
  endfor
  return views
endfunction

" The agenda (or date prompt) read empty, while the server wrote it in
" place: read again in a moment, its windows then as `views` has them
" (by window), else on the line last told to the server.
function! s:read_empty_view(nr, views) abort
  let pending = getbufvar(a:nr, 'org_lsp_reread', v:null)
  if !empty(a:views) || type(pending) == v:t_none
    let views = a:views
    if empty(views)
      for win in win_findbuf(a:nr)
        let views[win] = {'lnum': getbufvar(a:nr, 'org_lsp_cursor_line', 1)}
      endfor
    endif
    call setbufvar(a:nr, 'org_lsp_reread', views)
  endif
  if type(pending) == v:t_none
    call timer_start(20, function('s:read_view_again', [a:nr, 0]))
  endif
endfunction

function! s:read_view_again(nr, tries, ...) abort
  let win = get(win_findbuf(a:nr), 0, -1)
  if !bufloaded(a:nr) || win == -1
    call setbufvar(a:nr, 'org_lsp_reread', v:null)
    return
  endif
  call win_execute(win, 'silent edit!')
  if getbufline(a:nr, 1, '$') ==# [''] && a:tries < 10
    call timer_start(20, function('s:read_view_again', [a:nr, a:tries + 1]))
    return
  endif
  let views = getbufvar(a:nr, 'org_lsp_reread', {})
  call setbufvar(a:nr, 'org_lsp_reread', v:null)
  for [view_win, view] in items(type(views) == v:t_dict ? views : {})
    call win_execute(str2nr(view_win), 'call winrestview(' . string(view) . ')')
  endfor
  call s:reload_views()
endfunction

" Have the client send the whole agenda (or date prompt) again when Vim
" read it anew. A buffer read again is not a change Vim tells listeners
" of, and the clients that learn of changes from them (vim-lsp, coc.nvim)
" keep the old text, while the server maps the agenda's lines by the text
" the client sent.
" Vim reads the file again on its own too, at times without autocommands,
" so the lines last given are kept (`b:org_lsp_given`), and given again
" when the buffer has others: put back as two changes, which such a client
" sends as the whole text.
function! s:give_view(nr) abort
  let lines = getbufline(a:nr, 1, '$')
  if lines ==# ['']
    return s:read_empty_view(a:nr, {})
  endif
  if lines ==# getbufvar(a:nr, 'org_lsp_given', lines)
    return
  endif
  call setbufvar(a:nr, 'org_lsp_given', lines)
  let saved = {}
  for name in ['readonly', 'modifiable', 'undolevels']
    let saved[name] = getbufvar(a:nr, '&' . name)
  endfor
  call setbufvar(a:nr, '&readonly', 0)
  call setbufvar(a:nr, '&modifiable', 1)
  call setbufvar(a:nr, '&undolevels', -1)
  " The windows that show it keep their cursor, which emptying the buffer
  " would put on the first line.
  let windows = win_findbuf(a:nr)
  for win in windows
    call win_execute(win, 'let w:org_lsp_view = winsaveview()')
  endfor
  try
    silent call deletebufline(a:nr, 1, '$')
    call setbufline(a:nr, 1, lines)
  finally
    call setbufvar(a:nr, '&modified', 0)
    for [name, value] in items(saved)
      call setbufvar(a:nr, '&' . name, value)
    endfor
    for win in windows
      call win_execute(win, 'call winrestview(w:org_lsp_view) | unlet w:org_lsp_view')
    endfor
  endtry
  call s:call('flush', a:nr)
  call s:changed(a:nr)
endfunction

" Paint buffer `nr` with the tokens it had when it had the lines `old`:
" those of the lines before and after the part that changed.
function! s:repaint_kept(nr, old) abort
  let tokens = get(s:painted, a:nr, [[], 0])[0]
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
  if s:is_prompt(path)
    call s:open_prompt(path, win)
  elseif s:is_roam_buffer(path)
    " The org-roam buffer beside the file, as Emacs shows it in a window of
    " its own; gone with its window, which tells the server.
    if win == -1
      botright vsplit
      let w:org_lsp_from = from
      execute 'edit' fnameescape(path)
      let w:org_lsp_opened = expand('%:p')
      setlocal bufhidden=wipe
    endif
    if get(a:params, 'takeFocus', v:true) is v:false
      call win_gotoid(from)
      return {'success': v:true}
    endif
  elseif s:is_agenda(path) || s:is_capture(path)
    if win != -1
      call win_gotoid(win)
      call s:reload_views()
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
  " The status line follows the cursor, moved from a callback, as after
  " Vim's own motions.
  redrawstatus
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
  " On Windows, the URL's handler without cmd.exe, which would read `&`,
  " `|` and `%VAR%` in it.
  let opener = has('mac') ? ['open'] : has('win32') ? ['rundll32', 'url.dll,FileProtocolHandler'] : ['xdg-open']
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
        let view = winsaveview()
        let hidden = &hidden
        set hidden
        execute 'silent keepalt keepjumps noautocmd buffer' nr
        silent update
        execute 'silent keepalt keepjumps noautocmd buffer' current
        " Coming back reads the buffer again when its file changed
        " ('autoread'), with no autocommands: the agenda, read empty while
        " the server wrote it, is read again in a moment, its window then
        " where it was.
        if s:is_view_type(&filetype) && getline(1, '$') ==# ['']
          call s:read_empty_view(current, {win_getid(): view})
        else
          call winrestview(view)
        endif
        let &hidden = hidden
        " A client that did not tell the server of the file (coc.nvim
        " loads the files it edits for it) tells it of the file written.
        if exists('*org_lsp#adapter#' . org_lsp#adapter() . '#saved')
          call s:call('saved', nr)
        endif
      endif
    endif
  endfor
endfunction

" The file of buffer `nr`, by which the windows opened for it know it:
" the agenda's or the date prompt's buffer may be another once read again.
function! s:path_of(nr) abort
  return fnamemodify(bufname(a:nr), ':p')
endfunction

" Whether the plugin opened window `win` for buffer `nr`.
function! s:opened_for(win, nr) abort
  return getwinvar(a:win, 'org_lsp_opened', '') ==# s:path_of(a:nr)
endfunction

" Close buffer `nr` and the windows the plugin opened for it; a window that
" was there before goes back to the buffer it showed.
function! org_lsp#close_buffer(nr) abort
  for win in win_findbuf(a:nr)
    if s:opened_for(win, a:nr) && winnr('$') > 1
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
    " Text that ends with a newline ends with the buffer's last line.
    if len(lines) > 1 && lines[-1] ==# ''
      call remove(lines, -1)
    endif
    call appendbufline(a:nr, lines_count, lines)
    return
  endif
  " Text that ends with a newline at the end of the buffer leaves an empty
  " last piece, which is the buffer's own end of line.
  if end.line >= lines_count && len(lines) > 1 && lines[-1] ==# ''
    call remove(lines, -1)
  endif
  " The lines replaced in place, then those more added or those fewer
  " deleted: the cursor stays on its line, as an edit leaves it.
  let kept = min([removed, len(lines)])
  call setbufline(a:nr, start.line + 1, lines[: kept - 1])
  if len(lines) > removed
    call appendbufline(a:nr, start.line + removed, lines[removed :])
  elseif removed > len(lines)
    call deletebufline(a:nr, start.line + kept + 1, start.line + removed)
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
  if a:method ==# 'org-lsp/showDocument'
    " `window/showDocument` as a notification (`show_document_notification`),
    " answered as one.
    let result = org_lsp#show_document(a:params.params)
    call s:call('notify', 'org-lsp/showDocumentResult', {'id': a:params.id, 'result': result})
  elseif a:method ==# 'org-lsp/save'
    call org_lsp#save(get(a:params, 'uris', []))
  elseif a:method ==# 'org-lsp/clock'
    call s:clock_update(a:params)
  elseif a:method ==# 'org-lsp/timer'
    call s:timer_update(a:params)
  elseif a:method ==# 'org-lsp/virtualIndent'
    call s:virtual_indent(a:params)
  elseif a:method ==# 'org-lsp/display'
    call s:display(a:params)
  elseif a:method ==# 'org-lsp/lineHighlights'
    call s:line_highlights(a:params)
  elseif a:method ==# 'org-lsp/codeBlocks'
    call s:code_blocks(a:params)
  elseif a:method ==# 'org-lsp/syntax'
    call s:syntax(a:params)
  elseif a:method ==# 'org-lsp/columns'
    call s:columns_show(a:params)
  elseif a:method ==# 'org-lsp/folds'
    call s:journal_folds(a:params)
  elseif a:method ==# 'org-lsp/runCommand' && type(a:params) == v:t_dict && type(get(a:params, 'command', 0)) == v:t_string
    " A command to run as from its key, where the cursor is, outside the
    " client's callback: a capture an org-protocol URL started.
    let Run = {-> org_lsp#run(a:params.command, {'argument': get(a:params, 'argument', {})})}
    call timer_start(0, {-> Run()})
  elseif a:method ==# 'org-lsp/message' && type(a:params) == v:t_dict
    call s:say(get(a:params, 'message', ''), get(a:params, 'type', 3))
  elseif a:method ==# 'org-lsp/viewChanged'
    " The agenda's file is new: read it again, outside the client's
    " callback.
    call timer_start(0, {-> s:reload_views()})
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
    call s:repaint_all(nr)
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
" line wraps into (`wrap-prefix`), where 'breakindent' lines those up with
" the line's own text, as Org does past the prefix: its indentation, a
" headline's title, a list item a column after its bullet
" (`org-list-item-body-column`).
function! s:virtual_indent(params) abort
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr < 0 || !bufloaded(nr) || !s:has_virtual_text()
    return
  endif
  call s:indent_prop_types()
  call prop_remove({'type': 'org_lsp_indent', 'bufnr': nr, 'all': 1})
  call prop_remove({'type': 'org_lsp_star', 'bufnr': nr, 'all': 1})
  let last = getbufinfo(nr)[0].linecount
  let hidden_stars = {}
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
      let hidden_stars[line + 1] = [[1, stars, repeat(' ', stars), '']]
    endif
  endfor
  " Concealed and blanks in their place, where Vim conceals: the color of
  " the background is no color when the colorscheme gives none.
  call setbufvar(nr, 'org_lsp_stars', hidden_stars)
  call s:conceal(nr)
  if !empty(a:params.lines)
    for win in win_findbuf(nr)
      call setwinvar(win, '&breakindent', 1)
      call setwinvar(win, '&breakindentopt', 'list:-1')
    endfor
    call setbufvar(nr, '&formatlistpat', '^\*\+ \|^\s*\%([-+]\|\s\*\|\d\+[.)]\)\s')
  endif
endfunction

" ----- concealing

" What Org hides shows hidden: link brackets and targets and emphasis
" markers, which the tokens mark `hidden`, and the text that something
" else stands in for (`org-lsp/display`): an entity, shown as its
" character, a sub- or superscript in Unicode, the stars of org-bullets'
" bullet, a shrunk table field. Vim conceals text in a window with matches
" (`matchaddpos()` with `conceal`), whatever text properties it has; what
" stands in shows as text of its own before the hidden text, in the look
" of what it stands for, as Vim colors a concealed character only as
" `Conceal`. On the cursor's line the text shows as it is, to edit it, as
" in Neovim: Vim shows concealed text there ('concealcursor' empty), and
" the text standing in goes.

function! s:conceal_on() abort
  return org_lsp#config().conceal && s:has_virtual_text()
endfunction

" What stands in for hidden text in a file: `items` of `[line, character,
" end line, end character, shown, look?]`. Kept with the buffer by line,
" `[column, length, shown, look]` in bytes from 1.
function! s:display(params) abort
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr < 0 || !bufloaded(nr)
    return
  endif
  let lines = getbufline(nr, 1, '$')
  let shown = {}
  for item in get(a:params, 'items', [])
    let [line, character, end_line, end_character, text] = item[0 : 4]
    " Within one line, as all of them are, and of the text the buffer has:
    " newer items are on their way.
    if line != end_line || line >= len(lines)
      continue
    endif
    let start = s:byte(lines[line], character)
    let end = s:byte(lines[line], end_character)
    if end <= start
      continue
    endif
    call add(get(extend(shown, {line + 1: []}, 'keep'), line + 1), [start + 1, end - start, text, get(item, 5, '')])
  endfor
  call setbufvar(nr, 'org_lsp_shown', shown)
  call s:conceal(nr)
endfunction

" The text property type of text standing in, in the look of the token
" `look` (`stars.level1`, `bullet`), or its type's, or the text's own.
function! s:shown_type(look) abort
  let type = 'org_lsp_shown_' . substitute(a:look, '\W', '_', 'g')
  if empty(prop_type_get(type))
    let group = ''
    for candidate in [a:look, matchstr(a:look, '^[^.]*')]
      if !empty(candidate) && hlexists(s:group_name(candidate))
        let group = s:group_name(candidate)
        break
      endif
    endfor
    call prop_type_add(type, empty(group) ? {} : {'highlight': group})
    call add(s:shown_types, type)
  endif
  return type
endfunction

let s:shown_types = []

" The stand-ins of buffer `nr` by line: those the server sent, and blanks
" for the hidden leading stars of headlines.
function! s:stand_ins(nr) abort
  let all = deepcopy(getbufvar(a:nr, 'org_lsp_shown', {}))
  for [line, items] in items(getbufvar(a:nr, 'org_lsp_stars', {}))
    let all[line] = items + get(all, line, [])
  endfor
  return all
endfunction

" Hide what the buffer `nr` hides in each window that shows it, and show
" what stands in for it but on the cursor's line.
function! s:conceal(nr) abort
  if !s:conceal_on()
    return
  endif
  for win in win_findbuf(a:nr)
    call s:conceal_window(win, a:nr)
  endfor
  call s:show_stand_ins(a:nr, bufnr() == a:nr ? s:revealed_line() : 0)
endfunction

function! s:conceal_window(win, nr) abort
  for id in getwinvar(a:win, 'org_lsp_conceal', [])
    silent! call matchdelete(id, a:win)
  endfor
  let positions = copy(getbufvar(a:nr, 'org_lsp_hidden', []))
  for [line, items] in items(s:stand_ins(a:nr))
    for [column, length; _] in items
      call add(positions, [str2nr(line), column, length])
    endfor
  endfor
  let ids = []
  if !empty(positions)
    call add(ids, matchaddpos('Conceal', positions, 10, -1, {'window': a:win, 'conceal': ''}))
    if getwinvar(a:win, '&conceallevel') == 0
      call setwinvar(a:win, '&conceallevel', 2)
    endif
  endif
  call setwinvar(a:win, 'org_lsp_conceal', ids)
  call setwinvar(a:win, 'org_lsp_conceal_buffer', a:nr)
endfunction

" The text standing in for hidden text on every line but `cursor`.
function! s:show_stand_ins(nr, cursor) abort
  if !empty(s:shown_types)
    call prop_remove({'types': s:shown_types, 'bufnr': a:nr, 'all': 1})
  endif
  let last = getbufinfo(a:nr)[0].linecount
  for [line, items] in items(s:stand_ins(a:nr))
    if str2nr(line) != a:cursor && str2nr(line) <= last
      call s:show_line(a:nr, str2nr(line), items)
    endif
  endfor
  call setbufvar(a:nr, 'org_lsp_shown_cursor', a:cursor)
endfunction

function! s:show_line(nr, line, items) abort
  for [column, length, text, look] in a:items
    silent! call prop_add(a:line, column, {'type': s:shown_type(look), 'bufnr': a:nr, 'text': text})
  endfor
endfunction

" The cursor moved to another line: the text standing in shows again on
" the line it left, and goes from the one it is on.
function! s:conceal_cursor() abort
  let nr = bufnr()
  let shown = s:stand_ins(nr)
  let before = get(b:, 'org_lsp_shown_cursor', 0)
  let now = s:revealed_line()
  if empty(shown) || before == now
    return
  endif
  if has_key(shown, before)
    call s:show_line(nr, before, shown[before])
  endif
  if has_key(shown, now) && !empty(s:shown_types)
    call prop_remove({'types': s:shown_types, 'bufnr': nr, 'all': 1}, now)
  endif
  let b:org_lsp_shown_cursor = now
endfunction

" The cursor's line when Vim shows its concealed text, in the mode at hand
" ('concealcursor' does not name it), else 0: there the text standing in
" goes, and elsewhere it stays, as in Neovim.
function! s:revealed_line() abort
  let mode = mode()
  let letter = mode =~# '^[iR]' ? 'i' : mode =~# "^[vV\<C-v>]" ? 'v' : mode =~# '^c' ? 'c' : 'n'
  return stridx(&l:concealcursor, letter) >= 0 ? 0 : line('.')
endfunction

" A window that showed an Org buffer and shows another now: its matches
" were the other buffer's.
function! s:conceal_enter() abort
  let shown = get(w:, 'org_lsp_conceal_buffer', -1)
  if shown >= 0 && shown != bufnr()
    for id in get(w:, 'org_lsp_conceal', [])
      silent! call matchdelete(id)
    endfor
    let w:org_lsp_conceal = []
    let w:org_lsp_conceal_buffer = -1
  endif
  if exists('b:org_lsp_attached') && (!empty(get(b:, 'org_lsp_hidden', [])) || !empty(s:stand_ins(bufnr())))
    call s:conceal(bufnr())
  endif
endfunction

augroup org_lsp_conceal
  autocmd!
  autocmd BufWinEnter,WinEnter * call s:conceal_enter()
  autocmd OptionSet concealcursor if exists('b:org_lsp_attached') | call s:conceal_cursor() | endif
augroup END

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

" 'foldtext' of Org buffers: the fold's first line, then what the server
" says closed folds show after it (`org-ellipsis`), as Emacs shows a folded
" heading.
function! org_lsp#foldtext() abort
  let text = substitute(getline(v:foldstart), '\t', repeat(' ', &tabstop), 'g')
  return text . get(s:, 'ellipsis', '...')
endfunction

function! s:got_ellipsis(error, result) abort
  if empty(a:error) && type(a:result) == v:t_string
    let s:ellipsis = a:result
    redraw!
  endif
endfunction

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

function! s:refold_windows(nr) abort
  for win in win_findbuf(a:nr)
    call win_execute(win, 'call org_lsp#refold()')
  endfor
endfunction

function! s:got_folds(nr, error, result) abort
  if !empty(a:error) || type(a:result) != v:t_list || !bufexists(a:nr)
    return
  endif
  let lines_count = getbufinfo(a:nr)[0].linecount
  " The same folds for as many lines as last time (text typed in a line).
  let got = [a:result, lines_count]
  if got == getbufvar(a:nr, 'org_lsp_folds_got', [])
    return
  endif
  call setbufvar(a:nr, 'org_lsp_folds_got', got)
  " How many folds begin on each line less how many ended on the line
  " before: summed up, each line's depth.
  let delta = repeat([0], lines_count + 1)
  let starts = {}
  let ranges = []
  for range in sort(copy(a:result), {a, b -> a.startLine - b.startLine})
    " Folds of older text, which had more lines: newer ones are on their
    " way.
    if range.endLine <= range.startLine || range.startLine >= lines_count
      continue
    endif
    call add(ranges, [range.startLine + 1, range.endLine + 1])
    let delta[range.startLine] += 1
    let delta[min([range.endLine, lines_count - 1]) + 1] -= 1
    let starts[range.startLine] = 1
  endfor
  let levels = []
  let depth = 0
  for line in range(lines_count)
    let depth += delta[line]
    call add(levels, has_key(starts, line) ? '>' . depth : depth)
  endfor
  call setbufvar(a:nr, 'org_lsp_fold_ranges', ranges)
  if levels == getbufvar(a:nr, 'org_lsp_folds', [])
    return
  endif
  call setbufvar(a:nr, 'org_lsp_folds', levels)
  " Not while typing (a heading `<Leader><CR>` made): `:normal` would put
  " the cursor back from the end of the line. Once insert mode ends.
  if mode() =~# '^[iR]'
    execute 'autocmd InsertLeave * ++once call s:refold_windows(' . a:nr . ')'
  else
    call s:refold_windows(a:nr)
  endif
  " Folds org-journal asked for before the buffer had any.
  let pending = getbufvar(a:nr, 'org_lsp_journal_folds', {})
  if !empty(pending)
    call setbufvar(a:nr, 'org_lsp_journal_folds', {})
    call s:set_journal_folds(a:nr, pending)
  endif
endfunction

" The headlines of a document org-journal showed, folded as it folds them,
" or the subtrees a yank or paste folded (`org-lsp/folds`): once the
" buffer has its folds.
function! s:journal_folds(params) abort
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr < 0
    return
  endif
  if empty(getbufvar(nr, 'org_lsp_folds', []))
    call setbufvar(nr, 'org_lsp_journal_folds', a:params)
  else
    call s:set_journal_folds(nr, a:params)
  endif
endfunction

" A fold that closes other lines than the server's range is not the
" server's yet (the buffer's folds come a moment after an edit): it is
" opened again, and tried again a moment later; the last time it stays
" closed.
function! s:set_journal_folds(nr, params, ...) abort
  let tries = get(a:, 1, 0)
  let stale = []
  for win in win_findbuf(a:nr)
    for fold in get(a:params, 'open', [])
      call win_execute(win, 'silent! ' . (fold.startLine + 1) . 'foldopen')
    endfor
    for fold in get(a:params, 'close', [])
      let line = fold.startLine + 1
      " (A line in a fold closed already stays as it is.)
      if win_execute(win, 'echon foldclosed(' . line . ')') !=# '-1'
        continue
      endif
      call win_execute(win, 'silent! ' . line . 'foldclose')
      let closed = win_execute(win, 'echon foldclosed(' . line . ') . "," . foldclosedend(' . line . ')')
      if tries < 40 && closed !=# line . ',' . (get(fold, 'endLine', fold.startLine) + 1)
        call win_execute(win, 'silent! ' . line . 'foldopen')
        call add(stale, fold)
      endif
    endfor
  endfor
  if !empty(stale)
    let again = {'uri': a:params.uri, 'open': [], 'close': stale}
    call timer_start(50, {-> s:set_journal_folds(a:nr, again, tries + 1)})
  endif
endfunction

" org-journal's files need not have a name that says they are Org files
" (`org-journal-is-journal`): those of its directory without an extension
" are, as the server says (`org-lsp/journal`), here and in the buffers
" already loaded.
function! s:journal_directory(nr, ...) abort
  let tries = get(a:, 1, 0)
  " Once the client runs: asked before, the request fails, and the
  " directory stays unknown.
  if exists('s:journal_asked') || !bufloaded(a:nr) || !s:call('ready', a:nr)
    return
  endif
  let s:journal_asked = 1
  call org_lsp#request(a:nr, 'org-lsp/journal', v:null, function('s:got_journal', [a:nr, tries]))
endfunction

function! s:got_journal(nr, tries, error, result) abort
  " coc.nvim may refuse a request while it starts ("Plugin not ready"):
  " asked again a moment later.
  if !empty(a:error)
    unlet! s:journal_asked
    if a:tries < 20
      call timer_start(300, {-> s:journal_directory(a:nr, a:tries + 1)})
    endif
    return
  endif
  if type(a:result) != v:t_dict || type(get(a:result, 'directory', 0)) != v:t_string
    return
  endif
  let s:journal_dir = a:result.directory
  augroup org_lsp_journal
    autocmd!
    execute 'autocmd BufNewFile,BufRead ' . fnameescape(s:journal_dir) . '* call s:journal_file(expand("<afile>:p"))'
  augroup END
  for info in getbufinfo({'bufloaded': 1})
    if empty(getbufvar(info.bufnr, '&filetype')) && s:is_journal_file(info.name)
      call setbufvar(info.bufnr, '&filetype', 'org')
    endif
  endfor
endfunction

" The journal's directory, once the server has said it.
function! org_lsp#journal_dir() abort
  return get(s:, 'journal_dir', '')
endfunction

function! s:is_journal_file(path) abort
  return exists('s:journal_dir') && stridx(a:path, s:journal_dir) == 0 && a:path[len(s:journal_dir):] !~# '\.'
endfunction

function! s:journal_file(path) abort
  if empty(&filetype) && s:is_journal_file(a:path)
    setfiletype org
  endif
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
    " A closed fold shows its first line and `org-ellipsis`, nothing after.
    setlocal foldtext=org_lsp#foldtext()
    " Vim's own `fold:-` gives way; a character the user chose stays.
    if &l:fillchars !~# 'fold:' && index(['', '-'], matchstr(&fillchars, 'fold:\zs.')) >= 0
      let others = filter(split(&fillchars, ','), {_, item -> item !~# '^fold:'})
      let &l:fillchars = join(others + ['fold: '], ',')
    endif
    if !exists('s:ellipsis')
      let s:ellipsis = '...'
      call org_lsp#request(bufnr(), 'org-lsp/ellipsis', v:null, function('s:got_ellipsis'))
    endif
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
  " What TAB does before it folds (`org.cycle`): a new, empty entry
  " changes level, the start of the file cycles globally.
  let integrate = v:false
  if !empty(org_lsp#adapter()) && org_lsp#ready(bufnr())
    let [nr, context] = s:cursor_context()
    " On the blank that ends a line (a new `** `, left with <Esc>), TAB is
    " at the end of the line, where Emacs's point is once it is typed: the
    " level keeps cycling from there.
    let text = getline('.')
    if mode() ==# 'n' && col('.') == len(text) && text =~# '\s$'
      let context.position.character = s:character(text, len(text))
    endif
    let answer = org_lsp#request_sync(nr, 'workspace/executeCommand', {'command': 'org.cycle', 'arguments': [context]}, 1000)
    let result = get(answer, 'result', v:null)
    if !empty(get(answer, 'error', {}))
      return s:warn(get(answer.error, 'message', 'TAB failed'))
    elseif type(result) == v:t_dict && type(get(result, 'edits', 0)) == v:t_list
      return s:apply_carried(context.textDocument.uri, result)
    elseif type(result) == v:t_dict && get(result, 'global', v:false) is v:true
      return org_lsp#global_cycle()
    endif
    let integrate = type(result) == v:t_dict && get(result, 'integrateLists', v:false) is v:true
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
  " Lists stay open but with `org_cycle_include_plain_lists` `integrate`.
  if !integrate && !s:is_item(start)
    call filter(children, {_, r -> !s:is_item(r[0])})
  endif
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

" Whether line `line` starts a list item.
function! s:is_item(line) abort
  return getline(a:line) =~# '^\%(\s*\%([-+]\|\d\+[.)]\)\|\s\+\*\)\%(\s\|$\)'
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
      \ '@markup.raw': 'String', '@markup.math': 'Special', '@markup.environment': 'Special',
      \ '@markup.heading': 'Title', '@function.macro': 'Macro',
      \ '@punctuation.special': 'Delimiter',
      \ '@markup.strong': 'OrgLspBold', '@markup.italic': 'OrgLspItalic',
      \ '@markup.underline': 'OrgLspUnderline', '@markup.strikethrough': 'OrgLspStrikethrough',
      \ '@markup.quote': 'OrgLspItalic',
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
    let group = s:group_name(look.token)
    if !empty(candidates)
      let target = candidates[-1]
      for candidate in candidates
        if hlexists(candidate)
          let target = candidate
          break
        endif
      endfor
      execute 'highlight default link' group target
    elseif has_key(look, 'background')
      " Org's background, which no standard group has: a habit's days.
      let color = get(look.background, &background, look.background.dark)
      execute 'highlight default' group 'guibg=' . color 'ctermbg=' . s:cterm_color(color)
    else
      continue
    endif
    let type = 'org_lsp_' . look.token
    " A type with several modifiers over one with one, over a modifier
    " alone, over a type.
    let dots = len(substitute(look.token, '[^.]', '', 'g'))
    let priority = look.token =~# '^\*\.' ? 101 : dots > 1 ? 103 : dots ? 102 : 100
    if empty(prop_type_get(type))
      call prop_type_add(type, {'highlight': group, 'combine': 1, 'priority': priority})
    endif
    let s:selectors[look.token] = type
  endfor
  call s:define_faces()
endfunction

" The color of a 256-color terminal's 6x6x6 cube nearest to `#rrggbb`, for
" a terminal without 'termguicolors'.
function! s:cterm_color(hex) abort
  let steps = [0, 95, 135, 175, 215, 255]
  let index = 16
  for [at, weight] in [[1, 36], [3, 6], [5, 1]]
    let value = str2nr(a:hex[at : at + 1], 16)
    let distances = map(copy(steps), {_, step -> abs(step - value)})
    let index += weight * index(distances, min(distances))
  endfor
  return index
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
    echomsg 'Command not allowed in this line'
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

" The tokens of buffer `nr` painted again on all its lines, as something
" else than its text changed how they show (the faces came).
function! s:repaint_all(nr) abort
  silent! call remove(s:painted, a:nr)
  call s:update_tokens(a:nr)
endfunction

function! s:update_tokens(nr) abort
  let params = {'textDocument': {'uri': org_lsp#uri(a:nr)}}
  call org_lsp#request(a:nr, 'textDocument/semanticTokens/full', params, function('s:got_tokens', [a:nr, getbufvar(a:nr, 'changedtick')]))
endfunction

function! s:got_tokens(nr, tick, error, result) abort
  if !empty(a:error) || type(a:result) != v:t_dict || !bufloaded(a:nr) || getbufvar(a:nr, 'changedtick') != a:tick
    return
  endif
  if !exists('s:faces')
    call s:load_faces(a:nr)
  endif
  call s:paint(a:nr, getbufline(a:nr, 1, '$'), get(a:result, 'data', []))
endfunction

" Faces of particular TODO keywords and priorities (`org_todo_keyword_faces`,
" `org_priority_faces`), as the server has them: a highlight group and a
" text property type for each, over the server's highlighting.
function! s:load_faces(nr) abort
  let s:faces = {}
  call org_lsp#request(a:nr, 'org-lsp/faces', v:null, function('s:got_faces', [a:nr]))
endfunction

function! s:got_faces(nr, error, result) abort
  if !empty(a:error) || type(a:result) != v:t_dict
    return
  endif
  let s:faces = a:result
  call s:define_faces()
  if !empty(filter(values(s:faces), '!empty(v:val)')) && bufloaded(a:nr)
    call s:repaint_all(a:nr)
  endif
endfunction

function! s:face_group(kind, name) abort
  let prefix = get({'todo': 'Todo', 'priority': 'Priority', 'tag': 'Tag'}, a:kind, 'Priority')
  return 'OrgLsp' . prefix . 'Face_' . substitute(a:name, '\W', '_', 'g')
endfunction

function! s:define_faces() abort
  for [kind, list] in items(get(s:, 'faces', {}))
    for [name, face] in items(list)
      call s:define_face(kind, name, face)
    endfor
  endfor
endfunction

" The highlight group and text property type of the face of keyword or
" priority `name`; returns the type.
function! s:define_face(kind, name, face) abort
  let [kind, name, face] = [a:kind, a:name, a:face]
  let group = s:face_group(kind, name)
  let attributes = filter(['bold', 'italic', 'underline', 'strikethrough'], {_, a -> get(face, a, v:false)})
  let attr = empty(attributes) ? 'NONE' : join(attributes, ',')
  let colors = ''
  if has_key(face, 'foreground')
    let colors .= ' guifg=' . face.foreground
  endif
  if has_key(face, 'background')
    let colors .= ' guibg=' . face.background
  endif
  try
    execute 'highlight default' group 'term=' . attr 'cterm=' . attr 'gui=' . attr colors
  catch
  endtry
  " A color name the terminal knows, without 'termguicolors'.
  for [key, value] in [['ctermfg', get(face, 'foreground', '')], ['ctermbg', get(face, 'background', '')]]
    if value =~# '^\a\+$'
      try
        execute 'highlight' group key . '=' . value
      catch
      endtry
    endif
  endfor
  let type = 'org_lsp_face_' . kind . '_' . substitute(name, '\W', '_', 'g')
  if empty(prop_type_get(type))
    call prop_type_add(type, {'highlight': group, 'combine': 1, 'priority': 110})
  endif
  let s:selectors['face:' . kind . ':' . name] = type
  return type
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

" The lines whose tokens are not those painted last, `[first, last]` from
" 0, or `[]` for none: the tokens alike at the start, and at the end once
" moved by the lines added since (as the text properties moved with the
" text), stay as they are.
function! s:changed_lines(nr, tokens, line_count) abort
  let [old, old_count] = get(s:painted, a:nr, [v:null, -1])
  if old is v:null
    return [0, a:line_count - 1]
  endif
  let [n_old, n_new] = [len(old), len(a:tokens)]
  " The longest alike start and end, each found by halves.
  let [low, high] = [0, min([n_old, n_new])]
  while low < high
    let mid = (low + high + 1) / 2
    if old[: mid - 1] == a:tokens[: mid - 1]
      let low = mid
    else
      let high = mid - 1
    endif
  endwhile
  let same_start = low
  if same_start == n_old && n_old == n_new && old_count == a:line_count
    return []
  endif
  let moved = a:line_count - old_count
  let shifted = moved == 0 ? old : map(copy(old), {_, t -> [t[0] + moved] + t[1 :]})
  let [low, high] = [0, min([n_old, n_new]) - same_start]
  while low < high
    let mid = (low + high + 1) / 2
    if shifted[n_old - mid :] == a:tokens[n_new - mid :]
      let low = mid
    else
      let high = mid - 1
    endif
  endwhile
  let same_end = low
  let first = same_start > 0 ? a:tokens[same_start - 1][0] : 0
  let last = same_end > 0 ? a:tokens[n_new - same_end][0] : a:line_count - 1
  return [first, max([first, last])]
endfunction

" The first of `tokens`, in order of their lines, on line `line` or
" after it, found by halves.
function! s:first_token_from(tokens, line) abort
  let [low, high] = [0, len(a:tokens)]
  while low < high
    let mid = (low + high) / 2
    if a:tokens[mid][0] < a:line
      let low = mid + 1
    else
      let high = mid
    endif
  endwhile
  return low
endfunction

" The bytes `[start, end]` of `length` characters from `character` of
" `text`, line `line`, as the server's encoding `encoding` counts them;
" `ascii` remembers the lines of ASCII alone, where they are the same.
function! s:byte_range(encoding, ascii, line, text, character, length) abort
  if a:encoding ==# 'utf-8'
    return [a:character, a:character + a:length]
  endif
  if !has_key(a:ascii, a:line)
    let a:ascii[a:line] = a:text !~# '[^\x00-\x7f]'
  endif
  return a:ascii[a:line] ? [a:character, a:character + a:length]
        \ : [s:byte_in(a:encoding, a:text, a:character), s:byte_in(a:encoding, a:text, a:character + a:length)]
endfunction

function! s:paint_absolute(nr, lines, tokens) abort
  if empty(s:selectors)
    call org_lsp#define_highlights()
  endif
  " Not past the buffer's lines, which it may have fewer of by now (the
  " agenda read again).
  let line_count = len(getbufline(a:nr, 1, '$'))
  let changed = s:changed_lines(a:nr, a:tokens, line_count)
  let s:painted[a:nr] = [a:tokens, line_count]
  let manifest = org_lsp#manifest()
  let types = map(copy(manifest.tokenTypes), 'v:val.id')
  let modifiers = map(copy(manifest.tokenModifiers), 'v:val.id')
  let props = {}
  let lines = a:lines
  let is_org = getbufvar(a:nr, '&filetype') ==# 'org'
  let hidden_bit = index(modifiers, 'hidden')
  " Columns as the server counts them, in bytes: the same in UTF-8, and on
  " a line of ASCII alone.
  let encoding = s:encoding()
  let ascii = {}
  " The hidden text of the whole buffer, for concealing; the rest painted
  " only on the lines that changed.
  let hidden = []
  if hidden_bit >= 0
    for [line, character, length, type_index, modifier_bits] in filter(copy(a:tokens), 'and(v:val[4], ' . (1 << hidden_bit) . ') && v:val[0] < ' . line_count)
      let text = get(lines, line, '')
      let [start, end] = s:byte_range(encoding, ascii, line, text, character, length)
      let end = min([end, len(text)])
      if end > start
        call add(hidden, [line + 1, start + 1, end - start])
      endif
    endfor
  endif
  if empty(changed)
    return
  endif
  " The tokens of those lines, in order of their lines.
  let first_token = s:first_token_from(a:tokens, changed[0])
  let last_token = s:first_token_from(a:tokens, changed[1] + 1)
  " The selectors of each type with its modifiers, made once.
  let looks = {}
  for [line, character, length, type_index, modifier_bits] in a:tokens[first_token : last_token - 1]
    if line >= line_count
      continue
    endif
    let type = get(types, type_index, '')
    let text = get(lines, line, '')
    let [start, end] = s:byte_range(encoding, ascii, line, text, character, length)
    let end = min([end, len(text)])
    if end <= start
      continue
    endif
    let key = type_index . ':' . modifier_bits
    if !has_key(looks, key)
      let selectors = [type]
      let names = []
      for bit in range(len(modifiers))
        if and(modifier_bits, 1 << bit)
          call add(selectors, '*.' . modifiers[bit])
          call add(selectors, type . '.' . modifiers[bit])
          call add(names, modifiers[bit])
        endif
      endfor
      " A look of the type with all of them (`agendaHabit.ready.future`).
      if len(names) > 1
        call add(selectors, type . '.' . join(names, '.'))
      endif
      let looks[key] = selectors
    endif
    let selectors = copy(looks[key])
    " A keyword or a priority with a face of its own.
    if type ==# 'todoKeyword' || type ==# 'priority'
      let name = text[start : end - 1]
      if type ==# 'priority'
        let name = substitute(name, '^\[#\(.\{-}\)\]$', '\1', '')
      endif
      call add(selectors, 'face:' . (type ==# 'todoKeyword' ? 'todo' : 'priority') . ':' . name)
    endif
    " `org-tag-faces`: each tag of a headline in an Org file that has a face
    " of its own, its name without the colons.
    if type ==# 'tag' && is_org
      let at = start
      while 1
        let found = matchstrpos(text, ':\zs[^: \t]\+\ze:', at)
        if found[1] < 0 || found[1] >= end
          break
        endif
        let selector = 'face:tag:' . found[0]
        if has_key(s:selectors, selector)
          call add(get(extend(props, {selector: []}, 'keep'), selector), [line + 1, found[1] + 1, line + 1, found[2] + 1])
        endif
        " The colon after a tag starts the next.
        let at = found[2]
      endwhile
    endif
    for selector in selectors
      if has_key(s:selectors, selector)
        call add(get(extend(props, {selector: []}, 'keep'), selector), [line + 1, start + 1, line + 1, end + 1])
      endif
    endfor
  endfor
  if changed[1] >= changed[0]
    call prop_remove({'types': values(s:selectors), 'bufnr': a:nr, 'all': 1}, changed[0] + 1, min([changed[1] + 1, line_count]))
  endif
  for [selector, positions] in items(props)
    call prop_add_list({'bufnr': a:nr, 'type': s:selectors[selector]}, positions)
  endfor
  if hidden != getbufvar(a:nr, 'org_lsp_hidden', [])
    call setbufvar(a:nr, 'org_lsp_hidden', hidden)
    call s:conceal(a:nr)
  endif
endfunction

" ----- attaching

let s:pending = {}

" After a change, highlighting and folds are fetched again, once typing
" pauses; at once after a change made outside Insert mode (a command's
" edit).
function! s:changed(nr) abort
  if has_key(s:pending, a:nr)
    call timer_stop(s:pending[a:nr])
  endif
  " A line `o` opens is changed while its indentation is asked for, and
  " typed on next.
  let s:pending[a:nr] = timer_start(mode() =~# '^[iR]' || s:waiting ? 200 : 20, {-> s:refresh(a:nr)})
endfunction

function! s:refresh(nr) abort
  silent! call remove(s:pending, a:nr)
  if s:waiting
    call s:changed(a:nr)
    return
  endif
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
  if getbufvar(a:nr, '&filetype') ==# 'org'
    call s:journal_directory(a:nr)
  endif
endfunction

" Set up an Org buffer, the agenda, or a capture buffer.
function! org_lsp#attach() abort
  if empty(org_lsp#manifest())
    if org_lsp#config().download && empty(org_lsp#server_command()) && !org_lsp#download#running()
      call org_lsp#download()
    endif
    return
  endif
  if exists('b:org_lsp_attached')
    return
  endif
  let b:org_lsp_attached = 1
  let nr = bufnr()
  " A buffer loaded again (`:bunload`, then opened) has none of the text
  " properties painted before.
  silent! call remove(s:painted, nr)
  " The agenda and the calendar are the server's to write: typing in them
  " changes nothing, as in Emacs's read-only buffers.
  if &filetype ==# get(org_lsp#manifest().agenda, 'languageId', 'org-agenda')
    setlocal autoread readonly nomodifiable noswapfile bufhidden=hide nowrap nonumber norelativenumber nofoldenable
    call org_lsp#set_keys('agenda')
  elseif &filetype ==# get(get(org_lsp#manifest(), 'datePrompt', {}), 'languageId', 'org-date')
    " A calendar to move about in, never to type into.
    setlocal autoread readonly nomodifiable noswapfile bufhidden=wipe nowrap nonumber norelativenumber
          \ nofoldenable nolist signcolumn=no scrolloff=0 sidescrolloff=0 winfixheight
    call org_lsp#set_keys('calendar')
  else
    call org_lsp#set_keys('org')
    if s:is_capture(bufname(nr))
      call org_lsp#set_keys('capture')
    endif
    if org_lsp#config().indent
      call s:indent_attach()
    endif
    call s:src_syntax()
  endif
  augroup org_lsp_buffer
    execute 'autocmd! * <buffer=' . nr . '>'
    execute 'autocmd TextChanged,InsertLeave,BufReadPost <buffer=' . nr . '> call s:changed(' . nr . ')'
    execute 'autocmd BufWinEnter <buffer=' . nr . '> call s:changed(' . nr . ')'
    execute 'autocmd CursorMoved,CursorMovedI,ModeChanged <buffer=' . nr . '> call s:conceal_cursor()'
    if s:is_view_type(&filetype)
      let b:org_lsp_given = getline(1, '$')
      execute 'autocmd FileChangedShellPost,BufReadPost <buffer=' . nr . '> call s:give_view(' . nr . ')'
    endif
  augroup END
  " Every change, also those an LSP client makes for the server (the TODO
  " keyword a command changed), which Vim's `TextChanged` sees only after
  " the next key or 'updatetime'.
  " One for each buffer, though it outlives the buffer's variables when
  " the buffer is unloaded.
  if has_key(s:listeners, nr)
    call listener_remove(s:listeners[nr])
  endif
  let s:listeners[nr] = listener_add(function('s:listened'), nr)
  " The client may still be starting the server: ask again until it
  " answers.
  call s:first_refresh(nr, 0)
endfunction

" The listener of each buffer, by its number.
let s:listeners = {}

function! s:listened(nr, start, end, added, changes) abort
  call s:changed(a:nr)
endfunction

" The editor's names of languages (the server's, `org-lsp/codeBlocks`)
" where Vim's syntax file has another.
let s:src_syntaxes = {'elisp': 'lisp', 'bash': 'sh'}

" Whether each language has a syntax file, by name, for the runtime path
" it was looked for in: looking goes through the whole runtime path, for
" every block each time the code is shown. A missing one is looked for
" again after a while, in case it was installed since.
let s:syntax_files = {'runtimepath': '', 'found': {}}

function! s:has_syntax(lang) abort
  if s:syntax_files.runtimepath !=# &runtimepath
    let s:syntax_files = {'runtimepath': &runtimepath, 'found': {}}
  endif
  let now = reltimefloat(reltime())
  let known = get(s:syntax_files.found, a:lang, v:null)
  if known is v:null || (!known.found && now - known.at > 10)
    let known = {'found': !empty(globpath(&runtimepath, 'syntax/' . a:lang . '.vim')), 'at': now}
    let s:syntax_files.found[a:lang] = known
  endif
  return known.found
endfunction

" The plugin's syntax for Org (syntax/org.vim) in buffer `nr` in place of
" Vim's own, which 'runtimepath' finds first when the plugin comes after
" $VIMRUNTIME; the code of its blocks in their languages again. A timer
" that calls it passes its own id after `nr`.
let s:syntax_file = expand('<sfile>:p:h:h') . '/syntax/org.vim'
function! org_lsp#own_syntax(nr, ...) abort
  if !getbufvar(a:nr, 'org_lsp_syntax', 0)
    call org_lsp#reload_syntax(a:nr)
  endif
endfunction

" The plugin's syntax for Org loaded in buffer `nr` again, with the code
" of its blocks; in a window, as syntax is loaded, or when the buffer next
" shows in one.
function! org_lsp#reload_syntax(nr) abort
  let win = bufwinid(a:nr)
  if getbufvar(a:nr, '&syntax') !=# 'org'
    return
  endif
  call setbufvar(a:nr, 'org_lsp_syntax_stale', win < 0)
  if win < 0
    return
  endif
  call win_execute(win, ['syntax clear', 'unlet! b:current_syntax', 'source ' . fnameescape(s:syntax_file)])
  if bufnr() == a:nr
    call s:src_syntax()
  endif
endfunction

" The TODO keywords of a file and how its headlines look
" (`org-lsp/syntax`), for its syntax to color them before the tokens come:
" kept in `b:org_lsp_syntax_info`, and the last in `g:org_lsp_syntax_info`
" for files opened next.
function! s:syntax(params) abort
  let info = filter(copy(a:params), 'v:key !=# "uri"')
  let g:org_lsp_syntax_info = info
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr < 0 || !bufloaded(nr) || getbufvar(nr, 'org_lsp_syntax_info', {}) == info
    return
  endif
  call setbufvar(nr, 'org_lsp_syntax_info', info)
  call org_lsp#reload_syntax(nr)
endfunction

" Buffers of the plugin's syntax for Org loaded again after a new color
" scheme, whose colors the groups of keywords and tags with faces of their
" own are made from.
function! org_lsp#reload_syntaxes() abort
  for buffer in getbufinfo({'bufloaded': 1})
    if getbufvar(buffer.bufnr, 'org_lsp_syntax', 0)
      call org_lsp#reload_syntax(buffer.bufnr)
    endif
  endfor
endfunction

" Where the code of a file's source, export and inline source blocks is
" (`org-lsp/codeBlocks`): `blocks` of `{range, language, name, kind}`.
" Kept with the buffer, and shown when it is the current one, as syntax
" works in the current buffer.
function! s:code_blocks(params) abort
  let nr = bufnr(org_lsp#uri_to_path(a:params.uri))
  if nr < 0 || !bufloaded(nr)
    return
  endif
  call setbufvar(nr, 'org_lsp_code_blocks', get(a:params, 'blocks', []))
  " Each change brings them: while the buffer is still being changed, the
  " refresh to come shows them.
  if bufnr() == nr && !s:waiting && !has_key(s:pending, nr)
    call s:src_syntax()
  endif
endfunction

" The code of source and export blocks (`#+begin_src python`) and inline
" source blocks (`src_python{…}`) in its language's syntax, as Emacs shows
" it with `org-src-fontify-natively`, where the server says it is: the
" language's syntax file included once, as Vim's Markdown does for fenced
" code, and a region for each piece of code from where it starts to where
" it ends, made again each time the server says. For older text than the
" buffer has, newer blocks are on their way.
function! s:src_syntax() abort
  let blocks = get(b:, 'org_lsp_code_blocks', [])
  " The same blocks as last made, in the syntax loaded then.
  let made = [blocks, get(b:, 'org_lsp_syntax_loads', 0), org_lsp#config().src_highlight]
  if made == get(b:, 'org_lsp_src_made', [])
    return
  endif
  let b:org_lsp_src_made = made
  let groups = get(b:, 'org_lsp_src_groups', [])
  for group in groups
    execute 'silent! syntax clear ' . group
  endfor
  let b:org_lsp_src_groups = []
  if !org_lsp#config().src_highlight || empty(blocks)
    return
  endif
  let last = line('$')
  let groups = {}
  let encoding = s:encoding()
  " The languages included, checked once each.
  let included = {}
  for block in blocks
    let lang = get(s:src_syntaxes, block.language, block.language)
    if type(lang) != v:t_string || lang !~# '^\w\+$' || !s:has_syntax(lang)
      continue
    endif
    let start = block.range.start
    let end = block.range.end
    if end.line >= last
      continue
    endif
    let cluster = 'orgLspSrc_' . lang
    " Vim's syntax for the buffer forgets the file when it is set again.
    " `syntax list` pads a short name with blanks before `cluster=`.
    if !has_key(included, cluster) && execute('silent! syntax list @' . cluster) !~# '\<' . cluster . '\s\+cluster='
      let current = get(b:, 'current_syntax', v:null)
      unlet! b:current_syntax
      execute 'silent! syntax include @' . cluster . ' syntax/' . lang . '.vim'
      if current is v:null
        unlet! b:current_syntax
      else
        let b:current_syntax = current
      endif
    endif
    let included[cluster] = 1
    let group = 'orgLspSrcCode_' . lang
    let groups[group] = 1
    let col = s:byte_in(encoding, getline(start.line + 1), start.character)
    let end_col = s:byte_in(encoding, getline(end.line + 1), end.character)
    " Within the blocks of the plugin's syntax for Org (syntax/org.vim),
    " whose regions take the whole block.
    execute printf('syntax region %s start=/\%%%dl\%%%dc/ end=/\%%%dl\%%%dc/ keepend contains=@%s containedin=orgSyntaxRawBlock',
          \ group, start.line + 1, col + 1, end.line + 1, end_col + 1, cluster)
  endfor
  " The files included sync as suits their language alone: Org's sync from
  " the headline before (syntax/org.vim) again, as a block ends before the
  " next headline. From the start, a change far in the file takes longer
  " than 'redrawtime'.
  syntax sync clear
  syntax sync match orgSyntaxSync grouphere NONE /^\*\+\s/
  let b:org_lsp_src_groups = keys(groups)
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
