-- The Neovim plugin downloading the server when none is found.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/download.lua
--
-- The "release" is a directory with the debug build packed as CI packs it,
-- reached through a file:// URL. Exits non-zero on the first failed check.

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
  check(vim.wait(10000, predicate, 10), 'timed out waiting for ' .. message)
end

-- The plugin without a build of its own (the checkout has one), and no
-- `org-lsp` on PATH.
local plugin = vim.fn.tempname()
vim.fn.mkdir(plugin, 'p')
for _, dir in ipairs({ 'lua', 'plugin' }) do
  vim.system({ 'cp', '-R', vim.fs.joinpath(root, dir), plugin }):wait()
end
vim.env.PATH = vim.iter(vim.split(vim.env.PATH, ':')):filter(function(dir)
  return vim.fn.executable(vim.fs.joinpath(dir, 'org-lsp')) == 0
end):join(':')

vim.opt.runtimepath:prepend(plugin)
local download = require('org-lsp.download')
local platform = download.platform()
check(platform ~= nil, 'this platform has a build: ' .. vim.inspect(vim.uv.os_uname()))

-- The release, as CI publishes it.
local releases = vim.fn.tempname()
vim.fn.mkdir(vim.fs.joinpath(releases, 'nightly'), 'p')
local staged = vim.fn.tempname()
vim.fn.mkdir(staged, 'p')
vim.uv.fs_copyfile(server, vim.fs.joinpath(staged, 'org-lsp'))
local packed = vim.system({ 'tar', '-czf', vim.fs.joinpath(releases, 'nightly', ('org-lsp-%s.tar.gz'):format(platform)), '-C', staged, 'org-lsp' }):wait()
eq(packed.code, 0, 'the release is packed')
download.base_url = 'file://' .. releases
local data = vim.fn.tempname()
download.dir = function()
  return data
end

local messages = {}
vim.notify = function(message, level)
  table.insert(messages, { message, level or vim.log.levels.INFO })
end

vim.g.org_lsp = {}
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')
local org_lsp = require('org-lsp')
eq(org_lsp.server_command(), nil, 'no server is found before the download')

-- 1. Opening an Org file downloads the server and starts it.
local file = vim.fn.tempname() .. '.org'
vim.fn.writefile({ '* TODO Task' }, file)
vim.cmd.edit(file)
local bufnr = vim.api.nvim_get_current_buf()
local client
wait('the downloaded server to attach', function()
  client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  return client ~= nil and client.initialized
end)
eq(client.config.cmd, { download.path() }, 'the downloaded server runs')
eq(org_lsp.server_command(), { download.path() }, 'the downloaded server is found from now on')
check(vim.uv.fs_stat(download.path()).mode % 512 == tonumber('755', 8), 'the server is executable')
check((download.version() or ''):match('^nightly %d%d%d%d%-%d%d%-%d%d'), 'the release is noted: ' .. tostring(download.version()))
check(vim.fn.isdirectory(vim.fs.joinpath(data, 'staging')) == 0, 'the unpacked archive is cleaned up')
check(messages[1][1]:find('downloading the server (nightly)', 1, true), 'the download is announced: ' .. vim.inspect(messages))
check(vim.iter(messages):any(function(message)
  return message[1]:find('downloaded the server to', 1, true)
end), 'the download is reported: ' .. vim.inspect(messages))

-- 2. :OrgLspDownload fetches it again and restarts the server in the open
-- Org buffers.
local first = client.id
messages = {}
vim.cmd('OrgLspDownload')
wait('the server to restart', function()
  client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  return client ~= nil and client.id ~= first and client.initialized
end)
eq(#vim.lsp.get_clients({ name = 'org_lsp' }), 1, 'only the new server runs')

-- 3. A release that does not exist is an error, and the server stays.
org_lsp.setup({ release = 'v0.0.0-missing' })
messages = {}
vim.cmd('OrgLspDownload')
wait('the failure', function()
  return vim.iter(messages):any(function(message)
    return message[2] == vim.log.levels.ERROR
  end)
end)
local failure = vim.iter(messages):find(function(message)
  return message[2] == vim.log.levels.ERROR
end)
check(failure[1]:find('v0.0.0-missing', 1, true), 'the failure names the release: ' .. failure[1])
check(vim.uv.fs_stat(download.path()) ~= nil, 'the server downloaded before stays')
check(not client:is_stopped(), 'the running server keeps running')

io.stdout:write(('ok: %d download checks passed\n'):format(checks))
vim.cmd('qall!')
