local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('compl')
H.init({ settings = {} })
H.section('completion', function()
  H.open(dir .. '/c.org', { '* A :work:', ':PROPERTIES:', ':Ef', ':Color: red', ':END:', '* B', ':PROPERTIES:', ':Effort: 1:00', ':CUSTOM_ID: bee', ':END:', '#+', '#+STARTUP: ', '#+begin_', '\\alp', '* ', 'x [[', '* T :', ':LOG', '#+BEGIN_', 'x [[*', 'x [[#', 'x [[file:' })
  local c = H.client()
  local function comp(l, ch)
    local r = c:request_sync('textDocument/completion', { textDocument = { uri = vim.uri_from_bufnr(0) }, position = { line = l, character = ch } }, 3000, 0)
    local items = r and r.result and (r.result.items or r.result) or {}
    return items
  end
  local function show(name, l, ch)
    local items = comp(l, ch)
    H.out(name .. ': ' .. #items .. ' ' .. vim.inspect(vim.tbl_map(function(i) return { i.label, i.insertText or (i.textEdit and i.textEdit.newText) } end, vim.list_slice(items, 1, 6))))
    return items
  end
  show(':Ef in drawer', 2, 3)
  show('#+begin_', 12, 8)
  local bi = show('#+BEGIN_', 18, 8)
  H.out('  begin labels: ' .. table.concat(vim.tbl_map(function(i) return i.label end, vim.tbl_filter(function(i) return i.label:lower():find('begin') end, bi)), ' '))
  show('tag after :', 16, 5)
  local items = show('[[', 15, 4)
  H.out('  [[ labels: ' .. table.concat(vim.tbl_map(function(i) return i.label end, items), ' '))
  show('[[*', 19, 5)
  show('[[#', 20, 5)
  show('[[file:', 21, 9)
end)
H.done()
