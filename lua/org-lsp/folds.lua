--- Visibility cycling over the folds the server provides.
---
--- The server says where folds are; opening and closing them is the
--- editor's job. These two commands reproduce Org's TAB and S-TAB.
local M = {}
local notify = require('org-lsp.notify')

--- What a closed fold shows after its first line (`org-ellipsis`), as the
--- server announced it.
M.ellipsis = '...'

vim.api.nvim_set_hl(0, 'OrgLspEllipsis', { link = 'Comment', default = true })

--- The folding ranges of the current buffer, as { start, finish } pairs of
--- 1-based line numbers, or nil when the server cannot be asked.
local function ranges(bufnr)
  local client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  if not client then
    return nil
  end
  local params = { textDocument = vim.lsp.util.make_text_document_params(bufnr) }
  local response = client:request_sync('textDocument/foldingRange', params, 1000, bufnr)
  if not response or response.err or not response.result then
    return nil
  end
  local out = {}
  for _, range in ipairs(response.result) do
    table.insert(out, { range.startLine + 1, range.endLine + 1 })
  end
  return out
end

--- Whether line `line` starts a list item, which folds with its
--- headline's children only with `org_cycle_include_plain_lists`
--- `integrate`.
---@param bufnr integer
---@param line integer 1-based
---@return boolean
local function item(bufnr, line)
  local text = vim.api.nvim_buf_get_lines(bufnr, line - 1, line, false)[1] or ''
  return text:match('^%s*[-+]%s') ~= nil
    or text:match('^%s*[-+]$') ~= nil
    or text:match('^%s*%d+[.)]%s') ~= nil
    or text:match('^%s*%d+[.)]$') ~= nil
    or text:match('^%s+%*%s') ~= nil
end

--- What TAB does before it folds (`org.cycle`): an edit of a new entry,
--- which is made, or global cycling, or how to fold. Nil when TAB is
--- done; else whether lists fold with a headline's children.
---@param bufnr integer
---@return { global: boolean?, integrateLists: boolean? }?
local function before_folding(bufnr)
  local client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  if not client then
    return {}
  end
  local context = vim.lsp.util.make_position_params(0, client.offset_encoding)
  -- On the blank that ends a line (a new `** `, left with <Esc>), TAB is
  -- at the end of the line, where Emacs's point is once it is typed: the
  -- level keeps cycling from there.
  local text = vim.api.nvim_get_current_line()
  local column = vim.api.nvim_win_get_cursor(0)[2]
  if vim.api.nvim_get_mode().mode == 'n' and column == #text - 1 and text:sub(-1):match('%s') then
    context.position.character = vim.str_utfindex(text, client.offset_encoding, #text, false)
  end
  local params = { command = 'org.cycle', arguments = { context } }
  local response = client:request_sync('workspace/executeCommand', params, 1000, bufnr)
  if not response then
    return {}
  end
  if response.err then
    notify(response.err.message, vim.log.levels.ERROR)
    return nil
  end
  local result = response.result or {}
  if type(result.edits) == 'table' then
    require('org-lsp').apply_carried(result, bufnr, client)
    return nil
  end
  return result
end

--- Whether line `line` is a headline with the ARCHIVE tag of its own.
---@param bufnr integer
---@param line integer 1-based
---@return boolean
local function archived(bufnr, line)
  local text = vim.api.nvim_buf_get_lines(bufnr, line - 1, line, false)[1] or ''
  local tags = text:match('^%*+%s') and text:match('%s(:[%w_@#%%:]+:)%s*$')
  return tags ~= nil and tags:find(':ARCHIVE:', 1, true) ~= nil
end

--- `org-cycle-hide-archived-subtrees`: close the folds of archived
--- headlines from `start` to `finish`, after the cycling opened them.
---@param bufnr integer
---@param all integer[][]
---@param start integer
---@param finish integer
local function hide_archived(bufnr, all, start, finish)
  for _, range in ipairs(all) do
    -- In a fold closed already, `foldclose` would close one around it.
    if range[1] >= start and range[2] <= finish and vim.fn.foldclosed(range[1]) == -1 and archived(bufnr, range[1]) then
      vim.cmd(('silent! %dfoldclose'):format(range[1]))
    end
  end
end

--- Cycle the subtree (or block, drawer, list item) at the cursor:
--- folded -> children visible -> everything visible -> folded. Archived
--- subtrees stay closed, as `org-cycle-hide-archived-subtrees` keeps them.
--- First the server says what TAB does there before it folds: a new,
--- empty entry changes level, the start of the file cycles globally.
function M.cycle()
  -- In a table, TAB goes to the next field (`org-table-next-field`).
  if vim.api.nvim_get_current_line():match('^%s*|') then
    return require('org-lsp').run('org.table.next.field')
  end
  local bufnr = vim.api.nvim_get_current_buf()
  -- A new entry's level, or global cycling at the start of the file.
  local before = before_folding(bufnr)
  if not before then
    return
  elseif before.global then
    return M.global_cycle()
  end
  local all = ranges(bufnr)
  if not all then
    return
  end
  local line = vim.fn.line('.')
  -- The fold that starts on this line, or else the innermost one around it.
  local current
  for _, range in ipairs(all) do
    if range[1] == line then
      current = range
      break
    elseif range[1] < line and line <= range[2] and (not current or range[1] > current[1]) then
      current = range
    end
  end
  if not current then
    return
  end
  local start, finish = current[1], current[2]

  local inner, children = {}, {}
  for _, range in ipairs(all) do
    if range[1] > start and range[2] <= finish then
      table.insert(inner, range)
    end
  end
  for _, range in ipairs(inner) do
    local nested = false
    for _, other in ipairs(inner) do
      if other ~= range and other[1] < range[1] and range[2] <= other[2] then
        nested = true
        break
      end
    end
    -- Lists stay open but with `org_cycle_include_plain_lists` `integrate`.
    if not nested and (before.integrateLists or not item(bufnr, range[1]) or item(bufnr, start)) then
      table.insert(children, range)
    end
  end

  local function stays_closed()
    if archived(bufnr, start) then
      notify('Subtree is archived and stays closed', vim.log.levels.INFO)
    end
  end
  if vim.fn.foldclosed(start) ~= -1 then
    -- Folded: show the direct children, each still folded.
    vim.cmd(('silent! %d,%dfoldopen!'):format(start, finish))
    for _, child in ipairs(children) do
      vim.cmd(('silent! %dfoldclose'):format(child[1]))
    end
    hide_archived(bufnr, all, start, finish)
    return stays_closed()
  end
  for _, range in ipairs(inner) do
    if vim.fn.foldclosed(range[1]) ~= -1 then
      -- Children: something inside is still folded, so show it all.
      vim.cmd(('silent! %d,%dfoldopen!'):format(start, finish))
      hide_archived(bufnr, all, start, finish)
      return stays_closed()
    end
  end
  -- Everything visible: fold it again.
  vim.cmd(('silent! %dfoldclose'):format(start))
end

--- Cycle the whole file between overview (all folded) and everything
--- visible but the archived subtrees.
function M.global_cycle()
  -- In a table, S-TAB goes to the previous field (`org-table-previous-field`).
  if vim.api.nvim_get_current_line():match('^%s*|') then
    return require('org-lsp').run('org.table.previous.field')
  end
  if vim.wo.foldlevel == 0 then
    vim.cmd('normal! zR')
    local bufnr = vim.api.nvim_get_current_buf()
    local all = ranges(bufnr)
    if all then
      hide_archived(bufnr, all, 1, vim.api.nvim_buf_line_count(bufnr))
    end
  else
    vim.cmd('normal! zM')
  end
end

--- The highlight groups of the semantic tokens over `row`, from the
--- marks Neovim set for them (`@lsp.type.…`, `@lsp.mod.…`,
--- `@lsp.typemod.…`): a function giving those at a column, in the order
--- Neovim applies them.
---@param marks vim.api.keyset.get_extmark_item[]
---@param row integer
---@param width integer
---@return fun(col: integer): string[]
local function token_groups(marks, row, width)
  local tokens = {}
  for index, mark in ipairs(marks) do
    local details = mark[4]
    local group = details.hl_group
    if type(group) == 'string' and group:match('^@lsp%.') then
      table.insert(tokens, {
        first = mark[2] < row and 0 or mark[3],
        last = (details.end_row or mark[2]) > row and width or (details.end_col or width),
        group = group,
        priority = details.priority or 0,
        index = index,
      })
    end
  end
  table.sort(tokens, function(a, b)
    if a.priority ~= b.priority then
      return a.priority < b.priority
    end
    return a.index < b.index
  end)
  return function(col)
    local groups = {}
    for _, token in ipairs(tokens) do
      if token.first <= col and col < token.last then
        table.insert(groups, token.group)
      end
    end
    return groups
  end
end

--- 'foldtext' of Org windows: a closed fold shows its first line as it
--- shows open, highlighted, its hidden text hidden and what stands in for
--- it shown, then `org-ellipsis`, as Emacs shows a folded heading.
---@return [string, string|string[]][]
function M.foldtext()
  local bufnr = vim.api.nvim_get_current_buf()
  local row = vim.v.foldstart - 1
  local line = vim.api.nvim_buf_get_lines(bufnr, row, row + 1, false)[1] or ''
  local concealed, inline = {}, {}
  local marks = vim.api.nvim_buf_get_extmarks(bufnr, -1, { row, 0 }, { row, -1 }, { details = true, overlap = true })
  local groups_at = token_groups(marks, row, #line)
  for _, mark in ipairs(marks) do
    local col, details = mark[3], mark[4]
    if mark[2] ~= row then
      -- A mark from a line before, kept for the tokens over this one.
    elseif details.conceal and details.end_col and (details.end_row or row) == row and vim.wo.conceallevel > 0 then
      table.insert(concealed, { col, details.end_col, details.conceal, details.hl_group or 'Conceal' })
    elseif details.virt_text_pos == 'inline' and details.virt_text then
      inline[col] = vim.list_extend(inline[col] or {}, details.virt_text)
    end
  end
  local chunks = {}
  local function add(text, group)
    local last = chunks[#chunks]
    if last and vim.deep_equal(last[2], group) then
      last[1] = last[1] .. text
    else
      table.insert(chunks, { text, group })
    end
  end
  local col = 0
  while col <= #line do
    for _, chunk in ipairs(inline[col] or {}) do
      add(chunk[1], chunk[2] or {})
    end
    if col == #line then
      break
    end
    local after = col + vim.str_utf_end(line, col + 1) + 1
    local hidden
    for _, range in ipairs(concealed) do
      if range[1] <= col and col < range[2] then
        hidden = range
        break
      end
    end
    if hidden then
      if hidden[1] == col and hidden[3] ~= '' then
        add(hidden[3], hidden[4])
      end
    else
      local text = line:sub(col + 1, after)
      add(text == '\t' and (' '):rep(vim.bo[bufnr].tabstop) or text, groups_at(col))
    end
    col = after
  end
  add(M.ellipsis, 'OrgLspEllipsis')
  return chunks
end

return M
