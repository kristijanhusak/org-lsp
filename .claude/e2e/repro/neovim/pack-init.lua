-- init.lua for the vim.pack install check: as docs/neovim.org says, with
-- the plugin from a local clone. MODE: cmd | path | download.
local mode = os.getenv('MODE')
local copy = os.getenv('COPY')
_G.pack_events = {}
vim.api.nvim_create_autocmd('PackChanged', {
  callback = function(ev)
    table.insert(_G.pack_events, { name = ev.data.spec.name, kind = ev.data.kind, active = ev.data.active })
    if ev.data.spec.name == 'org-lsp' and ev.data.kind == 'update' then
      if not ev.data.active then
        vim.cmd.packadd('org-lsp')
      end
      vim.cmd('OrgLspDownload')
    end
  end,
})
local cfg = { settings = { org_agenda_files = { os.getenv('AGENDA') } } }
if mode == 'cmd' then
  cfg.cmd = { '/home/user/org-lsp/target/debug/org-lsp' }
  cfg.download = false
elseif mode == 'path' then
  cfg.download = false
end
vim.g.org_lsp = cfg
_G.notes = {}
local notify = vim.notify
vim.notify = function(m, l)
  table.insert(_G.notes, tostring(m))
  io.stdout:write('notify: ' .. tostring(m) .. '\n')
end
vim.pack.add({ { src = 'file://' .. copy, name = 'org-lsp' } }, { confirm = false })
if mode == 'download' then
  require('org-lsp.download').base_url = 'file://' .. os.getenv('RELEASES')
end
