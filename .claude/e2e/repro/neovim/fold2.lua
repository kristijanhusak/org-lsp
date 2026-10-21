local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('fold2')
H.init({ settings = {} })
H.section('tab level', function()
  H.open(dir .. '/n.org', { '* A', '** ' })
  vim.api.nvim_win_set_cursor(0, { 2, 2 })
  for i = 1, 4 do
    H.press('<Tab>')
    H.sleep(800)
    H.out('  TAB ' .. i .. ': ' .. vim.inspect(H.line(2)) .. ' cursor ' .. vim.inspect(H.cursor()) .. ' notes ' .. vim.inspect(H.notes))
  end
end)
H.section('num', function()
  H.open(dir .. '/i.org', { '#+STARTUP: num', '* A', '** B' })
  H.sleep(1000)
  local function nums()
    local t = {}
    for _, m in ipairs(vim.api.nvim_buf_get_extmarks(0, -1, 0, -1, { details = true })) do
      if m[4].virt_text then table.insert(t, m[2] .. ':' .. m[4].virt_text[1][1]) end
    end
    return t
  end
  H.out('  before: ' .. vim.inspect(nums()))
  vim.cmd('Org num mode')
  H.sleep(1500)
  H.out('  after toggle: ' .. vim.inspect(nums()) .. ' notes ' .. vim.inspect(H.notes))
  vim.cmd('Org num mode')
  H.sleep(1500)
  H.out('  after 2nd toggle: ' .. vim.inspect(nums()))
end)
H.section('narrow save', function()
  H.open(dir .. '/w.org', { '* A', 'a text', '* B' })
  local org = vim.api.nvim_get_current_buf()
  H.press('<Leader>ons')
  H.wait('narrowed', function() return vim.api.nvim_get_current_buf() ~= org end)
  vim.api.nvim_buf_set_lines(0, 1, 2, false, { 'changed' })
  vim.cmd('write')
  H.sleep(2000)
  H.out('  org buffer: ' .. vim.inspect(H.lines(org)) .. ' modified=' .. tostring(vim.bo[org].modified) .. ' disk=' .. vim.inspect(vim.fn.readfile(dir .. '/w.org')))
end)
H.done()
