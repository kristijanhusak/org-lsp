-- The Neovim of the screenshots: the plugin from this checkout, the server
-- built in it, Neovim's own colors, and the options a scene asks for
-- (`SHOT_SETTINGS`, JSON), with the clock fixed.
local root = vim.env.SHOT_ROOT
local workspace = vim.env.SHOT_WORKSPACE

vim.o.termguicolors = true
vim.o.background = 'dark'
vim.cmd.colorscheme(vim.env.SHOT_COLORS or 'default')
vim.o.number = false
vim.o.signcolumn = 'no'
vim.o.laststatus = 2
vim.o.showmode = false
vim.o.ruler = false
vim.o.showcmd = false
vim.o.shortmess = vim.o.shortmess .. 'IFW'
vim.o.swapfile = false
vim.o.wrap = false
vim.o.fillchars = 'eob: ,vert:│'
vim.api.nvim_set_hl(0, 'WinSeparator', { fg = '#585858' })
vim.o.splitbelow = true
vim.g.mapleader = ' '
-- The name of the file, or of the server's view (`Org Agenda`).
function _G.ShotName()
  local name = vim.fn.expand('%:t')
  return (name:gsub('%%20', ' '))
end
vim.o.statusline = ' %{v:lua.ShotName()}%( %m%)'

local settings = {
  clock = { today = '2026-05-18', now = '10:30' },
  org_agenda_files = { workspace .. '/*.org' },
  org_todo_keywords = { 'TODO', 'NEXT', 'WAITING', '|', 'DONE', 'CANCELLED' },
  org_modules = { 'org-habit' },
}
for key, value in pairs(vim.json.decode(vim.env.SHOT_SETTINGS or '{}')) do
  settings[key] = value
end
vim.g.org_lsp = { cmd = { root .. '/target/debug/org-lsp' }, settings = settings }
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('syntax on')
vim.cmd('runtime plugin/org-lsp.lua')
