-- A small harness for the end-to-end scripts of the neovim tester.
-- Unlike tests/nvim/*.lua it goes on after a failed check, records every
-- notification, and reports at the end.
--
--   local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
--   H.init({ settings = { ... }, config = { ... } })
--   H.section('name', function() ... end)
--   H.done()

local H = {}
H.root = '/home/user/org-lsp'
H.server = os.getenv('ORG_LSP_BIN') or (H.root .. '/target/debug/org-lsp')
H.scratch = '/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/neovim'
H.fails, H.passes = {}, 0
H.notes = {} -- every notification: { message, level }
H.current = '?'

local function out(s)
  io.stdout:write(s .. '\n')
  io.stdout:flush()
end
H.out = out

function H.check(cond, msg)
  if cond then
    H.passes = H.passes + 1
  else
    table.insert(H.fails, ('[%s] %s'):format(H.current, msg))
    out(('FAIL [%s]: %s'):format(H.current, msg))
  end
  return cond
end

function H.eq(actual, expected, msg)
  return H.check(vim.deep_equal(actual, expected), ('%s\n    expected: %s\n    actual:   %s'):format(msg, vim.inspect(expected), vim.inspect(actual)))
end

function H.match(actual, pattern, msg)
  return H.check(type(actual) == 'string' and actual:find(pattern) ~= nil, ('%s\n    pattern: %s\n    actual:  %s'):format(msg, pattern, vim.inspect(actual)))
end

--- Wait for predicate (running fed keys meanwhile); returns whether it held.
function H.wait(msg, pred, timeout)
  local ok = vim.wait(timeout or 6000, function()
    vim.api.nvim_feedkeys('', 'x', false)
    local okp, r = pcall(pred)
    return okp and r
  end, 20)
  if msg then
    H.check(ok, 'timed out waiting for ' .. msg)
  end
  return ok
end

function H.sleep(ms)
  vim.wait(ms, function()
    vim.api.nvim_feedkeys('', 'x', false)
    return false
  end, 20)
end

function H.press(keys)
  vim.api.nvim_feedkeys(vim.keycode(keys), 'mx', false)
end

--- Press without waiting for the keys to run ('m' only), for keys that open
--- prompts the script answers.
function H.feed(keys)
  vim.api.nvim_feedkeys(vim.keycode(keys), 'm', false)
end

function H.lines(buf)
  return vim.api.nvim_buf_get_lines(buf or 0, 0, -1, false)
end

function H.line(n, buf)
  return vim.api.nvim_buf_get_lines(buf or 0, n - 1, n, false)[1]
end

function H.cursor()
  return vim.api.nvim_win_get_cursor(0)
end

function H.tmpdir(name)
  local d = H.scratch .. '/run/' .. (name or 'x') .. '-' .. vim.fn.getpid() .. '-' .. math.random(1e6)
  vim.fn.mkdir(d, 'p')
  return d
end

--- Write `lines` to `path` and edit it; waits for the server to attach.
function H.open(path, lines)
  if lines then
    vim.fn.mkdir(vim.fs.dirname(path), 'p')
    vim.fn.writefile(lines, path)
  end
  vim.cmd.edit(vim.fn.fnameescape(path))
  local buf = vim.api.nvim_get_current_buf()
  H.wait('the server on ' .. vim.fs.basename(path), function()
    local c = vim.lsp.get_clients({ bufnr = buf, name = 'org_lsp' })[1]
    return c and c.initialized
  end, 10000)
  return buf
end

function H.client()
  return vim.lsp.get_clients({ name = 'org_lsp' })[1]
end

--- Answer `read_key` with the keys queued; record the prompts.
H.key_prompts = {}
H.key_answers = {}
function H.keys(list)
  H.key_answers = list
end

H.inputs_asked = {}
H.input_answers = {}
function H.inputs(list)
  H.input_answers = list
end

H.select_asked = {}
H.select_answers = {}
--- Each answer: a label (string, matched exactly or as a pattern), a
--- number (index) or false (cancel).
function H.selects(list)
  H.select_answers = list
end

function H.last_note()
  local n = H.notes[#H.notes]
  return n and n[1]
end

--- Notifications since index `from`, joined.
function H.notes_since(from)
  local t = {}
  for i = from + 1, #H.notes do
    table.insert(t, H.notes[i][1])
  end
  return t
end

function H.note_seen(pattern, from)
  for i = (from or 0) + 1, #H.notes do
    if H.notes[i][1]:find(pattern, 1, true) then
      return true
    end
  end
  return false
end

function H.init(opts)
  opts = opts or {}
  math.randomseed(vim.uv.hrtime())
  local cfg = vim.tbl_deep_extend('force', { cmd = { H.server } }, opts.config or {})
  cfg.settings = opts.settings or cfg.settings or {}
  if opts.no_cmd then
    cfg.cmd = nil
  end
  vim.g.org_lsp = cfg
  vim.opt.runtimepath:prepend(H.root)
  vim.cmd('filetype plugin on')
  vim.cmd('syntax on')
  if opts.before then
    opts.before()
  end
  vim.cmd('runtime plugin/org-lsp.lua')
  local plugin = require('org-lsp')
  H.plugin = plugin
  local notify = vim.notify
  vim.notify = function(message, level, o)
    table.insert(H.notes, { tostring(message), level or vim.log.levels.INFO })
    if os.getenv('H_VERBOSE') then
      out('  notify: ' .. tostring(message))
    end
  end
  if not opts.real_prompts then
    plugin.read_key = function(prompt, choices, expert)
      table.insert(H.key_prompts, { prompt = prompt, choices = choices and vim.tbl_map(function(c)
        return c.key .. ' ' .. c.label
      end, choices) or nil, expert = expert })
      if os.getenv('H_VERBOSE') then
        out('  read_key: ' .. prompt)
      end
      local a = table.remove(H.key_answers, 1)
      if a == false then
        return nil
      end
      return a
    end
    vim.ui.input = function(o, on_confirm)
      table.insert(H.inputs_asked, { prompt = o.prompt, default = o.default })
      if os.getenv('H_VERBOSE') then
        out('  input: ' .. tostring(o.prompt) .. ' default=' .. tostring(o.default))
      end
      local a = table.remove(H.input_answers, 1)
      if a == false then
        a = nil
      end
      on_confirm(a)
    end
    vim.ui.select = function(items, o, on_choice)
      local fmt = o.format_item or tostring
      local labels = vim.tbl_map(fmt, items)
      table.insert(H.select_asked, { prompt = o.prompt, labels = labels })
      if os.getenv('H_VERBOSE') then
        out('  select: ' .. tostring(o.prompt) .. ' ' .. vim.inspect(labels))
      end
      local a = table.remove(H.select_answers, 1)
      if type(a) == 'number' then
        return on_choice(items[a], a)
      end
      if type(a) == 'string' then
        for i, l in ipairs(labels) do
          if l == a then
            return on_choice(items[i], i)
          end
        end
        for i, l in ipairs(labels) do
          if l:find(a) then
            return on_choice(items[i], i)
          end
        end
      end
      on_choice(nil, nil)
    end
  end
end

--- Run `fn` as a section; a Lua error in it is a failure, not the end.
function H.section(name, fn)
  H.current = name
  local before = #H.fails
  vim.v.errmsg = ''
  local ok, err = xpcall(fn, debug.traceback)
  if not ok then
    H.check(false, 'Lua error: ' .. tostring(err))
  end
  if vim.v.errmsg ~= '' then
    H.check(false, 'v:errmsg: ' .. vim.v.errmsg)
  end
  local bad = H.errors_in_messages()
  H.seen_bad = H.seen_bad or 0
  if #bad > H.seen_bad then
    H.check(false, 'new errors in :messages: ' .. table.concat(vim.list_slice(bad, H.seen_bad + 1), ' | '))
    H.seen_bad = #bad
  end
  -- Close stray floating windows and leave insert mode between sections.
  pcall(vim.cmd, 'stopinsert')
  out(('-- %s: %s'):format(name, #H.fails == before and 'ok' or ('%d failed'):format(#H.fails - before)))
end

--- The prompt window (calendar), if one shows.
function H.calendar_win()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == 'org-date' then
      return win
    end
  end
end

function H.agenda_buf()
  for _, b in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(b) and vim.bo[b].filetype == 'org-agenda' then
      return b
    end
  end
end

function H.errors_in_messages()
  local msgs = vim.api.nvim_exec2('messages', { output = true }).output
  local bad = {}
  for l in msgs:gmatch('[^\n]+') do
    if l:find('E%d+:') or l:find('stack traceback') or l:find('Error') then
      table.insert(bad, l)
    end
  end
  return bad
end

function H.done()
  H.current = 'end'
  local bad = H.errors_in_messages()
  H.check(#bad <= (H.seen_bad or 0), ':messages has errors: ' .. table.concat(bad, ' | '))
  local warn = {}
  for _, n in ipairs(H.notes) do
    if n[2] and n[2] >= vim.log.levels.WARN then
      table.insert(warn, n[1])
    end
  end
  out('notifications at WARN/ERROR: ' .. vim.inspect(warn))
  out(('RESULT: %d passed, %d failed'):format(H.passes, #H.fails))
  for _, f in ipairs(H.fails) do
    out('  - ' .. f)
  end
  vim.cmd(#H.fails == 0 and 'qall!' or 'cquit 1')
end

return H
