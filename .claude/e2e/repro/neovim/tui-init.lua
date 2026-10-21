-- init for the tmux-driven checks: the plugin from the checkout.
vim.g.org_lsp = vim.tbl_deep_extend('force', { cmd = { '/home/user/org-lsp/target/debug/org-lsp' }, settings = { clock = { today = '2026-10-21', now = '12:44' } } }, vim.json.decode(os.getenv('ORG_LSP_JSON') or '{}'))
vim.opt.runtimepath:prepend('/home/user/org-lsp')
vim.cmd('filetype plugin on')
vim.cmd('syntax on')
