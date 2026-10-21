local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('links2')
vim.fn.writefile({ '* Elsewhere', 'Points to [[file:t.org::*Target heading][it]].' }, dir .. '/o.org')
H.init({ settings = { org_agenda_files = { dir }, clock = { today = '2026-10-21', now = '10:30' }, org_modules = { 'ol-man', 'org-ctags' } } })
local press, line, lines, wait = H.press, H.line, H.lines, H.wait
H.section('lsp link features', function()
  H.open(dir .. '/t.org', { '* Target heading', 'See [[Target heading]] and <2026-10-23 Fri>.', '[[man:ls::-l]] [[info:elisp#Buffers]]', '<<topic>> here' })
  local c = H.client()
  local uri = vim.uri_from_bufnr(0)
  vim.cmd('badd ' .. dir .. '/o.org')
  H.sleep(500)
  local refs = c:request_sync('textDocument/references', { textDocument = { uri = uri }, position = { line = 0, character = 4 }, context = { includeDeclaration = false } }, 3000, 0)
  H.out('  references: ' .. vim.inspect(vim.tbl_map(function(l) return vim.fs.basename(vim.uri_to_fname(l.uri)) .. ':' .. l.range.start.line end, refs and refs.result or {})))
  H.check(refs and refs.result and #refs.result >= 2, 'references in every file')
  local ws = c:request_sync('workspace/symbol', { query = 'Elsewhere' }, 3000, 0)
  H.out('  workspace symbols: ' .. vim.inspect(vim.tbl_map(function(s) return s.name end, ws and ws.result or {})))
  H.check(ws and ws.result and #ws.result >= 1, 'workspace symbols find headlines of every file')
  local rn = c:request_sync('textDocument/rename', { textDocument = { uri = uri }, position = { line = 0, character = 4 }, newName = 'New target' }, 3000, 0)
  local changed = {}
  for u, edits in pairs(rn and rn.result and rn.result.changes or {}) do changed[vim.fs.basename(vim.uri_to_fname(u))] = #edits end
  for _, dc in ipairs(rn and rn.result and rn.result.documentChanges or {}) do changed[vim.fs.basename(vim.uri_to_fname(dc.textDocument.uri))] = #dc.edits end
  H.out('  rename edits: ' .. vim.inspect(changed))
  H.check(changed['o.org'] and changed['t.org'], 'renaming updates links in both files')
  -- timestamp opens agenda
  vim.api.nvim_win_set_cursor(0, { 2, 30 })
  press('<Leader>oo')
  wait('agenda of the day', function() return vim.bo.filetype == 'org-agenda' end, 5000)
  H.out('  timestamp -> ' .. tostring(line(1)) .. ' / ' .. tostring(line(2)))
  vim.cmd('wincmd p')
  vim.cmd('buffer ' .. vim.fn.bufnr(dir .. '/t.org'))
  -- man link
  local opened = {}
  vim.ui.open = function(u) table.insert(opened, u) return nil, nil end
  vim.api.nvim_win_set_cursor(0, { 3, 3 })
  press('<Leader>oo')
  H.sleep(3000)
  H.out('  man link -> ' .. vim.api.nvim_buf_get_name(0) .. ' line ' .. vim.api.nvim_get_current_line():sub(1, 60) .. ' notes ' .. vim.inspect(H.notes))
  vim.cmd('silent! buffer ' .. vim.fn.bufnr(dir .. '/t.org'))
  vim.api.nvim_win_set_cursor(0, { 3, 20 })
  press('<Leader>oo')
  H.sleep(1500)
  H.out('  info link opened: ' .. vim.inspect(opened))
  -- ctags
  local n = #H.notes
  vim.cmd('Org ctags create')
  H.sleep(3000)
  H.out('  ctags create: ' .. vim.inspect(H.notes_since(n)) .. ' TAGS exists ' .. vim.fn.filereadable(dir .. '/TAGS'))
end)
H.done()
