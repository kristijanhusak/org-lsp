-- End-to-end test of the Neovim shim against the real server.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/e2e.lua
--
-- Exits non-zero on the first failed check.

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

--- Wait until `predicate` holds; fail the test if it never does.
local function wait(message, predicate)
  check(vim.wait(5000, predicate, 10), 'timed out waiting for ' .. message)
end

--- Type keys in normal mode, with mappings applied.
local function press(keys)
  vim.api.nvim_feedkeys(vim.keycode(keys), 'mx', false)
end

--- A headline with its tags where `org-todo` puts them: ending at column 77.
local function tagged(headline, tags)
  return headline .. (' '):rep(77 - vim.fn.strdisplaywidth(headline) - #tags) .. tags
end

local function line(number)
  return vim.api.nvim_buf_get_lines(0, number - 1, number, false)[1]
end

-- Load the plugin the way a plugin manager would: the repo root on the
-- runtime path, then its plugin/ file.
vim.g.org_lsp = { cmd = { server } }
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('syntax on')
vim.cmd('runtime plugin/org-lsp.lua')

local file = vim.fn.tempname() .. '.org'
vim.fn.writefile(vim.fn.readfile(vim.fs.joinpath(root, 'tests', 'nvim', 'sample.org')), file)
vim.cmd.edit(file)
local bufnr = vim.api.nvim_get_current_buf()
eq(vim.bo.filetype, 'org', 'filetype is detected')

-- 1. The server attaches without any configuration beyond the binary path.
local client
wait('the server to attach', function()
  client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  return client ~= nil and client.initialized
end)
eq(client.server_info.name, 'org-lsp', 'server identifies itself')
eq(client.offset_encoding, 'utf-8', 'UTF-8 positions are negotiated')

-- 2. Semantic tokens arrive and carry the expected types and modifiers.
local function token_at(row, col)
  local tokens = vim.lsp.semantic_tokens.get_at_pos(bufnr, row, col)
  return tokens and tokens[1]
end
wait('semantic tokens', function()
  return token_at(1, 0) ~= nil
end)
eq(token_at(0, 0).type, 'keyword', '#+TITLE: is a keyword token')
eq(token_at(1, 0).type, 'stars', 'headline stars')
check(token_at(1, 0).modifiers.level1, 'stars carry the level modifier')
eq(token_at(1, 2).type, 'todoKeyword', 'TODO keyword')
eq(token_at(1, 7).type, 'headline', 'headline title')
check(token_at(1, 14).modifiers.bold and token_at(1, 14).modifiers.level1, 'bold inside a title keeps both modifiers')
eq(token_at(1, 20).type, 'tag', 'tags')
eq(token_at(2, 19).type, 'link', 'link')
eq(token_at(4, 0).modifiers.level2, true, 'second-level stars')
check(token_at(2, 3) == nil, 'plain text has no token')
-- Columns after multi-byte characters land on the right text.
eq(token_at(9, 2).type, 'headline', 'title after wide characters')

-- 3. Token groups are linked to real highlight groups.
eq(vim.api.nvim_get_hl(0, { name = '@lsp.typemod.headline.level1.org' }).link, 'Title', 'level 1 links to Title')
eq(vim.api.nvim_get_hl(0, { name = '@lsp.mod.bold.org' }).link, '@markup.strong', 'bold links to @markup.strong')
-- A group nvim-orgmode themes define takes precedence once it exists.
vim.api.nvim_set_hl(0, '@org.headline.level2', { fg = '#123456' })
require('org-lsp.highlights').define()
vim.cmd('hi clear @lsp.typemod.headline.level2.org')
require('org-lsp.highlights').define()
eq(vim.api.nvim_get_hl(0, { name = '@lsp.typemod.headline.level2.org' }).link, '@org.headline.level2', 'prefers @org.* groups')
wait('regex syntax to be replaced', function()
  return vim.bo.syntax == ''
end)

-- 4. Folding comes from the server; the file opens unfolded.
eq(vim.wo.foldmethod, 'expr', 'foldmethod')
wait('folds', function()
  return vim.fn.foldlevel(3) > 0
end)
eq(vim.fn.foldclosed(2), -1, 'file starts with everything visible')
vim.api.nvim_win_set_cursor(0, { 2, 0 })
press('<Tab>')
eq({ vim.fn.foldclosed(2), vim.fn.foldclosedend(2) }, { 2, 7 }, 'TAB folds the subtree (blank line stays outside)')
press('<Tab>')
eq(vim.fn.foldclosed(2), -1, 'TAB again opens it')
eq({ vim.fn.foldclosed(5), vim.fn.foldclosedend(5) }, { 5, 7 }, '...with the child still folded')
press('<Tab>')
eq(vim.fn.foldclosed(5), -1, 'third TAB shows everything')
press('<S-Tab>')
eq(vim.fn.foldclosed(2), 2, 'S-TAB folds the whole file')
press('<S-Tab>')
eq(vim.fn.foldclosed(2), -1, 'S-TAB again unfolds it')

-- 5. TODO cycling through the default key, from inside the entry's body.
vim.api.nvim_win_set_cursor(0, { 3, 10 })
press('cit')
wait('TODO -> DONE', function()
  return line(2) == tagged('* DONE First *bold*', ':tag:')
end)
eq(vim.api.nvim_win_get_cursor(0), { 3, 10 }, 'the cursor does not move')
-- `.` repeats the action.
press('.')
wait('DONE -> none', function()
  return line(2) == tagged('* First *bold*', ':tag:')
end)
press('ciT')
wait('none -> DONE (backward)', function()
  return line(2) == tagged('* DONE First *bold*', ':tag:')
end)
-- On an empty line, where `g@l` cannot run.
vim.api.nvim_win_set_cursor(0, { 4, 0 })
eq(line(4), '', 'line 4 is empty')
press('cit')
wait('DONE -> none from an empty line', function()
  return line(2) == tagged('* First *bold*', ':tag:')
end)
-- A headline with wide characters, to exercise position encoding in edits.
vim.api.nvim_win_set_cursor(0, { 10, 0 })
press('cit')
wait('keyword inserted before wide characters', function()
  return line(10) == '* TODO Ünïcödé 😀 heading'
end)
-- Tokens follow the edit.
wait('tokens after the edit', function()
  local token = token_at(9, 2)
  return token ~= nil and token.type == 'todoKeyword'
end)
-- Nothing happens before the first headline.
vim.api.nvim_win_set_cursor(0, { 1, 0 })
press('cit')
vim.wait(200)
eq(line(1), '#+TITLE: Sample', 'no action outside a headline')

-- 6. The outline.
local response = client:request_sync('textDocument/documentSymbol', { textDocument = vim.lsp.util.make_text_document_params(bufnr) }, 2000, bufnr)
local symbols = response and response.result or {}
eq(vim.tbl_map(function(symbol) return symbol.name end, symbols), { 'First *bold*', 'Second', 'Ünïcödé 😀 heading' }, 'top-level outline')
eq(symbols[1].children[1].name, 'Child', 'nested outline')
eq(symbols[3].detail, 'TODO', 'outline detail shows the keyword')

-- 7. The plugin can locate the server.
eq(require('org-lsp').server_command(), { server }, 'server command')

print(('ok: %d checks passed'):format(checks))
vim.cmd('qall!')
