-- TESTING.org: Clocking, Capture, Refile and archive, Links, Footnotes,
-- Notes and logging.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('misc')
vim.fn.writefile({ 'snippet text' }, dir .. '/snippet.txt')
vim.fn.writefile({ 'notes txt' }, dir .. '/notes.txt')
H.init({
  settings = {
    clock = { today = '2026-10-21', now = '10:30' },
    org_agenda_files = { dir },
    org_log_done = 'time',
    org_log_refile = os.getenv('LOG_REFILE') or 'time',
    org_log_reschedule = 'time',
    org_link_abbrev_alist = { ddg = 'https://duckduckgo.com/?q=%s' },
    org_file_apps = { { 'txt', 'cp %s copy.txt' } },
    org_capture_templates = {
      { key = 't', description = 'Task', target = { 'file+headline', dir .. '/inbox.org', 'Tasks' }, template = '* TODO %?\n  %U' },
      { key = 'n', description = 'Named', target = { 'file', dir .. '/inbox.org' }, template = '* %^{Name}\n%a\n%c\n%[snippet.txt]' },
      { key = 'd', description = 'Diary', target = { 'file+olp+datetree', dir .. '/diary.org' }, template = '* %?' },
      { key = 'e', description = 'Elisp', target = { 'file', dir .. '/inbox.org' }, template = '* %(upcase "abc")' },
    },
  },
})
local eq, check, press, line, lines, wait = H.eq, H.check, H.press, H.line, H.lines, H.wait
local function has(pattern, buf)
  return vim.iter(H.lines(buf)):any(function(l) return l:find(pattern) end)
end
local function disk(path)
  local b = vim.fn.bufnr(path)
  if b > 0 and vim.api.nvim_buf_is_loaded(b) then return H.lines(b) end
  return vim.fn.filereadable(path) == 1 and vim.fn.readfile(path) or {}
end

H.section('clocking', function()
  H.open(dir .. '/clock.org', { '* TODO Task', ':PROPERTIES:', ':Effort: 0:30', ':END:', '* Other', '** Sub' })
  local n = #H.notes
  press('<Leader>oxi')
  wait('CLOCK line in LOGBOOK', function() return has('^CLOCK: %[2026%-10%-21 Wed 10:30%]$') and has(':LOGBOOK:') end)
  H.out('  clock in: ' .. vim.inspect(lines()) .. ' status=' .. H.plugin.clock_status() .. ' notes ' .. vim.inspect(H.notes_since(n)))
  check(H.plugin.clock_status():find('Task') ~= nil, 'statusline clock: ' .. H.plugin.clock_status())
  press('<Leader>oxo')
  wait('clocked out', function() return has('^CLOCK: %[2026%-10%-21 Wed 10:30%]%-%-%[2026%-10%-21 Wed 10:30%] =>  0:00$') end)
  eq(H.plugin.clock_status(), '', 'no clock in statusline after out')
  press('<Leader>oxi')
  H.sleep(500)
  press('<Leader>oxq')
  wait('cancelled', function()
    local c = 0
    for _, l in ipairs(lines()) do if l:find('^CLOCK:') then c = c + 1 end end
    return c == 1
  end)
  H.out('  after cancel: ' .. vim.inspect(H.notes_since(n)))
  -- goto from another file
  press('<Leader>oxi')
  H.sleep(500)
  H.open(dir .. '/other.org', { '* X' })
  press('<Leader>oxj')
  wait('goto clocked entry', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)) == 'clock.org' and line(H.cursor()[1]) == '* TODO Task' end)
  press('<Leader>oxo')
  H.sleep(500)
  -- clock in last
  vim.api.nvim_win_set_cursor(0, { 6, 0 })
  press('<Leader>oxx')
  H.sleep(800)
  H.out('  oxx: ' .. vim.inspect(H.notes_since(n)) .. ' status ' .. H.plugin.clock_status())
  check(H.plugin.clock_status():find('Task') ~= nil, 'clock in last: Task again')
  press('<Leader>oxo')
  H.sleep(500)
  -- display
  press('<Leader>oxd')
  H.sleep(800)
  local hints = vim.lsp.inlay_hint.get({ bufnr = 0 })
  H.out('  oxd hints: ' .. vim.inspect(vim.tbl_map(function(h) return h.inlay_hint.label end, hints)) .. ' notes ' .. vim.inspect(H.notes_since(n)))
  -- clock table
  vim.api.nvim_win_set_cursor(0, { 4, 0 })
  press('o<Esc>')
  H.sleep(500)
  press('<Leader>oxr')
  wait('clock table', function() return has('^#%+BEGIN: clocktable') end)
  local row
  for i, l in ipairs(lines()) do if l:find('^#%+BEGIN: clocktable') then row = i end end
  H.out('  clocktable: ' .. vim.inspect(vim.list_slice(lines(), row, row + 8)))
  vim.api.nvim_win_set_cursor(0, { row, 2 })
  press('<Leader>oxu')
  H.sleep(800)
  -- shift block on BEGIN line with S-Right? (":block today" -> ":block yesterday" etc.)
  vim.api.nvim_buf_set_lines(0, row - 1, row, false, { '#+BEGIN: clocktable :scope file :maxlevel 2 :block today' })
  H.sleep(300)
  vim.api.nvim_win_set_cursor(0, { row, 50 })
  press('<S-Left>')
  wait('block shifted', function() return line(row):find(':block 2026%-10%-20') or line(row):find(':block yesterday') end, 4000)
  H.out('  after S-Left on BEGIN: ' .. line(row))
end)

H.section('clock effort exceeded and dangling', function()
  H.open(dir .. '/eff.org', { '* TODO Long', ':PROPERTIES:', ':Effort: 0:30', ':END:', ':LOGBOOK:', 'CLOCK: [2026-10-21 Wed 09:00]--[2026-10-21 Wed 10:00] =>  1:00', ':END:', '* TODO Dangling', ':LOGBOOK:', 'CLOCK: [2026-10-21 Wed 10:00]', ':END:' })
  local n = #H.notes
  H.keys({ 'K', '\r' })
  H.inputs({ '' })
  press('<Leader>oxz')
  H.sleep(1500)
  H.out('  resolve: keys ' .. vim.inspect(H.key_prompts[#H.key_prompts]) .. ' inputs ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]) .. ' notes ' .. vim.inspect(H.notes_since(n)))
  H.out('  after resolve: ' .. vim.inspect(lines()))
  check(H.key_prompts[#H.key_prompts] and H.key_prompts[#H.key_prompts].prompt:find('Dangling clock started %d+ mins ago') ~= nil, 'asks about the dangling clock')
  n = #H.notes
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  press('<Leader>oxi')
  H.sleep(1500)
  H.out('  effort passed: ' .. vim.inspect(H.notes_since(n)))
  check(H.note_seen('should be finished by now', n), 'Task ‘Long’ should be finished by now. (0:30)')
  press('<Leader>oxo')
  H.sleep(500)
end)

H.section('capture', function()
  H.open(dir .. '/src.org', { '* Source heading', 'some text' })
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  H.keys({ 't' })
  press('<Leader>oc')
  wait('capture buffer', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)):find('^CAPTURE') ~= nil end)
  H.out('  menu: ' .. vim.inspect(H.key_prompts[#H.key_prompts].choices))
  H.out('  capture buffer: ' .. vim.inspect(lines()) .. ' cursor ' .. vim.inspect(H.cursor()) .. ' mode ' .. vim.api.nvim_get_mode().mode)
  eq(H.cursor(), { 1, 7 }, 'cursor at %?')
  vim.api.nvim_buf_set_text(0, 0, 7, 0, 7, { 'Captured task' })
  press('<C-c>')
  wait('captured into inbox', function() return has('^%*%* TODO Captured task', vim.fn.bufadd(dir .. '/inbox.org')) or vim.iter(disk(dir .. '/inbox.org')):any(function(l) return l:find('TODO Captured task') end) end)
  H.out('  inbox: ' .. vim.inspect(disk(dir .. '/inbox.org')))
  -- abort
  H.keys({ 't' })
  press('<Leader>oc')
  wait('capture buffer 2', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)):find('^CAPTURE') ~= nil end)
  local before = table.concat(disk(dir .. '/inbox.org'), '\n')
  press('<Leader>ok')
  wait('aborted', function() return not vim.fs.basename(vim.api.nvim_buf_get_name(0)):find('^CAPTURE') end)
  eq(table.concat(disk(dir .. '/inbox.org'), '\n'), before, 'abort leaves the target unchanged')
  -- %^{Name}, %a, %c, %[file]
  H.open(dir .. '/src.org')
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  vim.fn.setreg('a', 'from register a')
  H.keys({ 'n' })
  H.inputs({ 'My name' })
  vim.api.nvim_feedkeys('"a', 'n', false)
  press('<Leader>oc')
  wait('capture n', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)):find('^CAPTURE') ~= nil end)
  H.out('  template n: ' .. vim.inspect(lines()) .. ' asked ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]))
  check(has('^%* My name'), '%^{Name}')
  check(has('%[%[file:.*src%.org::'), '%a links back')
  check(has('from register a'), '%c is the register typed')
  check(has('snippet text'), '%[snippet.txt]')
  press('<Leader>ok')
  H.sleep(500)
  -- datetree
  H.keys({ 'd' })
  press('<Leader>oc')
  wait('capture d', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)):find('^CAPTURE') ~= nil end)
  vim.api.nvim_buf_set_text(0, 0, 2, 0, 2, { 'Diary entry' })
  press('<C-c>')
  wait('diary', function() return vim.iter(disk(dir .. '/diary.org')):any(function(l) return l:find('Diary entry') end) end)
  H.out('  diary: ' .. vim.inspect(disk(dir .. '/diary.org')))
  eq(vim.list_slice(disk(dir .. '/diary.org'), 1, 3), { '* 2026', '** 2026-10 October', '*** 2026-10-21 Wednesday' }, 'date tree')
  -- %(upcase) via Emacs
  H.keys({ 'e' })
  press('<Leader>oc')
  wait('capture e', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)):find('^CAPTURE') ~= nil end, 20000)
  H.out('  elisp template: ' .. vim.inspect(lines()))
  check(has('ABC'), '%(upcase "abc") -> ABC')
  press('<Leader>ok')
  H.sleep(500)
  -- refile from capture buffer
  H.keys({ 't' })
  press('<Leader>oc')
  wait('capture t again', function() return vim.fs.basename(vim.api.nvim_buf_get_name(0)):find('^CAPTURE') ~= nil end)
  vim.api.nvim_buf_set_text(0, 0, 7, 0, 7, { 'Refiled from capture' })
  H.selects({ 'Source heading' })
  press('<Leader>or')
  wait('refiled from capture into src.org', function() return vim.iter(disk(dir .. '/src.org')):any(function(l) return l:find('Refiled from capture') end) end)
  H.out('  src.org: ' .. vim.inspect(disk(dir .. '/src.org')) .. ' asked ' .. vim.inspect(H.select_asked[#H.select_asked]))
end)

H.section('refile and archive', function()
  H.open(dir .. '/r.org', { '* Inbox', '** Item one', 'text', '** Item two', '* Projects', '** Alpha', '* DONE Old', '* Done two', '* Archive :ARCHIVE:' })
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  H.selects({ 'Projects' })
  press('<Leader>oy')
  wait('copied', function()
    local c = 0
    for _, l in ipairs(lines()) do if l == '** Item one' then c = c + 1 end end
    return c == 2
  end)
  H.out('  after copy: ' .. vim.inspect(lines()))
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  H.selects({ 'Projects' })
  press('<Leader>or')
  wait('refiled', function() return line(2) == '** Item two' end)
  H.out('  after refile: ' .. vim.inspect(lines()))
  check(has('Refiled on %['), 'org_log_refile time: Refiled on […]')
  H.selects({ 'Inbox' })
  vim.cmd('Org refile goto')
  wait('goto target', function() return line(H.cursor()[1]) == '* Inbox' end)
  vim.cmd('Org refile goto last')
  H.sleep(800)
  H.out('  goto last -> ' .. line(H.cursor()[1]))
  -- refile to another file
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  H.selects({ 'Tasks' })
  press('<Leader>or')
  H.sleep(1200)
  H.out('  refile to other file asked: ' .. vim.inspect(H.select_asked[#H.select_asked]))
  -- archive
  local n = #H.notes
  local row
  for i, l in ipairs(lines()) do if l == '* DONE Old' then row = i end end
  vim.api.nvim_win_set_cursor(0, { row, 0 })
  press('<Leader>o$')
  wait('archived', function() return vim.iter(disk(dir .. '/r.org_archive')):any(function(l) return l:find('DONE Old') end) end)
  H.out('  archive file: ' .. vim.inspect(disk(dir .. '/r.org_archive')) .. ' notes ' .. vim.inspect(H.notes_since(n)))
  check(vim.iter(disk(dir .. '/r.org_archive')):any(function(l) return l:find(':ARCHIVE_TIME:') end), 'ARCHIVE_ properties')
  for i, l in ipairs(lines()) do if l == '* Done two' then row = i end end
  vim.api.nvim_win_set_cursor(0, { row, 0 })
  press('<Leader>oxA')
  wait('archive sibling', function() return has('^%* Archive') and has('^%*%* Done two') end)
  H.out('  after sibling: ' .. vim.inspect(lines()))
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  press('<Leader>oA')
  wait('ARCHIVE tag', function() return line(1):find(':ARCHIVE:$') end)
  press('<Leader>oA')
  wait('ARCHIVE tag off', function() return not line(1):find(':ARCHIVE:') end)
  -- archive done with confirmation
  H.open(dir .. '/ad.org', { '* DONE a', '* TODO b', '* DONE c' })
  H.inputs({ 'y', 'y' })
  H.keys({ 'y', 'y' })
  vim.cmd('Org archive done')
  wait('archive done', function() return #lines() == 1 end, 8000)
  H.out('  archive done: ' .. vim.inspect(lines()) .. ' asked ' .. vim.inspect(H.key_prompts[#H.key_prompts]) .. vim.inspect(H.inputs_asked[#H.inputs_asked]) .. ' notes ' .. vim.inspect(H.notes_since(n)))
end)

H.section('links', function()
  H.open(dir .. '/l.org', {
    '#+LINK: gh https://github.com/%s',
    '* Links',
    '[[gh:org-lsp]] [[ddg:org]] [[Target]] [[#cid]] [[target]] [[file:other.org::*X]]',
    '[[No such heading]] [[file:notes.txt]] [[shell:echo hi]]',
    '* Target',
    ':PROPERTIES:',
    ':CUSTOM_ID: cid',
    ':END:',
    '<<target>> here',
  })
  local opened = {}
  local ui_open = vim.ui.open
  vim.ui.open = function(url) table.insert(opened, url); return nil, nil end
  local function follow(col, row)
    vim.api.nvim_win_set_cursor(0, { row or 3, col })
    press('<Leader>oo')
    H.sleep(700)
  end
  follow(3)
  eq(opened[#opened], 'https://github.com/org-lsp', '#+LINK abbreviation')
  follow(17)
  eq(opened[#opened], 'https://duckduckgo.com/?q=org', 'org_link_abbrev_alist')
  follow(30)
  eq(line(H.cursor()[1]), '* Target', 'link to headline')
  follow(42)
  eq(line(H.cursor()[1]), '* Target', 'custom id')
  follow(50)
  eq(line(H.cursor()[1]), '<<target>> here', 'target')
  H.out('  cursor at target: ' .. vim.inspect(H.cursor()))
  follow(62)
  H.out('  file:other.org::*X -> ' .. vim.api.nvim_buf_get_name(0) .. ' ' .. tostring(line(H.cursor()[1])))
  if vim.fs.basename(vim.api.nvim_buf_get_name(0)) == 'other.org' then
    eq(line(H.cursor()[1]), '* X', 'file link with search')
    vim.cmd('buffer ' .. vim.fn.bufnr(dir .. '/l.org'))
    H.sleep(300)
  end
  -- no such heading
  H.inputs({ 'yes' })
  H.keys({ 'y' })
  follow(3, 4)
  H.out('  no such heading asked: ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]) .. vim.inspect(H.key_prompts[#H.key_prompts]))
  check(H.wait(nil, function() return has('^%* No such heading') end, 3000), 'made the heading')
  -- file apps
  vim.cmd('buffer ' .. vim.fn.bufnr(dir .. '/l.org'))
  local n = #H.notes
  local r
  for i, l in ipairs(lines()) do if l:find('file:notes.txt') then r = i end end
  vim.api.nvim_win_set_cursor(0, { r, (line(r):find('file:notes')) })
  press('<Leader>oo')
  H.sleep(1500)
  H.out('  file apps: ' .. vim.inspect(H.notes_since(n)))
  check(H.note_seen('Running cp', n) and H.note_seen('copy.txt...done', n), 'Running cp …/notes.txt copy.txt...done')
  -- shell
  vim.api.nvim_win_set_cursor(0, { r, (line(r):find('shell:echo')) })
  H.inputs({ 'yes' })
  H.keys({ 'y' })
  press('<Leader>oo')
  H.sleep(1500)
  H.out('  shell: ' .. vim.inspect(H.notes_since(n)) .. ' asked ' .. vim.inspect(H.inputs_asked[#H.inputs_asked]) .. vim.inspect(H.key_prompts[#H.key_prompts]))
  check(H.note_seen('hi', n), 'shell link says hi')
  vim.api.nvim_win_set_cursor(0, { r, (line(r):find('shell:echo')) })
  H.inputs({ 'no' })
  H.keys({ 'n' })
  local m = #H.notes
  press('<Leader>oo')
  H.sleep(1000)
  check(H.note_seen('Abort', m), 'no says Abort: ' .. vim.inspect(H.notes_since(m)))
  -- gd on a link
  vim.api.nvim_win_set_cursor(0, { 3, 30 })
  vim.lsp.buf.definition()
  H.sleep(800)
  eq(line(H.cursor()[1]), '* Target', 'gd follows the link')
  -- ]l [l
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  press(']l')
  H.sleep(500)
  H.out('  ]l -> ' .. vim.inspect(H.cursor()))
  press('[l')
  H.sleep(500)
  H.out('  [l -> ' .. vim.inspect(H.cursor()))
  -- store + insert
  vim.api.nvim_win_set_cursor(0, { 5, 0 })
  press('<Leader>ols')
  H.sleep(500)
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  press('o<Esc>')
  H.selects({ 1 })
  H.inputs({ 'file:l.org::*Target', 'Desc' })
  press('<Leader>oli')
  H.sleep(1000)
  H.out('  inserted link: ' .. line(3) .. ' asked ' .. vim.inspect(vim.list_slice(H.inputs_asked, #H.inputs_asked - 1)) .. vim.inspect(H.select_asked[#H.select_asked]))
  vim.ui.open = ui_open
  -- id create
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  vim.cmd('Org id create')
  wait('ID property', function() return has('^:ID:%s+%x') end)
end)

H.section('footnotes', function()
  H.open(dir .. '/f.org', { '* A', 'Text [fn:1] and [fn:2].', '* Footnotes', '[fn:1] One.' })
  vim.api.nvim_win_set_cursor(0, { 2, 6 })
  press('<Leader>oo')
  wait('to definition', function() return H.cursor()[1] == 4 end)
  press('<Leader>oo')
  wait('back to reference', function() return H.cursor()[1] == 2 end)
  local diags = vim.diagnostic.get(0)
  H.out('  diagnostics: ' .. vim.inspect(vim.tbl_map(function(d) return { d.lnum, d.message, d.severity } end, diags)))
  check(vim.iter(diags):any(function(d) return d.message:lower():find('fn:2') or d.message:lower():find('footnote') end), 'missing definition reported')
  vim.api.nvim_win_set_cursor(0, { 2, 23 })
  H.inputs({ 'y' })
  H.keys({ 'y' })
  press('<Leader>of')
  H.sleep(3000)
  H.out('  of on undefined: ' .. vim.inspect(lines()) .. ' asked ' .. vim.inspect(H.key_prompts[#H.key_prompts]) .. vim.inspect(H.inputs_asked[#H.inputs_asked]))
end)

H.done()
