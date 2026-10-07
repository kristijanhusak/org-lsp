--- Messages for the user, titled `Org` where notifications show a title
--- (nvim-notify, snacks.nvim, fidget.nvim and the like).

---@param message string
---@param level? integer `vim.log.levels`, INFO by default
return function(message, level)
  vim.notify(message, level or vim.log.levels.INFO, { title = 'Org' })
end
