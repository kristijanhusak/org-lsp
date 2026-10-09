-- The previews of links to images (`org.link.preview`), as Neovim draws
-- them with `vim.ui.img`. A stand-in for `vim.ui.img` records what the
-- plugin asks for: older versions have none, and headless Neovim has no
-- terminal to draw in.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/images.lua

local root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(vim.fs.abspath(debug.getinfo(1, 'S').source:sub(2)))))
local server = os.getenv('ORG_LSP_BIN') or vim.fs.joinpath(root, 'target', 'debug', 'org-lsp')

local checks = 0
local function check(condition, message)
  checks = checks + 1
  if not condition then
    io.stderr:write(('FAIL: %s\n'):format(message))
    vim.cmd('cquit 1')
  end
end

local function eq(actual, expected, message)
  check(vim.deep_equal(actual, expected), ('%s\n  expected: %s\n  actual:   %s'):format(message, vim.inspect(expected), vim.inspect(actual)))
end

local function wait(message, predicate)
  check(vim.wait(5000, predicate, 10), 'timed out waiting for ' .. message)
end

-- What the plugin draws: each image's options, by id.
local drawn = {}
local next_id = 0
vim.ui.img = {
  set = function(data_or_id, opts)
    if type(data_or_id) == 'string' then
      next_id = next_id + 1
      drawn[next_id] = vim.deepcopy(opts)
      drawn[next_id].bytes = #data_or_id
      return next_id
    end
    drawn[data_or_id] = vim.tbl_extend('force', drawn[data_or_id], opts)
    return data_or_id
  end,
  get = function(id)
    return drawn[id] and vim.deepcopy(drawn[id])
  end,
  del = function(id)
    local found = drawn[id] ~= nil
    drawn[id] = nil
    return found
  end,
}
local function images()
  return vim.tbl_count(drawn)
end

-- Cells of 10 by 20 pixels, whatever terminal runs the test.
vim.g.org_lsp = { cmd = { server }, images = { max_width = 40, max_height = 10, cell = { width = 10, height = 20 } } }
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')
-- (The default 24 lines and 80 columns of a headless Neovim.)

-- A picture 200×100 pixels, and a file that opens with its preview.
local directory = vim.fn.tempname()
vim.fn.mkdir(directory, 'p')
local png = directory .. '/pic.png'
local made = vim.system({ 'convert', '-size', '200x100', 'xc:red', png }):wait()
check(made.code == 0, 'ImageMagick makes the picture')
local file = directory .. '/notes.org'
vim.fn.writefile({ '#+STARTUP: inlineimages', '* Pictures', '[[file:pic.png]]', 'After.' }, file)
vim.cmd.edit(file)
local bufnr = vim.api.nvim_get_current_buf()
local previews = require('org-lsp.images')

-- 1. `#+STARTUP: inlineimages`: the picture below its link, at once.
wait('the picture of the link', function()
  return images() == 1
end)
local list = previews.list(bufnr)
eq(#list, 1, 'one preview')
eq({ list[1].kind, list[1].path, list[1].line }, { 'link', png, 2 }, 'the link of line 3')
local image = drawn[next_id]
eq({ list[1].width, list[1].height }, { 20, 5 }, 'the cells of its size in pixels')
eq({ image.width, image.height }, { nil, nil }, 'drawn at its own size, its proportions kept')
local link_row = vim.fn.screenpos(0, 3, 1).row
eq(image.row, link_row + 1, 'on the line below the link')
-- Lines are kept free for it: the next line of text shows below it.
eq(vim.fn.screenpos(0, 4, 1).row, link_row + 1 + list[1].height, 'the text goes on below the picture')

-- 2. On the link, the key takes the picture away, and shows it again.
vim.api.nvim_win_set_cursor(0, { 3, 2 })
require('org-lsp').run('org.link.preview')
wait('the picture to go', function()
  return images() == 0 and #previews.list(bufnr) == 0
end)
require('org-lsp').run('org.link.preview')
wait('the picture again', function()
  return images() == 1
end)

-- 3. Scrolled out of sight, it goes; back in sight, it comes back.
vim.api.nvim_buf_set_lines(bufnr, 0, 0, false, vim.split(('x\n'):rep(60), '\n', { trimempty = true }))
vim.api.nvim_win_set_cursor(0, { 1, 0 })
vim.cmd('normal! zt')
-- What Neovim's main loop sends as the window scrolls, which a script run
-- with -l does not have.
vim.api.nvim_exec_autocmds('WinScrolled', {})
wait('the picture out of sight', function()
  return images() == 0 and #previews.list(bufnr) == 1
end)
vim.api.nvim_win_set_cursor(0, { 63, 0 })
vim.cmd('normal! zt')
vim.api.nvim_exec_autocmds('WinScrolled', {})
wait('the picture in sight', function()
  return images() == 1
end)

-- 4. Its link deleted, it goes for good.
vim.api.nvim_buf_set_lines(bufnr, 62, 63, false, {})
vim.api.nvim_exec_autocmds('TextChanged', { buffer = bufnr })
wait('the picture gone with its link', function()
  return images() == 0 and #previews.list(bufnr) == 0
end)

-- 5. A count of 16 shows the whole file's; 64 takes them all away.
vim.api.nvim_buf_set_lines(bufnr, -1, -1, false, { '[[./pic.png]]', '* Other', '[[file:pic.png]]' })
require('org-lsp').run('org.link.preview', { count = 16 })
wait('every picture', function()
  return #previews.list(bufnr) == 2
end)
require('org-lsp').run('org.link.preview', { count = 64 })
wait('no picture', function()
  return #previews.list(bufnr) == 0 and images() == 0
end)

-- 6. Wider than `max_width`: only its width is given, the terminal keeps
-- its proportions, and the lines below are those it then takes.
local wide = directory .. '/wide.png'
check(vim.system({ 'convert', '-size', '800x100', 'xc:blue', wide }):wait().code == 0, 'a wide picture')
vim.api.nvim_buf_set_lines(bufnr, -1, -1, false, { '* Wide', '[[file:wide.png]]' })
vim.api.nvim_win_set_cursor(0, { vim.api.nvim_buf_line_count(bufnr), 3 })
require('org-lsp').run('org.link.preview')
wait('the wide picture', function()
  return vim.iter(previews.list(bufnr)):any(function(entry)
    return entry.path == wide
  end)
end)
local scaled = vim.iter(previews.list(bufnr)):find(function(entry)
  return entry.path == wide
end)
eq({ scaled.width, scaled.height }, { 40, 3 }, 'as wide as it may be, as high as that makes it')
wait('the wide picture drawn', function()
  return vim.iter(vim.tbl_values(drawn)):any(function(opts)
    return opts.width == 40
  end)
end)
local opts = vim.iter(vim.tbl_values(drawn)):find(function(opts)
  return opts.width == 40
end)
eq(opts.height, nil, 'its height left to the terminal')
require('org-lsp').run('org.link.preview', { count = 64 })
wait('no picture again', function()
  return #previews.list(bufnr) == 0
end)

-- 7. An `attachment:` link: the file in the entry's attachment directory,
-- as Emacs finds it (`org-attach-expand`).
if vim.fn.executable('emacs') == 1 then
  local attached = directory .. '/data'
  vim.fn.mkdir(attached, 'p')
  vim.fn.writefile(vim.fn.readfile(png, 'b'), attached .. '/shot.png', 'b')
  vim.api.nvim_buf_set_lines(bufnr, -1, -1, false, { '* Attached', ':PROPERTIES:', ':DIR: ' .. attached, ':END:', '[[attachment:shot.png]]' })
  vim.api.nvim_win_set_cursor(0, { vim.api.nvim_buf_line_count(bufnr), 3 })
  require('org-lsp').run('org.link.preview')
  check(vim.wait(60000, function()
    return #previews.list(bufnr) == 1
  end, 50), 'timed out waiting for the attached picture')
  eq(previews.list(bufnr)[1].path, attached .. '/shot.png', 'the attached file')
end

print(('ok: %d checks passed'):format(checks))
vim.cmd('qall!')
