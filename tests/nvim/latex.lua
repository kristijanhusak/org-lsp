-- LaTeX previews when a file opens (`#+STARTUP: latexpreview`), made by
-- Emacs as Org makes them. Stand-ins for `latex` and `dvipng` take the
-- place of a TeX installation: `latex` leaves a DVI file, `dvipng` a small
-- picture. A stand-in for `vim.ui.img` records what the plugin draws.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/latex.lua

local root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(vim.fs.abspath(debug.getinfo(1, 'S').source:sub(2)))))
local server = os.getenv('ORG_LSP_BIN') or vim.fs.joinpath(root, 'target', 'debug', 'org-lsp')

if vim.fn.executable('emacs') == 0 then
  print('ok: 0 checks passed (no Emacs)')
  vim.cmd('qall!')
end

local checks = 0
local function check(condition, message)
  checks = checks + 1
  if not condition then
    io.stderr:write(('FAIL: %s\n'):format(message))
    vim.cmd('cquit 1')
  end
end

local function wait(message, predicate)
  check(vim.wait(60000, predicate, 50), 'timed out waiting for ' .. message)
end

vim.ui.img = {
  set = function(_, _)
    return 1
  end,
  get = function()
    return nil
  end,
  del = function()
    return true
  end,
}

-- The stand-ins, first on the PATH of the server and its Emacs.
local directory = vim.fn.tempname()
local bin = directory .. '/bin'
vim.fn.mkdir(bin, 'p')
-- A PNG of 16 by 8 pixels.
local png = vim.base64.decode(
  'iVBORw0KGgoAAAANSUhEUgAAABAAAAAICAYAAADwdn+XAAAAFUlEQVR4nGP4z8DwnxLMMGrAcDAAAA01/wGXrQmDAAAAAElFTkSuQmCC'
)
local out = assert(io.open(bin .. '/picture.png', 'wb'))
out:write(png)
out:close()
vim.fn.writefile({
  '#!/bin/sh',
  'dir=.; prev=; for a; do [ "$prev" = -output-directory ] && dir=$a; prev=$a; f=$a; done',
  'base=$(basename "${f%.tex}"); : > "$dir/$base.dvi"',
}, bin .. '/latex')
vim.fn.writefile({
  '#!/bin/sh',
  'out=; prev=; for a; do [ "$prev" = -o ] && out=$a; prev=$a; done',
  'cp ' .. vim.fn.shellescape(bin .. '/picture.png') .. ' "$out"',
}, bin .. '/dvipng')
vim.fn.setfperm(bin .. '/latex', 'rwxr-xr-x')
vim.fn.setfperm(bin .. '/dvipng', 'rwxr-xr-x')
vim.env.PATH = bin .. ':' .. vim.env.PATH

vim.g.org_lsp = { cmd = { server }, images = { cell = { width = 10, height = 20 } } }
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')
local previews = require('org-lsp.images')

-- 1. `#+STARTUP: latexpreview`: the fragments as images, as the file opens.
local file = directory .. '/math.org'
vim.fn.writefile({ '#+STARTUP: latexpreview', '* Math', 'Inline $x^2$ here.', '\\[ \\int_0^1 x\\,dx \\]' }, file)
vim.cmd.edit(file)
local bufnr = vim.api.nvim_get_current_buf()
wait('the fragments as images', function()
  return #previews.list(bufnr) == 2
end)
for _, entry in ipairs(previews.list(bufnr)) do
  check(entry.kind == 'latex' and entry.path:match('ltximg/') ~= nil, 'a LaTeX image of ltximg/')
end

-- 2. Without the word, none.
local plain = directory .. '/plain.org'
vim.fn.writefile({ '* Math', 'Inline $x^2$ here.' }, plain)
vim.cmd.edit(plain)
local other = vim.api.nvim_get_current_buf()
vim.wait(3000)
check(#previews.list(other) == 0, 'no image without latexpreview')

print(('ok: %d checks passed'):format(checks))
vim.cmd('qall!')
