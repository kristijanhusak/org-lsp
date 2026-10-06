-- End-to-end test of the agenda in Neovim against the real server.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/agenda.lua
--
-- Exits non-zero on the first failed check.

local root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(vim.fs.abspath(debug.getinfo(1, 'S').source:sub(2)))))
local server = os.getenv('ORG_LSP_BIN') or vim.fs.joinpath(root, 'target', 'debug', 'org-lsp')

local checks = 0
local function check(condition, message)
  checks = checks + 1
  if not condition then
    io.stderr:write(('FAIL: %s\n'):format(message))
    vim.cmd('cquit 1')
  end
end

local function eq(actual, expected, message)
  check(vim.deep_equal(actual, expected), ('%s\n  expected: %s\n  actual:   %s'):format(message, vim.inspect(expected), vim.inspect(actual)))
end

--- Wait until `predicate` holds; fail the test if it never does.
local function wait(message, predicate)
  check(vim.wait(5000, predicate, 10), 'timed out waiting for ' .. message)
end

--- Type keys in normal mode, with mappings applied.
local function press(keys)
  vim.api.nvim_feedkeys(vim.keycode(keys), 'mx', false)
end

local function lines()
  return vim.api.nvim_buf_get_lines(0, 0, -1, false)
end

local function line(number)
  return lines()[number]
end

local function cursor_line()
  return line(vim.api.nvim_win_get_cursor(0)[1])
end

--- Wait until the current buffer is the agenda and its first line is `title`.
local function wait_for_view(title)
  wait('the agenda to show `' .. title .. '`', function()
    return vim.bo.filetype == 'org-agenda' and line(1) == title
  end)
end

-- Two agenda files in a directory of their own, and a fixed clock:
-- Monday 18 May 2026, 10:30.
local directory = vim.fn.tempname()
vim.fn.mkdir(directory, 'p')
local work = vim.fs.joinpath(directory, 'work.org')
vim.fn.writefile({
  '* TODO [#A] Write report :office:',
  'SCHEDULED: <2026-05-18 Mon 09:00>',
  '* TODO Call the client',
  'DEADLINE: <2026-05-20 Wed>',
  '* Meeting <2026-05-19 Tue 14:00-15:00>',
}, work)
vim.fn.writefile({ '* TODO Water plants', 'SCHEDULED: <2026-05-16 Sat +1w>', '* DONE Buy milk' }, vim.fs.joinpath(directory, 'home.org'))

vim.g.org_lsp = {
  cmd = { server },
  settings = {
    agendaFiles = { directory },
    clock = { today = '2026-05-18', now = '10:30' },
    captureTemplates = {
      { key = 't', description = 'Task', target = { 'file', vim.fs.joinpath(directory, 'inbox.org') }, template = '* TODO %?\n%U' },
    },
  },
  -- A key of the user's own, for a command the server binds no key to.
  mappings = { global = { ['org.agenda.todos'] = '<F7>' } },
}
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')

-- The dispatcher asks which view to show, by key, as Emacs's menu does.
local choice, menu
require('org-lsp').read_key = function(prompt, choices)
  menu = { prompt = prompt, keys = vim.tbl_map(function(item)
    return item.key .. ' ' .. item.label
  end, choices or {}) }
  return choice
end

-- 1. From a buffer that is not an Org file, with no server running yet.
eq(#vim.lsp.get_clients(), 0, 'the server does not run before it is needed')
eq(vim.fn.maparg('\\oa', 'n', false, true).buffer, 0, 'the agenda key is global')
choice = 'a'
press('\\oa')
wait_for_view('Week-agenda (W21):')
eq(menu, {
  prompt = 'Press key for agenda command',
  keys = {
    'a Agenda for the current week or day',
    't List of all TODO entries',
    'm Match a TAGS/PROP/TODO query',
    'M Like m, but only TODO entries',
  },
}, 'the menu of views')
eq(vim.api.nvim_buf_get_name(0), 'org-agenda:/Org%20Agenda', 'the agenda is a document of the server')
eq(vim.fn.winlayout()[1], 'col', 'the agenda opens in a horizontal split')
eq(lines(), {
  'Week-agenda (W21):',
  'Monday     18 May 2026 W21',
  '               8:00...... ----------------',
  '  work:        9:00...... Scheduled:  TODO [#A] Write report            :office:',
  '              10:00...... ----------------',
  '              10:30...... now - - - - - - - - - - - - - - - - - - - - - - - - -',
  '              12:00...... ----------------',
  '              14:00...... ----------------',
  '              16:00...... ----------------',
  '              18:00...... ----------------',
  '              20:00...... ----------------',
  '  home:       Sched. 2x:  TODO Water plants',
  '  work:       In   2 d.:  TODO Call the client',
  'Tuesday    19 May 2026',
  '  work:       14:00-15:00 Meeting ',
  'Wednesday  20 May 2026',
  '  work:       Deadline:   TODO Call the client',
  'Thursday   21 May 2026',
  'Friday     22 May 2026',
  'Saturday   23 May 2026',
  '  home:       Scheduled:  TODO Water plants',
  'Sunday     24 May 2026',
}, 'the week agenda')
eq(cursor_line(), 'Monday     18 May 2026 W21', 'the cursor is on today')
eq({ vim.bo.modifiable, vim.bo.buftype, vim.bo.modified }, { false, 'nofile', false }, 'the agenda cannot be edited')

-- 2. The server announced the keys the plugin sets before the server runs.
local org_lsp = require('org-lsp')
local announced = {}
for _, command in ipairs(org_lsp.manifest.commands) do
  for _, key in ipairs(command.keys) do
    if key.preset == 'vim' and key['in'] == 'global' then
      announced[command.command] = key.key
    end
  end
end
eq(
  announced,
  { ['org.agenda.dispatch'] = '<Leader>oa', ['org.capture'] = '<Leader>oc', ['org.clock.goto'] = '<Leader>oxj' },
  'the built-in global keys are the ones the server names'
)
eq(vim.fn.maparg('f', 'n', false, true).desc, 'Agenda: go forward in time', 'agenda keys carry the server\'s titles')
eq(vim.fn.maparg('f', 'n', false, true).buffer, 1, 'agenda keys are local to the agenda')
eq(vim.fn.maparg('f', 'n', false, true).nowait, 1, 'agenda keys run at once')
eq(vim.fn.maparg('vd', 'n', false, true).nowait, 1, 'also two-key ones')

-- 3. Highlighting comes from the server here too.
local agenda_buf = vim.api.nvim_get_current_buf()
local function token_at(row, col)
  local tokens = vim.lsp.semantic_tokens.get_at_pos(agenda_buf, row, col)
  return tokens and tokens[1]
end
wait('semantic tokens in the agenda', function()
  return token_at(0, 0) ~= nil
end)
eq(token_at(0, 0).type, 'agendaStructure', 'the title line')
eq({ token_at(1, 0).type, token_at(1, 0).modifiers.today }, { 'agendaDate', true }, 'today\'s date')
--- The token at the start of `text` in line `row`.
local function token_of(row, text)
  return token_at(row, assert(line(row + 1):find(text, 1, true)) - 1)
end
eq(token_of(3, '[#A]').type, 'priority', 'a priority cookie inside an entry')
eq(token_of(3, 'TODO').type, 'todoKeyword', 'a keyword inside an entry')
eq(token_of(3, ':office:').type, 'tag', 'tags of an entry')
eq({ token_of(3, 'Write').type, token_of(3, 'Write').modifiers.today }, { 'agendaScheduled', true }, 'scheduled today')
eq(token_of(11, 'Water').modifiers.past, true, 'scheduled in the past')
eq({ token_of(12, 'Call').type, token_of(12, 'Call').modifiers.imminent }, { 'agendaDeadline', nil }, 'an upcoming deadline')
eq(token_of(5, 'now').modifiers.current, true, 'the current time')
check(token_of(14, 'Meeting') == nil or token_of(14, 'Meeting').type == 'agendaEvent', 'a plain event')
eq(vim.api.nvim_get_hl(0, { name = '@lsp.type.agendaStructure.org-agenda' }).link, 'Title', 'agenda tokens are linked')
eq(vim.api.nvim_get_hl(0, { name = '@lsp.typemod.agendaDate.today.org-agenda' }).link, '@markup.strong', 'today is linked')
eq(vim.api.nvim_get_hl(0, { name = '@lsp.type.todoKeyword.org-agenda' }).link, 'DiagnosticError', 'Org tokens are linked in the agenda')

-- 4. Moving through time and changing the span.
-- The cursor is placed by the server, right after the new text arrives.
local function wait_for_cursor(text)
  wait('the cursor on `' .. text .. '`', function()
    return cursor_line() == text
  end)
end
press('f')
wait_for_view('Week-agenda (W22):')
wait_for_cursor('Monday     25 May 2026 W22')
press('b')
press('b')
wait_for_view('Week-agenda (W20):')
press('.')
wait_for_view('Week-agenda (W21):')
wait_for_cursor('Monday     18 May 2026 W21')
-- Asked for today again from elsewhere in the same view, only the cursor moves.
vim.api.nvim_win_set_cursor(0, { 20, 0 })
press('.')
wait_for_cursor('Monday     18 May 2026 W21')
-- A day view of Wednesday has three lines.
vim.api.nvim_win_set_cursor(0, { 17, 0 })
press('vd')
wait_for_view('Day-agenda (W21):')
eq(lines(), { 'Day-agenda (W21):', 'Wednesday  20 May 2026', '  work:       Deadline:   TODO Call the client' }, 'day view of the day under the cursor')
-- The month view is much longer: the cursor must land on the day it was
-- on in the new text, not on a line of the old one.
press('vm')
wait_for_view('Month-agenda (W18-W22):')
wait_for_cursor('Wednesday  20 May 2026')
press('vw')
wait_for_view('Week-agenda (W21):')
wait_for_cursor('Wednesday  20 May 2026')

-- 4b. `J` goes to a date picked in the calendar (`org-agenda-goto-date`):
-- the week of that day, with the cursor on it.
local function prompt_window()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == 'org-date' then
      return win
    end
  end
end
local function in_calendar_on(day)
  return function()
    vim.api.nvim_feedkeys('', 'x', false)
    local win = prompt_window()
    if not win or vim.api.nvim_get_current_win() ~= win then
      return false
    end
    local col = vim.api.nvim_win_get_cursor(win)[2]
    return vim.api.nvim_get_current_line():sub(col, col + 1) == day
  end
end
press('J')
wait('the calendar on today', in_calendar_on('18'))
press('j')
wait('a week later', in_calendar_on('25'))
press('l')
wait('a day later', in_calendar_on('26'))
press('<CR>')
wait_for_view('Week-agenda (W22):')
wait_for_cursor('Tuesday    26 May 2026')
check(prompt_window() == nil, 'the calendar is closed')
press('.')
wait_for_view('Week-agenda (W21):')
wait_for_cursor('Monday     18 May 2026 W21')

-- 5. Changing an entry from the agenda edits its file and the agenda follows.
vim.api.nvim_win_set_cursor(0, { 13, 0 })
eq(cursor_line(), '  work:       In   2 d.:  TODO Call the client', 'on an entry')
press('t')
-- Done, the entry no longer warns of its deadline; it stays on the day itself.
wait('the agenda to show the new state', function()
  return line(13) == 'Tuesday    19 May 2026'
end)
eq(line(16), '  work:       Deadline:   DONE Call the client', 'the entry is done on its deadline day')
eq(vim.api.nvim_win_get_cursor(0)[1], 13, 'the cursor stays where it was')
local work_buf = vim.fn.bufnr(work)
check(work_buf > 0 and vim.api.nvim_buf_is_loaded(work_buf), 'the file of the entry is loaded')
eq(vim.api.nvim_buf_get_lines(work_buf, 2, 3, false), { '* DONE Call the client' }, 'the entry changed in its file')
-- `autoSave`: a change made from the agenda is saved.
wait('the file of the entry to be saved', function()
  return not vim.bo[work_buf].modified and vim.fn.readfile(work)[3] == '* DONE Call the client'
end)
-- Org's `org-save-all-org-buffers` saves what is left unsaved when asked.
vim.api.nvim_buf_set_lines(work_buf, -1, -1, false, { '' })
require('org-lsp').run('org.agenda.save')
wait('the file to be saved', function()
  return not vim.bo[work_buf].modified
end)

-- 6. Priority and date change from the agenda, and the cursor goes with
-- the entry when it moves to another line.
local home_buf = vim.fn.bufnr(vim.fs.joinpath(directory, 'home.org'))
local function home_line(number)
  home_buf = home_buf > 0 and home_buf or vim.fn.bufnr(vim.fs.joinpath(directory, 'home.org'))
  return home_buf > 0 and vim.api.nvim_buf_get_lines(home_buf, number - 1, number, false)[1] or nil
end
local function find(text)
  for number, content in ipairs(lines()) do
    if content == text then
      return number
    end
  end
  error('no agenda line `' .. text .. '`')
end
vim.api.nvim_win_set_cursor(0, { find('  home:       Sched. 2x:  TODO Water plants'), 14 })
press('+')
wait('the priority in the file', function()
  return home_line(1) == '* TODO [#B] Water plants'
end)
wait_for_cursor('  home:       Sched. 2x:  TODO [#B] Water plants')
press('+')
wait_for_cursor('  home:       Sched. 2x:  TODO [#A] Water plants')
-- One step later takes a date that has passed to today, …
press('<S-Right>')
wait('the date in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-18 Mon +1w>'
end)
wait_for_cursor('  home:       Scheduled:  TODO [#A] Water plants')
-- … and the next one to tomorrow, where the cursor follows the entry.
press('<S-Right>')
wait('the next day in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-19 Tue +1w>'
end)
wait('the cursor to follow the entry to Tuesday', function()
  local row = vim.api.nvim_win_get_cursor(0)[1]
  return line(row) == '  home:       Scheduled:  TODO [#A] Water plants' and line(row - 2) == 'Tuesday    19 May 2026'
end)
eq(line(vim.api.nvim_win_get_cursor(0)[1] - 1), '  work:       14:00-15:00 Meeting ', 'below what Tuesday already had')
eq(vim.api.nvim_win_get_cursor(0)[2], 14, 'the cursor keeps its column')
-- Held down, the key repeats faster than Neovim usually reports changes
-- to the server. No press is lost: each builds on the one before.
for _ = 1, 5 do
  press('<S-Right>')
  vim.wait(30)
end
wait('five more days in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-24 Sun +1w>'
end)
wait('the cursor to follow the entry to Sunday', function()
  local row = vim.api.nvim_win_get_cursor(0)[1]
  return line(row) == '  home:       Scheduled:  TODO [#A] Water plants' and line(row - 1) == 'Sunday     24 May 2026'
end)
press('5<S-Left>')
wait('back on Tuesday', function()
  local row = vim.api.nvim_win_get_cursor(0)[1]
  return home_line(2) == 'SCHEDULED: <2026-05-19 Tue +1w>'
    and line(row) == '  home:       Scheduled:  TODO [#A] Water plants'
    and line(row - 2) == 'Tuesday    19 May 2026'
end)
-- A count is a number of days. Back to how it was.
press('3<S-Left>')
wait('the old date in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-16 Sat +1w>'
end)
wait_for_cursor('  home:       Sched. 2x:  TODO [#A] Water plants')
press('-')
wait_for_cursor('  home:       Sched. 2x:  TODO [#B] Water plants')
press('-')
wait_for_cursor('  home:       Sched. 2x:  TODO [#C] Water plants')
press('-')
wait('the priority to be gone', function()
  return home_line(1) == '* TODO Water plants'
end)
wait_for_cursor('  home:       Sched. 2x:  TODO Water plants')
-- `org-agenda-priority` asks which one.
do
  local input = vim.ui.input
  local answers = { 'a', ' ' }
  vim.ui.input = function(_, on_confirm)
    on_confirm(table.remove(answers, 1))
  end
  press('<Leader>o,')
  wait('the priority typed in the file', function()
    return home_line(1) == '* TODO [#A] Water plants'
  end)
  wait_for_cursor('  home:       Sched. 2x:  TODO [#A] Water plants')
  press('<Leader>o,')
  wait('the priority removed again', function()
    return home_line(1) == '* TODO Water plants'
  end)
  wait_for_cursor('  home:       Sched. 2x:  TODO Water plants')
  vim.ui.input = input
end

-- 6b. Scheduling from the agenda (`org-agenda-schedule`) opens the
-- calendar on the entry's date; the day taken there goes to its file, and
-- the cursor follows the entry.
press('<Leader>ois')
wait('the calendar on the scheduled day', in_calendar_on('16'))
eq(vim.api.nvim_buf_get_lines(0, -2, -1, false), { 'SCHEDULED Date+time [2026-05-16]:    => <2026-05-16 Sat>' }, 'the minibuffer')
press('j')
wait('a week later', in_calendar_on('23'))
press('<CR>')
wait('the new date in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-23 Sat +1w>'
end)
wait('the calendar to close and the cursor to follow the entry to Saturday', function()
  vim.api.nvim_feedkeys('', 'x', false)
  local row = vim.api.nvim_win_get_cursor(0)[1]
  return prompt_window() == nil
    and line(row) == '  home:       Scheduled:  TODO Water plants'
    and line(row - 1) == 'Saturday   23 May 2026'
end)
-- Typed in the minibuffer, back to where it was.
local input = vim.ui.input
vim.ui.input = function(_, on_confirm)
  on_confirm('2026-05-16')
end
press('<Leader>ois')
wait('the calendar on Saturday', in_calendar_on('23'))
press('i')
wait('the old date in the file again', function()
  return home_line(2) == 'SCHEDULED: <2026-05-16 Sat +1w>'
end)
-- The entry repeats weekly, so Saturday still shows it: the cursor stays.
wait('the agenda to show the entry today again', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return prompt_window() == nil and vim.tbl_contains(lines(), '  home:       Sched. 2x:  TODO Water plants')
end)
wait('the cursor to stay on the entry on Saturday', function()
  local row = vim.api.nvim_win_get_cursor(0)[1]
  return line(row) == '  home:       Scheduled:  TODO Water plants' and line(row - 1) == 'Saturday   23 May 2026'
end)
vim.api.nvim_win_set_cursor(0, { find('  home:       Sched. 2x:  TODO Water plants'), 14 })
vim.ui.input = input

-- 7. Enter goes to the entry.
vim.api.nvim_win_set_cursor(0, { 16, 0 })
press('<CR>')
wait('the file of the entry', function()
  return vim.api.nvim_get_current_buf() == work_buf
end)
eq(vim.api.nvim_win_get_cursor(0), { 3, 0 }, 'the cursor is on the headline')
-- An edit in the file reaches the agenda once the file is saved.
vim.api.nvim_buf_set_lines(work_buf, 0, 1, false, { '* TODO [#A] Write the report :office:' })
vim.cmd('silent write')
wait('the agenda to follow the saved file', function()
  local text = vim.api.nvim_buf_get_lines(agenda_buf, 3, 4, false)[1]
  return text == '  work:        9:00...... Scheduled:  TODO [#A] Write the report        :office:'
end)

-- 8. Closing the agenda, and opening a view by command.
vim.api.nvim_set_current_buf(agenda_buf)
press('q')
check(not vim.api.nvim_buf_is_valid(agenda_buf), 'q closes the agenda')
eq(#vim.api.nvim_tabpage_list_wins(0), 1, 'and the split it opened')
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq(lines(), {
  'Global list of TODO items of type: ALL',
  '  work:       TODO [#A] Write the report                                :office:',
  '  home:       TODO Water plants',
}, 'the TODO list, most urgent first')
-- `:Org agenda` is the dispatcher; its menu can be dismissed.
choice = nil
vim.cmd('Org agenda')
vim.wait(200)
eq(line(1), 'Global list of TODO items of type: ALL', 'dismissing the menu changes nothing')
vim.cmd('Org agenda list')
wait_for_view('Week-agenda (W21):')
vim.cmd('Org agenda todos')
wait_for_view('Global list of TODO items of type: ALL')
eq(vim.fn.getcompletion('Org agenda.l', 'cmdline'), { 'agenda.list', 'agenda.later', 'agenda.log' }, 'commands complete')

-- 9. A command that cannot work says why.
local messages = {}
vim.notify = function(message)
  table.insert(messages, message)
end
press('f')
wait('the message of a failed command', function()
  return #messages > 0
end)
eq(messages, { 'org-lsp: The TODO list has no dates to move through' }, 'the server explains')

-- 10. `<Leader>oc` captures: a template by its key, then a buffer of its
-- own; `<C-c>` there puts the entry into its file. This one does not exist
-- yet: the server writes it.
local plugin = require('org-lsp')
local read_key = plugin.read_key
plugin.read_key = function()
  return 't'
end
-- What the plugin asks of insert mode.
local started
local command = vim.cmd
vim.cmd = setmetatable({}, {
  __index = command,
  __call = function(_, text)
    if type(text) == 'string' and text:match('^startinsert') then
      started = text
    end
    return command(text)
  end,
})
press('<Leader>oc')
local inbox = vim.fs.joinpath(directory, 'inbox.org')
wait('the capture buffer', function()
  return vim.fs.basename(vim.api.nvim_buf_get_name(0)) == 'CAPTURE-inbox.org'
end)
plugin.read_key = read_key
eq(lines(), { '* TODO ', '[2026-05-18 Mon 10:30]' }, 'the template, filled in')
-- Ready to type where `%?` was: insert mode, after the last character. (A
-- script never gets back to Neovim's main loop, where insert mode would
-- start; what the plugin asked for is what can be seen here.)
eq(started, 'startinsert!', 'insert mode in the capture buffer')
eq(vim.api.nvim_win_get_cursor(0), { 1, 7 }, 'the cursor where %? was')
wait('the capture keys', function()
  return vim.fn.maparg('<C-c>', 'n') ~= ''
end)
vim.api.nvim_buf_set_lines(0, 0, 1, false, { '* TODO Call mom' })
press('<C-c>')
wait('the entry in its file', function()
  return vim.uv.fs_stat(inbox) ~= nil and vim.deep_equal(vim.fn.readfile(inbox), { '* TODO Call mom', '[2026-05-18 Mon 10:30]' })
end)
wait('the capture buffer to close', function()
  for _, buffer in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(buffer) and vim.fs.basename(vim.api.nvim_buf_get_name(buffer)) == 'CAPTURE-inbox.org' then
      return false
    end
  end
  return true
end)

-- 11. Where the agenda and capture buffers open: `win_split_mode`.
local function only_window()
  vim.cmd('silent! only')
  vim.cmd.edit(work)
end
local function windows()
  return #vim.api.nvim_tabpage_list_wins(0)
end
local function is_float()
  return vim.api.nvim_win_get_config(0).relative ~= ''
end
only_window()
org_lsp.setup({ win_split_mode = 'vsplit' })
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq({ windows(), vim.fn.winlayout()[1] }, { 2, 'row' }, 'vsplit: beside')
press('q')
eq(windows(), 1, 'vsplit: q closes it')

org_lsp.setup({ win_split_mode = 'float' })
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
check(is_float(), 'float: a floating window')
local agenda_float = vim.api.nvim_get_current_win()
-- The calendar of a date prompt floats over it, at the bottom.
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('<Leader>ois')
wait('the calendar over the float', function()
  return vim.bo.filetype == 'org-date'
end)
local config = vim.api.nvim_win_get_config(0)
eq({ config.relative, config.zindex > vim.api.nvim_win_get_config(agenda_float).zindex }, { 'editor', true }, 'the calendar floats above the agenda')
press('q')
wait('the calendar to go', function()
  return vim.api.nvim_get_current_win() == agenda_float
end)
-- Going to an entry leaves the float for the window under it.
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('<CR>')
wait('the entry from the float', function()
  return vim.api.nvim_get_current_buf() == work_buf
end)
eq({ windows(), is_float() }, { 1, false }, 'float: the entry opens under it')

org_lsp.setup({ win_split_mode = 'current' })
local before = vim.api.nvim_get_current_buf()
vim.cmd('vsplit')
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq(windows(), 2, 'current: no new window')
press('q')
eq({ windows(), vim.api.nvim_get_current_buf() }, { 2, before }, 'current: q goes back to the buffer it replaced')

only_window()
local given
org_lsp.setup({
  win_split_mode = function(bufnr)
    given = bufnr
    vim.cmd('topleft split')
    return vim.api.nvim_get_current_win()
  end,
})
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq({ given, vim.api.nvim_win_get_number(0) }, { vim.api.nvim_get_current_buf(), 1 }, 'a function opens the window')
press('q')

-- The capture buffer opens the same way, and goes with its window.
only_window()
org_lsp.setup({ win_split_mode = 'split' })
plugin.read_key = function()
  return 't'
end
press('<Leader>oc')
wait('the capture buffer', function()
  return vim.fs.basename(vim.api.nvim_buf_get_name(0)) == 'CAPTURE-inbox.org'
end)
plugin.read_key = read_key
eq({ windows(), vim.fn.winlayout()[1] }, { 2, 'col' }, 'capture: a horizontal split')
press('<Leader>ok')
wait('the capture to be given up', function()
  return windows() == 1
end)
eq(vim.api.nvim_get_current_buf(), work_buf, 'capture: back in the file')

print(('ok: %d checks passed'):format(checks))
vim.cmd('qall!')
