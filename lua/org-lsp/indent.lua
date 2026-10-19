--- Indentation as Org does it, computed by the server: new lines get the
--- column `org-indent-line` gives them ('indentexpr', from the server's
--- on-type formatting), and `=` re-indents lines as `org-indent-region`
--- does (range formatting). `org_adapt_indentation` decides how far text goes.
---
--- Files with `org-indent-mode` (`org_startup_indented`, `#+STARTUP: indent`)
--- are shown indented without being changed: the server says how far each
--- line goes (`org-lsp/virtualIndent`), and the prefixes are inline virtual
--- text. Neovim cannot repeat them on the lines a long line wraps into
--- (`wrap-prefix`): 'breakindent' lines those up with the line's own text,
--- as Org does past the prefix, a headline's title, an item's text.
local M = {}

local namespace = vim.api.nvim_create_namespace('org_lsp_virtual_indent')

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

--- Show the prefixes the server sent for a document (`org-lsp/virtualIndent`):
--- `lines` of `[line, prefix, wrap, hidden stars]`, for its `version`.
---@param params { uri: string, version: integer, lines: integer[][] }
function M.virtual(params)
  local bufnr = require('org-lsp.buffers').of(params.uri)
  -- For older text than the buffer has: newer prefixes are on their way.
  if not bufnr or vim.lsp.util.buf_versions[bufnr] ~= params.version then
    return
  end
  -- An entry in a float (`K` in the agenda) is shown as it is: indented,
  -- its lines would be wider than the float made for them.
  if vim.b[bufnr].org_lsp_preview then
    return
  end
  vim.api.nvim_buf_clear_namespace(bufnr, namespace, 0, -1)
  for _, line in ipairs(params.lines) do
    local row, prefix, hidden = line[1], line[2], line[4]
    if prefix > 0 then
      pcall(vim.api.nvim_buf_set_extmark, bufnr, namespace, row, 0, {
        virt_text = { { (' '):rep(prefix) } },
        virt_text_pos = 'inline',
        right_gravity = false,
        undo_restore = false,
      })
    end
    -- Hidden stars keep their place: each shows as a space.
    for star = 0, hidden - 1 do
      pcall(vim.api.nvim_buf_set_extmark, bufnr, namespace, row, star, {
        end_col = star + 1,
        conceal = ' ',
        undo_restore = false,
      })
    end
  end
  -- A line that wraps goes on as `org-indent-mode`'s `wrap-prefix` has it,
  -- without the prefix, which Neovim does not repeat there: at its own
  -- indentation, a headline after its stars and a space, a list item a
  -- column after its bullet (`org-list-item-body-column`).
  if #params.lines > 0 then
    vim.bo[bufnr].formatlistpat = M.WRAP_PATTERN
    for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
      vim.wo[win][0].breakindent = true
      vim.wo[win][0].breakindentopt = 'list:-1'
    end
  end
end

--- What the lines a long line wraps into line up with, past their own
--- indentation, for 'breakindentopt' `list:-1`: a headline's stars and a
--- space, a list item's bullet and a space.
M.WRAP_PATTERN = [[^\*\+ \|^\s*\%([-+]\|\s\*\|\d\+[.)]\)\s]]

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
