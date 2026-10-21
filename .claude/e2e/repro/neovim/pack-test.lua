local fails = 0
local function check(c, m)
  io.stdout:write((c and 'ok   ' or 'FAIL ') .. m .. '\n')
  if not c then fails = fails + 1 end
end
local mode = os.getenv('MODE')
check(vim.g.loaded_org_lsp == true, 'plugin loaded by vim.pack.add')
local path = vim.fn.globpath(vim.o.packpath, 'pack/core/opt/org-lsp', 0, 1)[1]
check(path ~= nil, 'installed under pack/core/opt: ' .. tostring(path))
local file = os.getenv('AGENDA') .. '/a.org'
vim.cmd.edit(file)
check(vim.bo.filetype == 'org', 'org filetype')
local ok = vim.wait(20000, function()
  local c = vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' })[1]
  return c and c.initialized
end, 50)
check(ok, 'server attached (' .. mode .. ')')
local c = vim.lsp.get_clients({ bufnr = 0, name = 'org_lsp' })[1]
io.stdout:write('cmd: ' .. vim.inspect(c and c.config.cmd) .. '\n')
if mode == 'download' then
  check(vim.tbl_contains(_G.notes, 'org-lsp: downloading the server (nightly)…'), 'download announced')
  check(vim.iter(_G.notes):any(function(n) return n:find('^org%-lsp: downloaded the server to ') end), 'download done')
  check(c and c.config.cmd[1] == vim.fn.stdpath('data') .. '/org-lsp/org-lsp', 'the downloaded server runs')
end
-- checkhealth
vim.cmd('checkhealth org-lsp')
local health = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n')
io.stdout:write('---- checkhealth\n' .. health .. '\n----\n')
check(health:find('org%-lsp') ~= nil and not health:find('ERROR'), 'checkhealth without errors')
check(health:find(os.getenv('AGENDA'), 1, true) ~= nil or health:find('agenda', 1, true) ~= nil, 'checkhealth mentions the agenda files')
vim.cmd('bwipeout')
-- An update: a second commit in the copy, then vim.pack.update with force.
if os.getenv('UPDATE') == '1' then
  local copy = os.getenv('COPY')
  vim.fn.writefile({ 'x' }, copy .. '/UPDATED')
  vim.system({ 'git', '-C', copy, 'add', 'UPDATED' }):wait()
  local r = vim.system({ 'git', '-C', copy, '-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-qm', 'second' }):wait()
  check(r.code == 0, 'second commit in the copy')
  local before = c and c.id
  local n = #_G.notes
  vim.pack.update({ 'org-lsp' }, { force = true })
  local updated = vim.wait(300000, function()
    return vim.iter(_G.pack_events):any(function(e) return e.kind == 'update' end)
  end, 50)
  check(updated, 'PackChanged update fired: ' .. vim.inspect(_G.pack_events))
  check(vim.uv.fs_stat(path .. '/UPDATED') ~= nil, 'the plugin directory has the new commit')
  if mode == 'download' then
    local dl = vim.wait(20000, function()
      return vim.iter({ unpack(_G.notes, n + 1) }):any(function(m) return m:find('^org%-lsp: downloaded the server to ') end)
    end, 50)
    check(dl, 'OrgLspDownload ran from the hook: ' .. vim.inspect({ unpack(_G.notes, n + 1) }))
    local restarted = vim.wait(10000, function()
      local nc = vim.lsp.get_clients({ name = 'org_lsp' })[1]
      return nc and nc.id ~= before and nc.initialized and #vim.lsp.get_clients({ bufnr = vim.fn.bufnr(file), name = 'org_lsp' }) == 1
    end, 50)
    check(restarted, 'the server restarted in the open Org file')
  else
    io.stdout:write('notes after update: ' .. vim.inspect({ unpack(_G.notes, n + 1) }) .. '\n')
  end
end
io.stdout:write('messages: ' .. vim.api.nvim_exec2('messages', { output = true }).output .. '\n')
io.stdout:write(('RESULT %s: %d failed\n'):format(mode, fails))
vim.cmd(fails == 0 and 'qall!' or 'cquit 1')
