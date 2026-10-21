local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('hover')
H.init({ settings = {} })
H.section('hover', function()
  H.open(dir .. '/w.org', { '* TODO Call', 'See [[*Other][other]] and [fn:1] and [[https://example.org]].', '* TODO Other', 'SCHEDULED: <2026-10-21 Wed>', '* Footnotes', '[fn:1] A note.' })
  local c = H.client()
  for _, col in ipairs({ 6, 28, 40 }) do
    local r = c:request_sync('textDocument/hover', { textDocument = { uri = vim.uri_from_bufnr(0) }, position = { line = 1, character = col } }, 3000, 0)
    H.out(col .. ': ' .. vim.inspect(r and r.result and r.result.contents))
  end
end)
H.done()
