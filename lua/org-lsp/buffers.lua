--- The buffers of the files the server names.
local M = {}

--- The loaded buffer of `uri`, if there is one. Unlike
--- `vim.uri_to_bufnr`, it makes no buffer for a file that is not open.
---@param uri string
---@return integer?
function M.of(uri)
  local name = vim.uri_to_fname(uri)
  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(bufnr) and (vim.api.nvim_buf_get_name(bufnr) == name or vim.uri_from_bufnr(bufnr) == uri) then
      return bufnr
    end
  end
  return nil
end

return M
