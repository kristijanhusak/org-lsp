-- The syntax for Org (editors/vim/syntax/org.vim) against the server's
-- semantic tokens: in the Org files of the repository, every character the
-- syntax colors is one the server colors the same way, the token's look
-- being the one its group shows (a type with a modifier over a modifier
-- alone over a type, as Neovim and the Vim plugin layer them). What the
-- syntax leaves alone the server may color; blanks are not compared.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/syntax.lua [FILE…]
--
-- Exits non-zero after listing the places where they differ.

local root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(vim.fs.abspath(debug.getinfo(1, 'S').source:sub(2)))))
local server = os.getenv('ORG_LSP_BIN') or vim.fs.joinpath(root, 'target', 'debug', 'org-lsp')

vim.g.org_lsp = { cmd = { server } }
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('syntax on')
vim.cmd('runtime plugin/org-lsp.lua')

local files = _G.arg and #_G.arg > 0 and _G.arg or nil
if not files then
  files = { vim.fs.joinpath(root, 'tests', 'nvim', 'syntax.org') }
  vim.list_extend(files, vim.fn.globpath(root, 'tests/fixtures/*.org', false, true))
  vim.list_extend(files, vim.fn.globpath(root, 'tests/e2e/**/*.org', false, true))
  -- The docs but the reference of Emacs's options, generated and longer
  -- than the others together.
  vim.list_extend(files, vim.tbl_filter(function(file)
    return not file:match('emacs%-options%.org$')
  end, vim.fn.globpath(root, 'docs/**/*.org', false, true)))
  vim.list_extend(files, { vim.fs.joinpath(root, 'README.org') })
end

-- Lines of the parser's own fixtures, written to try it, where a pattern
-- cannot see what the server does.
local known = {
  ['tests/fixtures/objects.org:2'] = 'a link description that started on the line before',
  ['tests/fixtures/objects2.org:18'] = 'links whose brackets are escaped or empty',
  ['tests/fixtures/objects2.org:35'] = 'a LaTeX environment in a line',
  ['tests/fixtures/placement.org:32'] = 'a priority after the first word of a title',
  ['tests/fixtures/placement.org:8'] = 'a footnote reference that ends a plain link',
  ['tests/fixtures/placement.org:50'] = 'an entity as a description term',
  ['tests/fixtures/placement.org:53'] = 'a row of a table.el table',
}

--- The looks the plugin shows: selectors with a group.
local looks

--- The looks a token shows, as a set: Neovim and the Vim plugin put the
--- group of its type, of each modifier and of its type with each modifier
--- over one another.
local function looks_of(token)
  local shown = {}
  if looks[token.type] then
    shown[token.type] = true
  end
  for modifier in pairs(token.modifiers) do
    for _, selector in ipairs({ '*.' .. modifier, token.type .. '.' .. modifier }) do
      if looks[selector] then
        shown[selector] = true
      end
    end
  end
  return shown
end

--- The selector of the syntax group at a place, `orgSyntax_stars_level1`
--- being `stars.level1`; nil for none. The colored groups contain only
--- colored ones, so the innermost group says.
local seen = {}
local function syntax_at(line, col)
  local name = vim.fn.synIDattr(vim.fn.synID(line, col, 0), 'name')
  seen[name] = true
  local selector = name:match('^orgSyntax_(.-)__') or name:match('^orgSyntax_(.*)$')
  -- Code in a block's language (src.lua), an Org block's in this syntax.
  for _, id in ipairs(selector and vim.fn.synstack(line, col) or {}) do
    if vim.fn.synIDattr(id, 'name'):match('^orgLspSrcCode_') then
      return nil
    end
  end
  return selector and (selector:gsub('^Any', '*'):gsub('_', '.'))
end

--- The tokens of the whole buffer, by line: `{ start, finish, looks }`
--- with byte columns from 0, the end excluded. Neovim asks for the
--- visible lines only, and these are all of them.
local function tokens_of(buf)
  local client = vim.lsp.get_clients({ bufnr = buf, name = 'org_lsp' })[1]
  local legend = client.server_capabilities.semanticTokensProvider.legend
  local response = client:request_sync('textDocument/semanticTokens/full', {
    textDocument = { uri = vim.uri_from_bufnr(buf) },
  }, 20000, buf)
  local data = assert(response and response.result, 'no tokens').data
  local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local by_line, line, start = {}, 0, 0
  for i = 1, #data, 5 do
    if data[i] > 0 then
      line, start = line + data[i], data[i + 1]
    else
      start = start + data[i + 1]
    end
    local modifiers = {}
    for bit = 0, #legend.tokenModifiers - 1 do
      if require('bit').band(data[i + 4], require('bit').lshift(1, bit)) ~= 0 then
        modifiers[legend.tokenModifiers[bit + 1]] = true
      end
    end
    local text = lines[line + 1] or ''
    local function byte(index)
      return vim.str_byteindex(text, client.offset_encoding, math.min(index, vim.str_utfindex(text, client.offset_encoding)), false)
    end
    by_line[line + 1] = by_line[line + 1] or {}
    table.insert(by_line[line + 1], {
      byte(start),
      byte(start + data[i + 2]),
      looks_of({ type = legend.tokenTypes[data[i + 3] + 1], modifiers = modifiers }),
    })
  end
  return by_line
end

local failures, compared = {}, 0
for _, file in ipairs(files) do
  vim.cmd('edit ' .. vim.fn.fnameescape(file))
  local buf = vim.api.nvim_get_current_buf()
  assert(vim.wait(20000, function()
    local client = vim.lsp.get_clients({ bufnr = buf, name = 'org_lsp' })[1]
    return client ~= nil and client.initialized and require('org-lsp').manifest ~= nil
  end, 20), 'no server for ' .. file)
  if not looks then
    looks = {}
    for _, look in ipairs(require('org-lsp').manifest.highlights) do
      if #look.vim > 0 or look.background then
        looks[look.token] = true
      end
    end
  end
  local tokens = tokens_of(buf)
  for row, text in ipairs(vim.api.nvim_buf_get_lines(buf, 0, -1, false)) do
    for col = 1, #text do
      local selector = not text:sub(col, col):match('%s') and syntax_at(row, col)
      if selector then
        compared = compared + 1
        local token = vim.iter(tokens[row] or {}):find(function(token)
          return token[1] <= col - 1 and col - 1 < token[2]
        end)
        local shown = token and token[3] or {}
        local place = ('%s:%d'):format(vim.fs.relpath(root, file) or file, row)
        if not shown[selector] and not known[place] then
          table.insert(failures, ('%s:%d:%d %q: syntax %s, server %s'):format(
            vim.fn.fnamemodify(file, ':.'), row, col, text, selector, table.concat(vim.tbl_keys(shown), '+')))
        end
      end
    end
  end
  vim.cmd('bwipeout!')
end

-- Each colored group colors something: a pattern that never matches
-- would leave its part to the server alone.
vim.cmd('edit ' .. vim.fn.fnameescape(files[1]))
for _, line in ipairs(vim.split(vim.fn.execute('syntax list'), '\n')) do
  local group = line:match('^(orgSyntax_%S+)')
  if group and not seen[group] then
    table.insert(failures, group .. ' colors nothing in the files')
  end
end

-- With the plugin after $VIMRUNTIME in 'runtimepath', which finds Vim's
-- own syntax for Org first, the plugin's takes its place.
vim.opt.runtimepath:remove(root)
vim.opt.runtimepath:append(root)
vim.cmd('enew')
vim.api.nvim_buf_set_lines(0, 0, -1, false, { '* Level one' })
vim.bo.filetype = 'org'
if not vim.wait(2000, function()
  return vim.fn.synIDattr(vim.fn.synID(1, 1, 1), 'name') == 'orgSyntax_stars_level1'
end) then
  table.insert(failures, 'Vim\'s own syntax for Org kept, the plugin coming after $VIMRUNTIME: '
    .. vim.fn.synIDattr(vim.fn.synID(1, 1, 1), 'name'))
end

for i, failure in ipairs(failures) do
  if i > (tonumber(os.getenv("ORG_LSP_SYNTAX_SHOW")) or 200) then
    io.stderr:write(('… and %d more\n'):format(#failures - 200))
    break
  end
  io.stderr:write(failure .. '\n')
end
io.stdout:write(('%d files, %d characters compared, %d differ\n'):format(#files, compared, #failures))
vim.cmd(#failures > 0 and 'cquit 1' or 'qall!')
