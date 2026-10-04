--- `:checkhealth org-lsp`
local M = {}

function M.check()
  local health = vim.health
  health.start('org-lsp')

  if vim.fn.has('nvim-0.11') == 1 then
    health.ok('Neovim ' .. tostring(vim.version()))
  else
    health.error('Neovim 0.11 or newer is required')
    return
  end

  local cmd = require('org-lsp').server_command()
  if not cmd then
    health.error('Server binary not found', {
      'Run :OrgLspBuild (needs cargo)',
      'or put `org-lsp` on your PATH',
      'or set `cmd` in the configuration',
    })
    return
  end
  local ok, result = pcall(function()
    return vim.system({ cmd[1], '--version' }, { text = true }):wait(5000)
  end)
  if ok and result.code == 0 then
    health.ok(vim.trim(result.stdout) .. ' (' .. cmd[1] .. ')')
  else
    health.error('Could not run ' .. cmd[1])
  end

  local attached = #vim.lsp.get_clients({ name = 'org_lsp' })
  health.info(('%d running client(s)'):format(attached))
end

return M
