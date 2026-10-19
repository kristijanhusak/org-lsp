--- An agenda entry in a float at the cursor (`K`): the server's hover for
--- the line, in an Org buffer the server highlights as it highlights any
--- Org file.
local M = {}
local notify = require('org-lsp.notify')

--- The previews opened so far: each buffer gets a name of its own, which
--- is the URI the server knows its text by.
local count = 0

--- The float of the agenda buffer, while it shows.
---@type table<integer, integer>
local floats = {}

--- The Org text of a hover: what its fenced block holds.
---@param value string
---@return string[]
local function org_lines(value)
  local lines = vim.split(value, '\n', { plain = true })
  if #lines >= 2 and lines[1]:match('^```') and lines[#lines]:match('^```') then
    lines = vim.list_slice(lines, 2, #lines - 1)
  end
  return lines
end

--- The blank columns on each side of the text.
local PADDING = 2

--- The narrowest the float is: room for a headline and its planning line.
local MIN_WIDTH = 50

--- The width of the float: as wide as the longest line with the blanks
--- on each side, at least `MIN_WIDTH`, within the editor.
---@param lines string[]
local function width_of(lines)
  local width = 1
  for _, line in ipairs(lines) do
    width = math.max(width, vim.fn.strdisplaywidth(line))
  end
  return math.min(math.max(width + 2 * PADDING, MIN_WIDTH), math.max(1, vim.o.columns - 4))
end

--- Open `lines` in a float below the cursor of the current window, the
--- buffer attached to `client` for its highlighting. Closed when the cursor
--- leaves the line.
---@param client vim.lsp.Client
---@param lines string[]
---@return integer win
function M.open(client, lines)
  count = count + 1
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_name(buf, ('org-preview:/%d'):format(count))
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].modifiable = false
  vim.bo[buf].bufhidden = 'wipe'
  -- Highlighting and concealing only: no keys, folds or indenting.
  vim.b[buf].org_lsp_preview = true
  vim.bo[buf].filetype = 'org'
  local width = width_of(lines)
  local win = vim.api.nvim_open_win(buf, false, {
    relative = 'cursor',
    row = 1,
    col = 0,
    width = width,
    height = math.max(1, math.min(#lines, vim.o.lines - 4)),
    style = 'minimal',
    border = vim.fn.exists('&winborder') == 1 and vim.o.winborder ~= '' and vim.o.winborder or 'rounded',
  })
  vim.wo[win].wrap = true
  vim.wo[win].linebreak = true
  vim.wo[win].foldenable = false
  -- The blanks on the left; those on the right are in the width.
  vim.wo[win].foldcolumn = tostring(PADDING)
  vim.api.nvim_win_set_hl_ns(win, vim.api.nvim_create_namespace('org_lsp_preview'))
  vim.api.nvim_set_hl(vim.api.nvim_create_namespace('org_lsp_preview'), 'FoldColumn', { link = 'NormalFloat' })
  vim.lsp.buf_attach_client(buf, client.id)
  vim.keymap.set('n', 'q', function()
    pcall(vim.api.nvim_win_close, win, true)
  end, { buffer = buf, desc = 'Close the entry' })
  return win
end

--- Show `lines` in a float for the buffer `source`, closed as a hover is:
--- when the cursor leaves the line, or the buffer for another window than
--- the float; from the float, when that is left.
---@param source integer
---@param client vim.lsp.Client
---@param lines string[]
---@return integer win
local function present(source, client, lines)
  local win = M.open(client, lines)
  floats[source] = win
  local group = vim.api.nvim_create_augroup('org_lsp_preview_' .. source, { clear = true })
  local function close()
    floats[source] = nil
    pcall(vim.api.nvim_win_close, win, true)
    pcall(vim.api.nvim_del_augroup_by_id, group)
  end
  vim.api.nvim_create_autocmd({ 'CursorMoved', 'CursorMovedI', 'InsertEnter' }, {
    group = group,
    buffer = source,
    callback = close,
  })
  vim.api.nvim_create_autocmd('BufLeave', {
    group = group,
    buffer = source,
    callback = vim.schedule_wrap(function()
      if vim.api.nvim_get_current_win() ~= win then
        close()
      end
    end),
  })
  vim.api.nvim_create_autocmd('WinLeave', {
    group = group,
    buffer = vim.api.nvim_win_get_buf(win),
    callback = vim.schedule_wrap(close),
  })
  vim.api.nvim_create_autocmd('WinClosed', {
    group = group,
    pattern = tostring(win),
    callback = close,
  })
  return win
end

--- Show the entry of the agenda line under the cursor; when its float
--- shows already, go into it, as a second `K` does with Neovim's hover.
function M.show()
  local agenda = vim.api.nvim_get_current_buf()
  local open = floats[agenda]
  if open and vim.api.nvim_win_is_valid(open) then
    return vim.api.nvim_set_current_win(open)
  end
  local client = vim.lsp.get_clients({ bufnr = agenda, name = 'org_lsp' })[1]
  if not client then
    return
  end
  local params = vim.lsp.util.make_position_params(0, client.offset_encoding)
  local from = vim.api.nvim_get_current_win()
  local cursor = vim.api.nvim_win_get_cursor(from)
  client:request('textDocument/hover', params, function(err, result)
    local contents = type(result) == 'table' and result.contents or nil
    if err or type(contents) ~= 'table' or type(contents.value) ~= 'string' then
      return notify('org-lsp: No entry on this line', vim.log.levels.INFO)
    end
    -- The cursor moved on while the server answered.
    if vim.api.nvim_get_current_win() ~= from or not vim.deep_equal(vim.api.nvim_win_get_cursor(from), cursor) then
      return
    end
    present(agenda, client, org_lines(contents.value))
  end, agenda)
end

--- Neovim's own hover (`K`, `vim.lsp.buf.hover()`) shows the server's Org
--- text as Markdown, which it highlights only with a tree-sitter parser
--- for Org. Org text of org-lsp's alone goes to the float of `M.open`
--- instead, highlighted by the server; a second hover goes into it, as
--- into Neovim's.
function M.install()
  local util = vim.lsp.util
  if util.org_lsp_hover then
    return
  end
  local original = util.open_floating_preview
  util.org_lsp_hover = true
  util.open_floating_preview = function(contents, syntax, opts)
    opts = opts or {}
    local source = vim.api.nvim_get_current_buf()
    local client = vim.lsp.get_clients({ bufnr = source, name = 'org_lsp' })[1]
    -- The server fences its Org text with more backticks than it holds.
    local fence = type(contents) == 'table' and #contents >= 2 and type(contents[1]) == 'string' and contents[1]:match('^(```+)org$')
    local org = client
      and opts.focus_id == 'textDocument/hover'
      and syntax == 'markdown'
      and fence
      and contents[#contents] == fence
    if not org then
      return original(contents, syntax, opts)
    end
    local open = floats[source]
    if open and vim.api.nvim_win_is_valid(open) then
      if opts.focus ~= false then
        vim.api.nvim_set_current_win(open)
      end
      return vim.api.nvim_win_get_buf(open), open
    end
    local win = present(source, client, vim.list_slice(contents, 2, #contents - 1))
    return vim.api.nvim_win_get_buf(win), win
  end
end

return M
