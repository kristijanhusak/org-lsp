-- org-protocol URLs reach Neovim: `org-lsp protocol URL`, as the system
-- runs it for a bookmarklet, hands the URL to the server of this Neovim,
-- which stores the link or asks Neovim to start the capture
-- (`org-lsp/runCommand`), whose buffer then shows what the URL brought.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/protocol.lua
--
-- Exits non-zero on the first failed check.

local root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(vim.fs.abspath(debug.getinfo(1, 'S').source:sub(2)))))
local server = os.getenv('ORG_LSP_BIN') or vim.fs.joinpath(root, 'target', 'debug', 'org-lsp')

local function check(condition, message)
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

-- A state directory of the test's own, where the server says it listens
-- and the subcommand looks.
local state = vim.fn.tempname()
vim.env.XDG_STATE_HOME = state
local directory = vim.fn.tempname()
vim.fn.mkdir(directory, 'p')
local notes = vim.fs.joinpath(directory, 'notes.org')
vim.fn.writefile({ '* Web' }, notes)

local messages = {}
vim.g.org_lsp = {
  cmd = { server },
  settings = {
    org_directory = directory,
    org_default_notes_file = notes,
    org_capture_templates = {
      { key = 'w', description = 'Web', target = { 'file+headline', '', 'Web' }, template = { '* %:description', '%:link', '%i%?' } },
    },
  },
}
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')
local notify = vim.notify
vim.notify = function(message, ...)
  table.insert(messages, message)
  return notify(message, ...)
end

vim.cmd.edit(notes)
local bufnr = vim.api.nvim_get_current_buf()
wait('the server to attach', function()
  local client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  return client ~= nil and client.initialized
end)
wait('the server to listen', function()
  return #vim.fn.glob(vim.fs.joinpath(state, 'org-lsp', 'protocol', '*.json'), false, true) > 0
end)

--- `org-lsp protocol URL`, as the system runs it.
local function protocol(url)
  local done = vim.system({ server, 'protocol', url }, { text = true }):wait(10000)
  check(done.code == 0, ('org-lsp protocol %s failed: %s'):format(url, done.stderr))
  return vim.trim(done.stdout)
end

-- store-link: Emacs's message, from the subcommand and in Neovim.
local said = protocol('org-protocol://store-link?url=https%3A%2F%2Forgmode.org%2F&title=Org+mode')
local message = '‘C-c C-l’ to insert new Org link, ‘C-y’ to insert "https://orgmode.org/"'
eq(said, message, 'what the subcommand says')
wait('the message in Neovim', function()
  return vim.tbl_contains(messages, message)
end)

-- capture: the capture buffer, with what the URL brought.
protocol('org-protocol://capture?template=w&url=https%3A%2F%2Forgmode.org%2Fmanual%2F&title=The+Org+Manual&body=Selected+text')
local capture
wait('the capture buffer', function()
  for _, buffer in ipairs(vim.api.nvim_list_bufs()) do
    if vim.fs.basename(vim.api.nvim_buf_get_name(buffer)):match('^CAPTURE%-') and vim.api.nvim_buf_is_loaded(buffer) then
      capture = buffer
      return true
    end
  end
  return false
end)
eq(vim.api.nvim_get_current_buf(), capture, 'the capture buffer has the cursor')
eq(
  vim.api.nvim_buf_get_lines(capture, 0, -1, false),
  { '** The Org Manual', 'https://orgmode.org/manual/', 'Selected text' },
  'the capture buffer'
)
wait('Org saying so', function()
  return vim.tbl_contains(messages, 'Item captured.')
end)
vim.cmd('stopinsert')

-- Finished, the entry is in the notes.
require('org-lsp').run('org.capture.finish')
wait('the entry in the notes', function()
  return vim.tbl_contains(vim.api.nvim_buf_get_lines(bufnr, 0, -1, false), '** The Org Manual')
end)

print('protocol: all checks passed')
vim.cmd('qall!')
