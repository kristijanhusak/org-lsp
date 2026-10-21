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
  press('A <Esc>')
  vim.api.nvim_win_set_cursor(0, { 2, 1 }) -- on the space: inserted before it
  press('<Leader>oi.')
  wait('calendar', in_calendar)
  press('<CR>')
  wait('active timestamp', function() return line(2) == 'x<2026-10-21 Wed> ' end)
  H.out('  cursor after insert: ' .. vim.inspect(H.cursor()))
  press('<Leader>oi.')
  wait('calendar 2', in_calendar)
  press('l')
  press('<CR>')
  wait('range', function() return line(2) == 'x<2026-10-21 Wed>--<2026-10-22 Thu> ' end)
  H.out('  range: ' .. line(2))
  H.sleep(500) -- neovim-8
  press('o<Esc>')
  press('<Leader>oi!')
  wait('calendar 3', in_calendar)
  press('<CR>')
  wait('inactive', function() return line(3) == '[2026-10-21 Wed]' end)
end)

-- typing shows the answer: checked in the TUI (neovim-6)

H.section('calendar keys', function()
  H.open(dir .. '/c.org', { '* A', 'SCHEDULED: <2026-10-21 Wed>' })
  vim.api.nvim_win_set_cursor(0, { 2, 14 })
  press('<Leader>ois')
  wait('calendar on 21', function() return in_calendar() and cal_day() == '21' end)
  local function step(key, day, msg)
    press(key)
    H.wait(msg .. ' (' .. key .. ' -> ' .. day .. ')', function() return cal_day() == day end, 3000)
  end
  step('l', '22', 'day later')
  step('<S-Right>', '23', 'day later shift')
  step('h', '22', 'day earlier')
  step('<S-Left>', '21', 'day earlier shift')
  step('j', '28', 'week later')
  step('<S-Down>', '4', 'week later shift (Nov 4)')
  step('k', '28', 'week earlier')
  step('<S-Up>', '21', 'week earlier shift')
  step('<M-S-Right>', '21', 'month later (Nov 21)')
  check(H.wait(nil, function() return last_cal_line():find('2026%-11%-21') end, 3000), 'month later is 2026-11-21: ' .. last_cal_line())
  step('<M-S-Left>', '21', 'month earlier')
  step('<M-S-Down>', '21', 'year later')
  check(H.wait(nil, function() return last_cal_line():find('2027%-10%-21') end, 3000), 'year later 2027-10-21: ' .. last_cal_line())
  step('<M-S-Up>', '21', 'year earlier')
  H.wait('text back in 2026', function() return last_cal_line():find('2026%-10%-21') end, 3000)
  local first = cal_lines()[1]
  H.out('  before >: first=' .. first .. ' cur=' .. vim.api.nvim_buf_get_name(0) .. ' last=' .. last_cal_line())
  press('>')
  H.wait('scroll later', function() return cal_lines()[1] ~= first end, 3000)
  H.out('  after >: ' .. cal_lines()[1])
  press('<')
  H.sleep(500)
  H.out('  after <: ' .. vim.inspect(cal_lines()[1]) .. ' cur=' .. vim.api.nvim_buf_get_name(0) .. ' day=' .. tostring(cal_day()))
  H.wait('scroll earlier', function() return cal_lines()[1] == first end, 3000)
  -- counts: neovim-7
  step('.', '21', 'today')
  step('l', '22', 'day later')
  press('<CR>')
  H.sleep(300)
  H.out('  after pick: buf=' .. vim.api.nvim_buf_get_name(0) .. ' ' .. vim.inspect(lines()))
  wait('picked', function() return line(2) == 'SCHEDULED: <2026-10-22 Thu>' end)
  wait('closed', closed)
  eq(H.cursor(), { 2, 14 }, 'the cursor on the same text after the date goes in')
  -- q and <Esc> cancel
  press('<Leader>ois')
  wait('calendar', in_calendar)
  press('q')
  wait('q closes', closed)
  press('<Leader>ois')
  wait('calendar', in_calendar)
  press('<Esc>')
  wait('Esc closes', closed)
  eq(line(2), 'SCHEDULED: <2026-10-22 Thu>', 'cancel changes nothing')
end)

H.section('time picking 12:44', function()
  H.open(dir .. '/tm.org', { '* A', 'SCHEDULED: <2026-10-21 Wed 12:44>' })
  press('<Leader>ois')
  wait('calendar', in_calendar)
  press('t')
  H.sleep(300)
  H.out(table.concat(cal_lines(), '\n'))
  press('l') -- to minutes
  H.sleep(200)
  press('k')
  H.wait('12:45', function() return last_cal_line():find('12:45') end, 3000)
  press('k')
  H.wait('12:50', function() return last_cal_line():find('12:50') end, 3000)
  press('q')
  H.wait('closed', closed)
  press('<Leader>ois')
  wait('calendar', in_calendar)
  press('t')
  H.sleep(300)
  press('l')
  H.sleep(200)
  press('j')
  H.wait('12:40 down from 12:44', function() return last_cal_line():find('12:40') end, 3000)
  press('t')
  H.sleep(200)
  press('<CR>')
  wait('scheduled with the time', function() return line(2) and line(2):find('12:40') end)
  eq(line(2), 'SCHEDULED: <2026-10-21 Wed 12:40>', 'picked time')
  H.out('  scheduled: ' .. tostring(line(2)))
end)

H.section('week start / six weeks / months', function()
  H.open(dir .. '/w.org', { '* A', 'SCHEDULED: <2026-08-03 Mon>' })
  vim.api.nvim_win_set_cursor(0, { 2, 14 })
  press('<Leader>ois')
  wait('calendar', in_calendar)
  local l = cal_lines()
  H.out(table.concat(l, '\n'))
  local w = H.calendar_win()
  eq(vim.api.nvim_win_get_height(w) >= #l, true, 'the whole calendar in view (height ' .. vim.api.nvim_win_get_height(w) .. ', lines ' .. #l .. ')')
  local ws = tonumber(os.getenv('WEEK_START') or '0')
  if ws == 1 then
    check(l[2]:match('^%s*Mo') ~= nil, 'weeks start on Monday: ' .. l[2])
  else
    check(l[2]:match('^%s*Su') ~= nil, 'weeks start on Sunday: ' .. l[2])
  end
  local months = tonumber(os.getenv('MONTHS') or '3')
  if months == 1 then
    check(not l[1]:find('September'), 'one month shown: ' .. l[1])
  end
  -- to a month of six weeks (August 2026 starts on a Saturday: six weeks when weeks start on Sunday;
  -- with Monday start, Aug 2026 also has 6 rows)
  eq(cal_day(), '3', 'on the day')
  press('q')
  wait('closed', closed)
end)

H.section('goto calendar', function()
  H.open(dir .. '/g.org', { '* A', 'SCHEDULED: <2026-12-25 Fri>' })
  vim.api.nvim_win_set_cursor(0, { 2, 14 })
  press('<Leader>o>')
  wait('calendar', function() return H.calendar_win() ~= nil end)
  H.sleep(300)
  local l = cal_lines()
  H.out(table.concat(l, '\n'))
  check(not l[#l]:find('Date'), 'no prompt under it: ' .. l[#l])
  check(vim.iter(l):any(function(x) return x:find('December 2026') end), 'on December 2026')
  if vim.api.nvim_get_current_win() == H.calendar_win() then
    eq(cal_day(), '25', 'on its day')
  end
  if H.calendar_win() then vim.api.nvim_set_current_win(H.calendar_win()) end
  press('q')
  wait('closed', closed)
end)

H.section('view kept with scrolloff', function()
  local L = { '* A' }
  for i = 1, 200 do L[#L + 1] = 'line ' .. i end
  L[#L + 1] = '* B'
  H.open(dir .. '/v.org', L)
  vim.o.scrolloff = 5
  vim.api.nvim_win_set_cursor(0, { 150, 3 })
  vim.cmd('normal! zt')
  local top = vim.fn.line('w0')
  local win = vim.api.nvim_get_current_win()
  press('<Leader>oi.')
  wait('calendar', in_calendar)
  press('<CR>')
  wait('inserted', function() return line(150):find('<2026') end)
  wait('closed', closed)
  H.sleep(200)
  H.out('  view: top before ' .. top .. ' after ' .. vim.fn.line('w0') .. ' cursor ' .. vim.inspect(H.cursor()))
  eq({ vim.api.nvim_get_current_win(), vim.fn.line('w0'), H.cursor()[1] }, { win, top, 150 }, 'same window, same lines in view, cursor on the line')
  vim.o.scrolloff = 0
end)

H.section('C-a C-x parts, S-Up/Down, cid, repeater', function()
  H.open(dir .. '/x.org', { '* A', 'SCHEDULED: <2026-10-21 Wed 10:00 +1w -2d>' })
  local function at(col, key, expect, msg)
    vim.api.nvim_win_set_cursor(0, { 2, col })
    press(key)
    H.wait(nil, function() return line(2) == expect end, 3000)
    eq(line(2), expect, msg)
  end
  at(13, '<C-a>', 'SCHEDULED: <2027-10-21 Thu 10:00 +1w -2d>', 'year up')
  at(18, '<C-x>', 'SCHEDULED: <2027-09-21 Tue 10:00 +1w -2d>', 'month down')
  at(21, '3<C-a>', 'SCHEDULED: <2027-09-24 Fri 10:00 +1w -2d>', 'day +3')
  at(28, '<C-a>', 'SCHEDULED: <2027-09-24 Fri 11:00 +1w -2d>', 'hour up')
  at(31, '<C-x>', 'SCHEDULED: <2027-09-24 Fri 10:55 +1w -2d>', 'minute down by 5')
  at(13, '<S-Up>', 'SCHEDULED: <2027-09-25 Sat 10:55 +1w -2d>', 'S-Up a day later')
  at(13, '<S-Down>', 'SCHEDULED: <2027-09-24 Fri 10:55 +1w -2d>', 'S-Down a day earlier')
  vim.api.nvim_win_set_cursor(0, { 2, 21 })
  press('cid')
  wait('calendar', in_calendar)
  H.inputs({ '2026-11-02 10:55' })
  press('i')
  wait('changed with cid, repeater and warning kept', function() return line(2) == 'SCHEDULED: <2026-11-02 Mon 10:55 +1w -2d>' end)
  H.out('  cid -> ' .. line(2) .. ' cursor ' .. vim.inspect(H.cursor()))
end)

H.done()
