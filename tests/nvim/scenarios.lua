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

local function fail(message)
  io.stderr:write(('FAIL: %s\n'):format(message))
  vim.cmd('cquit 1')
end

local name = names[1]
local spec = json(vim.fs.joinpath(e2e, 'scenarios', name .. '.json'))
local exceptions = json(vim.fs.joinpath(e2e, 'exceptions.json'))
local excepted = vim.tbl_get(exceptions, 'editors', 'nvim') or {}

if spec.emacs and not emacs_found() then
  if os.getenv('ORG_LSP_REQUIRE_EMACS') then
    fail('Emacs is required but not found')
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
-- The echo area is not looked at: what a script would print of it is
-- noise.
vim.o.showmode = false
vim.api.nvim_echo = function() end
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')
local plugin = require('org-lsp')

-- What the server said, as the user sees it: notifications.
local messages = {}
vim.notify = function(message)
  table.insert(messages, (tostring(message):gsub('^org%-lsp: ', '')))
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
        vim.wait(5000, function()
          return done
        end, 2)
      end
    end
    vim.wait(20)
    local pending = false
    for _, request in pairs(lsp and lsp.requests or {}) do
      pending = pending or request.type == 'pending'
    end
    local state = { vim.api.nvim_get_current_buf(), vim.api.nvim_win_get_cursor(0), #messages, vim.fn.mode() }
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
  for line in (read(vim.fs.joinpath(e2e, 'expected', name .. '.txt'))):gmatch('([^\n]*)\n') do
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
  vim.wait(10000, function()
    local lsp = vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' })[1]
    return lsp ~= nil and lsp.initialized and plugin.manifest ~= nil
  end, 10)
end

-- What the plugin asked of insert mode: a script never returns to the
-- main loop, where insert mode would start, so typing goes where it was
-- asked to start.
local insert_at_end
do
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
  for _, kind in ipairs(step.expect or {}) do
    local want = expected[index .. ' ' .. kind]
    if not want then
      fail(('step %d: no expected %s; run the server\'s runner with ORG_LSP_UPDATE_E2E=1'):format(index, kind))
    end
    local want_text = #want > 0 and table.concat(want, '\n') .. '\n' or ''
    -- What comes later than the server's last answer (a window that
    -- closes) is given a moment more.
    local got
    vim.wait(3000, function()
      got = snapshot(kind):gsub(vim.pesc(directory), '$DIR')
      if got == want_text then
        return true
      end
      settle()
      return false
    end, 50)
    if got ~= want_text then
      fail(('%s step %d %s:\n--- expected\n%s--- actual\n%s'):format(name, index, kind, want_text, got))
    end
  end
  if vim.tbl_contains(step.expect or {}, 'messages') then
    messages = {}
  end
end

for index, step in ipairs(spec.steps) do
  run_step(index, step)
end
vim.cmd('qall!')
