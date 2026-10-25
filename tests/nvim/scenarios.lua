-- The shared end-to-end scenarios (tests/e2e/scenarios/*.json) in Neovim,
-- by pressing the keys the plugin sets for each step's command.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/scenarios.lua [NAME…]
--
-- Without names, every scenario runs, each in a Neovim of its own, a few
-- at a time; with names, those run here, one after the other. What a
-- scenario expects is what the server's own runner saw
-- (server/org-lsp/tests/e2e_scenarios.rs, tests/e2e/expected/NAME.txt).
-- A command with no key in Neovim fails, unless tests/e2e/exceptions.json
-- says why under `editors.nvim`.
--
--   nvim --headless -u NONE -l tests/nvim/scenarios.lua --typed [NAME…]
--
-- runs them typed instead, as a user meets them: each in Neovim's own
-- terminal UI in tmux, nothing replaced, 'updatetime' short so that
-- `CursorHold` comes between keys. That Neovim plays the scenario on
-- timers and writes down the keys of each step, and the answer to what
-- the screen asks: a key at a key prompt, text in the command line, the
-- number of a choice that the list shows. This runner types them through
-- `tmux send-keys`, a key at a time, and presses Enter at a hit-enter
-- prompt, as a user does. Neovim's own typed checks are with them, in
-- tests/nvim/typed/NAME.json: steps that may type `keys` as they are, and
-- look for text on the `screen`.

local root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(vim.fs.abspath(debug.getinfo(1, 'S').source:sub(2)))))
local server = os.getenv('ORG_LSP_BIN') or vim.fs.joinpath(root, 'target', 'debug', 'org-lsp')
local e2e = vim.fs.joinpath(root, 'tests', 'e2e')

local function read(path)
  local file = assert(io.open(path, 'rb'))
  local text = file:read('*a')
  file:close()
  return text
end

local function json(path)
  return vim.json.decode(read(path), { luanil = { object = true, array = true } })
end

local function emacs_found()
  return vim.fn.executable('emacs') == 1
end

-- ----- running every scenario

local names = _G.arg or {}
local typed_run = names[1] == '--typed'
if typed_run then
  table.remove(names, 1)
end
-- The scenario this Neovim plays typed, in tmux.
local typed = os.getenv('ORG_LSP_TYPED')
if typed then
  names = { typed }
end
local tmux = { 'tmux', '-L', 'org-lsp-typed', '-f', '/dev/null' }

-- ----- typing them, each in a terminal of its own

if typed_run then
  if #names == 0 then
    for _, directory in ipairs({ vim.fs.joinpath(e2e, 'scenarios'), vim.fs.joinpath(root, 'tests', 'nvim', 'typed') }) do
      for name, kind in vim.fs.dir(directory) do
        if kind == 'file' and name:match('%.json$') then
          table.insert(names, (name:gsub('%.json$', '')))
        end
      end
    end
    table.sort(names)
  end
  local jobs = tonumber(os.getenv('ORG_LSP_E2E_JOBS') or '') or 4
  local timeout = tonumber(os.getenv('ORG_LSP_TYPED_TIMEOUT') or '') or 120
  local script = vim.fs.abspath(debug.getinfo(1, 'S').source:sub(2))
  local running, failed, done, next_index = {}, {}, 0, 1
  local started = vim.uv.hrtime()
  local function start(name)
    local session = ('ntyped-%d-%s'):format(vim.uv.os_getpid(), name)
    local home = vim.fn.tempname()
    local result = home .. '/result'
    vim.fn.mkdir(home, 'p')
    -- This environment (tmux's server may have been started from another),
    -- but the terminal's own, and a home of its own: no configuration,
    -- history or swap files of the machine's.
    local env = { 'env' }
    for key, value in pairs(vim.fn.environ()) do
      if not vim.tbl_contains({ 'TERM', 'COLORTERM', 'TMUX', 'TMUX_PANE', 'NVIM' }, key) then
        table.insert(env, key .. '=' .. value)
      end
    end
    vim.list_extend(env, { 'ORG_LSP_TYPED=' .. name, 'ORG_LSP_TYPED_RESULT=' .. result, 'ORG_LSP_TYPED_SESSION=' .. session })
    for _, xdg in ipairs({ 'CONFIG', 'DATA', 'STATE', 'CACHE' }) do
      table.insert(env, ('XDG_%s_HOME=%s/%s'):format(xdg, home, xdg:lower()))
    end
    local command = vim.list_extend(vim.list_extend(vim.deepcopy(tmux), { 'new-session', '-d', '-s', session, '-x', '80', '-y', '24' }), env)
    vim.list_extend(command, { vim.v.progpath, '-u', 'NONE', '-S', script })
    local out = vim.system(command, { text = true }):wait()
    if out.code ~= 0 then
      error('tmux: ' .. (out.stderr or ''))
    end
    running[name] = { session = session, result = result, since = vim.uv.hrtime(), typed = 0 }
  end
  --- Type what the scenario asks to have typed, a key at a time, as fast
  --- as a quick typist.
  local function type_asked(job)
    local path = ('%s.keys.%d'):format(job.result, job.typed + 1)
    if not vim.uv.fs_stat(path) then
      return
    end
    job.typed = job.typed + 1
    local first = true
    for _, part in ipairs(vim.json.decode(read(path))) do
      for _, key in ipairs(part.text and vim.fn.split(part.text, '\\zs') or { part.key }) do
        if not first then
          vim.wait(30)
        end
        first = false
        local command = vim.list_extend(vim.deepcopy(tmux), { 'send-keys', '-t', job.session })
        vim.list_extend(command, part.text and { '-l', '--', key } or { key })
        vim.system(command):wait()
      end
    end
  end
  --- Press Enter at a hit-enter prompt (a message longer than the echo
  --- area), as a user does; while it shows, the scenario cannot go on.
  --- How many times is written for the scenario to see, which fails when
  --- one came before what was asked could be answered.
  local function enter_if_asked(job)
    local now = vim.uv.hrtime()
    if now - (job.looked or 0) < 300e6 then
      return
    end
    job.looked = now
    local out = vim.system(vim.list_extend(vim.deepcopy(tmux), { 'capture-pane', '-p', '-t', job.session }), { text = true }):wait()
    local lines = vim.split(vim.trim(out.stdout or ''), '\n')
    if lines[#lines] and lines[#lines]:match('^Press ENTER or type command to continue') then
      vim.system(vim.list_extend(vim.deepcopy(tmux), { 'send-keys', '-t', job.session, 'Enter' })):wait()
      job.enters = (job.enters or 0) + 1
      local file = assert(io.open(job.result .. '.enters.part', 'wb'))
      file:write(tostring(job.enters))
      file:close()
      vim.uv.fs_rename(job.result .. '.enters.part', job.result .. '.enters')
    end
  end
  local function finish(name, text)
    local session = running[name].session
    if not text then
      local screen = vim.system(vim.list_extend(vim.deepcopy(tmux), { 'capture-pane', '-p', '-t', session }), { text = true }):wait()
      local log = vim.uv.fs_stat(running[name].result .. '.log') and vim.split(read(running[name].result .. '.log'), '\n') or {}
      text = ('FAIL: no result after %d s; what it did last:\n%s\nthe screen:\n%s'):format(timeout, table.concat(vim.list_slice(log, math.max(1, #log - 30)), '\n'), screen.stdout or '')
    end
    vim.system(vim.list_extend(vim.deepcopy(tmux), { 'kill-session', '-t', session }), { text = true }):wait()
    running[name] = nil
    done = done + 1
    if text:match('^ok') then
      io.stdout:write(('ok   %s%s\n'):format(name, (text:sub(3)):gsub('\n$', '')))
    else
      table.insert(failed, name)
      io.stdout:write(('FAIL %s (typed)\n%s\n'):format(name, text))
    end
  end
  while done < #names do
    while next_index <= #names and vim.tbl_count(running) < jobs do
      start(names[next_index])
      next_index = next_index + 1
    end
    vim.wait(10)
    for name, job in pairs(running) do
      type_asked(job)
      enter_if_asked(job)
      if vim.uv.fs_stat(job.result) then
        -- (The result is written whole, then renamed.)
        finish(name, read(job.result))
      elseif (vim.uv.hrtime() - job.since) / 1e9 > timeout then
        finish(name, nil)
      end
    end
  end
  io.stdout:write(('%d scenarios typed, %d failed, %.1f s\n'):format(#names, #failed, (vim.uv.hrtime() - started) / 1e9))
  vim.cmd(#failed > 0 and 'cquit 1' or 'qall!')
  return
end

if #names == 0 then
  local all = {}
  for name, kind in vim.fs.dir(vim.fs.joinpath(e2e, 'scenarios')) do
    if kind == 'file' and name:match('%.json$') then
      table.insert(all, (name:gsub('%.json$', '')))
    end
  end
  table.sort(all)
  local jobs = tonumber(os.getenv('ORG_LSP_E2E_JOBS') or '') or 4
  local running, failed, done, next_index = {}, {}, 0, 1
  local started = vim.uv.hrtime()
  local function start(name)
    local command = { vim.v.progpath, '--headless', '-u', 'NONE', '-l', debug.getinfo(1, 'S').source:sub(2), name }
    running[name] = vim.system(command, { text = true }, function(result)
      vim.schedule(function()
        running[name] = nil
        done = done + 1
        if result.code ~= 0 then
          table.insert(failed, name)
          io.stdout:write(('FAIL %s\n%s%s\n'):format(name, result.stdout or '', result.stderr or ''))
        else
          io.stdout:write(('ok   %s%s\n'):format(name, (result.stdout or ''):gsub('\n$', '')))
        end
      end)
    end)
  end
  while done < #all do
    while next_index <= #all and vim.tbl_count(running) < jobs do
      start(all[next_index])
      next_index = next_index + 1
    end
    vim.wait(50)
  end
  io.stdout:write(('%d scenarios, %d failed, %.1f s\n'):format(#all, #failed, (vim.uv.hrtime() - started) / 1e9))
  vim.cmd(#failed > 0 and 'cquit 1' or 'qall!')
  return
end

-- ----- one scenario

--- The end of a typed scenario: what it gives the runner, then Neovim
--- quits.
local function typed_result(text)
  local path = os.getenv('ORG_LSP_TYPED_RESULT')
  local out = assert(io.open(path .. '.part', 'wb'))
  out:write(text)
  out:close()
  vim.uv.fs_rename(path .. '.part', path)
  vim.cmd('qall!')
end

local function fail(message)
  if typed then
    -- Out of the scenario, which `typed_result` ends.
    error({ failed = message }, 0)
  end
  io.stderr:write(('FAIL: %s\n'):format(message))
  vim.cmd('cquit 1')
end

local name = names[1]
-- Neovim's own typed checks, of what a scenario does not see: the screen.
local own = vim.fs.joinpath(root, 'tests', 'nvim', 'typed', name .. '.json')
local spec = json(typed and vim.uv.fs_stat(own) and own or vim.fs.joinpath(e2e, 'scenarios', name .. '.json'))
local exceptions = json(vim.fs.joinpath(e2e, 'exceptions.json'))
local excepted = vim.tbl_get(exceptions, 'editors', 'nvim') or {}

if spec.emacs and not emacs_found() then
  if os.getenv('ORG_LSP_REQUIRE_EMACS') then
    if typed then
      return typed_result('FAIL: Emacs is required but not found\n')
    end
    fail('Emacs is required but not found')
  end
  if typed then
    return typed_result('ok (no Emacs, skipped)\n')
  end
  io.stdout:write(' (no Emacs, skipped)\n')
  vim.cmd('qall!')
  return
end

-- The scenario's files, in a directory of their own.
local directory = vim.fn.tempname()
vim.fn.mkdir(directory, 'p')
directory = vim.uv.fs_realpath(directory)
local files = {}
if spec.files_from then
  local from = vim.fs.joinpath(root, 'tests', spec.files_from)
  for file, kind in vim.fs.dir(from) do
    if kind == 'file' and (file:match('%.org$') or file:match('%.org_archive$')) then
      files[file] = read(vim.fs.joinpath(from, file))
    end
  end
end
for file, text in pairs(spec.files or {}) do
  files[file] = type(text) == 'table' and table.concat(text, '\n') .. '\n' or text
end
for file, text in pairs(files) do
  local path = vim.fs.joinpath(directory, file)
  vim.fn.mkdir(vim.fs.dirname(path), 'p')
  local out = assert(io.open(path, 'wb'))
  out:write(text)
  out:close()
end

--- `value` with `$DIR` in its strings replaced by the directory.
local function substitute(value)
  if type(value) == 'string' then
    return (value:gsub('%$DIR', directory))
  elseif type(value) == 'table' then
    local out = {}
    for key, item in pairs(value) do
      out[key] = substitute(item)
    end
    return out
  end
  return value
end

-- The options of the server's runner: the clock and the agenda files,
-- then the scenario's.
local settings = {
  clock = spec.clock or { today = '2026-05-18', now = '10:30' },
  org_agenda_files = { directory },
  org_directory = directory,
  org_default_notes_file = directory .. '/notes.org',
}
for key, value in pairs(spec.options or {}) do
  settings[key] = substitute(value)
end
if spec.emacs then
  local lisp = os.getenv('ORG_LISP_DIR')
  settings.emacs_config = (lisp and ('(add-to-list \'load-path %q) '):format(lisp) or '') .. (settings.emacs_config or '')
end

vim.g.org_lsp = { cmd = { server }, settings = settings }
if typed then
  -- `CursorHold` and the like between the keys, as a user who types
  -- slower than that meets them.
  vim.o.updatetime = 100
else
  -- The echo area is not looked at: what a script would print of it is
  -- noise.
  vim.o.showmode = false
  vim.api.nvim_echo = function() end
end
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')
local plugin = require('org-lsp')

-- What the server said, as the user sees it: notifications, which a
-- typed scenario still shows.
local messages = {}
do
  local show = vim.notify
  vim.notify = function(message, ...)
    table.insert(messages, tostring(message))
    if typed then
      return show(message, ...)
    end
  end
end

-- What is asked is answered from the step's answers, in turn: keys,
-- text typed in the input box, a choice from a list.
local answers = {}
local function next_answer(prompt)
  if #answers == 0 then
    fail(('`%s` is asked and the step has no answer left'):format(prompt))
  end
  return table.remove(answers, 1)
end
-- (A typed scenario types them.)
if not typed then
  plugin.read_key = function(prompt)
    local answer = next_answer(prompt)
    return answer
  end
  vim.ui.input = function(opts, on_confirm)
    local answer = next_answer(opts.prompt)
    vim.schedule(function()
      on_confirm(answer)
    end)
  end
  vim.ui.select = function(items, opts, on_choice)
    local answer = next_answer(opts.prompt)
    local label = type(answer) == 'table' and answer.choice or answer
    local format = opts.format_item or tostring
    local picked
    for _, item in ipairs(items) do
      if format(item) == label then
        picked = picked or item
      end
    end
    for _, item in ipairs(items) do
      if not picked and format(item):find(label, 1, true) then
        picked = item
      end
    end
    if not picked then
      fail(('no choice `%s` in %s'):format(label, vim.inspect(vim.tbl_map(format, items))))
    end
    vim.schedule(function()
      on_choice(picked)
    end)
  end
end

-- What column view shows, as the server says it.
local columns
do
  local view = require('org-lsp.columns')
  local show = view.show
  view.show = function(params)
    columns = params
    return show(params)
  end
end

local function client()
  return vim.lsp.get_clients({ name = 'org_lsp' })[1]
end

--- Wait `ms`. A typed scenario runs in a coroutine, and Neovim goes on
--- meanwhile, reading what is typed.
local function sleep(ms)
  if not typed then
    return vim.wait(ms)
  end
  local co = coroutine.running()
  vim.defer_fn(function()
    _G.org_lsp_typed_resume(co)
  end, ms)
  coroutine.yield()
end

--- Wait until `condition` holds, at most `timeout` ms.
local function wait_until(condition, timeout, interval)
  if not typed then
    return vim.wait(timeout, condition, interval)
  end
  local deadline = vim.uv.hrtime() + timeout * 1e6
  while not condition() do
    if vim.uv.hrtime() > deadline then
      return false
    end
    sleep(interval or 10)
  end
  return true
end

--- Wait until the server has handled everything, and the plugin has done
--- what it asked: nothing changes any more between two round trips.
local function settle()
  local last
  for _ = 1, 300 do
    local lsp = client()
    if lsp then
      for _ = 1, 2 do
        local done = false
        lsp:request('org-lsp/test-sync', vim.NIL, function()
          done = true
        end)
        wait_until(function()
          return done
        end, 5000, 2)
      end
    end
    -- (What the plugin does a moment later, too, when typed.)
    sleep(typed and 60 or 20)
    local pending = false
    for _, request in pairs(lsp and lsp.requests or {}) do
      pending = pending or request.type == 'pending'
    end
    local state = { vim.api.nvim_get_current_buf(), vim.api.nvim_win_get_cursor(0), #messages, vim.fn.mode() }
    if typed then
      vim.list_extend(state, { vim.fn.mode(1), vim.fn.getcmdline(), #vim.api.nvim_list_wins() })
    end
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      table.insert(state, vim.api.nvim_buf_is_loaded(bufnr) and vim.api.nvim_buf_get_changedtick(bufnr) or -1)
    end
    if not pending and vim.deep_equal(state, last) then
      return
    end
    last = state
  end
end

local function press(keys)
  vim.api.nvim_feedkeys(vim.keycode(keys), 'mx', false)
end

local agenda, date_prompt = 'org-agenda:/Org%20Agenda', 'org-agenda:/Org%20Date'

--- How a buffer is named in the snapshots, as the server's runner names
--- documents.
local function name_of(bufnr)
  local uri = vim.uri_from_bufnr(bufnr)
  if uri == agenda then
    return 'agenda'
  elseif uri == date_prompt then
    return 'date prompt'
  end
  local path = vim.api.nvim_buf_get_name(bufnr)
  if vim.startswith(path, directory .. '/') then
    return path:sub(#directory + 2)
  end
  return vim.fs.basename(path)
end

local function buffer_of(uri)
  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(bufnr) and vim.uri_from_bufnr(bufnr) == uri then
      return bufnr
    end
  end
end

local function text_of(bufnr)
  return table.concat(vim.api.nvim_buf_get_lines(bufnr, 0, -1, false), '\n'):gsub('\n*$', '') .. '\n'
end

--- The scope of the current buffer's keys, and the scopes it falls back
--- to: a capture buffer has the keys of Org files too, and the global
--- keys work everywhere.
local function scopes()
  local filetype = vim.bo.filetype
  if require('org-lsp.columns').on(0) then
    return { 'columns', filetype == 'org-agenda' and 'agenda' or 'org', 'global' }
  elseif filetype == 'org-agenda' then
    return { 'agenda', 'global' }
  elseif filetype == 'org-date' then
    return { 'calendar', 'global' }
  end
  local prefix = plugin.manifest and plugin.manifest.capture and plugin.manifest.capture.prefix
  if prefix and vim.startswith(vim.fs.basename(vim.api.nvim_buf_get_name(0)), prefix) then
    return { 'capture', 'org', 'global' }
  elseif filetype == 'org' then
    return { 'org', 'global' }
  end
  return { 'global' }
end

--- The key the plugin set for `command` where the cursor is, the first
--- of its keys; each of them is checked to be mapped to it in the buffer.
local function key_of(command)
  for _, scope in ipairs(scopes()) do
    for _, entry in ipairs(plugin.keys(scope)) do
      if entry.command == command and entry.keys[1] then
        for _, lhs in ipairs(scope == 'global' and {} or entry.keys) do
          local map = vim.fn.maparg(lhs, 'n', false, true)
          if map.buffer ~= 1 or map.desc ~= entry.title then
            fail(('%s: `%s` is not mapped to %s in %s'):format(name, lhs, command, scope))
          end
        end
        return entry.keys[1]
      end
    end
  end
end

local function columns_text()
  if not columns then
    return '(no column view)\n'
  end
  if columns.lines == nil or columns.lines == vim.NIL then
    return '(column view off)\n'
  end
  local out = { ((columns.header or ''):gsub('%s+$', '')) }
  for _, row in ipairs(columns.lines) do
    local values = (table.concat(row[3] or {}, ' | '):gsub('%s+$', ''))
    table.insert(out, ('%d: %s'):format(row[1] + 1, values))
  end
  return table.concat(out, '\n') .. '\n'
end

local function snapshot(kind)
  if kind == 'agenda' then
    local bufnr = buffer_of(agenda)
    return bufnr and text_of(bufnr) or '(no agenda)\n'
  elseif kind == 'buffer' then
    return ('[%s]\n%s'):format(name_of(0), text_of(0))
  elseif kind == 'cursor' then
    local line = vim.api.nvim_win_get_cursor(0)[1]
    return ('%s:%d: %s\n'):format(name_of(0), line, vim.api.nvim_get_current_line())
  elseif kind == 'messages' then
    return #messages > 0 and table.concat(messages, '\n') .. '\n' or ''
  elseif kind == 'columns' then
    return columns_text()
  end
  local file = assert(kind:match('^file:(.*)'), 'unknown snapshot kind ' .. kind)
  local path = vim.fs.joinpath(directory, file)
  local bufnr = buffer_of(vim.uri_from_fname(path))
  if bufnr then
    return text_of(bufnr)
  end
  if vim.uv.fs_stat(path) then
    return read(path):gsub('\n*$', '') .. '\n'
  end
  return '(no file)\n'
end

--- The expected snapshots, by step and kind.
local expected = {}
do
  local current
  local path = vim.fs.joinpath(e2e, 'expected', name .. '.txt')
  for line in (vim.uv.fs_stat(path) and read(path) or ''):gmatch('([^\n]*)\n') do
    local step, kind = line:match('^=== (%d+) (.+)$')
    if step then
      current = step .. ' ' .. kind
      expected[current] = {}
    elseif current then
      table.insert(expected[current], line)
    end
  end
end

--- Put the cursor where the step says, as a user moves it there.
local function place(step)
  local row, column
  if step.line then
    row, column = step.line, step.column or 0
  elseif step.at then
    for number, text in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
      local at = text:find(step.at, 1, true)
      if at then
        row, column = number, at - 1 + (step.column or 0)
        break
      end
    end
    if not row then
      fail(('`%s` is not in %s'):format(step.at, name_of(0)))
    end
  end
  if row then
    vim.api.nvim_win_set_cursor(0, { row, column })
    -- `CursorMoved` comes from the main loop, which a script does not run.
    vim.api.nvim_exec_autocmds('CursorMoved', { buffer = 0 })
    settle()
  end
end

local function wait_for_attach()
  wait_until(function()
    local lsp = vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' })[1]
    return lsp ~= nil and lsp.initialized and plugin.manifest ~= nil
  end, 10000, 10)
end

-- What the plugin asked of insert mode: a script never returns to the
-- main loop, where insert mode would start, so typing goes where it was
-- asked to start.
local insert_at_end
if not typed then
  local command = vim.cmd
  vim.cmd = setmetatable({}, {
    __index = command,
    __call = function(_, text)
      if type(text) == 'string' and text:match('^startinsert') then
        insert_at_end = text == 'startinsert!'
      end
      return command(text)
    end,
  })
  local startinsert = command.startinsert
  rawset(vim.cmd, 'startinsert', function(opts)
    insert_at_end = type(opts) == 'table' and opts.bang or false
    return startinsert(opts)
  end)
end

local check

local function run_step(index, step)
  if step.open then
    vim.cmd.edit(vim.fn.fnameescape(vim.fs.joinpath(directory, step.open)))
    wait_for_attach()
    settle()
  end
  if step.focus then
    local bufnr = step.focus == 'agenda' and buffer_of(agenda) or buffer_of(vim.uri_from_fname(vim.fs.joinpath(directory, step.focus)))
    if not bufnr then
      fail(('step %d: no buffer %s to go to'):format(index, step.focus))
    end
    local win = vim.fn.win_findbuf(bufnr)[1]
    if win then
      vim.api.nvim_set_current_win(win)
    else
      vim.api.nvim_set_current_buf(bufnr)
    end
    settle()
  end
  if step.close then
    -- The agenda's own key.
    local quit = key_of('quit')
    if not quit then
      fail(('step %d: no key to close %s'):format(index, name_of(0)))
    end
    press(quit)
    settle()
  end
  place(step)
  if step.type then
    local cursor = vim.api.nvim_win_get_cursor(0)
    local line = vim.api.nvim_get_current_line()
    local column = insert_at_end and #line or cursor[2]
    insert_at_end = nil
    local lines = vim.split(step.type, '\n', { plain = true })
    vim.api.nvim_buf_set_text(0, cursor[1] - 1, column, cursor[1] - 1, column, lines)
    local last = cursor[1] - 1 + #lines
    local after = #lines == 1 and column + #lines[1] or #lines[#lines]
    vim.api.nvim_win_set_cursor(0, { last, after })
    settle()
  end
  if step.command then
    local key = key_of(step.command)
    if not key then
      if excepted[step.command] then
        -- Run as the plugin runs a command without a key.
        answers = vim.deepcopy(step.answers or {})
        plugin.run(step.command, { count = step.count })
      else
        fail(('step %d: %s has no key in Neovim here (%s)'):format(index, step.command, table.concat(scopes(), ', ')))
      end
    else
      answers = vim.deepcopy(step.answers or {})
      insert_at_end = nil
      if step.wait == 'columns' then
        columns = nil
      end
      -- Out of insert mode, which a key before may have started.
      vim.api.nvim_feedkeys(vim.keycode('<C-\\><C-n>'), 'nx', false)
      press((step.count and tostring(step.count) or '') .. key)
    end
    settle()
    -- Dates are typed in the calendar's minibuffer.
    while #answers > 0 and vim.bo.filetype == 'org-date' do
      press(assert(key_of('org.calendar.type'), 'no key for org.calendar.type'))
      settle()
    end
    if step.wait == 'columns' then
      vim.wait(30000, function()
        return columns ~= nil
      end, 50)
      settle()
    end
    if #answers > 0 then
      fail(('step %d: %s did not ask for %s'):format(index, step.command, vim.inspect(answers)))
    end
  end
  check(index, step)
end

--- The step's snapshots, as they are expected.
function check(index, step)
  for _, kind in ipairs(step.expect or {}) do
    local want = expected[index .. ' ' .. kind]
    if not want then
      fail(('step %d: no expected %s; run the server\'s runner with ORG_LSP_UPDATE_E2E=1'):format(index, kind))
    end
    local want_text = #want > 0 and table.concat(want, '\n') .. '\n' or ''
    -- What comes later than the server's last answer (a window that
    -- closes) is given a moment more.
    local got
    wait_until(function()
      got = snapshot(kind):gsub(vim.pesc(directory), '$DIR')
      if got == want_text then
        return true
      end
      settle()
      return false
    end, 3000, 50)
    if got ~= want_text then
      fail(('%s step %d %s:\n--- expected\n%s--- actual\n%s'):format(name, index, kind, want_text, got))
    end
  end
  if vim.tbl_contains(step.expect or {}, 'messages') then
    messages = {}
  end
end

if not typed then
  for index, step in ipairs(spec.steps) do
    run_step(index, step)
  end
  vim.cmd('qall!')
  return
end

-- ----- one scenario, typed

local session = os.getenv('ORG_LSP_TYPED_SESSION')

--- What the scenario does and what Neovim reads, for the runner to show
--- when it is stuck, and with a failure.
local traced = {}
local trace_file = assert(io.open(os.getenv('ORG_LSP_TYPED_RESULT') .. '.log', 'w'))
local function trace(text)
  table.insert(traced, text)
  trace_file:write(text, '\n')
  trace_file:flush()
end

-- Every key Neovim has read, as it was typed.
local seen = ''
vim.on_key(function(key, keys)
  seen = seen .. (keys or '')
  trace(('  read %s (%s) in mode %s'):format(vim.fn.keytrans(keys or ''), vim.fn.keytrans(key), vim.fn.mode(1)))
end)

--- What the terminal shows. (Not by `vim.system():wait()`, which a
--- `<C-c>` typed meanwhile interrupts.)
local function screen()
  local command = vim.tbl_map(vim.fn.shellescape, vim.list_extend(vim.deepcopy(tmux), { 'capture-pane', '-p', '-t', session }))
  local pipe = assert(io.popen(table.concat(command, ' ')))
  local text = pipe:read('*a')
  pipe:close()
  return text
end

-- Keys of Vim's notation as tmux names them, and those typed as
-- characters.
local tmux_names = {
  cr = 'Enter',
  enter = 'Enter',
  ['return'] = 'Enter',
  esc = 'Escape',
  tab = 'Tab',
  space = 'Space',
  bs = 'BSpace',
  up = 'Up',
  down = 'Down',
  left = 'Left',
  right = 'Right',
  home = 'Home',
  ['end'] = 'End',
  pageup = 'PPage',
  pagedown = 'NPage',
  del = 'DC',
  insert = 'IC',
}
local characters = { lt = '<', bslash = '\\', bar = '|', leader = vim.g.mapleader or '\\', localleader = vim.g.maplocalleader or '\\' }

--- `keys` (Vim's notation) as what tmux sends: text, and keys by name.
local function tmux_keys(keys)
  local out, i = {}, 1
  local function text(chars)
    local last = out[#out]
    if last and last.text then
      last.text = last.text .. chars
    else
      table.insert(out, { text = chars })
    end
  end
  while i <= #keys do
    local token = keys:match('^<([^<>]+)>', i)
    local modifiers, key = (token or ''):match('^(.-)([^-]+)$')
    local lower = key and key:lower()
    if token and modifiers == '' and characters[lower] then
      text(characters[lower])
    elseif token and (tmux_names[lower] or lower:match('^f%d+$') or #key == 1) then
      local base = tmux_names[lower] or (lower:match('^f%d+$') and key:upper()) or key
      modifiers = modifiers:upper():gsub('A%-', 'M-')
      if modifiers == 'S-' and base == 'Tab' then
        modifiers, base = '', 'BTab'
      end
      table.insert(out, { key = modifiers .. base })
    else
      token = nil
      local length = vim.str_utf_end(keys, i) + 1
      text(keys:sub(i, i + length - 1))
      i = i + length
    end
    if token then
      i = i + #token + 2
    end
  end
  return out
end

--- Text as Vim's notation writes it.
local function literal(text)
  return (text:gsub('<', '<lt>'))
end

local typed_count = 0

--- Type `keys` (Vim's notation) in the terminal, and wait until Neovim has
--- read them.
local function type_keys(keys)
  trace('type ' .. keys)
  local want = #vim.keycode(keys)
  local before = #seen
  -- The runner types them, from outside: this Neovim does nothing
  -- meanwhile but read them. (Its own timers would not do: what is
  -- scheduled waits while the keys typed so far may start a mapping.)
  typed_count = typed_count + 1
  local path = ('%s.keys.%d'):format(os.getenv('ORG_LSP_TYPED_RESULT'), typed_count)
  local out = assert(io.open(path .. '.part', 'wb'))
  out:write(vim.json.encode(tmux_keys(keys)))
  out:close()
  vim.uv.fs_rename(path .. '.part', path)
  if not wait_until(function()
    return #seen - before >= want
  end, 10000, 10) then
    fail(('`%s` was typed, and Neovim read %s'):format(keys, vim.inspect(seen:sub(before + 1))))
  end
end

--- How many hit-enter prompts the runner has pressed Enter at.
local function enters()
  local file = io.open(os.getenv('ORG_LSP_TYPED_RESULT') .. '.enters', 'rb')
  if not file then
    return 0
  end
  local count = tonumber(file:read('*a')) or 0
  file:close()
  return count
end

--- Out of insert mode, or the command line, to normal mode.
local function to_normal()
  local mode = vim.fn.mode(1)
  if mode:match('^r') then
    fail(('a hit-enter prompt is open:\n%s'):format(screen()))
  end
  if mode ~= 'n' then
    type_keys('<C-\\><C-n>')
    settle()
  end
end

--- An Ex command, typed.
local function ex(command)
  to_normal()
  type_keys(':' .. literal(command) .. '<CR>')
  settle()
end

--- Answer what is asked as a user does, by what the screen shows.
local function answer(index, value)
  local mode = vim.fn.mode(1)
  if type(value) == 'table' then
    -- A list of choices (`inputlist()`): the number of the one wanted.
    local lines = vim.split(screen(), '\n')
    local number
    for _, exact in ipairs({ true, false }) do
      for _, line in ipairs(lines) do
        local n, label = line:match('^(%d+): (.-)%s*$')
        if n and not number and (label == value.choice or (not exact and label:find(value.choice, 1, true))) then
          number = n
        end
      end
    end
    if not number then
      fail(('step %d: no choice `%s` on the screen:\n%s'):format(index, value.choice, table.concat(lines, '\n')))
    end
    type_keys(number .. '<CR>')
  elseif mode == 'c' then
    -- Text in the command line, where a default may be.
    type_keys('<C-u>' .. literal(value) .. '<CR>')
  elseif plugin.manifest.datePrompt and vim.bo.filetype == plugin.manifest.datePrompt.languageId then
    -- A date is typed in the calendar's minibuffer.
    type_keys(assert(key_of('org.calendar.type'), 'no key for org.calendar.type'))
    settle()
    if vim.fn.mode(1) ~= 'c' then
      fail(('step %d: the calendar asks for no date to type:\n%s'):format(index, screen()))
    end
    type_keys('<C-u>' .. literal(value) .. '<CR>')
  else
    -- A key.
    type_keys(literal(value))
  end
  settle()
end

local function run_typed_step(index, step)
  trace(('step %d %s'):format(index, vim.json.encode(step)))
  if step.open then
    ex('edit ' .. vim.fn.fnameescape(vim.fs.joinpath(directory, step.open)))
    wait_for_attach()
    settle()
  end
  if step.focus then
    local bufnr = step.focus == 'agenda' and buffer_of(agenda) or buffer_of(vim.uri_from_fname(vim.fs.joinpath(directory, step.focus)))
    if not bufnr then
      fail(('step %d: no buffer %s to go to'):format(index, step.focus))
    end
    local win = vim.fn.win_findbuf(bufnr)[1]
    ex(win and ('%dwincmd w'):format(vim.fn.win_id2win(win)) or ('buffer %d'):format(bufnr))
  end
  if step.close then
    local quit = key_of('quit')
    if not quit then
      fail(('step %d: no key to close %s'):format(index, name_of(0)))
    end
    to_normal()
    type_keys(quit)
    settle()
  end
  local row, column
  if step.line then
    row, column = step.line, step.column or 0
  elseif step.at then
    for number, text in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
      local at = text:find(step.at, 1, true)
      if at then
        row, column = number, at - 1 + (step.column or 0)
        break
      end
    end
    if not row then
      fail(('`%s` is not in %s'):format(step.at, name_of(0)))
    end
  end
  if row then
    ex(('call cursor(%d, %d)'):format(row, column + 1))
  end
  if step.keys then
    -- Keys as they are, wherever they go.
    type_keys(step.keys)
    settle()
  end
  if step.type then
    if not vim.fn.mode():match('^[iR]') then
      to_normal()
      type_keys('i')
    end
    type_keys((literal(step.type):gsub('\n', '<CR>')))
    settle()
  end
  if step.command then
    local key = key_of(step.command)
    if not key and not excepted[step.command] then
      fail(('step %d: %s has no key in Neovim here (%s)'):format(index, step.command, table.concat(scopes(), ', ')))
    end
    if step.wait == 'columns' then
      columns = nil
    end
    to_normal()
    local before = enters()
    if key then
      type_keys((step.count and tostring(step.count) or '') .. key)
    else
      -- From the command line, as a command without a key runs.
      type_keys(':Org ' .. step.command:gsub('^org%.', ''):gsub('%.', ' ') .. '<CR>')
    end
    settle()
    for _, value in ipairs(step.answers or {}) do
      -- What is asked shows, not a message that waits for Enter first.
      if enters() > before then
        fail(('step %d: a hit-enter prompt came before %s could be answered'):format(index, vim.inspect(value)))
      end
      answer(index, value)
    end
    if enters() > before then
      trace('(Enter pressed at a hit-enter prompt)')
    end
    if step.wait == 'columns' then
      wait_until(function()
        return columns ~= nil
      end, 30000, 50)
      settle()
    end
  end
  -- What the screen shows (Neovim's own checks).
  for _, text in ipairs(type(step.screen) == 'string' and { step.screen } or step.screen or {}) do
    local shown
    if not wait_until(function()
      shown = screen()
      return shown:find(text, 1, true) ~= nil
    end, 3000, 50) then
      fail(('step %d: `%s` is not on the screen:\n%s'):format(index, text, shown))
    end
  end
  check(index, step)
end

--- Play the scenario on, from where it waited.
function _G.org_lsp_typed_resume(co)
  local ok, err = coroutine.resume(co)
  if not ok then
    local message = type(err) == 'table' and err.failed or (tostring(err) .. '\n' .. debug.traceback(co))
    local last = table.concat(vim.list_slice(traced, math.max(1, #traced - 40)), '\n')
    typed_result(('FAIL: %s\n--- what it did last\n%s\n--- the screen\n%s'):format(message, last, screen()))
  elseif coroutine.status(co) == 'dead' then
    typed_result('ok\n')
  end
end

vim.api.nvim_create_autocmd('VimEnter', {
  once = true,
  callback = function()
    _G.org_lsp_typed_resume(coroutine.create(function()
      for index, step in ipairs(spec.steps) do
        run_typed_step(index, step)
      end
    end))
  end,
})
