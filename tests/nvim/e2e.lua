-- End-to-end test of the Neovim shim against the real server.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/e2e.lua
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

--- A headline with its tags where `org-todo` puts them: ending at column 77.
local function tagged(headline, tags)
  return headline .. (' '):rep(77 - vim.fn.strdisplaywidth(headline) - #tags) .. tags
end

local function line(number)
  return vim.api.nvim_buf_get_lines(0, number - 1, number, false)[1]
end

-- Load the plugin the way a plugin manager would: the repo root on the
-- runtime path, then its plugin/ file.
vim.g.org_lsp = { cmd = { server } }
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('syntax on')
vim.cmd('runtime plugin/org-lsp.lua')

local file = vim.fn.tempname() .. '.org'
vim.fn.writefile(vim.fn.readfile(vim.fs.joinpath(root, 'tests', 'nvim', 'sample.org')), file)
vim.cmd.edit(file)
local bufnr = vim.api.nvim_get_current_buf()
eq(vim.bo.filetype, 'org', 'filetype is detected')

-- 1. The server attaches without any configuration beyond the binary path.
local client
wait('the server to attach', function()
  client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  return client ~= nil and client.initialized
end)
eq(client.server_info.name, 'org-lsp', 'server identifies itself')
eq(client.offset_encoding, 'utf-8', 'UTF-8 positions are negotiated')

-- 2. Semantic tokens arrive and carry the expected types and modifiers.
local function token_at(row, col)
  local tokens = vim.lsp.semantic_tokens.get_at_pos(bufnr, row, col)
  return tokens and tokens[1]
end
wait('semantic tokens', function()
  return token_at(1, 0) ~= nil
end)
eq(token_at(0, 0).type, 'keyword', '#+TITLE: is a keyword token')
eq(token_at(1, 0).type, 'stars', 'headline stars')
check(token_at(1, 0).modifiers.level1, 'stars carry the level modifier')
eq(token_at(1, 2).type, 'todoKeyword', 'TODO keyword')
eq(token_at(1, 7).type, 'headline', 'headline title')
check(token_at(1, 14).modifiers.bold and token_at(1, 14).modifiers.level1, 'bold inside a title keeps both modifiers')
eq(token_at(1, 20).type, 'tag', 'tags')
eq(token_at(2, 19).type, 'link', 'link')
eq(token_at(4, 0).modifiers.level2, true, 'second-level stars')
check(token_at(2, 3) == nil, 'plain text has no token')
-- Columns after multi-byte characters land on the right text.
eq(token_at(9, 2).type, 'headline', 'title after wide characters')

-- 3. Token groups are linked to real highlight groups.
eq(vim.api.nvim_get_hl(0, { name = '@lsp.typemod.headline.level1.org' }).link, 'Title', 'level 1 links to Title')
eq(vim.api.nvim_get_hl(0, { name = '@lsp.mod.bold.org' }).link, '@markup.strong', 'bold links to @markup.strong')
-- A group nvim-orgmode themes define takes precedence once it exists.
vim.api.nvim_set_hl(0, '@org.headline.level2', { fg = '#123456' })
vim.cmd('hi clear @lsp.typemod.headline.level2.org')
vim.api.nvim_exec_autocmds('ColorScheme', {})
eq(vim.api.nvim_get_hl(0, { name = '@lsp.typemod.headline.level2.org' }).link, '@org.headline.level2', 'prefers @org.* groups')
wait('regex syntax to be replaced', function()
  return vim.bo.syntax == ''
end)

-- 4. Folding comes from the server; the file opens unfolded.
eq(vim.wo.foldmethod, 'expr', 'foldmethod')
wait('folds', function()
  return vim.fn.foldlevel(3) > 0
end)
eq(vim.fn.foldclosed(2), -1, 'file starts with everything visible')
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('<Tab>')
eq({ vim.fn.foldclosed(2), vim.fn.foldclosedend(2) }, { 2, 7 }, 'TAB folds the subtree')
press('<Tab>')
eq(vim.fn.foldclosed(2), -1, 'TAB again opens it')
eq({ vim.fn.foldclosed(5), vim.fn.foldclosedend(5) }, { 5, 7 }, '...with the child still folded')
press('<Tab>')
eq(vim.fn.foldclosed(5), -1, 'third TAB shows everything')
-- A single blank line before the next headline folds with the subtree,
-- as in Emacs (`org-cycle-separator-lines`).
vim.api.nvim_win_set_cursor(0, { 8, 0 })
press('<Tab>')
eq({ vim.fn.foldclosed(8), vim.fn.foldclosedend(8) }, { 8, 9 }, 'the blank line is folded into the subtree')
press('<Tab>')
eq(vim.fn.foldclosed(8), -1, 'and shown again')
press('<S-Tab>')
eq(vim.fn.foldclosed(2), 2, 'S-TAB folds the whole file')
press('<S-Tab>')
eq(vim.fn.foldclosed(2), -1, 'S-TAB again unfolds it')

-- 5. TODO cycling through the default key, from inside the entry's body.
vim.api.nvim_win_set_cursor(0, { 3, 10 })
press('cit')
wait('TODO -> DONE', function()
  return line(2) == tagged('* DONE First *bold*', ':tag:')
end)
eq(vim.api.nvim_win_get_cursor(0), { 3, 10 }, 'the cursor does not move')
-- `.` repeats the action.
press('.')
wait('DONE -> none', function()
  return line(2) == tagged('* First *bold*', ':tag:')
end)
press('ciT')
wait('none -> DONE (backward)', function()
  return line(2) == tagged('* DONE First *bold*', ':tag:')
end)
-- On an empty line, where `g@l` cannot run.
vim.api.nvim_win_set_cursor(0, { 4, 0 })
eq(line(4), '', 'line 4 is empty')
press('cit')
wait('DONE -> none from an empty line', function()
  return line(2) == tagged('* First *bold*', ':tag:')
end)
-- A headline with wide characters, to exercise position encoding in edits.
vim.api.nvim_win_set_cursor(0, { 10, 0 })
press('cit')
wait('keyword inserted before wide characters', function()
  return line(10) == '* TODO Ünïcödé 😀 heading'
end)
-- Tokens follow the edit.
wait('tokens after the edit', function()
  local token = token_at(9, 2)
  return token ~= nil and token.type == 'todoKeyword'
end)
-- Nothing happens before the first headline.
vim.api.nvim_win_set_cursor(0, { 1, 0 })
press('cit')
vim.wait(200)
eq(line(1), '#+TITLE: Sample', 'no action outside a headline')

-- 6. The outline.
local response = client:request_sync('textDocument/documentSymbol', { textDocument = vim.lsp.util.make_text_document_params(bufnr) }, 2000, bufnr)
local symbols = response and response.result or {}
eq(vim.tbl_map(function(symbol) return symbol.name end, symbols), { 'First *bold*', 'Second', 'Ünïcödé 😀 heading' }, 'top-level outline')
eq(symbols[1].children[1].name, 'Child', 'nested outline')
eq(symbols[3].detail, 'TODO', 'outline detail shows the keyword')

-- 7. The plugin can locate the server.
eq(require('org-lsp').server_command(), { server }, 'server command')

-- 8. Keys come from the server's table and can be replaced or removed.
local map = vim.fn.maparg('cit', 'n', false, true)
eq({ map.buffer, map.desc }, { 1, 'Change the TODO state' }, 'cit is buffer-local and described by the server')
require('org-lsp').setup({ mappings = { org = { ['org.todo'] = 'gt', ['org.todo.previous'] = false } } })
local other = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* TODO other' }, other)
vim.cmd.edit(other)
wait('the server to attach to a second file', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
eq(vim.fn.maparg('gt', 'n', false, true).desc, 'Change the TODO state', 'a configured key replaces the default')
eq(vim.fn.maparg('cit', 'n'), '', 'the default key is not set then')
eq(vim.fn.maparg('ciT', 'n'), '', 'false removes a key')
press('gt')
wait('the configured key to work', function()
  return line(1) == '* DONE other'
end)

-- 9. Priorities, with `.` to go on.
local dated = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* TODO Call them', 'SCHEDULED: <2026-05-18 Mon 10:58 +1w>', 'Version 7 of the plan.', '', '* Next' }, dated)
vim.cmd.edit(dated)
wait('the server to attach to the dated file', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
local function becomes(number, text, message)
  wait(message .. ': `' .. text .. '`', function()
    -- Run keys that were fed while waiting, as the editor's main loop
    -- does when it is not busy running this script.
    vim.api.nvim_feedkeys('', 'x', false)
    return line(number) == text
  end)
end
vim.api.nvim_win_set_cursor(0, { 2, 3 })
press('ciR')
becomes(1, '* TODO [#B] Call them', 'priority from anywhere in the entry')
press('.')
becomes(1, '* TODO [#A] Call them', '`.` raises it again')
press('cir')
becomes(1, '* TODO [#B] Call them', 'and down')
-- `org-priority` asks which one; a space removes it.
do
  local input = vim.ui.input
  local prompts = {}
  local answers = { 'c', ' ', 'b' }
  vim.ui.input = function(opts, on_confirm)
    table.insert(prompts, opts.prompt)
    on_confirm(table.remove(answers, 1))
  end
  press('<Leader>o,')
  becomes(1, '* TODO [#C] Call them', 'the priority typed')
  press('<Leader>o,')
  becomes(1, '* TODO Call them', 'a space removes it')
  press('<Leader>o,')
  becomes(1, '* TODO [#B] Call them', 'and back')
  eq(prompts[1], 'Priority A-C, SPC to remove: ', 'Org\'s question')
  vim.ui.input = input
end

-- 10. Timestamps: the part under the cursor, with counts and `.`.
vim.api.nvim_win_set_cursor(0, { 2, 21 })
press('<C-a>')
becomes(2, 'SCHEDULED: <2026-05-19 Tue 10:58 +1w>', 'the day')
eq(vim.api.nvim_win_get_cursor(0), { 2, 21 }, 'the cursor stays on the day')
press('3<C-a>')
becomes(2, 'SCHEDULED: <2026-05-22 Fri 10:58 +1w>', 'a count')
press('.')
becomes(2, 'SCHEDULED: <2026-05-25 Mon 10:58 +1w>', '`.` repeats with the count')
press('2.')
becomes(2, 'SCHEDULED: <2026-05-27 Wed 10:58 +1w>', 'a count before `.` replaces it')
press('<C-x>')
becomes(2, 'SCHEDULED: <2026-05-26 Tue 10:58 +1w>', 'down')
vim.api.nvim_win_set_cursor(0, { 2, 30 })
press('<C-a>')
becomes(2, 'SCHEDULED: <2026-05-26 Tue 11:00 +1w>', 'minutes go to the next step of five')
vim.api.nvim_win_set_cursor(0, { 2, 34 })
press('<C-a>')
becomes(2, 'SCHEDULED: <2026-05-26 Tue 11:00 +2w>', 'the repeater')
vim.api.nvim_win_set_cursor(0, { 2, 13 })
press('<S-Up>')
becomes(2, 'SCHEDULED: <2026-05-27 Wed 11:00 +2w>', 'a day later from anywhere in the timestamp')
press('<S-Down>')
becomes(2, 'SCHEDULED: <2026-05-26 Tue 11:00 +2w>', 'and a day earlier')

-- 11. Away from a timestamp the keys keep their own meaning.
local warnings = {}
local notify = vim.notify
vim.notify = function(message)
  table.insert(warnings, message)
end
vim.api.nvim_win_set_cursor(0, { 3, 0 })
press('<C-a>')
becomes(3, 'Version 8 of the plan.', 'Ctrl-A still increments a number')
press('5<C-a>')
becomes(3, 'Version 13 of the plan.', 'with its count')
press('<C-x>')
becomes(3, 'Version 12 of the plan.', 'Ctrl-X still decrements')
press('.')
becomes(3, 'Version 11 of the plan.', 'and `.` repeats that')
eq(line(2), 'SCHEDULED: <2026-05-26 Tue 11:00 +2w>', 'the timestamp is left alone')
eq(warnings, {}, 'without a complaint from the server')
vim.notify = notify

-- 12. Commands that ask for a date open the calendar Emacs shows with the
-- prompt at the bottom, in normal mode; `i` there asks for the answer with
-- `vim.ui.input`.
local function prompt_window()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == 'org-date' then
      return win
    end
  end
end
local asked
local answer
local input = vim.ui.input
vim.ui.input = function(opts, on_confirm)
  asked = opts
  on_confirm(answer)
end
local function in_calendar()
  vim.api.nvim_feedkeys('', 'x', false)
  return prompt_window() ~= nil and vim.api.nvim_get_current_win() == prompt_window()
end
answer = '2026-06-03'
vim.api.nvim_win_set_cursor(0, { 1, 0 })
vim.cmd('startinsert')
press('<Leader>ois')
wait('the calendar', in_calendar)
eq(asked, nil, 'nothing is asked until `i`')
wait('normal mode in the calendar', function()
  return vim.api.nvim_get_mode().mode == 'n'
end)
press('i')
becomes(2, 'SCHEDULED: <2026-06-03 Wed +2w>', 'scheduled, the repeater kept')
eq({ asked.prompt, asked.default }, { 'SCHEDULED Date+time [2026-05-26]: ', '11:00' }, 'the prompt starts from the old time')
wait('the calendar to close with the answer', function()
  return prompt_window() == nil
end)
asked = nil
press('<Leader>oid')
wait('the calendar again', in_calendar)
press('q')
wait('the calendar to close', function()
  return prompt_window() == nil
end)
eq(line(2), 'SCHEDULED: <2026-06-03 Wed +2w>', 'a cancelled prompt changes nothing')
eq(asked, nil, 'and asked nothing')
answer = '2026-06-01 9:00'
vim.api.nvim_win_set_cursor(0, { 3, 0 })
press('<Leader>oi!')
wait('the calendar for a timestamp', in_calendar)
press('i')
becomes(3, '[2026-06-01 Mon 09:00]Version 11 of the plan.', 'an inactive timestamp')
wait('the cursor after it', function()
  return vim.deep_equal(vim.api.nvim_win_get_cursor(0), { 3, 22 })
end)

-- 13. The calendar Emacs shows with the prompt opens at the bottom. Its
-- keys move the selected day; RET takes it, `i` types an answer that ends
-- with it, `q` cancels.
local function on_prompt_day(day)
  return function()
    vim.api.nvim_feedkeys('', 'x', false)
    local win = prompt_window()
    if not win or vim.api.nvim_get_current_win() ~= win then
      return false
    end
    local row, col = unpack(vim.api.nvim_win_get_cursor(win))
    return vim.api.nvim_get_current_line():sub(col, col + 1) == day
  end
end
answer = nil
vim.api.nvim_win_set_cursor(0, { 1, 0 })
press('<Leader>ois')
wait('the calendar on the scheduled day', on_prompt_day(' 3'))
local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
eq(lines[1], '           May 2026                June 2026                July 2026', 'three months, as in Emacs')
eq(lines[#lines], 'SCHEDULED Date+time [2026-06-03]:    => <2026-06-03 Wed>', 'the minibuffer below')
eq(vim.api.nvim_win_get_height(0), #lines, 'as high as its text')
-- Its keys run at once, whatever longer mappings of the user's start
-- with them (`tn`).
eq(vim.fn.maparg('t', 'n', false, true).nowait, 1, 'the calendar keys do not wait')
press('l')
wait('a day later', on_prompt_day(' 4'))
press('<CR>')
becomes(2, 'SCHEDULED: <2026-06-04 Thu +2w>', 'the day taken with RET')
wait('the calendar to close', function()
  return prompt_window() == nil
end)
press('<Leader>ois')
wait('the calendar again', on_prompt_day(' 4'))
press('j')
wait('a week later', on_prompt_day('11'))
answer = '9:00'
press('i')
wait('the minibuffer prompt', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return asked.prompt == 'SCHEDULED Date+time [2026-06-04]: '
end)
becomes(2, 'SCHEDULED: <2026-06-11 Thu 09:00 +2w>', 'the typed time on the selected day')
wait('the calendar to close after typing', function()
  return prompt_window() == nil
end)
answer = nil
press('<Leader>ois')
wait('the calendar once more', function()
  return prompt_window() ~= nil
end)
press('q')
wait('the calendar to close when cancelled', function()
  return prompt_window() == nil
end)
eq(line(2), 'SCHEDULED: <2026-06-11 Thu 09:00 +2w>', 'cancelling changes nothing')
vim.ui.input = input

-- 14. With keys on the keywords, `org.todo` (`cit` by default, `gt` as
-- configured above) asks for one key (`org-todo`'s fast selection); a
-- state that records a note asks for its text.
local keyed = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '#+TODO: TODO(t) WAIT(w@/!) | DONE(d!)', '* TODO Call them' }, keyed)
vim.cmd.edit(keyed)
wait('the server to attach to the file with keys', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
local plugin = require('org-lsp')
local read_key = plugin.read_key
local shown, offered
plugin.read_key = function(prompt, choices)
  shown = prompt
  offered = vim.tbl_map(function(choice)
    return choice.key .. ' ' .. choice.label
  end, choices)
  return 'w'
end
local keyed_buf = vim.api.nvim_get_current_buf()
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('gt')
-- The note is written in a buffer of its own, as in Emacs's `*Org Note*`.
wait('the note buffer', function()
  return vim.fs.basename(vim.api.nvim_buf_get_name(0)) == 'CAPTURE-Note.org'
end)
eq(vim.api.nvim_buf_get_lines(0, 0, -1, false), { '# Insert note for state change from "TODO" to "WAIT".', '', '' }, 'what the note is for')
eq(vim.api.nvim_win_get_cursor(0), { 3, 0 }, 'the note goes on the third line')
vim.api.nvim_buf_set_lines(0, 2, 3, false, { 'Waiting for Bob' })
press('<C-c>')
wait('the note under the headline', function()
  local lines = vim.api.nvim_buf_get_lines(keyed_buf, 0, -1, false)
  return lines[2] == '* WAIT Call them'
    and (lines[3] or ''):match('^%- State "WAIT"       from "TODO"       %[.*%] \\\\$') ~= nil
end)
eq(vim.api.nvim_buf_get_lines(keyed_buf, 3, 4, false), { '  Waiting for Bob' }, 'with its text')
wait('the note buffer to close', function()
  return vim.api.nvim_get_current_buf() == keyed_buf
end)
eq(shown, 'Select a TODO state', 'what is asked')
eq(offered, { 't TODO', 'w WAIT', 'd DONE', '  clear' }, 'the keys offered')
-- A key that selects nothing changes nothing.
plugin.read_key = function()
  return 'x'
end
press('gt')
vim.wait(200)
eq(line(2), '* WAIT Call them', 'an unknown key quits')
plugin.read_key = read_key

-- 15. `<Leader>or` refiles the subtree under a target picked from a list.
local refiled = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* Inbox', '** Call Bob', 'Notes.', '* Projects', '** Alpha' }, refiled)
vim.cmd.edit(refiled)
wait('the server to attach to the file to refile in', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
local select = vim.ui.select
local offered, asked_to
vim.ui.select = function(items, opts, on_choice)
  offered = vim.tbl_map(opts.format_item, items)
  asked_to = opts.prompt
  on_choice(items[2])
end
vim.api.nvim_win_set_cursor(0, { 3, 0 })
press('<Leader>or')
becomes(4, '** Call Bob', 'the subtree under its new parent')
eq(vim.api.nvim_buf_get_lines(0, 0, -1, false), { '* Inbox', '* Projects', '** Alpha', '** Call Bob', 'Notes.' }, 'moved as the last child')
eq(offered, { 'Inbox', 'Projects' }, 'the top-level headlines of the file')
eq(asked_to, 'Refile subtree "Call Bob" to: ', 'what is refiled')
vim.ui.select = select

-- 16. A key is read with the choices shown in a window, which goes when
-- the key is typed.
local windows = #vim.api.nvim_list_wins()
local listed
local open_win = vim.api.nvim_open_win
vim.api.nvim_open_win = function(buf, enter, config)
  listed = { lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false), title = config.title }
  return open_win(buf, enter, config)
end
vim.api.nvim_feedkeys('m', 'n', false)
local key = plugin.read_key('Select a capture template', { { key = 't', label = 'Task' }, { key = 'm', label = 'Meetings...' } })
vim.api.nvim_open_win = open_win
eq(key, 'm', 'the key typed')
eq(listed, { lines = { '[t] Task', '[m] Meetings...' }, title = ' Select a capture template ' }, 'the choices shown')
eq(#vim.api.nvim_list_wins(), windows, 'the window of choices is closed')
vim.api.nvim_feedkeys(vim.keycode('<Esc>'), 'n', false)
eq(plugin.read_key('Select a TODO state', { { key = 't', label = 'TODO' } }), nil, '<Esc> gives up')

-- 18. What the server marks hidden is concealed: a link shows its
-- description, as `org-link-descriptive` shows it.
local linked = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* Read [[https://example.org][the site]] now' }, linked)
vim.cmd.edit(linked)
wait('the server to attach to the file with a link', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
eq(vim.wo.conceallevel, 2, 'conceallevel in Org windows')
local function concealed()
  local found = {}
  local client = vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' })[1]
  local namespace = vim.api.nvim_create_namespace('nvim.lsp.semantic_tokens:' .. client.id)
  for _, mark in ipairs(vim.api.nvim_buf_get_extmarks(0, namespace, 0, -1, { details = true })) do
    if mark[4].conceal then
      table.insert(found, vim.api.nvim_buf_get_text(0, mark[2], mark[3], mark[4].end_row, mark[4].end_col, {})[1])
    end
  end
  return found
end
wait('the link to be concealed', function()
  vim.cmd.redraw()
  return #concealed() == 2
end)
eq(concealed(), { '[[https://example.org][', ']]' }, 'the brackets and the target are concealed')

-- 19. Clocking: `<Leader>oxi` writes the clock line in a drawer, the
-- statusline shows the clock as Emacs's mode line does, `<Leader>oxo`
-- stops it.
local clocked = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* TODO Write it', ':PROPERTIES:', ':EFFORT: 1:00', ':END:' }, clocked)
vim.cmd.edit(clocked)
wait('the server to attach to the file to clock', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
eq(require('org-lsp').clock_status(), '', 'no clock runs')
press('<Leader>oxi')
wait('the clock line in its drawer', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return line(5) == ':LOGBOOK:' and (line(6) or ''):match('^CLOCK: %[%d+%-%d+%-%d+ %a+ %d+:%d+%]$') ~= nil
end)
wait('the clock in the statusline', function()
  return require('org-lsp').clock_status():match('^%[0:0%d/1:00%] %(Write it%)$') ~= nil
end)
press('<Leader>oxo')
wait('the clock to stop', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return (line(6) or ''):match('%]%-%-%[.*%] =>%s+%d+:%d%d$') ~= nil
end)
wait('the statusline to drop the clock', function()
  return require('org-lsp').clock_status() == ''
end)

-- 20. Indentation by the server: a line opened below a list item starts
-- at the item's text, as Org's RET has it, and `==` re-indents a line as
-- `org-indent-region` does. `gq` is left to wrap text.
local indented = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* Head', '- item', '     stray' }, indented)
vim.cmd.edit(indented)
wait('the server to attach to the file to indent', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
eq(vim.bo.indentexpr, "v:lua.require'org-lsp.indent'.expr()", 'indentexpr asks the server')
eq(vim.bo.formatexpr, '', 'gq wraps text')
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('obody<Esc>')
eq(line(3), '  body', 'a new line in a list item starts at its text')
vim.api.nvim_win_set_cursor(0, { 4, 0 })
press('==')
wait('the stray line to be re-indented', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return line(4) == '  stray'
end)

-- 21. A file with `org-indent-mode` shows indented by level, without
-- changing it: the text under a headline lines up with its title, and the
-- leading stars are hidden.
local virtual = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '#+STARTUP: indent', '* A', 'text', '** B' }, virtual)
vim.cmd.edit(virtual)
local virtual_ns = vim.api.nvim_create_namespace('org_lsp_virtual_indent')
local function prefixes()
  local found = {}
  for _, mark in ipairs(vim.api.nvim_buf_get_extmarks(0, virtual_ns, 0, -1, { details = true })) do
    local details = mark[4]
    if details.virt_text then
      table.insert(found, { mark[2], #details.virt_text[1][1] })
    elseif details.conceal then
      table.insert(found, { mark[2], 'star', mark[3] })
    end
  end
  return found
end
wait('the lines indented', function()
  return #prefixes() > 0
end)
eq(prefixes(), { { 2, 2 }, { 3, 1 }, { 3, 'star', 0 } }, 'the text below A at 2, B after 1 column, one star hidden')
eq(line(3), 'text', 'the file is not changed')

-- 22. `g?` lists the keys of the buffer, the user's own among them, in a
-- window that `q` closes.
local before_help = vim.api.nvim_get_current_win()
press('g?')
check(vim.api.nvim_win_get_config(0).relative ~= '', 'the help is a floating window')
local help = vim.api.nvim_buf_get_lines(0, 0, -1, false)
eq(help[1], 'Org file', 'the help starts with the keys of Org files')
check(vim.iter(help):any(function(text)
  return text:match('^  gt%s+Change the TODO state$') ~= nil
end), 'the key set in the configuration is listed with what it does:\n' .. table.concat(help, '\n'))
check(vim.iter(help):any(function(text)
  return text:match('^  g%?%s+Show the keys$') ~= nil
end), 'g? itself is listed')
check(vim.tbl_contains(help, 'Anywhere'), 'the global keys follow')
press('q')
eq(vim.api.nvim_get_current_win(), before_help, 'q closes the help')

-- 23. RET in insert mode is Org's: a headline splits with its tags kept,
-- a table goes to the next row, and in a source block's code it is RET.
local returns = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* TODO Write the report :work:', '| a | bb |', '#+begin_src sh', 'echo', '#+end_src' }, returns)
vim.cmd.edit(returns)
wait('the server for RET', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
vim.api.nvim_win_set_cursor(0, { 1, 11 })
press('a<CR>x<Esc>')
eq(vim.api.nvim_buf_get_lines(0, 0, 2, false), { '* TODO Write' .. (' '):rep(59) .. ':work:', 'xthe report' },
  'the title split, the tags kept in their column')
vim.api.nvim_win_set_cursor(0, { 3, 6 })
press('a<CR>y<Esc>')
eq(vim.api.nvim_buf_get_lines(0, 2, 4, false), { '| a | bb |', '|   | y   |' }, 'a new row, the cursor in its column')
vim.api.nvim_win_set_cursor(0, { 6, 3 })
press('a<CR>z<Esc>')
eq(vim.api.nvim_buf_get_lines(0, 5, 7, false), { 'echo', '  z' }, 'in code, RET is RET (indented as the block content)')

-- 24. `<Leader>o$` archives the subtree to the archive file, which is
-- written as Emacs saves it, and `<Leader>oA` tags a subtree as archived.
-- `org.archive.done` asks about each subtree without open TODO items.
local archived = vim.fn.tempname() .. '.org'
vim.fn.writefile({ 'Intro', '* DONE Old', 'Notes.', '* DONE Older', '* TODO New' }, archived)
vim.cmd.edit(archived)
wait('the server to attach to the file to archive in', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('<Leader>o$')
wait('the subtree archived', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return line(2) == '* DONE Older'
end)
local archive_lines = vim.fn.readfile(archived .. '_archive')
check(vim.tbl_contains(archive_lines, '* DONE Old') and vim.tbl_contains(archive_lines, ':ARCHIVE_CATEGORY: ' .. vim.fn.fnamemodify(archived, ':t:r')),
  'the archive file has the subtree and where it came from:\n' .. table.concat(archive_lines, '\n'))
vim.api.nvim_win_set_cursor(0, { 3, 0 })
press('<Leader>oA')
wait('the ARCHIVE tag', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return line(3):match(' :ARCHIVE:$') ~= nil
end)
press('<Leader>oA')
wait('the ARCHIVE tag gone', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return line(3) == '* TODO New'
end)
local asked_about = {}
plugin.read_key = function(prompt)
  table.insert(asked_about, prompt)
  return 'y'
end
vim.api.nvim_win_set_cursor(0, { 1, 0 })
plugin.run('org.archive.done')
wait('the answered subtree archived', function()
  vim.api.nvim_feedkeys('', 'x', false)
  return #vim.api.nvim_buf_get_lines(0, 0, -1, false) == 2
end)
plugin.read_key = read_key
eq(vim.api.nvim_buf_get_lines(0, 0, -1, false), { 'Intro', '* TODO New' }, 'the subtree said yes to went')
eq(asked_about, { 'Move subtree to archive? (no open TODO items)' }, 'one question, for the one subtree left')

-- 25. LSP range formatting (`textDocument/rangeFormatting`), as
-- `vim.lsp.buf.format()` asks for it with a range or in Visual mode: the
-- tables with a line in the range are aligned and its headlines' tags go
-- to their column; what is outside the range stays.
local ranged = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* A :x:', '| a | bbb |', '|cc|d|', '* B :y:', '|e|f|' }, ranged)
vim.cmd.edit(ranged)
wait('the server to attach to the file to format', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
vim.lsp.buf.format({ range = { start = { 2, 0 }, ['end'] = { 3, 0 } } })
eq(vim.api.nvim_buf_get_lines(0, 0, -1, false), { '* A :x:', '| a  | bbb |', '| cc | d   |', '* B :y:', '|e|f|' },
  'the table in the range aligned, the rest as it was')
vim.api.nvim_win_set_cursor(0, { 4, 0 })
vim.cmd('normal! Vj')
vim.lsp.buf.format()
vim.cmd('normal! \27')
eq(vim.api.nvim_buf_get_lines(0, 3, 5, false), { tagged('* B', ':y:'), '| e | f |' }, 'the selected lines formatted')
eq(line(1), '* A :x:', 'the headline before the selection as it was')

-- 17. `false` for a scope sets none of its keys, the plugin's own included.
require('org-lsp').setup({ mappings = { org = false } })
local bare = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* TODO bare' }, bare)
vim.cmd.edit(bare)
wait('the server to attach to a file without keys', function()
  return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) == 1
end)
eq({ vim.fn.maparg('gt', 'n'), vim.fn.maparg('cir', 'n'), vim.fn.maparg('<Tab>', 'n') }, { '', '', '' }, 'no keys in Org files')
eq(vim.fn.maparg('<Leader>oa', 'n', false, true).buffer, 0, 'global keys stay')

print(('ok: %d checks passed'):format(checks))
vim.cmd('qall!')
