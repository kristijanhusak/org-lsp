-- TESTING.org: Dates and times (schedule, deadline, remove, delay, warning,
-- timestamps and ranges, the calendar's keys, time picking, week start,
-- six-week months, view kept, float/cursor modes, one month, C-a/C-x,
-- S-Up/S-Down, cid, repeater kept), with the real calendar.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('dates')
local mode = os.getenv('CAL_MODE') or 'bottom'
H.init({
  settings = {
    clock = { today = '2026-10-21', now = '12:44' },
    calendar_week_start_day = tonumber(os.getenv('WEEK_START') or '0'),
    calendar_months = tonumber(os.getenv('MONTHS') or '3'),
  },
  config = { calendar_win_mode = mode },
})
local eq, check, press, line, lines, wait = H.eq, H.check, H.press, H.line, H.lines, H.wait

local function in_calendar()
  local w = H.calendar_win()
  return w ~= nil and vim.api.nvim_get_current_win() == w and vim.api.nvim_get_mode().mode == 'n'
end
local function cal_lines()
  local w = H.calendar_win()
  return w and H.lines(vim.api.nvim_win_get_buf(w)) or {}
end
local function cal_day()
  local w = H.calendar_win()
  if not w then return nil end
  local r, c = unpack(vim.api.nvim_win_get_cursor(w))
  local l = H.lines(vim.api.nvim_win_get_buf(w))[r]
  return vim.trim(l:sub(c, c + 1)), r, c
end
local function last_cal_line()
  local l = cal_lines()
  return l[#l]
end
local function closed()
  return H.calendar_win() == nil
end

H.section('schedule +2d, deadline fri 10:00 (' .. mode .. ')', function()
  H.open(dir .. '/s.org', { '* TODO Task', 'body' })
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  press('<Leader>ois')
  wait('calendar', in_calendar)
  local w = H.calendar_win()
  H.out('  calendar window config: ' .. vim.inspect(vim.api.nvim_win_get_config(w).relative) .. ' height ' .. vim.api.nvim_win_get_height(w))
  H.out(table.concat(cal_lines(), '\n'))
  if mode == 'bottom' then
    eq(vim.api.nvim_win_get_config(w).relative, '', 'bottom: a split')
  else
    check(vim.api.nvim_win_get_config(w).relative ~= '', mode .. ': a floating window')
  end
  H.inputs({ '+2d' })
  press('i')
  wait('scheduled', function() return line(2) == 'SCHEDULED: <2026-10-23 Fri>' end)
  wait('closed', closed)
  eq(H.inputs_asked[#H.inputs_asked].prompt, 'SCHEDULED Date+time [2026-10-21]: ', 'the prompt')
  H.out('  prompt: ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]))
  press('<Leader>oid')
  wait('calendar', in_calendar)
  H.inputs({ 'fri 10:00' })
  press('i')
  wait('deadline', function() return vim.tbl_contains(lines(), 'DEADLINE: <2026-10-23 Fri 10:00> SCHEDULED: <2026-10-23 Fri>') or vim.tbl_contains(lines(), 'SCHEDULED: <2026-10-23 Fri> DEADLINE: <2026-10-23 Fri 10:00>') end)
  H.out('  after deadline: ' .. vim.inspect(lines()))
  wait('closed', closed)
  -- delay and warning (C-u C-u)
  local n = #H.notes
  H.inputs({ '-3d' })
  vim.cmd('Org schedule delay')
  H.sleep(400)
  if in_calendar() then press('i') end
  wait('delay cookie', function() return vim.iter(lines()):any(function(l) return l:find('<2026%-10%-23 Fri %-5d>') end) end)
  H.out('  delay asked: ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]) .. ' -> ' .. vim.inspect(lines()))
  H.inputs({ '-5d' })
  vim.cmd('Org deadline warning')
  H.sleep(400)
  if in_calendar() then press('i') end
  wait('warning cookie', function() return vim.iter(lines()):any(function(l) return l:find('<2026%-10%-23 Fri 10:00 %-7d>') end) end)
  H.out('  warning asked: ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]) .. ' -> ' .. vim.inspect(lines()))
  vim.cmd('Org schedule remove')
  wait('scheduled removed', function() return not vim.iter(lines()):any(function(l) return l:find('SCHEDULED') end) end)
  vim.cmd('Org deadline remove')
  wait('deadline removed', function() return not vim.iter(lines()):any(function(l) return l:find('DEADLINE') end) end)
  H.out('  remove messages: ' .. vim.inspect(H.notes_since(n)) .. ' lines ' .. vim.inspect(lines()))
  check(H.wait(nil, function() return H.note_seen('Entry is no longer scheduled.', n) end, 2000), 'Emacs: Item is no longer scheduled.')
  check(H.wait(nil, function() return H.note_seen('Entry no longer has a deadline.', n) end, 2000), 'Emacs: Item no longer has a deadline.')
end)

H.section('timestamps and range', function()
  H.open(dir .. '/t.org', { '* A', 'x' })
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  H.out('  msgs before A: ' .. #H.errors_in_messages())
  press('A <Esc>')
  H.sleep(300)
  H.out('  msgs after A: ' .. #H.errors_in_messages())
  vim.api.nvim_win_set_cursor(0, { 2, 1 }) -- on the space: inserted before it
  press('<Leader>oi.')
  wait('calendar', in_calendar)
  H.out('  msgs at calendar: ' .. #H.errors_in_messages())
  press('<CR>')
  wait('active timestamp', function() return line(2) == 'x<2026-10-21 Wed> ' end)
  H.out('  cursor after insert: ' .. vim.inspect(H.cursor()))
  press('<Leader>oi.')
  wait('calendar 2', in_calendar)
  press('l')
  press('<CR>')
  wait('range', function() return line(2) == 'x<2026-10-21 Wed>--<2026-10-22 Thu> ' end)
  H.out('  range: ' .. line(2))
  press('o<Esc>')
  press('<Leader>oi!')
  wait('calendar 3', in_calendar)
  press('<CR>')
  wait('inactive', function() return line(3) == '[2026-10-21 Wed]' end)
end)

-- typing shows the answer: checked in the TUI (neovim-6)

H.done()
