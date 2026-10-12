--- What stands in for hidden text (`org-lsp/display`): an entity's
--- character (`org-pretty-entities`), a sub- or superscript in Unicode, a
--- shrunk table field. Each is concealed text with replacement characters,
--- so it shows as links do: as Org shows it, but as it is on the cursor's
--- line, where it is edited.
local M = {}

local namespace = vim.api.nvim_create_namespace('org_lsp_display')

vim.api.nvim_set_hl(0, 'OrgLspDisplay', { default = true })

---@param bufnr integer
---@param row integer
---@param from integer byte column
---@param to integer byte column
---@param char string
local function conceal(bufnr, row, from, to, char)
  pcall(vim.api.nvim_buf_set_extmark, bufnr, namespace, row, from, {
    end_col = to,
    conceal = char,
    hl_group = 'OrgLspDisplay',
    undo_restore = false,
  })
end

--- Show the text in place of the range `[from, to)` of line `row` (byte
--- columns). Neovim replaces a concealed character with one character at
--- most: the last characters of the range each give way to one of the
--- text's, the others are hidden. A text longer than the range is shown
--- after the hidden range, as virtual text.
---@param bufnr integer
---@param row integer
---@param line string
---@param from integer
---@param to integer
---@param shown string
local function show(bufnr, row, line, from, to, shown)
  local starts = {}
  local column = from
  while column < to do
    table.insert(starts, column)
    column = column + #vim.fn.strcharpart(line:sub(column + 1), 0, 1)
  end
  local chars = vim.fn.split(shown, [[\zs]])
  if #chars > #starts then
    conceal(bufnr, row, from, to, '')
    pcall(vim.api.nvim_buf_set_extmark, bufnr, namespace, row, from, {
      virt_text = { { shown, 'OrgLspDisplay' } },
      virt_text_pos = 'inline',
      undo_restore = false,
    })
    return
  end
  local hidden = #starts - #chars
  for index, start in ipairs(starts) do
    local finish = starts[index + 1] or to
    conceal(bufnr, row, start, finish, index > hidden and chars[index - hidden] or '')
  end
end

--- Show what the server sent for a document: `items` of `[line,
--- character, end line, end character, shown]`, for its `version`.
---@param client vim.lsp.Client
---@param params { uri: string, version: integer, items: any[][] }
function M.show(client, params)
  local bufnr = require('org-lsp.buffers').of(params.uri)
  -- For older text than the buffer has: newer is on its way.
  if not bufnr or vim.lsp.util.buf_versions[bufnr] ~= params.version then
    return
  end
  vim.api.nvim_buf_clear_namespace(bufnr, namespace, 0, -1)
  if not require('org-lsp').config.conceal then
    return
  end
  local lines = {}
  local function line_of(row)
    lines[row] = lines[row] or vim.api.nvim_buf_get_lines(bufnr, row, row + 1, false)[1] or ''
    return lines[row]
  end
  for _, item in ipairs(params.items) do
    local row, character, end_row, end_character, shown = unpack(item)
    -- Within one line, as all of them are.
    if row == end_row then
      local line = line_of(row)
      local from = vim.str_byteindex(line, client.offset_encoding, character, false)
      local to = vim.str_byteindex(line, client.offset_encoding, end_character, false)
      show(bufnr, row, line, from, to, shown)
    end
  end
end

return M
