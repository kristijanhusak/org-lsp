-- TESTING.org: Agenda. Menu with custom commands, views and navigation,
-- tags at the edge, habits, TODO list / tag / word search, edits written
-- back (todo, priority, schedule, deadline, tags, note, refile, archive),
-- filters, bulk, follow, outline path, entry text / log / clock report /
-- archives, inactive, hover, rebuild/save, appt, win_split_mode.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('agenda')
local work = dir .. '/work.org'
vim.fn.writefile({
  '#+FILETAGS: :office:',
  '* Projects',
  '** TODO [#A] Write report :urgent:',
  'SCHEDULED: <2026-10-21 Wed 09:00>',
  ':PROPERTIES:',
  ':Effort: 1:00',
  ':END:',
  'Report text here.',
  '** TODO Call the client',
  'DEADLINE: <2026-10-23 Fri>',
  '** WAIT Waiting thing',
  '** DONE Old thing',
  'CLOSED: [2026-10-20 Tue 10:00]',
  ':LOGBOOK:',
  'CLOCK: [2026-10-20 Tue 09:00]--[2026-10-20 Tue 10:00] =>  1:00',
  ':END:',
  '* Meeting <2026-10-22 Thu 14:00-15:00>',
  '* Note with inactive [2026-10-21 Wed]',
  '* Archived :ARCHIVE:',
  '** TODO hidden archived <2026-10-21 Wed>',
}, work)
vim.fn.writefile({
  '* TODO Water plants',
  'SCHEDULED: <2026-10-19 Mon .+2d>',
  ':PROPERTIES:',
  ':STYLE: habit',
  ':LAST_REPEAT: [2026-10-17 Sat 10:00]',
  ':END:',
  ':LOGBOOK:',
  '- State "DONE"       from "TODO"       [2026-10-17 Sat 10:00]',
  '- State "DONE"       from "TODO"       [2026-10-15 Thu 10:00]',
  ':END:',
  '* TODO Buy milk :home:',
  '* Target heading',
}, dir .. '/home.org')
H.init({
  settings = {
    clock = { today = '2026-10-21', now = '10:30' },
    org_agenda_files = { dir },
    org_todo_keywords = { 'TODO(t)', 'WAIT(w)', '|', 'DONE(d)' },
    org_modules = { 'org-habit' },
    org_log_done = 'time',
    org_agenda_custom_commands = {
      {
        key = 'd',
        description = 'Today, then what is waiting',
        blocks = {
          { type = 'agenda', settings = { org_agenda_span = 'day' } },
          { type = 'todo', match = 'WAIT', header = 'Waiting for' },
        },
      },
    },
  },
  config = { win_split_mode = os.getenv('SPLIT') or 'split' },
})
local eq, check, press, line, lines, wait = H.eq, H.check, H.press, H.line, H.lines, H.wait

local function agenda(key)
  H.keys({ key })
  press('<Leader>oa')
  return wait('agenda ' .. key, function() return vim.bo.filetype == 'org-agenda' and #lines() > 1 end)
end
local function goto_line(pattern)
  for i, l in ipairs(lines()) do
    if l:find(pattern) then
      vim.api.nvim_win_set_cursor(0, { i, 0 })
      return i
    end
  end
  H.check(false, 'no agenda line matching ' .. pattern .. ' in ' .. vim.inspect(lines()))
end
local function file_lines(path)
  local b = vim.fn.bufnr(path)
  if b > 0 and vim.api.nvim_buf_is_loaded(b) then
    return H.lines(b)
  end
  return vim.fn.readfile(path)
end
local function rebuilt(fn)
  local before = table.concat(lines(), '\n')
  fn()
  return H.wait(nil, function() return table.concat(lines(), '\n') ~= before end, 4000)
end

H.section('menu, week view (' .. (os.getenv('SPLIT') or 'split') .. ')', function()
  vim.cmd('enew')
  H.keys({ 'a' })
  press('<Leader>oa')
  wait('agenda', function() return vim.bo.filetype == 'org-agenda' end)
  local menu = H.key_prompts[#H.key_prompts]
  H.out('  menu: ' .. vim.inspect(menu.choices))
  check(vim.tbl_contains(menu.choices, 'd Today, then what is waiting'), 'the custom command in the menu')
  H.out(table.concat(lines(), '\n'))
  local win = vim.api.nvim_get_current_win()
  local mode = os.getenv('SPLIT') or 'split'
  local cfg = vim.api.nvim_win_get_config(win)
  H.out('  window: relative=' .. tostring(cfg.relative) .. ' layout=' .. vim.inspect(vim.fn.winlayout()[1]) .. ' wins=' .. #vim.api.nvim_list_wins())
  if mode == 'float' then check(cfg.relative ~= '', 'float') end
  if mode == 'vsplit' then eq(vim.fn.winlayout()[1], 'row', 'vsplit') end
  if mode == 'split' then eq(vim.fn.winlayout()[1], 'col', 'split') end
  if mode == 'current' then eq(#vim.api.nvim_list_wins(), 1, 'current window') end
  check(vim.iter(lines()):any(function(l) return l:find('Scheduled:  TODO %[#A%] Write report') end), 'scheduled entry')
  check(vim.iter(lines()):any(function(l) return l:find('In   2 d%.:  TODO Call the client') end), 'deadline warning')
  check(vim.iter(lines()):any(function(l) return l:find('%-%-%-%-%-%-%-%-') end), 'time grid')
  check(not vim.iter(lines()):any(function(l) return l:find('hidden archived') end), 'archived subtree left out')
  -- tags at the right edge of the window
  local tagline = goto_line(':office:urgent:')
  if tagline then
    local l = line(tagline)
    local width = vim.api.nvim_win_get_width(0)
    H.out('  tag line width ' .. vim.fn.strdisplaywidth(l) .. ' window ' .. width)
    check(vim.fn.strdisplaywidth(l) <= width and vim.fn.strdisplaywidth(l) >= width - 3, 'tags end at the right edge')
    if mode ~= 'current' then
      vim.cmd('vertical resize 70')
      if mode == 'split' or mode == 'float' then vim.o.columns = 100 end
      H.wait(nil, function()
        local i = goto_line(':office:urgent:')
        return i and vim.fn.strdisplaywidth(line(i)) <= vim.api.nvim_win_get_width(0) and vim.fn.strdisplaywidth(line(i)) >= vim.api.nvim_win_get_width(0) - 3
      end, 4000)
      local i = goto_line(':office:urgent:')
      H.out('  after resize: width ' .. vim.fn.strdisplaywidth(line(i)) .. ' window ' .. vim.api.nvim_win_get_width(0))
      check(vim.fn.strdisplaywidth(line(i)) >= vim.api.nvim_win_get_width(0) - 3 and vim.fn.strdisplaywidth(line(i)) <= vim.api.nvim_win_get_width(0), 'tags follow the resize')
      vim.o.columns = 80
    end
  end
end)

H.section('habit graph', function()
  local i = goto_line('Water plants')
  if not i then return end
  H.out('  habit line: ' .. line(i))
  local marks = vim.api.nvim_buf_get_extmarks(0, -1, { i - 1, 0 }, { i - 1, -1 }, { details = true })
  local groups = {}
  for _, m in ipairs(marks) do
    if m[4].hl_group then groups[m[4].hl_group] = true end
  end
  local inspected = vim.inspect_pos(0, i - 1, #line(i) - 3)
  local g = {}
  for _, t in ipairs(inspected.semantic_tokens) do table.insert(g, t.opts.hl_group) end
  H.out('  habit groups near end: ' .. vim.inspect(g))
  check(line(i):find('[!*]') ~= nil or #g > 0, 'a consistency graph')
end)

H.section('navigation', function()
  goto_line('^Wednesday')
  local title = line(1)
  check(rebuilt(function() press('f') end), 'f later')
  H.out('  after f: ' .. line(1) .. ' / ' .. line(2))
  check(rebuilt(function() press('b') end), 'b earlier')
  press('vd')
  wait('day view', function() return line(1):find('Day%-agenda') end)
  press('f')
  wait('next day', function() return vim.iter(lines()):any(function(l) return l:find('^Thursday') end) end)
  press('.')
  wait('today', function() return vim.iter(lines()):any(function(l) return l:find('^Wednesday%s+21 October') end) end)
  press('vm')
  wait('month view', function() return line(1):find('Month%-agenda') end)
  press('vw')
  wait('week view', function() return line(1):find('Week%-agenda') end)
  H.inputs({ '2026-12-25' })
  press('J')
  wait('calendar for J', function() return H.calendar_win() ~= nil end)
  H.sleep(200)
  press('i')
  wait('J goes to the date', function() return vim.iter(lines()):any(function(l) return l:find('25 December 2026') end) end)
  press('.')
  wait('back to today', function() return vim.iter(lines()):any(function(l) return l:find('21 October 2026') end) end)
end)

H.section('todo list, tag search, word search, custom', function()
  press('q')
  agenda('t')
  H.out(table.concat(lines(), '\n'))
  check(vim.iter(lines()):any(function(l) return l:find('TODO Buy milk') end), 'TODO list')
  press('q')
  H.inputs({ 'office+urgent' })
  agenda('m')
  wait('tag match', function() return vim.iter(lines()):any(function(l) return l:find('Write report') end) end)
  check(not vim.iter(lines()):any(function(l) return l:find('Call the client') end), 'only matching')
  H.out(table.concat(lines(), '\n'))
  press('q')
  H.inputs({ 'Report text' })
  agenda('s')
  wait('word search', function() return vim.iter(lines()):any(function(l) return l:find('Write report') end) end)
  press('q')
  agenda('d')
  H.out(table.concat(lines(), '\n'))
  check(vim.iter(lines()):any(function(l) return l:find('Waiting for') end), 'custom block header')
  check(vim.iter(lines()):any(function(l) return l:find('WAIT Waiting thing') end), 'custom WAIT block')
  press('q')
end)

H.section('edits from the agenda', function()
  agenda('a')
  goto_line('Call the client')
  H.keys({ 'w' })
  press('t')
  wait('todo changed in the line', function() return vim.iter(lines()):any(function(l) return l:find('WAIT Call the client') end) end)
  check(vim.tbl_contains(file_lines(work), '** WAIT Call the client'), 'written to the file')
  goto_line('Call the client')
  press('+')
  wait('priority', function() return vim.tbl_contains(file_lines(work), '** WAIT [#B] Call the client') end)
  press('<S-Up>')
  wait('priority up', function() return vim.tbl_contains(file_lines(work), '** WAIT [#A] Call the client') end)
  H.out('  line: ' .. vim.api.nvim_get_current_line())
  -- schedule with C-c C-s
  goto_line('Call the client')
  H.inputs({ '2026-10-22' })
  press('<C-c><C-s>')
  wait('calendar', function() return H.calendar_win() ~= nil end)
  H.sleep(200)
  press('i')
  wait('scheduled in file', function() return vim.iter(file_lines(work)):any(function(l) return l:find('SCHEDULED: <2026%-10%-22 Thu>') end) end)
  H.out('  after schedule: ' .. vim.inspect(file_lines(work)))
  wait('back in the agenda', function() return vim.bo.filetype == 'org-agenda' end)
  goto_line('Call the client')
  H.inputs({ '2026-10-24' })
  press('<C-c><C-d>')
  wait('calendar', function() return H.calendar_win() ~= nil end)
  H.sleep(200)
  press('i')
  wait('deadline in file', function() return vim.iter(file_lines(work)):any(function(l) return l:find('DEADLINE: <2026%-10%-24 Sat>') end) end)
  wait('back in the agenda', function() return vim.bo.filetype == 'org-agenda' end)
  -- tags
  goto_line('Call the client')
  H.inputs({ 'phone' })
  press('<C-c><C-q>')
  wait('tags in file', function() return vim.iter(file_lines(work)):any(function(l) return l:find('Call the client.*:phone:$') end) end)
  -- note
  goto_line('Call the client')
  press('z')
  wait('note buffer', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)) == 'CAPTURE-Note.org' end)
  H.out('  note: ' .. vim.inspect(lines()))
  vim.api.nvim_buf_set_lines(0, -2, -1, false, { 'agenda note' })
  press('<C-c>')
  wait('note in file', function() return vim.iter(file_lines(work)):any(function(l) return l:find('agenda note') end) end)
  wait('back in the agenda', function() return vim.bo.filetype == 'org-agenda' end)
  H.out('  work.org: ' .. vim.inspect(file_lines(work)))
  -- refile
  goto_line('Call the client')
  H.selects({ 'Target heading' })
  press('<C-c><C-w>')
  wait('refiled to home.org', function() return vim.iter(file_lines(dir .. '/home.org')):any(function(l) return l:find('Call the client') end) end)
  H.out('  refile asked: ' .. vim.inspect(H.select_asked[#H.select_asked]))
  -- archive
  wait('agenda', function() return vim.bo.filetype == 'org-agenda' end)
  H.sleep(300)
  local n = #H.notes
  if goto_line('Buy milk') then
    H.keys({ 'y' })
    H.inputs({ 'y' })
    press('<C-c><C-x><C-a>')
    wait('archived', function() return vim.fn.filereadable(dir .. '/home.org_archive') == 1 or vim.iter(file_lines(dir .. '/home.org')):any(function(l) return l:find('Buy milk.*ARCHIVE') end) end)
    H.out('  archive notes: ' .. vim.inspect(H.notes_since(n)))
  end
end)

H.section('filters, bulk, follow, outline path', function()
  if vim.bo.filetype ~= 'org-agenda' then agenda('a') end
  press('r')
  H.sleep(500)
  local all = #lines()
  H.keys({ 'u', 'r', 'g', '\r' })
  H.inputs({ 'urgent' })
  press('\\')
  H.sleep(800)
  H.out('  after \\ filter: ' .. vim.inspect(lines()) .. ' prompts ' .. vim.inspect(H.key_prompts[#H.key_prompts]))
  check(#lines() < all, 'tag filter hides lines')
  press('|')
  wait('filters removed', function() return #lines() == all end)
  goto_line('Water plants')
  press('<')
  H.sleep(600)
  check(not vim.iter(lines()):any(function(l) return l:find('Write report') end), 'category filter keeps home only')
  press('|')
  wait('removed', function() return #lines() == all end)
  H.keys({ '<', '\r' })
  H.inputs({ '<0:30', '0:30' })
  press('_')
  H.sleep(800)
  H.out('  after _ effort: ' .. #lines() .. ' prompts ' .. vim.inspect(H.key_prompts[#H.key_prompts]) .. ' ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]))
  press('|')
  wait('removed', function() return #lines() == all end)
  -- bulk
  goto_line('Write report')
  press('m')
  H.sleep(200)
  goto_line('Water plants')
  press('m')
  H.sleep(300)
  H.keys({ 't', 'd' })
  press('B')
  H.sleep(1500)
  H.out('  bulk prompts: ' .. vim.inspect(vim.list_slice(H.key_prompts, #H.key_prompts - 1)))
  check(vim.iter(file_lines(work)):any(function(l) return l:find('DONE %[#A%] Write report') end), 'bulk todo done on Write report')
  -- follow
  press('F')
  H.sleep(300)
  goto_line('Meeting')
  H.press('j')
  H.press('k')
  H.sleep(800)
  local shown = {}
  for _, w in ipairs(vim.api.nvim_list_wins()) do shown[#shown + 1] = vim.fs.basename(vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(w))) end
  H.out('  follow windows: ' .. vim.inspect(shown))
  check(vim.tbl_contains(shown, 'work.org'), 'follow shows the entry')
  eq(vim.bo.filetype, 'org-agenda', 'cursor stays in the agenda')
  press('F')
  -- outline path echo
  local echoed
  local echo = vim.api.nvim_echo
  vim.api.nvim_echo = function(chunks, ...)
    echoed = chunks[1] and chunks[1][1]
    return echo(chunks, ...)
  end
  goto_line('Meeting')
  H.press('j')
  H.press('k')
  H.sleep(600)
  vim.api.nvim_echo = echo
  H.out('  echoed: ' .. tostring(echoed))
  check(echoed and echoed:find('work.org') ~= nil, 'outline path in the echo area')
end)

H.section('entry text, log, clock report, archives, inactive', function()
  if vim.bo.filetype ~= 'org-agenda' then agenda('a') end
  press('E')
  H.sleep(600)
  check(vim.iter(lines()):any(function(l) return l:find('Report text here') end), 'E shows entry text')
  press('E')
  H.sleep(300)
  press('b')
  H.sleep(400)
  press('vl')
  H.sleep(800)
  H.out(table.concat(lines(), '\n'))
  check(vim.iter(lines()):any(function(l) return l:find('Closed:') or l:find('Clocked:') end), 'log mode shows closed/clocked')
  press('vl')
  press('R')
  H.sleep(800)
  check(vim.iter(lines()):any(function(l) return l:find('Clock summary') or l:find('#%+BEGIN') or l:find('Table') end), 'clock report')
  H.out('  report: ' .. vim.inspect(vim.list_slice(lines(), #lines() - 8)))
  press('R')
  press('.')
  H.sleep(300)
  press('va')
  H.sleep(800)
  check(vim.iter(lines()):any(function(l) return l:find('hidden archived') end), 'archives mode shows archived trees')
  press('va')
  H.sleep(300)
  local n = #H.notes
  press('[')
  H.sleep(800)
  H.out('  [ notes: ' .. vim.inspect(H.notes_since(n)))
  check(H.note_seen('Display now includes inactive timestamps as well', n), 'Display now includes inactive timestamps as well')
  check(vim.iter(lines()):any(function(l) return l:find('Note with inactive') end), 'inactive timestamp shown')
  press('r')
  H.sleep(800)
  check(not vim.iter(lines()):any(function(l) return l:find('Note with inactive') end), 'gone after r')
end)

H.section('hover, save, rebuild after file change', function()
  if vim.bo.filetype ~= 'org-agenda' then agenda('a') end
  goto_line('Meeting')
  vim.lsp.buf.hover()
  H.sleep(1000)
  local float
  for _, w in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(w).relative ~= '' then float = w end
  end
  check(float ~= nil, 'K shows a float')
  if float then
    local b = vim.api.nvim_win_get_buf(float)
    H.out('  hover: ' .. vim.inspect(H.lines(b)) .. ' ft=' .. vim.bo[b].filetype .. ' spell=' .. tostring(vim.wo[float].spell))
    eq(vim.bo[b].filetype, 'org', 'hover float is Org')
    eq(vim.wo[float].spell, false, 'no spell in the hover')
    vim.api.nvim_win_close(float, true)
  end
  -- change on disk, then r
  local b = vim.fn.bufnr(work)
  if b > 0 then vim.cmd('silent! bwipeout! ' .. b) end
  local wl = vim.fn.readfile(work)
  table.insert(wl, '* TODO Fresh from disk')
  table.insert(wl, 'SCHEDULED: <2026-10-21 Wed>')
  vim.fn.writefile(wl, work)
  press('r')
  wait('changed file shows after r', function() return vim.iter(lines()):any(function(l) return l:find('Fresh from disk') end) end)
  -- s saves
  goto_line('Fresh from disk')
  H.keys({ 'd' })
  press('t')
  H.sleep(800)
  local wb = vim.fn.bufnr(work)
  H.out('  work.org modified: ' .. tostring(wb > 0 and vim.bo[wb].modified))
  press('s')
  H.sleep(600)
  check(vim.iter(vim.fn.readfile(work)):any(function(l) return l:find('DONE Fresh from disk') end), 's saved the file')
end)

H.section('appt (Emacs)', function()
  if vim.bo.filetype ~= 'org-agenda' then agenda('a') end
  local n = #H.notes
  vim.cmd('Org agenda appt')
  H.wait(nil, function() return #H.notes > n end, 20000)
  H.out('  appt: ' .. vim.inspect(H.notes_since(n)))
  check(H.note_seen('appointment', n) or H.note_seen('No event', n) or H.note_seen('Added', n), 'appt says what it added')
end)

H.done()
