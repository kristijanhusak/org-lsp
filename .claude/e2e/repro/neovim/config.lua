-- Setup: config.toml under setup()'s settings, g?, <Leader>oa anywhere,
-- user keys, a FileType autocmd, special C-a/C-e/C-k, C-k protection,
-- promoting a top-level subtree.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('config')
-- The shared configuration file (run.sh is told ORG_LSP_CONFIG= empty).
local xdg = os.getenv('XDG_CONFIG_HOME')
vim.fn.mkdir(xdg .. '/org-lsp', 'p')
vim.fn.writefile({
  'org_log_done = "time"',
  'org_tags_column = -60',
  'org_special_ctrl_a_e = true',
  'org_special_ctrl_k = true',
  'org_ctrl_k_protect_subtree = true',
  'org_allow_promoting_top_level_subtree = true',
  ('org_agenda_files = ["%s"]'):format(dir),
}, xdg .. '/org-lsp/config.toml')
local filetype_ran = 0
H.init({
  settings = { org_tags_column = -50, clock = { today = '2026-10-21', now = '10:00' } },
  config = {
    mappings = { org = { ['org.todo'] = '<Leader>tt', ['org.priority.up'] = false }, agenda = { ['org.agenda.later'] = 'L' }, global = { ['org.capture'] = '<Leader>X' } },
  },
  before = function()
    vim.api.nvim_create_autocmd('FileType', {
      pattern = 'org',
      callback = function()
        filetype_ran = filetype_ran + 1
        vim.opt_local.shiftwidth = 4
        vim.opt_local.conceallevel = 1
        vim.opt_local.spell = true
        vim.opt_local.foldlevel = 0
        vim.opt_local.textwidth = 70
      end,
    })
  end,
})
local eq, check, press, line, lines, wait = H.eq, H.check, H.press, H.line, H.lines, H.wait

H.section('config.toml and settings', function()
  H.open(dir .. '/a.org', { '* TODO Task', '* B' })
  press('<Leader>tt')
  wait('done', function() return line(1) == '* DONE Task' end)
  wait('CLOSED from config.toml org_log_done', function() return (line(2) or ''):match('^CLOSED:') end)
  H.inputs({ 'x' })
  vim.api.nvim_win_set_cursor(0, { 3, 0 })
  press('<Leader>ot')
  wait('tag', function() return line(3):match(':x:$') end)
  eq(#line(3), 50, 'org_tags_column from setup() (-50) wins over config.toml (-60)')
  vim.cmd('checkhealth org-lsp')
  local health = table.concat(H.lines(), '\n')
  H.out(health)
  check(health:find('config.toml', 1, true) and not health:find('(not found)', 1, true), 'checkhealth reports the config file found')
  vim.cmd('bwipeout')
end)

H.section('user keys', function()
  H.open(dir .. '/k.org', { '* TODO Task' })
  eq(vim.fn.maparg('cit', 'n'), '', 'cit replaced by <Leader>tt')
  eq(vim.fn.maparg('ciR', 'n'), '', 'false removes ciR')
  eq(vim.fn.maparg('<Leader>tt', 'n', false, true).desc, 'Change the TODO state', 'user key described')
  eq(vim.fn.maparg('<Leader>X', 'n', false, true).desc, 'Capture', 'global capture key')
  eq(vim.fn.maparg('<Leader>oc', 'n'), '', 'default capture key gone')
  -- g?
  local wins = #vim.api.nvim_list_wins()
  press('g?')
  wait('help window', function() return #vim.api.nvim_list_wins() > wins end)
  local text = table.concat(H.lines(vim.api.nvim_win_get_buf(vim.api.nvim_list_wins()[#vim.api.nvim_list_wins()])), '\n')
  local helpbuf
  for _, w in ipairs(vim.api.nvim_list_wins()) do
    local b = vim.api.nvim_win_get_buf(w)
    if table.concat(H.lines(b), '\n'):find('Change the TODO state') then helpbuf = b end
  end
  check(helpbuf ~= nil, 'g? lists the keys')
  if helpbuf then
    local t = table.concat(H.lines(helpbuf), '\n')
    check(t:find('<Leader>tt', 1, true) ~= nil, 'g? shows the user key')
    check(t:find('Anywhere', 1, true) ~= nil, 'g? shows the global keys too')
  end
  press('q')
  wait('help closed', function() return #vim.api.nvim_list_wins() == wins end)
end)

H.section('FileType autocmd', function()
  H.open(dir .. '/f.org', { '* A', 'text [[https://x.org][desc]]' })
  check(filetype_ran > 0, 'FileType autocmd ran')
  H.sleep(500)
  eq({ vim.bo.shiftwidth, vim.bo.textwidth, vim.wo.spell }, { 4, 70, true }, 'buffer options kept')
  H.out('  conceallevel after attach: ' .. vim.wo.conceallevel .. ' foldlevel ' .. vim.wo.foldlevel)
  check(vim.wo.conceallevel == 1, "the user's conceallevel (1, set in FileType) is kept")
  -- the plugin overrides foldlevel to 99 (documented: user's foldlevel does not decide)
end)

H.section('agenda from anywhere, user agenda key', function()
  vim.cmd('enew')
  vim.bo.buftype = 'nofile'
  H.keys({ 'a' })
  press('<Leader>oa')
  wait('agenda', function() return vim.bo.filetype == 'org-agenda' end)
  H.out('  agenda: ' .. vim.inspect(H.lines()))
  eq(vim.fn.maparg('L', 'n', false, true).desc, 'Agenda: go forward in time', 'user agenda key')
  eq(vim.fn.maparg('f', 'n'), '', 'f not mapped then')
  local first = line(1)
  press('L')
  wait('later', function() return line(1) ~= first end)
  local wins = #vim.api.nvim_list_wins()
  press('g?')
  wait('agenda help', function() return #vim.api.nvim_list_wins() > wins end)
  local found
  for _, w in ipairs(vim.api.nvim_list_wins()) do
    local t = table.concat(H.lines(vim.api.nvim_win_get_buf(w)), '\n')
    if t:find('Agenda: go forward in time', 1, true) then found = t end
  end
  check(found and found:find('L', 1, true), 'agenda g? lists the keys')
  press('q')
  H.sleep(200)
  press('q')
  wait('agenda closed', function() return H.agenda_buf() == nil or vim.bo.filetype ~= 'org-agenda' end)
end)

H.section('special C-a C-e C-k', function()
  local head = '* TODO Title' .. (' '):rep(50 - 12 - 5) .. ':tag:'
  H.open(dir .. '/s.org', { head, '* B' })
  vim.api.nvim_win_set_cursor(0, { 1, 9 })
  vim.cmd('Org beginning of line')
  wait('C-a to the title', function() return H.cursor()[2] == 7 end)
  vim.cmd('Org beginning of line')
  wait('C-a again to bol', function() return H.cursor()[2] == 0 end)
  vim.cmd('Org end of line')
  wait('C-e to end of title', function() return H.cursor()[2] == 12 end)
  H.out('  C-e: ' .. vim.inspect(H.cursor()))
  vim.cmd('Org end of line')
  wait('C-e again after tags', function() return H.cursor()[2] >= #line(1) - 1 end)
  vim.api.nvim_win_set_cursor(0, { 1, 9 })
  vim.cmd('Org kill line')
  wait('C-k kills to the tags', function() return line(1):match('^%* TODO Ti%s+:tag:$') end)
  H.out('  after C-k: ' .. vim.inspect(line(1)) .. ' len ' .. #line(1) .. ' reg ' .. vim.inspect(vim.fn.getreg('"')))
  eq(#line(1), 50, 'tags stay aligned')
  vim.cmd('Org kill line')
  wait('C-k again kills the tags', function() return not line(1):find(':tag:') end)
  H.out('  after 2nd C-k: ' .. vim.inspect(line(1)))
end)

H.section('C-k protect', function()
  H.open(dir .. '/p.org', { '* A', 'a text', '* B' })
  wait('folds', function() return vim.fn.foldlevel(1) > 0 end)
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  press('<Tab>')
  eq(vim.fn.foldclosed(1), 1, 'folded')
  H.inputs({ 'n' })
  H.keys({ 'n' })
  local n = #H.notes
  vim.cmd('Org kill line')
  H.sleep(600)
  H.out('  asked: ' .. vim.inspect({ H.inputs_asked[#H.inputs_asked], H.key_prompts[#H.key_prompts], H.select_asked[#H.select_asked] }) .. ' notes ' .. vim.inspect(H.notes_since(n)))
  check(H.note_seen('‘C-k’ aborted as it would kill a hidden subtree', n), 'n says C-k aborted')
  eq(lines(), { '* A', 'a text', '* B' }, 'nothing killed')
end)

H.section('promote top level', function()
  H.open(dir .. '/t.org', { '* A', 'text' })
  press('<<')
  wait('# A', function() return line(1) == '# A' end)
end)

H.done()
