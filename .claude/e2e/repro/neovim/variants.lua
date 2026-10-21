-- User config variants: conceal off, win_split_mode, virtual_indent off,
-- folding off, mappings = false.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('variants')
vim.fn.writefile({ '* TODO Task', 'SCHEDULED: <2026-10-21 Wed>' }, dir .. '/w.org')
local mode = os.getenv('SPLIT') or 'split'
H.init({ settings = { org_agenda_files = { dir }, clock = { today = '2026-10-21', now = '10:30' } },
  config = { conceal = false, win_split_mode = mode, win_split_size = 0.5, folding = os.getenv('NOFOLD') == nil, mappings = os.getenv('NOMAP') and false or nil, virtual_indent = false } })
H.section('variants (' .. mode .. ')', function()
  H.open(dir .. '/l.org', { '#+STARTUP: indent', '* Read [[https://example.org][the site]] now', 'text' })
  H.sleep(800)
  H.eq(vim.wo.conceallevel, 0, 'conceal = false leaves conceallevel 0')
  local vt = vim.iter(vim.api.nvim_buf_get_extmarks(0, -1, 0, -1, { details = true })):any(function(m) return m[4].virt_text_pos == 'inline' end)
  H.check(not vt, 'virtual_indent = false: no inline indentation')
  if os.getenv('NOMAP') then
    H.eq(vim.fn.maparg('cit', 'n'), '', 'mappings = false: no keys')
    H.eq(vim.fn.maparg('<Leader>oa', 'n'), '', 'no global keys')
    return
  end
  if os.getenv('NOFOLD') then H.check(vim.wo.foldmethod ~= 'expr' or vim.wo.foldexpr ~= 'v:lua.vim.lsp.foldexpr()', 'folding = false') end
  local wins = #vim.api.nvim_list_wins()
  H.keys({ 'a' })
  H.press('<Leader>oa')
  H.wait('agenda', function() return vim.bo.filetype == 'org-agenda' end)
  local w = vim.api.nvim_get_current_win()
  local cfg = vim.api.nvim_win_get_config(w)
  H.out(('  %s: relative=%q wins %d->%d layout=%s height=%d width=%d'):format(mode, cfg.relative, wins, #vim.api.nvim_list_wins(), vim.fn.winlayout()[1], vim.api.nvim_win_get_height(w), vim.api.nvim_win_get_width(w)))
  -- capture opens the same way
  H.press('q')
  H.sleep(300)
end)
H.done()
