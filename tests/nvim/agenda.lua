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
    org_agenda_files = { directory },
    clock = { today = '2026-05-18', now = '10:30' },
    org_capture_templates = {
      { key = 't', description = 'Task', target = { 'file', vim.fs.joinpath(directory, 'inbox.org') }, template = '* TODO %?\n%U' },
    },
  },
  -- A key of the user's own, for a command the server binds no key to.
  mappings = { global = { ['org.agenda.todos'] = '<F7>' } },
}
-- A root for every server, as a configuration may give: the server
-- started for the agenda, with no file, is the one the Org files use too.
vim.fn.mkdir(vim.fs.joinpath(directory, '.git'), 'p')
vim.lsp.config('*', { root_markers = { '.git' } })
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
    's Search for keywords',
    'S Like s, but only TODO entries',
    '# List stuck projects',
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
-- `org_agenda_tags_column` `"auto"`: the tags end at the right edge of the
-- window, and follow it when it changes size.
local agenda_window = vim.api.nvim_get_current_win()
vim.cmd('leftabove 10vnew')
local narrow = vim.api.nvim_get_current_win()
vim.api.nvim_set_current_win(agenda_window)
wait('the tags at the narrower edge', function()
  local width = vim.api.nvim_win_get_width(agenda_window)
  return #line(4) == width and vim.endswith(line(4), ':office:')
end)
vim.api.nvim_win_close(narrow, true)
wait('the tags at column 80 again', function()
  return line(4) == '  work:        9:00...... Scheduled:  TODO [#A] Write report            :office:'
end)

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
  {
    ['org.agenda.dispatch'] = '<Leader>oa',
    ['org.capture'] = '<Leader>oc',
    ['org.clock.goto'] = '<Leader>oxj',
    ['org.journal.new.entry'] = '<Leader>jj',
  },
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
  -- Neovim 0.12 also lists the token that ends where this one starts.
  for _, token in ipairs(tokens or {}) do
    if token.start_col == col then
      return token
    end
  end
  return tokens and tokens[1]
end
--- The token at the start of `text` in line `row`.
local function token_of(row, text)
  return token_at(row, assert(line(row + 1):find(text, 1, true)) - 1)
end
-- The tokens of the text as it is now: those of the narrower window stay
-- until the new ones come, as the agenda is not cleared in between.
wait('semantic tokens in the agenda', function()
  local tag = token_of(3, ':office:')
  return token_at(0, 0) ~= nil and tag ~= nil and tag.type == 'tag'
end)
eq(token_at(0, 0).type, 'agendaStructure', 'the title line')
eq({ token_at(1, 0).type, token_at(1, 0).modifiers.today }, { 'agendaDate', true }, 'today\'s date')
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

-- 5. Changing an entry from the agenda edits its file. As in Emacs
-- (`org-agenda-change-all-lines`), every line of the entry shows the new
-- state where it is, until the agenda is built again.
vim.api.nvim_win_set_cursor(0, { 13, 0 })
eq(cursor_line(), '  work:       In   2 d.:  TODO Call the client', 'on an entry')
press('t')
wait('the agenda to show the new state', function()
  return line(13) == '  work:       In   2 d.:  DONE Call the client'
end)
eq(line(17), '  work:       Deadline:   DONE Call the client', 'the entry is done on its deadline day too')
eq(vim.api.nvim_win_get_cursor(0)[1], 13, 'the cursor stays where it was')
-- Done, the entry no longer warns of its deadline once the agenda is
-- built again; it stays on the day itself.
press('r')
wait('the agenda built again', function()
  return line(13) == 'Tuesday    19 May 2026'
end)
eq(line(16), '  work:       Deadline:   DONE Call the client', 'the entry is done on its deadline day')
local work_buf = vim.fn.bufnr(work)
check(work_buf > 0 and vim.api.nvim_buf_is_loaded(work_buf), 'the file of the entry is loaded')
eq(vim.api.nvim_buf_get_lines(work_buf, 2, 3, false), { '* DONE Call the client' }, 'the entry changed in its file')
-- `auto_save`: a change made from the agenda is saved.
wait('the file of the entry to be saved', function()
  return not vim.bo[work_buf].modified and vim.fn.readfile(work)[3] == '* DONE Call the client'
end)
-- Org's `org-save-all-org-buffers` saves what is left unsaved when asked.
vim.api.nvim_buf_set_lines(work_buf, -1, -1, false, { '' })
require('org-lsp').run('org.agenda.save')
wait('the file to be saved', function()
  return not vim.bo[work_buf].modified
end)

-- 6. Priority and date change from the agenda. The entry's lines stay
-- where they are, and so does the cursor; a new date shows at their end
-- (`org-agenda-show-new-time`).
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
-- `text` with `stamp` over its end, as Emacs shows a new date: at the
-- right edge of the window, but for the column a terminal keeps.
local function stamped(text, stamp)
  local info = vim.fn.getwininfo(vim.api.nvim_get_current_win())[1]
  local keep = info.width - info.textoff - 1 - #stamp - 1
  return (text .. string.rep(' ', keep)):sub(1, keep) .. stamp
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
local plants = '  home:       Sched. 2x:  TODO [#A] Water plants'
local row = vim.api.nvim_win_get_cursor(0)[1]
press('<S-Right>')
wait('the date in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-18 Mon +1w>'
end)
wait_for_cursor(stamped(plants, ' => <2026-05-18 Mon +1w> '))
-- … and the next one to tomorrow; the line stays where it is.
press('<S-Right>')
wait('the next day in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-19 Tue +1w>'
end)
wait_for_cursor(stamped(plants, ' => <2026-05-19 Tue +1w> '))
eq(vim.api.nvim_win_get_cursor(0), { row, 14 }, 'the cursor stays, in its column')
eq(
  line(find('Saturday   23 May 2026') + 1),
  stamped('  home:       Scheduled:  TODO [#A] Water plants', ' => <2026-05-19 Tue +1w> '),
  'the other line of the entry says so too'
)
-- Held down, the key repeats faster than Neovim usually reports changes
-- to the server. No press is lost: each builds on the one before.
for _ = 1, 5 do
  press('<S-Right>')
  vim.wait(30)
end
wait('five more days in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-24 Sun +1w>'
end)
wait_for_cursor(stamped(plants, ' => <2026-05-24 Sun +1w> '))
press('5<S-Left>')
wait('back on Tuesday', function()
  return home_line(2) == 'SCHEDULED: <2026-05-19 Tue +1w>'
end)
wait_for_cursor(stamped(plants, ' => <2026-05-19 Tue +1w> '))
-- A count is a number of days. Back to how it was.
press('3<S-Left>')
wait('the old date in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-16 Sat +1w>'
end)
wait_for_cursor(stamped(plants, ' => <2026-05-16 Sat +1w> '))
eq(vim.api.nvim_win_get_cursor(0), { row, 14 }, 'the cursor has not moved')
-- A new priority writes the line anew, without the date.
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
  press(',')
  wait('the priority typed in the file', function()
    return home_line(1) == '* TODO [#A] Water plants'
  end)
  wait_for_cursor('  home:       Sched. 2x:  TODO [#A] Water plants')
  press(',')
  wait('the priority removed again', function()
    return home_line(1) == '* TODO Water plants'
  end)
  wait_for_cursor('  home:       Sched. 2x:  TODO Water plants')
  vim.ui.input = input
end

-- 6b. Scheduling from the agenda (`org-agenda-schedule`) opens the
-- calendar on the entry's date; the day taken there goes to its file, and
-- the entry's line says so where it is, with the cursor on it.
local scheduled_from = vim.api.nvim_win_get_cursor(0)
press('<C-c><C-s>')
wait('the calendar on the scheduled day', in_calendar_on('16'))
eq(vim.api.nvim_buf_get_lines(0, -2, -1, false), { 'SCHEDULED Date+time [2026-05-16]:    => <2026-05-16 Sat>' }, 'the minibuffer')
press('j')
wait('a week later', in_calendar_on('23'))
press('<CR>')
wait('the new date in the file', function()
  return home_line(2) == 'SCHEDULED: <2026-05-23 Sat +1w>'
end)
wait('the calendar to close and the line to show the new date', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return prompt_window() == nil
    and cursor_line() == stamped('  home:       Sched. 2x:  TODO Water plants', ' S => Scheduled to <2026-05-23 Sat +1w> ')
end)
eq(vim.api.nvim_win_get_cursor(0), scheduled_from, 'the cursor is back where it was')
-- Typed in the minibuffer, back to where it was.
local input = vim.ui.input
vim.ui.input = function(_, on_confirm)
  on_confirm('2026-05-16')
end
press('<C-c><C-s>')
wait('the calendar on Saturday', in_calendar_on('23'))
press('i')
wait('the old date in the file again', function()
  return home_line(2) == 'SCHEDULED: <2026-05-16 Sat +1w>'
end)
wait('the calendar to close and the line to show the old date', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return prompt_window() == nil
    and cursor_line() == stamped('  home:       Sched. 2x:  TODO Water plants', ' S => Scheduled to <2026-05-16 Sat +1w> ')
end)
eq(vim.api.nvim_win_get_cursor(0), scheduled_from, 'the cursor is back where it was')
vim.ui.input = input
-- Cancelled, the calendar leaves the cursor where it was, also when the
-- agenda changed above and below it in the meantime.
press('<C-c><C-s>')
wait('the calendar again', in_calendar_on('16'))
local function edit_work(from, to)
  local text = vim.api.nvim_buf_get_lines(work_buf, 0, -1, false)
  for number, content in ipairs(text) do
    text[number] = content:gsub(from[1], to[1]):gsub(from[2], to[2])
  end
  vim.api.nvim_buf_set_lines(work_buf, 0, -1, false, text)
  vim.api.nvim_buf_call(work_buf, function()
    vim.cmd('silent write')
  end)
end
edit_work({ 'Write report', 'Meeting' }, { 'Write a report', 'A meeting' })
wait('the agenda to change under the calendar', function()
  return vim.tbl_contains(vim.api.nvim_buf_get_lines(agenda_buf, 0, -1, false), '  work:       14:00-15:00 A meeting ')
end)
check(prompt_window() ~= nil, 'the calendar is still open')
press('q')
wait('the calendar to close', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return prompt_window() == nil
end)
eq(vim.api.nvim_win_get_cursor(0), scheduled_from, 'the cursor is where it was')
eq(cursor_line(), '  home:       Sched. 2x:  TODO Water plants', 'on the entry, the agenda built anew')
eq(home_line(2), 'SCHEDULED: <2026-05-16 Sat +1w>', 'the file as it was')
edit_work({ 'Write a report', 'A meeting' }, { 'Write report', 'Meeting' })
wait('the agenda as it was', function()
  return vim.tbl_contains(lines(), '  work:       14:00-15:00 Meeting ')
end)
-- Built again, the agenda shows the entries where they now belong.
press('r')
wait('the agenda built again', function()
  return vim.tbl_contains(lines(), '  home:       Sched. 2x:  TODO Water plants')
end)
vim.api.nvim_win_set_cursor(0, { find('  home:       Sched. 2x:  TODO Water plants'), 14 })

-- 6c. TAB shows the entry in another window: a new one when the agenda
-- is alone, then the one that shows the file, an empty one, or the
-- window last left, as Emacs's `display-buffer` uses some window.
local agenda_win = vim.api.nvim_get_current_win()
vim.cmd('only')
local function tab_to_entry(message)
  vim.api.nvim_set_current_win(agenda_win)
  vim.api.nvim_win_set_cursor(agenda_win, { 16, 0 })
  press('<Tab>')
  wait(message, function()
    return vim.api.nvim_get_current_buf() == work_buf and vim.api.nvim_get_current_win() ~= agenda_win
  end)
  eq(vim.api.nvim_win_get_buf(agenda_win), agenda_buf, 'the agenda stays in its window')
  eq(vim.api.nvim_win_get_cursor(0), { 3, 0 }, 'the cursor is on the headline')
  return vim.api.nvim_get_current_win()
end
local entry_win = tab_to_entry('the file in a new window')
eq(#vim.api.nvim_tabpage_list_wins(0), 2, 'one window more')
eq(tab_to_entry('the file in the window that shows it'), entry_win, 'that window again')
vim.cmd('enew')
eq(tab_to_entry('the file in the empty window'), entry_win, 'the empty window')
eq(#vim.api.nvim_tabpage_list_wins(0), 2, 'no window more')
vim.cmd('edit ' .. vim.fn.fnameescape(vim.fn.tempname() .. '.txt'))
eq(tab_to_entry('the file in the window last left, which shows another file'), entry_win, 'that window')
eq(#vim.api.nvim_tabpage_list_wins(0), 2, 'not a new one')
vim.api.nvim_set_current_win(agenda_win)

-- 6d. F (follow mode): the entry of the cursor's line shows in another
-- window as the cursor moves, and the cursor stays in the agenda.
vim.cmd('only')
vim.api.nvim_win_set_cursor(agenda_win, { 14, 0 })
press('F')
local function shown_line()
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if win ~= agenda_win and vim.api.nvim_win_get_buf(win) == work_buf then
      return vim.api.nvim_win_get_cursor(win)[1]
    end
  end
end
local meeting = vim.fn.match(vim.api.nvim_buf_get_lines(work_buf, 0, -1, false), 'Meeting') + 1
wait('the entry of the line in another window', function()
  return shown_line() == meeting
end)
eq(vim.api.nvim_get_current_win(), agenda_win, 'the cursor stays in the agenda')
-- `CursorMoved` comes from the main loop, which a script does not run.
local function moved(keys)
  press(keys)
  vim.api.nvim_exec_autocmds('CursorMoved', { buffer = agenda_buf })
end
moved('jj')
wait('the entry of the next line with one', function()
  return shown_line() == 3
end)
eq(vim.api.nvim_get_current_win(), agenda_win, 'still in the agenda')
-- An entry of another file shows in that same window: no window more.
local back = vim.api.nvim_win_get_cursor(agenda_win)
vim.api.nvim_win_set_cursor(agenda_win, { find('  home:       Sched. 2x:  TODO Water plants'), 0 })
vim.api.nvim_exec_autocmds('CursorMoved', { buffer = agenda_buf })
wait('the other file in the same window', function()
  local wins = vim.api.nvim_tabpage_list_wins(0)
  return #wins == 2 and vim.iter(wins):any(function(win)
    return vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(win)):match('home%.org$') ~= nil
  end)
end)
vim.api.nvim_win_set_cursor(agenda_win, back)
vim.api.nvim_exec_autocmds('CursorMoved', { buffer = agenda_buf })
wait('back to the first file, in that window', function()
  return shown_line() == 3 and #vim.api.nvim_tabpage_list_wins(0) == 2
end)
press('F')
moved('kk')
vim.wait(200)
eq(shown_line(), 3, 'off: the other window stays')
vim.cmd('only')

-- 6e. The outline path of the line's entry in the echo area, out of the
-- message history (`org-agenda-show-outline-path`).
local echo, echoed = vim.api.nvim_echo, {}
vim.api.nvim_echo = function(chunks, history, opts)
  table.insert(echoed, { chunks[1][1], history })
  return echo(chunks, history, opts)
end
vim.api.nvim_win_set_cursor(agenda_win, { find('  home:       Sched. 2x:  TODO Water plants'), 0 })
vim.api.nvim_exec_autocmds('CursorMoved', { buffer = agenda_buf })
wait('the outline path', function()
  return #echoed > 0
end)
vim.api.nvim_echo = echo
eq(echoed[#echoed], { 'home.org', false }, 'the file of a top-level entry')

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
local messages, titles = {}, {}
vim.notify = function(message, _, opts)
  table.insert(messages, message)
  table.insert(titles, opts and opts.title)
end
press('f')
wait('the message of a failed command', function()
  return #messages > 0
end)
eq(messages, { 'org-lsp: The TODO list has no dates to move through' }, 'the server explains')
eq(titles, { 'Org' }, 'in a notification titled Org')
-- So are the messages the server sends of its own (`window/showMessage`).
local levels = {}
vim.notify = function(message, level, opts)
  table.insert(levels, { message, level, opts and opts.title })
end
require('org-lsp.view').handlers['window/showMessage'](nil, { type = 2, message = 'Careful' }, {})
eq(levels, { { 'Careful', vim.log.levels.WARN, 'Org' } }, 'a warning of the server, titled Org')

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
wait('the Org file attached', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
eq(#vim.lsp.get_clients({ name = 'org_lsp' }), 1, 'one server for the agenda and the Org files')
org_lsp.setup({ win_split_mode = 'vsplit' })
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq({ windows(), vim.fn.winlayout()[1] }, { 2, 'row' }, 'vsplit: beside')
press('q')
eq(windows(), 1, 'vsplit: q closes it')

-- `win_split_size`: lines, columns or a part of the editor.
org_lsp.setup({ win_split_mode = 'botright split', win_split_size = 7 })
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq(vim.api.nvim_win_get_height(0), 7, 'a split of 7 lines')
press('q')
org_lsp.setup({ win_split_mode = 'vsplit', win_split_size = 0.25 })
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq(vim.api.nvim_win_get_width(0), math.floor(vim.o.columns * 0.25), 'a quarter of the editor wide')
press('q')

org_lsp.setup({ win_split_mode = 'float' })
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
check(is_float(), 'float: a floating window')
local agenda_float = vim.api.nvim_get_current_win()
-- The calendar of a date prompt floats over it, at the bottom.
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('<C-c><C-s>')
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

-- 12. `r` rebuilds the agenda from the files as they are on disk, an open
-- file that changed there and not here included.
only_window()
vim.cmd('silent write')
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
local on_disk = vim.fn.readfile(work)
table.insert(on_disk, '* TODO Written elsewhere')
vim.fn.writefile(on_disk, work)
press('r')
wait('the entry written elsewhere', function()
  return vim.iter(lines()):any(function(text)
    return text:find('TODO Written elsewhere', 1, true) ~= nil
  end)
end)
eq(vim.api.nvim_buf_get_lines(work_buf, -2, -1, false), { '* TODO Written elsewhere' }, 'the open file read again')

-- 13. `g?` lists the agenda's keys.
local agenda_win = vim.api.nvim_get_current_win()
press('g?')
eq(lines()[1], 'Agenda', 'the help starts with the agenda keys')
check(vim.iter(lines()):any(function(text)
  return text:match('^  r%s+Agenda: rebuild$') ~= nil
end), 'r is listed')
press('<Esc>')
eq(vim.api.nvim_get_current_win(), agenda_win, '<Esc> closes the help')

-- 14. `<Leader>o$` archives the entry of the line, into the archive file
-- next to its own, and the entry leaves the agenda. `$` stays Vim's motion.
-- The archive file is new, and Neovim does not announce `documentChanges`
-- for an edit to make it: it is written rather than left unsaved.
eq(vim.fn.maparg('$', 'n'), '', '`$` is not mapped')
local written = vim.iter(lines()):enumerate():find(function(_, text)
  return text:find('TODO Written elsewhere', 1, true) ~= nil
end)
vim.api.nvim_win_set_cursor(0, { written, 0 })
press('<Leader>o$')
wait('the entry archived', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return not vim.iter(lines()):any(function(text)
    return text:find('TODO Written elsewhere', 1, true) ~= nil
  end)
end)
check(vim.tbl_contains(vim.fn.readfile(work .. '_archive'), '* TODO Written elsewhere'), 'in the archive file')
press('q')

-- 15. With `,` as the leader, `,` (priority) waits for the leader keys of
-- Org files, which the agenda has as well: `<Leader>ois` schedules.
vim.g.mapleader = ','
org_lsp.setup({ win_split_mode = 'split' })
press('<F7>')
wait_for_view('Global list of TODO items of type: ALL')
eq(vim.fn.maparg(',', 'n', false, true).nowait, 0, '`,` waits for the leader keys')
check(vim.fn.maparg(',ois', 'n') ~= '', 'the leader keys of Org files in the agenda')
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press(',ois')
wait('the calendar of the schedule prompt', function()
  return vim.bo.filetype == 'org-date'
end)
press('q')
wait('the calendar to go', function()
  return vim.bo.filetype ~= 'org-date'
end)
-- Back where it was asked from, not in the window under it.
eq({ vim.bo.filetype, vim.api.nvim_win_get_cursor(0) }, { 'org-agenda', { 2, 0 } }, 'back on the agenda line')
only_window()
vim.g.mapleader = nil

-- 16. `K`, Neovim's own hover, over an entry shows it in a float at the
-- cursor: the headline and its text, the float made an Org buffer the
-- server highlights. Moving the cursor closes it.
choice = 'a'
press('\\oa')
wait_for_view('Week-agenda (W21):')
local before_hover = lines()
vim.api.nvim_win_set_cursor(0, { 4, 30 })
-- With `syntax on`, as most have it, Neovim makes its float Markdown and
-- starts tree-sitter in it.
vim.cmd('syntax on')
-- And with spelling on in Markdown, as many have it: the float is
-- Markdown at first, and 'spell' is the window's.
local spell_group = vim.api.nvim_create_augroup('test_markdown_spell', { clear = true })
vim.api.nvim_create_autocmd('FileType', { group = spell_group, pattern = 'markdown', command = 'setlocal spell' })
press('K')
local float
wait('the entry in a float', function()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(win).relative ~= '' then
      float = win
      return true
    end
  end
end)
local float_buf = vim.api.nvim_win_get_buf(float)
wait('the float made an Org buffer', function()
  return vim.bo[float_buf].filetype == 'org'
end)
eq(vim.wo[float].spell, false, "Markdown's spelling gone from the float")
eq(vim.api.nvim_buf_get_lines(float_buf, 0, 2, false), { '* TODO [#A] Write the report :office:', 'SCHEDULED: <2026-05-18 Mon 09:00>' },
  'the headline and its text, as Org')
-- Not indented as `org-indent-mode` indents: wider than the float.
require('org-lsp.indent').virtual({
  uri = vim.uri_from_bufnr(float_buf),
  version = vim.lsp.util.buf_versions[float_buf],
  lines = { { 1, 2, 2, 0 } },
})
local indent_ns = vim.api.nvim_get_namespaces()['org_lsp_virtual_indent']
eq(indent_ns and #vim.api.nvim_buf_get_extmarks(float_buf, indent_ns, 0, -1, {}) or 0, 0, 'no virtual indentation in the float')
wait('the server\'s highlighting in the float', function()
  local tokens = vim.lsp.semantic_tokens.get_at_pos(float_buf, 0, 2)
  return tokens ~= nil and tokens[1] ~= nil and tokens[1].type == 'todoKeyword'
end)
eq(vim.api.nvim_get_current_win() ~= float, true, 'the cursor stays in the agenda')
eq(lines(), before_hover, 'the shown entry is no file of the agenda')
press('j')
-- (A script never gets back to Neovim's main loop, which reports the
-- moved cursor.)
vim.api.nvim_exec_autocmds('CursorMoved', { buffer = 0 })
wait('the float to close', function()
  return not vim.api.nvim_win_is_valid(float)
end)
only_window()
-- Spelling on in Org files instead: the float, an Org file, has it.
vim.api.nvim_clear_autocmds({ group = spell_group })
vim.api.nvim_create_autocmd('FileType', { group = spell_group, pattern = 'org', command = 'setlocal spell' })
choice = 'a'
press('\\oa')
wait_for_view('Week-agenda (W21):')
vim.api.nvim_win_set_cursor(0, { 4, 30 })
press('K')
local org_float
wait('the entry in a float', function()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(win).relative ~= '' then
      org_float = win
      return true
    end
  end
end)
wait('the float made an Org buffer', function()
  return vim.bo[vim.api.nvim_win_get_buf(org_float)].filetype == 'org'
end)
eq(vim.wo[org_float].spell, true, "Org's spelling in the float")
only_window()
vim.cmd('syntax off')
vim.api.nvim_del_augroup_by_id(spell_group)

-- 17. `I` clocks in on the entry of the line, which is highlighted from
-- edge to edge (`org-agenda-clocking`); `O` clocks out and the highlight
-- goes.
choice = 'a'
press('\\oa')
wait_for_view('Week-agenda (W21):')
local agenda_buf = vim.api.nvim_get_current_buf()
local clocking_ns = vim.api.nvim_create_namespace('org_lsp_line_highlights')
local function highlighted()
  return vim.tbl_map(function(mark)
    return { mark[2], mark[4].line_hl_group }
  end, vim.api.nvim_buf_get_extmarks(agenda_buf, clocking_ns, 0, -1, { details = true }))
end
vim.api.nvim_win_set_cursor(0, { 4, 30 })
press('I')
wait('the clocked line highlighted', function()
  return #highlighted() > 0
end)
eq(highlighted(), { { 3, '@lsp.type.agendaClocking.org-agenda' } }, 'the line of the clocked entry')
press('O')
wait('the highlight to go', function()
  return #highlighted() == 0
end)
only_window()

-- 18. `s` searches the files for words (`org-search-view`); `]` asks for
-- the search string again, with a word to leave out started.
do
  local input = vim.ui.input
  local asked
  local answers = { '+water', '+water -plants' }
  vim.ui.input = function(opts, on_confirm)
    asked = opts.default
    on_confirm(table.remove(answers, 1))
  end
  choice = 's'
  press('\\oa')
  wait_for_view('Search words: +water')
  check(vim.iter(lines()):any(function(text)
    return text:find('TODO Water plants', 1, true) ~= nil
  end), 'the entry with the word')
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  press(']')
  wait_for_view('Search words: +water -plants')
  eq(asked, '+water -', 'the search string, a word to leave out started')
  eq(lines(), { 'Search words: +water -plants' }, 'no entry left')
  vim.ui.input = input
end
only_window()

print(('ok: %d checks passed'):format(checks))
vim.cmd('qall!')
