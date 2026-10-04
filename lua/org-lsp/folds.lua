--- Visibility cycling over the folds the server provides.
---
--- The server says where folds are; opening and closing them is the
--- editor's job. These two commands reproduce Org's TAB and S-TAB.
local M = {}

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

--- Cycle the subtree (or block, drawer, list item) at the cursor:
--- folded -> children visible -> everything visible -> folded.
function M.cycle()
  local all = ranges(vim.api.nvim_get_current_buf())
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
    if not nested then
      table.insert(children, range)
    end
  end

  if vim.fn.foldclosed(start) ~= -1 then
    -- Folded: show the direct children, each still folded.
    vim.cmd(('silent! %d,%dfoldopen!'):format(start, finish))
    for _, child in ipairs(children) do
      vim.cmd(('silent! %dfoldclose'):format(child[1]))
    end
    return
  end
  for _, range in ipairs(inner) do
    if vim.fn.foldclosed(range[1]) ~= -1 then
      -- Children: something inside is still folded, so show it all.
      vim.cmd(('silent! %d,%dfoldopen!'):format(start, finish))
      return
    end
  end
  -- Everything visible: fold it again.
  vim.cmd(('silent! %dfoldclose'):format(start))
end

--- Cycle the whole file between overview (all folded) and everything visible.
function M.global_cycle()
  if vim.wo.foldlevel == 0 then
    vim.cmd('normal! zR')
  else
    vim.cmd('normal! zM')
  end
end

return M
