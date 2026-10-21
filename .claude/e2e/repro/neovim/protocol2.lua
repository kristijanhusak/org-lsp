-- org-protocol open-source with org_protocol_project_alist, and the editor
-- started by `org-lsp protocol` when none runs (org_protocol_editor, read
-- from the shared file only).
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('protocol')
local state = dir .. '/state'
vim.env.XDG_STATE_HOME = state
vim.fn.mkdir(dir .. '/proj/src', 'p')
vim.fn.writefile({ '* Source', 'line two' }, dir .. '/proj/src/page.org')
H.init({ settings = { org_protocol_project_alist = { { name = 'p', base_url = 'https://example.org/p/', working_directory = dir .. '/proj/' } } } })
H.section('open-source', function()
  H.open(dir .. '/x.org', { '* x' })
  H.wait('listening', function() return #vim.fn.glob(state .. '/org-lsp/protocol/*.json', false, true) > 0 end, 10000)
  local r = vim.system({ H.server, 'protocol', 'org-protocol://open-source?url=' .. vim.uri_encode('https://example.org/p/src/page.org') }, { text = true }):wait(10000)
  H.out('  protocol exit ' .. r.code .. ' out ' .. vim.inspect(r.stdout) .. ' err ' .. vim.inspect(r.stderr))
  H.wait('page.org opened', function() return vim.api.nvim_buf_get_name(0):find('proj/src/page%.org$') end, 10000)
end)
H.done()
