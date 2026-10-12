-- Entry point for Neovim: registers the Org language server and the editor
-- glue around it. Everything else lives in lua/org-lsp/.
if vim.g.loaded_org_lsp then
  return
end
vim.g.loaded_org_lsp = true

if vim.fn.has('nvim-0.12') == 0 then
  vim.notify('org-lsp needs Neovim 0.12 or newer', vim.log.levels.WARN, { title = 'Org' })
  return
end

require('org-lsp').start()
