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
  settings = { agendaFiles = { directory }, clock = { today = '2026-05-18', now = '10:30' } },
}
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')

-- The dispatcher asks which view to show; pick by title.
local choice
vim.ui.select = function(items, opts, on_choice)
  for _, item in ipairs(items) do
    if opts.format_item(item) == choice then
      return on_choice(item)
    end
  end
  on_choice(nil)
end

-- 1. From a buffer that is not an Org file, with no server running yet.
eq(#vim.lsp.get_clients(), 0, 'the server does not run before it is needed')
eq(vim.fn.maparg('\\oa', 'n', false, true).buffer, 0, 'the agenda key is global')
choice = 'Agenda for the current week or day'
press('\\oa')
wait_for_view('Week-agenda (W21):')
eq(vim.api.nvim_buf_get_name(0), 'org-agenda:/Org%20Agenda', 'the agenda is a document of the server')
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

-- 2. The server announced the key the plugin sets before the server runs.
local org_lsp = require('org-lsp')
local announced
for _, command in ipairs(org_lsp.manifest.commands) do
  for _, key in ipairs(command.keys) do
    if key.preset == 'vim' and key['in'] == 'global' then
      announced = { command.command, key.key }
    end
  end
end
eq(announced, { 'org.agenda.dispatch', '<Leader>oa' }, 'the built-in global key is the one the server names')
eq(vim.fn.maparg('f', 'n', false, true).desc, 'Agenda: go forward in time', 'agenda keys carry the server\'s titles')
eq(vim.fn.maparg('f', 'n', false, true).buffer, 1, 'agenda keys are local to the agenda')

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
press('f')
wait_for_view('Week-agenda (W22):')
eq(cursor_line(), 'Monday     25 May 2026 W22', 'forward: the cursor is on the first day')
press('b')
press('b')
wait_for_view('Week-agenda (W20):')
press('.')
wait_for_view('Week-agenda (W21):')
eq(cursor_line(), 'Monday     18 May 2026 W21', 'back on today')
-- A day view of Wednesday has three lines.
vim.api.nvim_win_set_cursor(0, { 17, 0 })
press('vd')
wait_for_view('Day-agenda (W21):')
eq(lines(), { 'Day-agenda (W21):', 'Wednesday  20 May 2026', '  work:       Deadline:   TODO Call the client' }, 'day view of the day under the cursor')
-- The month view is much longer: the cursor must land on today's line of
-- the new text, not on a line of the old one.
press('vm')
wait_for_view('Month-agenda (W18-W22):')
wait('the cursor to reach today', function()
  return cursor_line() == 'Monday     18 May 2026 W21'
end)
press('vw')
wait_for_view('Week-agenda (W21):')

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
eq(vim.bo[work_buf].modified, true, 'the change is not saved behind the user\'s back')

-- 6. Enter goes to the entry.
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

-- 7. Closing the agenda, and opening a view by command.
vim.api.nvim_set_current_buf(agenda_buf)
press('q')
check(not vim.api.nvim_buf_is_valid(agenda_buf), 'q closes the agenda')
vim.cmd('Org agenda todos')
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
eq(vim.fn.getcompletion('Org agenda.l', 'cmdline'), { 'agenda.list', 'agenda.later' }, 'commands complete')

-- 8. A command that cannot work says why.
local messages = {}
vim.notify = function(message)
  table.insert(messages, message)
end
press('f')
wait('the message of a failed command', function()
  return #messages > 0
end)
eq(messages, { 'org-lsp: The TODO list has no dates to move through' }, 'the server explains')

print(('ok: %d checks passed'):format(checks))
vim.cmd('qall!')
