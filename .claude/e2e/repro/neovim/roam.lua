-- TESTING.org: Notes (org-roam) and A journal (org-journal), with their
-- settings on.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local base = H.tmpdir('roam')
local roam, journal = base .. '/roam', base .. '/journal'
vim.fn.mkdir(roam .. '/daily', 'p')
vim.fn.mkdir(journal, 'p')
vim.fn.writefile({ ':PROPERTIES:', ':ID:       note-a', ':ROAM_ALIASES: "Alias A"', ':END:', '#+title: Note A', '', 'Text of A.' }, roam .. '/a.org')
vim.fn.writefile({ ':PROPERTIES:', ':ID:       note-b', ':END:', '#+title: Note B', '', 'Links to [[id:note-a][Note A]] here.', 'Mentions Note A without a link.', '[[roam:Note A]]', '', '* Head', ':PROPERTIES:', ':ID:       head-b', ':END:' }, roam .. '/b.org')
vim.fn.writefile({ ':PROPERTIES:', ':ID:       d20', ':END:', '#+title: 2026-10-20', '' }, roam .. '/daily/2026-10-20.org')
-- journal: yesterday with a TODO, a future day
vim.fn.writefile({ '* Tuesday, 10/20/26', '** 09:00 standup', '** TODO carried', 'a note' }, journal .. '/20261020')
vim.fn.writefile({ '* Friday, 10/23/26', '** TODO future thing', '<2026-10-23 Fri>' }, journal .. '/20261023')
H.init({
  settings = {
    clock = { today = '2026-10-21', now = '10:30' },
    org_roam_enabled = true,
    org_roam_directory = roam,
    org_roam_mode_sections = { 'backlinks', 'reflinks', 'unlinked-references' },
    org_journal_enabled = true,
    org_journal_dir = journal,
    org_journal_enable_agenda_integration = true,
    org_journal_follow_mode = true,
    org_agenda_files = { base .. '/agenda' },
  },
})
vim.fn.mkdir(base .. '/agenda', 'p')
local eq, check, press, line, lines, wait = H.eq, H.check, H.press, H.line, H.lines, H.wait
local function name() return vim.api.nvim_buf_get_name(0) end
local function has(p, b) return vim.iter(H.lines(b)):any(function(l) return l:find(p) end) end

H.section('roam find/insert/random/alias/tag', function()
  H.open(roam .. '/b.org')
  H.inputs({ 'Alias A' })
  press('<Leader>nf')
  wait('visited by alias', function() return name():find('/a%.org$') end)
  H.out('  offered: ' .. vim.inspect(H.plugin.completions))
  check(vim.tbl_contains(H.plugin.completions or {}, 'Alias A'), 'aliases offered')
  -- insert on a selection
  vim.api.nvim_buf_set_lines(0, -1, -1, false, { 'Note B' })
  local last = vim.api.nvim_buf_line_count(0)
  vim.api.nvim_win_set_cursor(0, { last, 0 })
  H.inputs({ 'Note B' })
  press('v$h<Leader>ni')
  wait('selection replaced by a link', function() return line(last) == '[[id:note-b][Note B]]' end)
  H.out('  insert on selection: ' .. line(last) .. ' asked ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]))
  -- random
  vim.cmd('Org roam node random')
  H.sleep(1000)
  check(name():find('%.org$') ~= nil, 'random node visited: ' .. name())
  -- alias remove
  H.open(roam .. '/a.org')
  H.selects({ 'Alias A' })
  H.inputs({ 'Alias A' })
  vim.cmd('Org roam alias remove')
  wait('alias removed', function() return not has('ROAM_ALIASES') or not has('Alias A') end)
  -- tag add on file node
  H.inputs({ 'tagx' })
  vim.api.nvim_win_set_cursor(0, { 6, 0 })
  vim.cmd('Org roam tag add')
  wait('#+filetags', function() return has('^#%+filetags: :tagx:') end)
  -- tag add on headline node
  H.open(roam .. '/b.org')
  vim.api.nvim_win_set_cursor(0, { 11, 0 })
  H.inputs({ 'heady' })
  vim.cmd('Org roam tag add')
  wait('headline tag', function() return line(10):find(':heady:$') end)
end)

H.section('roam: links', function()
  H.open(roam .. '/b.org')
  vim.api.nvim_win_set_cursor(0, { 8, 3 })
  press('<Leader>oo')
  wait('roam link followed', function() return name():find('/a%.org$') end)
  vim.cmd('buffer ' .. vim.fn.bufnr(roam .. '/b.org'))
  wait('link converted', function() return line(8) == '[[id:note-a][Note A]]' end)
  -- completion of [[roam:
  vim.api.nvim_buf_set_lines(0, -1, -1, false, { 'x [[roam:' })
  local n = vim.api.nvim_buf_line_count(0)
  local c = H.client()
  local r = c:request_sync('textDocument/completion', { textDocument = { uri = vim.uri_from_bufnr(0) }, position = { line = n - 1, character = 9 } }, 3000, 0)
  local items = r and r.result and (r.result.items or r.result) or {}
  H.out('  [[roam: completion: ' .. vim.inspect(vim.tbl_map(function(i) return { i.label, i.textEdit and i.textEdit.newText or i.insertText } end, items)))
  check(#items > 0, 'nodes offered after [[roam:')
  vim.api.nvim_buf_set_lines(0, n - 1, n, false, {})
  vim.cmd('silent write')
end)

H.section('roam buffer', function()
  H.open(roam .. '/a.org')
  local here = vim.api.nvim_get_current_win()
  press('<Leader>nl')
  local rw
  wait('roam buffer', function()
    for _, w in ipairs(vim.api.nvim_list_wins()) do
      if vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(w)):find('org%-roam%-buffer') then rw = w return true end
    end
  end)
  eq(vim.api.nvim_get_current_win(), here, 'cursor stays in the note')
  local rl = H.lines(vim.api.nvim_win_get_buf(rw))
  H.out(table.concat(rl, '\n'))
  check(vim.iter(rl):any(function(l) return l:find('Backlinks %(%d%)') end), 'Backlinks (n)')
  check(vim.iter(rl):any(function(l) return l:find('Unlinked References') end), 'Unlinked References section')
  -- follow an item
  vim.api.nvim_set_current_win(rw)
  for i, l in ipairs(H.lines()) do if l:find('Note B') and i > 3 then vim.api.nvim_win_set_cursor(0, { i, 0 }) break end end
  local item = vim.api.nvim_get_current_line()
  press('<Leader>oo')
  H.sleep(1500)
  H.out('  followed item "' .. item .. '" -> ' .. name() .. ':' .. H.cursor()[1] .. ' ' .. tostring(vim.api.nvim_get_current_line()))
  check(name():find('/b%.org$') and vim.api.nvim_get_current_line():find('note%-a') ~= nil, 'item visits the line of the link')
  -- moving into another node updates the buffer
  vim.api.nvim_win_set_cursor(0, { 11, 0 })
  H.press('j')
  H.press('k')
  H.sleep(1500)
  if vim.api.nvim_win_is_valid(rw) then
    local nl = H.lines(vim.api.nvim_win_get_buf(rw))
    H.out('  after moving into * Head: ' .. tostring(nl[1]))
    check(nl[1] and nl[1]:find('Head') ~= nil, 'roam buffer follows the node at the cursor')
  end
  press('<Leader>nl')
  H.sleep(1000)
  -- dedicated
  H.inputs({ 'Note A' })
  vim.cmd('Org roam buffer dedicated')
  H.sleep(1500)
  local ded
  for _, w in ipairs(vim.api.nvim_list_wins()) do
    local bn = vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(w))
    if bn:find('roam') and not bn:find('%.org$') or bn:find('Note A') or bn:find('dedicated') then ded = bn end
  end
  H.out('  dedicated: ' .. tostring(ded) .. ' wins ' .. vim.inspect(vim.tbl_map(function(w) return vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(w)) end, vim.api.nvim_list_wins())))
end)

H.section('dailies', function()
  H.open(roam .. '/a.org')
  press('<Leader>ndd')
  wait('today daily', function() return name():find('daily/2026%-10%-21%.org$') end)
  H.out('  today: ' .. vim.inspect(lines()))
  check(has('^:ID:') and has('^#%+title: 2026%-10%-21'), 'ID and title')
  press('<Leader>ndf')
  H.sleep(1000)
  H.out('  next from today: ' .. name() .. ' notes ' .. vim.inspect(H.notes))
  press('<Leader>ndb')
  wait('previous daily', function() return name():find('2026%-10%-20%.org$') end)
  press('<Leader>ndf')
  wait('next daily', function() return name():find('2026%-10%-21%.org$') end)
  -- capture to today
  H.keys({ 'd' })
  press('<Leader>ndn')
  wait('daily capture', function() return vim.fs.basename(name()):find('^CAPTURE') end)
  H.out('  capture buffer: ' .. vim.inspect(lines()))
  vim.api.nvim_buf_set_lines(0, -1, -1, false, { 'captured daily' })
  press('<C-c>')
  wait('captured to daily', function() return vim.iter(vim.fn.readfile(roam .. '/daily/2026-10-21.org')):any(function(l) return l:find('captured daily') end) or has('captured daily', vim.fn.bufnr(roam .. '/daily/2026-10-21.org')) end)
  -- goto date: calendar with Find daily-note and marks
  press('<Leader>ndc')
  wait('calendar', function() return H.calendar_win() ~= nil end)
  H.sleep(500)
  local cl = H.lines(vim.api.nvim_win_get_buf(H.calendar_win()))
  H.out(table.concat(cl, '\n'))
  check(cl[#cl]:find('^Find daily%-note') ~= nil, 'Find daily-note: prompt')
  local cb = vim.api.nvim_win_get_buf(H.calendar_win())
  local marks = vim.api.nvim_buf_get_extmarks(cb, -1, 0, -1, { details = true })
  local tokens = {}
  local cc = H.client()
  local tok = cc:request_sync('textDocument/semanticTokens/full', { textDocument = { uri = vim.uri_from_bufnr(cb) } }, 3000, cb)
  H.out('  calendar marks: ' .. #marks .. ' tokens ' .. tostring(tok and tok.result and #tok.result.data))
  press('h')
  H.sleep(300)
  press('<CR>')
  wait('daily of 20', function() return name():find('2026%-10%-20%.org$') end)
  -- directory
  H.selects({ '2026-10-21.org' })
  press('<Leader>nd.')
  H.sleep(1500)
  H.out('  directory asked: ' .. vim.inspect(H.select_asked[#H.select_asked]) .. ' -> ' .. name())
end)

H.section('journal', function()
  H.open(roam .. '/a.org')
  press('<Leader>jj')
  wait('today journal', function() return name():find('/20261021$') end)
  wait('server', function() return #vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' }) > 0 end)
  H.out('  journal: ' .. vim.inspect(lines()) .. ' cursor ' .. vim.inspect(H.cursor()))
  eq(line(1), '* Wednesday, 10/21/26', 'date heading')
  check(has('TODO carried'), 'yesterday TODO moved')
  check(line(#lines()):find('^%*%* 10:30 ?$') ~= nil, 'entry with the time')
  wait('other entries folded', function() return vim.fn.foldclosed(2) == 2 end)
  -- previous on the first entry of the day
  press('<Leader>jb')
  H.sleep(800)
  H.out('  jb -> ' .. name() .. ' notes ' .. vim.inspect(H.notes_since(0)))
  press('<Leader>jb')
  H.sleep(800)
  local n = #H.notes
  press('<Leader>jb')
  H.sleep(800)
  H.out('  jb on first: ' .. vim.inspect(H.notes_since(n)))
  check(H.note_seen('No journal entry before this one', n), 'No journal entry before this one')
  -- date entry
  vim.cmd('Org journal new date entry')
  wait('calendar', function() return H.calendar_win() ~= nil end)
  H.sleep(500)
  local cb = vim.api.nvim_win_get_buf(H.calendar_win())
  H.out(table.concat(H.lines(cb), '\n'))
  vim.api.nvim_set_current_win(H.calendar_win())
  press(']')
  H.sleep(800)
  H.out('  ] in calendar: ' .. H.lines(cb)[#H.lines(cb)] .. ' wins ' .. #vim.api.nvim_list_wins() .. ' cur ' .. vim.api.nvim_buf_get_name(0))
  check(vim.api.nvim_get_current_buf() == cb, 'follow mode: cursor stays in the calendar')
  H.inputs({ '2026-10-25' })
  vim.api.nvim_set_current_win(H.calendar_win())
  press('i')
  wait('date entry file', function() return name():find('/20261025$') end)
  H.out('  date entry: ' .. vim.inspect(lines()))
  check(line(#lines()):match('^%*%* ?$') ~= nil or line(#lines()) == '** ', 'entry without the time')
  -- scheduled entry
  H.inputs({ 'Dentist' })
  vim.cmd('Org journal new scheduled entry')
  H.sleep(500)
  if H.calendar_win() then
    vim.api.nvim_set_current_win(H.calendar_win())
    H.inputs({ '+1d', 'Dentist' })
    press('i')
  end
  H.sleep(2000)
  H.out('  scheduled entry: ' .. name() .. ' ' .. vim.inspect(lines()) .. ' asked ' .. vim.inspect(vim.list_slice(H.inputs_asked, #H.inputs_asked - 2)))
  -- search future scheduled, schedule view
  vim.cmd('Org journal search future scheduled')
  H.sleep(2000)
  H.out('  future scheduled: ' .. name() .. ' ' .. vim.inspect(vim.list_slice(lines(), 1, 6)))
  vim.cmd('Org journal schedule view')
  H.sleep(2000)
  H.out('  schedule view: ' .. name() .. ' ' .. vim.inspect(vim.list_slice(lines(), 1, 8)))
  -- agenda integration
  vim.cmd('Org agenda list')
  wait('agenda', function() return vim.bo.filetype == 'org-agenda' end)
  H.sleep(500)
  H.out(table.concat(lines(), '\n'))
  check(has('carried') or has('future thing'), 'journal TODOs in the week view')
end)

H.done()
