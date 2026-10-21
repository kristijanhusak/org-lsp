-- Remote image with org_display_remote_inline_images = "download": the
-- question (y n f d !), y shows it, n says it will not be downloaded.
-- And ATTR_ORG :width 100 with org_image_actual_width = false.
local H = dofile('/home/user/org-lsp/.claude/e2e/repro/neovim/h.lua')
local dir = H.tmpdir('rimg')
local drawn, next_id = {}, 0
vim.ui.img = {
  set = function(d, o) if type(d) == 'string' then next_id = next_id + 1; drawn[next_id] = vim.deepcopy(o); return next_id end; drawn[d] = vim.tbl_extend('force', drawn[d], o); return d end,
  get = function(id) return drawn[id] and vim.deepcopy(drawn[id]) end,
  del = function(id) local f = drawn[id] ~= nil; drawn[id] = nil; return f end,
}
vim.system({ 'convert', '-size', '200x100', 'xc:red', dir .. '/pic.png' }):wait()
local port = 18000 + math.random(1000)
local srv = vim.system({ 'python3', '-m', 'http.server', tostring(port), '--bind', '127.0.0.1', '--directory', dir })
H.init({ settings = { org_display_remote_inline_images = 'download', org_image_actual_width = false }, config = { images = { max_width = 40, max_height = 10, cell = { width = 10, height = 20 } } } })
H.section('remote', function()
  H.sleep(1000)
  H.open(dir .. '/r.org', { '* P', ('[[http://127.0.0.1:%d/pic.png]]'):format(port), '#+ATTR_ORG: :width 100', '[[file:pic.png]]' })
  vim.api.nvim_win_set_cursor(0, { 2, 3 })
  H.keys({ 'n' })
  H.inputs({ 'n' })
  local n = #H.notes
  H.press('<Leader>oxv')
  H.sleep(2000)
  H.out('  asked: ' .. vim.inspect(H.key_prompts[#H.key_prompts]) .. ' notes ' .. vim.inspect(H.notes_since(n)) .. ' drawn ' .. vim.tbl_count(drawn))
  H.keys({ 'y' })
  H.inputs({ 'y' })
  H.press('<Leader>oxv')
  H.sleep(3000)
  H.out('  after y: drawn ' .. vim.inspect(drawn) .. ' notes ' .. vim.inspect(H.notes_since(n)))
  H.check(vim.tbl_count(drawn) == 1, 'y shows the picture')
  vim.api.nvim_win_set_cursor(0, { 4, 3 })
  H.press('<Leader>oxv')
  H.sleep(1500)
  local w
  for _, o in pairs(drawn) do if o.width and o.width ~= 20 then w = o.width end end
  H.out('  attr width: ' .. vim.inspect(drawn))
end)
srv:kill(15)
H.done()
