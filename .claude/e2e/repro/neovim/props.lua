-- TESTING.org: Properties and column view.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('props')
H.init({ settings = { org_custom_properties = { 'ID' }, clock = { today = '2026-10-21', now = '10:30' } } })
local eq, check, press, line, lines, wait = H.eq, H.check, H.press, H.line, H.lines, H.wait

H.section('column view keys', function()
  H.open(dir .. '/c.org', { '#+COLUMNS: %25ITEM %TODO %Effort{:} %Size', '* Parent', '** TODO A', ':PROPERTIES:', ':Effort: 1:00', ':Size_ALL: S M L', ':Size: S', ':END:', '** TODO B', ':PROPERTIES:', ':Effort: 0:30', ':END:' })
  vim.api.nvim_win_set_cursor(0, { 2, 0 })
  press('<Leader>oxc')
  H.sleep(6000)
  local cols = require('org-lsp.columns')
  local marks = vim.api.nvim_buf_get_extmarks(0, -1, 0, -1, { details = true })
  local virt = {}
  for _, m in ipairs(marks) do
    if m[4].virt_text then
      table.insert(virt, m[2] .. ': ' .. table.concat(vim.tbl_map(function(c) return c[1] end, m[4].virt_text), ''))
    end
    if m[4].virt_lines then
      for _, vl in ipairs(m[4].virt_lines) do
        table.insert(virt, m[2] .. ' (vline): ' .. table.concat(vim.tbl_map(function(c) return c[1] end, vl), ''))
      end
    end
  end
  H.out('  column view overlays: ' .. vim.inspect(virt))
  check(vim.iter(virt):any(function(v) return v:find('Parent') and v:find('1:30') end), 'effort summed on the parent (1:30)')
  -- pick the Size column (3), next allowed value
  vim.api.nvim_win_set_cursor(0, { 3, 0 })
  press('lll')
  eq(cols.column(0), 3, 'Size column picked')
  press('n')
  wait('n: next allowed value M', function() return vim.iter(lines()):any(function(l) return l:find(':Size:%s+M') or l:find(':SIZE:%s+M') end) end, 8000)
  press('p')
  wait('p: previous allowed value S', function() return vim.iter(lines()):any(function(l) return l:find(':Size:%s+S') or l:find(':SIZE:%s+S') end) end, 8000)
  local n = #H.notes
  press('v')
  H.sleep(1500)
  H.out('  v says: ' .. vim.inspect(H.notes_since(n)))
  check(H.note_seen('S', n), 'v shows the value')
  -- move left/right: colmove.lua (neovim-12); rows: neovim-13
  press('>')
  H.sleep(3000)
  H.out('  after >: ' .. line(1))
  press('<')
  H.sleep(3000)
  H.out('  after <: ' .. line(1))
  H.selects({ 'Yes' })
  press('<M-S-Left>')
  H.sleep(4000)
  H.out('  after M-S-Left (delete): ' .. line(1) .. ' asked ' .. vim.inspect(H.key_prompts[#H.key_prompts]))
  check(not line(1):find('SIZE') and not line(1):find('Size'), 'Size column deleted')
  press('q')
  H.sleep(1000)
  local left = vim.iter(vim.api.nvim_buf_get_extmarks(0, -1, 0, -1, { details = true })):any(function(m) return m[4].virt_text ~= nil and m[4].virt_text_pos == 'overlay' end)
  check(not left, 'q quits column view')
end)

H.section('custom properties toggle', function()
  H.open(dir .. '/p.org', { '* A', ':PROPERTIES:', ':ID: 123', ':OTHER: x', ':END:' })
  vim.cmd('Org custom properties toggle')
  H.sleep(800)
  local hidden = vim.api.nvim_buf_get_extmarks(0, -1, 0, -1, { details = true })
  H.out('  marks: ' .. #hidden .. ' fold/conceal of line 3: ' .. vim.fn.foldclosed(3))
  local anyhide = vim.iter(hidden):any(function(m) return m[2] == 2 and (m[4].conceal_lines ~= nil or m[4].virt_text ~= nil) end) or vim.fn.foldclosed(3) == 3
  check(anyhide, 'the :ID: line is hidden')
  vim.cmd('Org custom properties toggle')
  H.sleep(800)
end)

H.section('property set / cycle / delete / effort / ordered', function()
  H.open(dir .. '/s.org', { '* A', ':PROPERTIES:', ':Color_ALL: red green blue', ':END:', '* B', ':PROPERTIES:', ':Color: red', ':END:' })
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  H.inputs({ 'Color' })
  H.selects({ 'green' })
  press('<Leader>oxp')
  wait('property set', function() return vim.iter(lines()):any(function(l) return l:find('^:Color:%s+green') end) end)
  H.out('  asked: ' .. vim.inspect(vim.list_slice(H.inputs_asked, #H.inputs_asked - 1)) .. ' select ' .. vim.inspect(H.select_asked[#H.select_asked]))
  H.out('  completions: ' .. vim.inspect(H.plugin.completions))
  local row
  for i, l in ipairs(lines()) do if l:find('^:Color:%s+green') then row = i end end
  vim.api.nvim_win_set_cursor(0, { row, 1 })
  press('<S-Right>')
  wait('cycled to blue', function() return line(row):find('blue') end)
  press('<S-Left>')
  wait('back to green', function() return line(row):find('green') end)
  -- C-c C-c in a drawer
  H.keys({ 'd' })
  H.inputs({ 'd' })
  local n = #H.notes
  press('<C-Space>')
  H.sleep(800)
  H.out('  C-c C-c in drawer: keys ' .. vim.inspect(H.key_prompts[#H.key_prompts]) .. ' notes ' .. vim.inspect(H.notes_since(n)) .. ' lines ' .. vim.inspect(lines()))
  -- delete property
  for i, l in ipairs(lines()) do if l == '* B' then vim.api.nvim_win_set_cursor(0, { i, 0 }) end end
  H.selects({ 'COLOR' })
  vim.cmd('Org property delete')
  wait('deleted from B', function() return not vim.tbl_contains(lines(), ':Color: red') end)
  H.out('  after delete: ' .. vim.inspect(lines()) .. ' notes ' .. vim.inspect(H.notes_since(n)))
  H.inputs({ 'Color_ALL' })
  H.selects({ 'Color_ALL' })
  vim.cmd('Org property delete globally')
  H.sleep(800)
  H.out('  after delete globally: ' .. vim.inspect(lines()) .. ' notes ' .. vim.inspect(H.notes_since(n)))
  -- effort
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  H.inputs({ '0:45' })
  H.selects({ '0:45' })
  press('<Leader>oxe')
  wait('effort', function() return vim.iter(lines()):any(function(l) return l:find(':Effort:%s+0:45') end) end)
  vim.cmd('Org ordered toggle')
  wait('ORDERED', function() return vim.iter(lines()):any(function(l) return l:find(':ORDERED:%s+t') end) end)
  H.out('  notes: ' .. vim.inspect(H.notes_since(n)))
  -- property names completed in a drawer
  local c = H.client()
  vim.api.nvim_buf_set_lines(0, 2, 2, false, { ':Ef' })
  H.sleep(300)
  local r = c:request_sync('textDocument/completion', { textDocument = { uri = vim.uri_from_bufnr(0) }, position = { line = 2, character = 3 } }, 3000, 0)
  local items = r and r.result and (r.result.items or r.result) or {}
  local labels = vim.tbl_map(function(i) return i.label end, items)
  H.out('  completion :Ef -> ' .. vim.inspect(vim.list_slice(labels, 1, 10)))
  check(vim.iter(labels):any(function(l) return l:lower():find('effort') end), 'property names completed (Emacs offers EFFORT:/effort:, org-pcomplete-case-double)')
end)

H.done()
