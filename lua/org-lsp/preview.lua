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
  -- An Org file shown in the float once it shows: what the user's setup
  -- does to an Org file's window does to it, as to Neovim's hover float.
  vim.bo[buf].filetype = 'org'
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
      return notify('Command not allowed in this line', vim.log.levels.INFO)
    end
    -- The cursor moved on while the server answered.
    if vim.api.nvim_get_current_win() ~= from or not vim.deep_equal(vim.api.nvim_win_get_cursor(from), cursor) then
      return
    end
    local win = M.open(client, org_lines(contents.value))
    floats[agenda] = win
    local group = vim.api.nvim_create_augroup('org_lsp_preview_' .. agenda, { clear = true })
    local function close()
      floats[agenda] = nil
      pcall(vim.api.nvim_win_close, win, true)
      pcall(vim.api.nvim_del_augroup_by_id, group)
    end
    -- It goes when the cursor leaves the line, or the agenda for another
    -- window than the float; from the float, when that is left.
    vim.api.nvim_create_autocmd({ 'CursorMoved', 'CursorMovedI', 'InsertEnter' }, {
      group = group,
      buffer = agenda,
      callback = close,
    })
    vim.api.nvim_create_autocmd('BufLeave', {
      group = group,
      buffer = agenda,
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
  end, agenda)
end

--- The window options Neovim sets on its hover float after showing it,
--- which it keeps; the others go back to what the float had then.
local FLOAT_OPTIONS = {
  wrap = true,
  linebreak = true,
  breakindent = true,
  smoothscroll = true,
  foldenable = true,
  conceallevel = true,
  concealcursor = true,
  scroll = true,
}

--- The local options of each float of Neovim's hover as it was shown,
--- before Markdown's settings (a user's `FileType markdown`) touched it.
---@type table<integer, table<string, any>>
local shown_with = {}

---@param win integer
---@return table<string, any>
local function window_options(win)
  local options = {}
  for name, info in pairs(vim.api.nvim_get_all_options_info()) do
    if info.scope == 'win' then
      options[name] = vim.api.nvim_get_option_value(name, { win = win })
    end
  end
  return options
end

--- The line of the Org block in a hover's Markdown: first (an agenda
--- line's entry), or after a line of its own (`` `LINK: *Other` `` and a
--- blank line, a link's target).
---@param lines string[]
---@return integer? the index of the fence
local function org_fence(lines)
  for index = 1, math.min(3, #lines) do
    if lines[index]:match('^```+org$') then
      return index
    end
  end
end

--- Neovim's own hover (`K`, `vim.lsp.buf.hover()`) shows the entry of an
--- agenda line, or the target of a link, as Markdown, an Org block it
--- colors only with a tree-sitter parser for Org. Once Neovim has shown the
--- float and set it up, the float is made an Org buffer of the server's as
--- `M.open` makes one, so the server highlights it: the fence goes, the
--- line before it as text, and the float fits the rest.
---@param buf integer
local function to_org(buf)
  if not vim.api.nvim_buf_is_valid(buf) then
    return
  end
  local win = vim.fn.bufwinid(buf)
  if win == -1 or vim.api.nvim_win_get_config(win).relative == '' then
    return
  end
  -- Neovim's hover float: the buffer it was asked from, by org-lsp.
  local source = vim.w[win].lsp_floating_bufnr
  if not source or vim.w[win]['textDocument/hover'] == nil then
    return
  end
  local client = vim.lsp.get_clients({ bufnr = source, name = 'org_lsp' })[1]
  local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local at = org_fence(lines)
  local fence = at and lines[at]:match('^(```+)org$')
  if not client or not fence or #lines < at + 1 or lines[#lines] ~= fence then
    return
  end
  -- What comes before the block, as text (`` `LINK: *Other` ``).
  local before = vim.tbl_map(function(line)
    return (line:gsub('^`(.*)`$', '%1'))
  end, vim.list_slice(lines, 1, at - 1))
  pcall(vim.treesitter.stop, buf)
  -- Markdown's window settings go, Org's come with the filetype: the
  -- float is as Neovim showed it, but for what it set on it since.
  local options = shown_with[win] or {}
  shown_with[win] = nil
  for name, value in pairs(options) do
    if not FLOAT_OPTIONS[name] and not vim.deep_equal(vim.api.nvim_get_option_value(name, { win = win }), value) then
      pcall(vim.api.nvim_set_option_value, name, value, { win = win })
    end
  end
  vim.bo[buf].modifiable = true
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.list_extend(before, vim.list_slice(lines, at + 1, #lines - 1)))
  vim.bo[buf].modifiable = false
  count = count + 1
  vim.api.nvim_buf_set_name(buf, ('org-preview:/%d'):format(count))
  vim.b[buf].org_lsp_preview = true
  vim.bo[buf].syntax = ''
  vim.bo[buf].filetype = 'org'
  vim.api.nvim_win_set_height(win, math.max(1, vim.api.nvim_win_text_height(win, {}).all))
  vim.lsp.buf_attach_client(buf, client.id)
end

--- Watch for Neovim's hover floats: `to_org` once Neovim is done with one.
function M.watch()
  local group = vim.api.nvim_create_augroup('org_lsp_hover', { clear = true })
  vim.api.nvim_create_autocmd('BufWinEnter', {
    group = group,
    callback = function(args)
      -- Only a buffer that may be the float of an Org block (Neovim
      -- writes the hover's lines before it opens its window): the floats
      -- of other plugins are left alone.
      if vim.bo[args.buf].buftype ~= 'nofile' or not org_fence(vim.api.nvim_buf_get_lines(args.buf, 0, 3, false)) then
        return
      end
      -- The options of a float as it is shown, before anything else.
      local shown = {}
      for _, win in ipairs(vim.fn.win_findbuf(args.buf)) do
        if vim.api.nvim_win_get_config(win).relative ~= '' and not shown_with[win] then
          shown_with[win] = window_options(win)
          table.insert(shown, win)
        end
      end
      vim.schedule(function()
        to_org(args.buf)
        -- Those of other floats are not needed.
        for _, win in ipairs(shown) do
          shown_with[win] = nil
        end
      end)
    end,
  })
end

return M
