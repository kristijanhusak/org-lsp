--- Lines hidden as Org hides them with overlays: the custom properties of
--- `org-toggle-custom-properties-visibility` (`org.custom.properties.toggle`,
--- `org_custom_properties`). The server names the lines in a command's
--- answer (`hiddenLines`); they are concealed whole (`conceal_lines`,
--- with 'conceallevel' above 0, which the `conceal` option sets), and the
--- marks go with the text, as Emacs's overlays do, until the next answer
--- shows them again. The cursor's line shows, as concealed text does.
local M = {}

local namespace = vim.api.nvim_create_namespace('org_lsp_hidden_lines')

--- Hide the lines of `params` (`{ uri, lines }`) in its buffer, those
--- hidden before shown again.
---@param params { uri: string, lines: integer[] }
function M.apply(params)
  local bufnr = require('org-lsp.buffers').of(params.uri)
  if not bufnr then
    return
  end
  vim.api.nvim_buf_clear_namespace(bufnr, namespace, 0, -1)
  local count = vim.api.nvim_buf_line_count(bufnr)
  for _, line in ipairs(params.lines or {}) do
    if line < count then
      vim.api.nvim_buf_set_extmark(bufnr, namespace, line, 0, { conceal_lines = '', undo_restore = false })
    end
  end
end

--- The lines of `bufnr` hidden now, from 0; for the tests.
---@param bufnr integer
---@return integer[]
function M.lines(bufnr)
  return vim.tbl_map(function(mark)
    return mark[2]
  end, vim.api.nvim_buf_get_extmarks(bufnr, namespace, 0, -1, {}))
end

return M
