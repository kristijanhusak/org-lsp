--- The buffers of the files the server names.
local M = {}

-- Windows and macOS file names ignore case, as `bufadd` does there.
local ignore_case = vim.fn.has('win32') == 1 or vim.fn.has('mac') == 1

--- A file name as buffers are compared by it.
---@param name string
---@return string
local function key(name)
  name = vim.fs.normalize(name)
  return ignore_case and name:lower() or name
end

--- The loaded buffer of `uri`, if there is one. Unlike
--- `vim.uri_to_bufnr`, it makes no buffer for a file that is not open.
---@param uri string
---@return integer?
function M.of(uri)
  local name = key(vim.uri_to_fname(uri))
  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    if
      vim.api.nvim_buf_is_loaded(bufnr)
      and (key(vim.api.nvim_buf_get_name(bufnr)) == name or vim.uri_from_bufnr(bufnr) == uri)
    then
      return bufnr
    end
  end
  return nil
end

return M
