--- Column view (`org-columns`), as the server says Emacs shows it
--- (`org-lsp/columns`): each headline covered by its columns, the header in
--- the window bar, and the column picked highlighted on the cursor's line.
--- Emacs's cursor moves from column to column there; `h` and `l` do here.
local M = {}

local namespace = vim.api.nvim_create_namespace('org_lsp_columns')

vim.api.nvim_set_hl(0, 'OrgLspColumn', { default = true, link = 'CursorColumn' })
vim.api.nvim_set_hl(0, 'OrgLspColumnCurrent', { default = true, link = 'Visual' })
vim.api.nvim_set_hl(0, 'OrgLspColumnTitle', { default = true, link = 'WinBar' })

---@class org_lsp.ColumnsView
---@field header string
---@field columns { key: string, title: string, width: integer }[]
---@field rows table<integer, { text: string, values: string[] }> by line, from 0
---@field keys string[] the keys set for it

---@type table<integer, org_lsp.ColumnsView>
local shown = {}

--- The picked column of buffer `bufnr`, from 0.
---@param bufnr integer
---@return integer
function M.column(bufnr)
  return vim.b[bufnr].org_lsp_column or 0
end

--- Whether column view shows in buffer `bufnr`.
---@param bufnr integer
---@return boolean
function M.on(bufnr)
  return shown[bufnr ~= 0 and bufnr or vim.api.nvim_get_current_buf()] ~= nil
end

--- The characters of each column in a line of the view: the column, then
--- ` | ` after it (`|` after the last one).
---@param view org_lsp.ColumnsView
---@param index integer from 0
---@return integer start, integer length
local function cell(view, index)
  local start = 0
  for i = 1, index do
    start = start + view.columns[i].width + 3
  end
  local length = view.columns[index + 1].width + (index + 1 == #view.columns and 2 or 3)
  return start, length
end

---@param bufnr integer
---@param row integer from 0
local function draw_line(bufnr, row)
  local view = shown[bufnr]
  local entry = view and view.rows[row]
  if not entry then
    return
  end
  local line = vim.api.nvim_buf_get_lines(bufnr, row, row + 1, false)[1] or ''
  -- The line's own text stays under it, hidden.
  local text = entry.text .. (' '):rep(math.max(0, vim.fn.strdisplaywidth(line) - vim.fn.strdisplaywidth(entry.text)))
  local chunks = { { text, 'OrgLspColumn' } }
  local win = vim.fn.bufwinid(bufnr)
  if win ~= -1 and vim.api.nvim_win_get_cursor(win)[1] - 1 == row then
    local start, length = cell(view, M.column(bufnr))
    chunks = {
      { vim.fn.strcharpart(text, 0, start), 'OrgLspColumn' },
      { vim.fn.strcharpart(text, start, length), 'OrgLspColumnCurrent' },
      { vim.fn.strcharpart(text, start + length), 'OrgLspColumn' },
    }
  end
  pcall(vim.api.nvim_buf_set_extmark, bufnr, namespace, row, 0, {
    id = row + 1,
    virt_text = chunks,
    virt_text_pos = 'overlay',
    undo_restore = false,
  })
end

--- The header in the bars of the windows that show buffer `bufnr`, where
--- Emacs puts it (`header-line-format`), lined up with the text.
---@param bufnr integer
local function set_header(bufnr)
  local view = shown[bufnr]
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    if vim.w[win].org_lsp_winbar == nil then
      vim.w[win].org_lsp_winbar = vim.wo[win].winbar
    end
    local offset = (vim.fn.getwininfo(win)[1] or {}).textoff or 0
    vim.wo[win].winbar = '%#OrgLspColumnTitle#' .. (' '):rep(offset) .. view.header:gsub('%%', '%%%%')
  end
end

---@param bufnr integer
function M.draw(bufnr)
  vim.api.nvim_buf_clear_namespace(bufnr, namespace, 0, -1)
  for row in pairs(shown[bufnr].rows) do
    draw_line(bufnr, row)
  end
  set_header(bufnr)
end

--- Picks the column `delta` columns away, as Emacs's `left` and `right`
--- move between them.
---@param bufnr integer
---@param delta integer
local function move(bufnr, delta)
  local view = shown[bufnr]
  vim.b[bufnr].org_lsp_column = math.max(0, math.min(#view.columns - 1, M.column(bufnr) + delta))
  draw_line(bufnr, vim.api.nvim_win_get_cursor(0)[1] - 1)
end

---@param bufnr integer
local function attach(bufnr)
  local org = require('org-lsp')
  local keys = org._set_keys('columns', bufnr)
  for _, lhs in ipairs({ 'h', '<Left>' }) do
    vim.keymap.set('n', lhs, function()
      move(bufnr, -1)
    end, { buffer = bufnr, desc = 'Column view: the column to the left' })
    table.insert(keys, lhs)
  end
  for _, lhs in ipairs({ 'l', '<Right>' }) do
    vim.keymap.set('n', lhs, function()
      move(bufnr, 1)
    end, { buffer = bufnr, desc = 'Column view: the column to the right' })
    table.insert(keys, lhs)
  end
  shown[bufnr].keys = keys
  local group = vim.api.nvim_create_augroup('org_lsp_columns_' .. bufnr, { clear = true })
  local last
  vim.api.nvim_create_autocmd('CursorMoved', {
    group = group,
    buffer = bufnr,
    callback = function()
      local row = vim.api.nvim_win_get_cursor(0)[1] - 1
      if last and last ~= row then
        draw_line(bufnr, last)
      end
      draw_line(bufnr, row)
      last = row
    end,
  })
  vim.api.nvim_create_autocmd('BufWinEnter', {
    group = group,
    buffer = bufnr,
    callback = function()
      set_header(bufnr)
    end,
  })
end

--- Column view off in buffer `bufnr`: the text as it is, the keys of Org
--- files again.
---@param bufnr integer
function M.off(bufnr)
  local view = shown[bufnr]
  if not view then
    return
  end
  shown[bufnr] = nil
  vim.api.nvim_buf_clear_namespace(bufnr, namespace, 0, -1)
  pcall(vim.api.nvim_del_augroup_by_name, 'org_lsp_columns_' .. bufnr)
  for _, lhs in ipairs(view.keys or {}) do
    pcall(vim.keymap.del, 'n', lhs, { buffer = bufnr })
  end
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    if vim.w[win].org_lsp_winbar ~= nil then
      vim.wo[win].winbar = vim.w[win].org_lsp_winbar
      vim.w[win].org_lsp_winbar = nil
    end
  end
  require('org-lsp')._set_keys('org', bufnr)
end

--- What the server says column view shows (`org-lsp/columns`).
---@param params table
function M.show(params)
  local bufnr = vim.uri_to_bufnr(params.uri)
  if not vim.api.nvim_buf_is_loaded(bufnr) then
    return
  end
  if params.lines == nil or params.lines == vim.NIL then
    return M.off(bufnr)
  end
  local first = shown[bufnr] == nil
  local view = { header = params.header, columns = params.columns, rows = {}, keys = first and {} or shown[bufnr].keys }
  for _, line in ipairs(params.lines) do
    view.rows[line[1]] = { text = line[2], values = line[3] }
  end
  shown[bufnr] = view
  vim.b[bufnr].org_lsp_column = math.max(0, math.min(#view.columns - 1, M.column(bufnr)))
  if first then
    attach(bufnr)
  end
  M.draw(bufnr)
end

return M
