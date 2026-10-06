--- Indentation as Org does it, computed by the server: new lines get the
--- column `org-indent-line` gives them ('indentexpr', from the server's
--- on-type formatting), and `=` re-indents lines as `org-indent-region`
--- does (range formatting). `adaptIndentation` decides how far text goes.
local M = {}

local SERVER = 'org_lsp'

---@param bufnr integer
---@return vim.lsp.Client?
local function client_for(bufnr)
  return vim.lsp.get_clients({ bufnr = bufnr, name = SERVER })[1]
end

--- 'indentexpr' of Org buffers: the column of line `v:lnum`, or -1 to
--- keep its indentation when the server does not answer in time.
---@return integer
function M.expr()
  local bufnr = vim.api.nvim_get_current_buf()
  local lnum = vim.v.lnum
  local client = client_for(bufnr)
  if not client then
    return -1
  end
  local params = {
    textDocument = vim.lsp.util.make_text_document_params(bufnr),
    position = { line = lnum - 1, character = 0 },
    ch = '\n',
    options = { tabSize = vim.bo[bufnr].tabstop, insertSpaces = true },
  }
  local response = client:request_sync('textDocument/onTypeFormatting', params, 500, bufnr)
  if not response or response.err then
    return -1
  end
  -- No edit: the line is indented as it should be.
  local edit = (response.result or {})[1]
  if not edit then
    return vim.fn.indent(lnum)
  end
  return vim.fn.strdisplaywidth(edit.newText)
end

--- The operator of `=`: `org-indent-region` over the lines moved over.
function M.operator()
  local bufnr = vim.api.nvim_get_current_buf()
  local first = vim.api.nvim_buf_get_mark(bufnr, '[')[1]
  local last = vim.api.nvim_buf_get_mark(bufnr, ']')[1]
  M.region(bufnr, first, last)
end

--- Re-indent lines `first` to `last` (1-based, inclusive).
---@param bufnr integer
---@param first integer
---@param last integer
function M.region(bufnr, first, last)
  local client = client_for(bufnr)
  if not client then
    return
  end
  local line_count = vim.api.nvim_buf_line_count(bufnr)
  local finish = last < line_count and { line = last, character = 0 }
    or { line = last - 1, character = #vim.api.nvim_buf_get_lines(bufnr, last - 1, last, false)[1] }
  local params = {
    textDocument = vim.lsp.util.make_text_document_params(bufnr),
    range = { start = { line = first - 1, character = 0 }, ['end'] = finish },
    options = { tabSize = vim.bo[bufnr].tabstop, insertSpaces = true },
  }
  local response = client:request_sync('textDocument/rangeFormatting', params, 2000, bufnr)
  if response and response.result and #response.result > 0 then
    vim.lsp.util.apply_text_edits(response.result, bufnr, client.offset_encoding)
  end
end

--- Indent the Org buffer `bufnr` with the server.
---@param bufnr integer
function M.attach(bufnr)
  vim.bo[bufnr].indentexpr = "v:lua.require'org-lsp.indent'.expr()"
  -- Re-indent only lines that are opened (`o`, `O`, Enter) or asked for
  -- (CTRL-F), as Org does on RET and TAB, not as text is typed.
  vim.bo[bufnr].indentkeys = 'o,O,!^F'
  -- `gq` keeps wrapping text: the server's range formatting is
  -- indentation, which `=` does.
  if vim.bo[bufnr].formatexpr == 'v:lua.vim.lsp.formatexpr()' then
    vim.bo[bufnr].formatexpr = ''
  end
  local operator = "v:lua.require'org-lsp.indent'.operator"
  vim.keymap.set({ 'n', 'x' }, '=', function()
    vim.o.operatorfunc = operator
    return 'g@'
  end, { buffer = bufnr, expr = true, desc = 'Org: indent lines as org-indent-region' })
  vim.keymap.set('n', '==', function()
    vim.o.operatorfunc = operator
    return 'g@_'
  end, { buffer = bufnr, expr = true, desc = 'Org: indent the line as org-indent-region' })
end

return M
