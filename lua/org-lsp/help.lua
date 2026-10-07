--- The keys of the buffer at hand, in a window: what `g?` shows in Org
--- files, capture buffers and the agenda. They are the keys this plugin
--- set, the user's included, with what each does.
local M = {}
local notify = require('org-lsp.notify')

local SCOPE_TITLES = {
  org = 'Org file',
  capture = 'Capture buffer',
  agenda = 'Agenda',
  calendar = 'Date prompt',
  global = 'Anywhere',
}

--- The lines of the help for `scopes`: a heading per scope, then each key
--- and what it does, the keys lined up.
---@param scopes string[]
---@return string[] lines
---@return integer[] headings the line numbers (0-based) of the headings
function M.lines(scopes)
  local plugin = require('org-lsp')
  local sections = {}
  local width = 0
  for _, scope in ipairs(scopes) do
    local entries = plugin.keys(scope)
    if #entries > 0 then
      table.insert(sections, { scope = scope, entries = entries })
      for _, entry in ipairs(entries) do
        width = math.max(width, vim.fn.strdisplaywidth(table.concat(entry.keys, ', ')))
      end
    end
  end
  local lines, headings = {}, {}
  for index, section in ipairs(sections) do
    if index > 1 then
      table.insert(lines, '')
    end
    table.insert(headings, #lines)
    table.insert(lines, SCOPE_TITLES[section.scope] or section.scope)
    for _, entry in ipairs(section.entries) do
      local keys = table.concat(entry.keys, ', ')
      table.insert(lines, ('  %s%s  %s'):format(keys, (' '):rep(width - vim.fn.strdisplaywidth(keys)), entry.title))
    end
  end
  return lines, headings
end

--- The scopes whose keys work in buffer `bufnr`, most specific first.
---@param bufnr integer
---@return string[]
function M.scopes(bufnr)
  local manifest = require('org-lsp').manifest or {}
  local filetype = vim.bo[bufnr].filetype
  if manifest.agenda and filetype == manifest.agenda.languageId then
    return { 'agenda', 'global' }
  end
  if manifest.datePrompt and filetype == manifest.datePrompt.languageId then
    return { 'calendar' }
  end
  local prefix = manifest.capture and manifest.capture.prefix
  if prefix and vim.startswith(vim.fs.basename(vim.api.nvim_buf_get_name(bufnr)), prefix) then
    return { 'capture', 'org', 'global' }
  end
  return { 'org', 'global' }
end

--- Show the keys of the current buffer in a floating window; `q` or
--- `<Esc>` closes it.
function M.show()
  local lines, headings = M.lines(M.scopes(vim.api.nvim_get_current_buf()))
  if #lines == 0 then
    return notify('org-lsp: no keys are set here', vim.log.levels.INFO)
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  local namespace = vim.api.nvim_create_namespace('org_lsp_help')
  for _, row in ipairs(headings) do
    vim.api.nvim_buf_set_extmark(buf, namespace, row, 0, { end_col = #lines[row + 1], hl_group = 'Title' })
  end
  vim.bo[buf].modifiable = false
  vim.bo[buf].bufhidden = 'wipe'
  local width = 0
  for _, line in ipairs(lines) do
    width = math.max(width, vim.fn.strdisplaywidth(line))
  end
  width = math.min(width + 2, vim.o.columns - 4)
  local height = math.min(#lines, vim.o.lines - vim.o.cmdheight - 4)
  local win = vim.api.nvim_open_win(buf, true, {
    relative = 'editor',
    row = math.floor((vim.o.lines - vim.o.cmdheight - height) / 2) - 1,
    col = math.floor((vim.o.columns - width) / 2),
    width = width,
    height = height,
    border = 'rounded',
    title = ' Org keys ',
    title_pos = 'center',
    style = 'minimal',
  })
  vim.wo[win].cursorline = true
  for _, key in ipairs({ 'q', '<Esc>', 'g?' }) do
    vim.keymap.set('n', key, function()
      if vim.api.nvim_win_is_valid(win) then
        vim.api.nvim_win_close(win, true)
      end
    end, { buffer = buf, nowait = true, desc = 'Close the help' })
  end
  -- Gone when the cursor leaves it.
  vim.api.nvim_create_autocmd('WinLeave', {
    buffer = buf,
    once = true,
    callback = function()
      if vim.api.nvim_win_is_valid(win) then
        vim.api.nvim_win_close(win, true)
      end
    end,
  })
  return win
end

return M
