-- Highlighting on screen: for each look of the manifest's `highlights`
-- table, a place in the shared sample (tests/e2e/highlight: an Org file,
-- the week's agenda, the TODO list and the date prompt) with a token it
-- applies to, where Neovim must show the token's group, linked to the
-- group the manifest names (the first one defined), which must have a
-- definition with the default colorscheme. Then what Org hides is
-- concealed: link brackets and targets, emphasis markers.
--
--   cargo build -p org-lsp && nvim --headless -u NONE -l tests/nvim/highlight.lua
--
-- Exits non-zero after listing every look that failed.

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

local function wait(message, predicate)
  check(vim.wait(10000, predicate, 10), 'timed out waiting for ' .. message)
end

-- The fixture in a directory of its own; `$DIR` in its options is that
-- directory.
local directory = vim.fn.tempname()
vim.fn.system({ 'cp', '-r', vim.fs.joinpath(root, 'tests', 'e2e', 'highlight'), directory })
check(vim.v.shell_error == 0, 'the fixture is copied')
local options = table.concat(vim.fn.readfile(vim.fs.joinpath(directory, 'options.json')), '\n')
local settings = vim.json.decode((options:gsub('%$DIR', directory)))

vim.g.org_lsp = { cmd = { server }, settings = settings }
vim.o.lines = 50
vim.o.columns = 140
vim.opt.runtimepath:prepend(root)
vim.cmd('filetype plugin on')
vim.cmd('runtime plugin/org-lsp.lua')

local manifest
local remaining = {}
local failures = {}
local checked = 0

--- The group Neovim gives what a selector matches (`highlights.lua`).
local function group_of(selector, filetype)
  local type, modifier = selector:match('^([^.]+)%.?(.*)$')
  if type == '*' then
    return ('@lsp.mod.%s.%s'):format(modifier, filetype)
  elseif modifier ~= '' then
    return ('@lsp.typemod.%s.%s.%s'):format(type, modifier, filetype)
  end
  return ('@lsp.type.%s.%s'):format(type, filetype)
end

--- The group the manifest names for a look: the first one defined, else
--- the last.
local function target_of(look)
  for _, candidate in ipairs(look.vim) do
    if not vim.tbl_isempty(vim.api.nvim_get_hl(0, { name = candidate })) then
      return candidate
    end
  end
  return look.vim[#look.vim]
end

local function matches(selector, token)
  local type, modifier = selector:match('^([^.]+)%.?(.*)$')
  return (type == '*' or type == token.type) and (modifier == '' or token.modifiers[modifier])
end

--- The server's tokens of buffer `bufnr`, as `{ line, from, to, type, modifiers }` (bytes).
local function tokens_of(bufnr, client)
  local response = client:request_sync('textDocument/semanticTokens/full', { textDocument = { uri = vim.uri_from_bufnr(bufnr) } }, 5000, bufnr)
  local data = response and response.result and response.result.data or {}
  local legend = client.server_capabilities.semanticTokensProvider.legend
  local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
  local out = {}
  local line, start = 0, 0
  for i = 1, #data, 5 do
    line = line + data[i]
    start = data[i] == 0 and start + data[i + 1] or data[i + 1]
    local text = lines[line + 1] or ''
    local modifiers = {}
    for bit, name in ipairs(legend.tokenModifiers) do
      if math.floor(data[i + 4] / 2 ^ (bit - 1)) % 2 == 1 then
        modifiers[name] = true
      end
    end
    table.insert(out, {
      line = line,
      from = vim.str_byteindex(text, client.offset_encoding, start, false),
      to = vim.str_byteindex(text, client.offset_encoding, start + data[i + 2], false),
      type = legend.tokenTypes[data[i + 3] + 1],
      modifiers = modifiers,
    })
  end
  return out
end

--- The highlight groups shown at a place of the current window's buffer,
--- by semantic tokens and by extmarks (the date prompt's, the agenda's
--- lines highlighted from edge to edge).
local function groups_at(bufnr, row, col)
  local found = {}
  local inspected = vim.inspect_pos(bufnr, row, col)
  for _, kind in ipairs({ 'semantic_tokens', 'extmarks' }) do
    for _, mark in ipairs(inspected[kind] or {}) do
      local opts = mark.opts or {}
      for _, key in ipairs({ 'hl_group', 'line_hl_group' }) do
        if opts[key] then
          found[opts[key]] = true
        end
      end
    end
  end
  return found
end

--- How specific a selector is: a type, a modifier, a type with a
--- modifier; Neovim gives the groups of each a higher priority.
local function class(selector)
  if vim.startswith(selector, '*.') then
    return 2
  end
  return selector:find('.', 1, true) and 3 or 1
end

--- Whether no more specific look applies to `token` than `look`: there
--- the look's colors must be what shows.
local function shows_alone(look, token)
  for _, other in ipairs(manifest.highlights) do
    if #other.vim > 0 and class(other.token) > class(look.token) and matches(other.token, token) then
      return false
    end
  end
  return true
end

local ATTRIBUTES = { 'fg', 'bg', 'bold', 'italic', 'underline', 'strikethrough', 'reverse' }

--- The colors and attributes shown at a place: the groups of the marks
--- there, the higher priorities over the lower.
local function effective(bufnr, row, col)
  local marks = {}
  local inspected = vim.inspect_pos(bufnr, row, col)
  for _, kind in ipairs({ 'semantic_tokens', 'extmarks' }) do
    for _, mark in ipairs(inspected[kind] or {}) do
      if mark.opts and mark.opts.hl_group then
        table.insert(marks, mark.opts)
      end
    end
  end
  table.sort(marks, function(a, b)
    return (a.priority or 0) < (b.priority or 0)
  end)
  local shown = {}
  for _, opts in ipairs(marks) do
    for key, value in pairs(vim.api.nvim_get_hl(0, { name = opts.hl_group, link = false })) do
      shown[key] = value
    end
  end
  return shown
end

--- Check the looks still to check on what buffer `bufnr` shows.
local function check_looks(bufnr, client, where)
  local filetype = vim.bo[bufnr].filetype
  local win = vim.fn.bufwinid(bufnr)
  check(win ~= -1, where .. ' is shown')
  local tokens = tokens_of(bufnr, client)
  check(#tokens > 0, where .. ' has tokens')
  for index = #remaining, 1, -1 do
    local look = remaining[index]
    -- A token the look alone colors, else any it applies to.
    local token, alone
    for _, candidate in ipairs(tokens) do
      if matches(look.token, candidate) then
        if shows_alone(look, candidate) then
          token, alone = candidate, true
          break
        end
        token = token or candidate
      end
    end
    if token then
      table.remove(remaining, index)
      checked = checked + 1
      local group = group_of(look.token, filetype)
      local text = vim.api.nvim_buf_get_text(bufnr, token.line, token.from, token.line, token.to, {})[1]
      local place = ('%s line %d %q'):format(where, token.line + 1, text)
      -- Highlighting is drawn for the lines in view.
      vim.api.nvim_win_set_cursor(win, { token.line + 1, token.from })
      vim.api.nvim_win_call(win, function()
        vim.cmd('normal! zz')
      end)
      local shown = vim.wait(3000, function()
        vim.cmd.redraw()
        return groups_at(bufnr, token.line, token.from)[group]
      end, 20)
      if not shown then
        table.insert(failures, ('%s: %s is not shown at %s (%s)'):format(look.token, group, place, table.concat(vim.tbl_keys(groups_at(bufnr, token.line, token.from)), ' ')))
      end
      local target = target_of(look)
      local link = vim.api.nvim_get_hl(0, { name = group }).link
      if link ~= target then
        table.insert(failures, ('%s: %s links to %s, not %s'):format(look.token, group, tostring(link), target))
      elseif vim.tbl_isempty(vim.api.nvim_get_hl(0, { name = group, link = false })) then
        table.insert(failures, ('%s: %s (%s) has no definition, at %s'):format(look.token, group, target, place))
      elseif alone and shown then
        -- What shows there is what the target gives.
        local wanted = vim.api.nvim_get_hl(0, { name = target, link = false })
        local got = effective(bufnr, token.line, token.from)
        for _, key in ipairs(ATTRIBUTES) do
          if wanted[key] ~= nil and got[key] ~= wanted[key] then
            table.insert(failures, ('%s: %s shows %s %s, not %s as %s, at %s'):format(look.token, group, key, tostring(got[key]), tostring(wanted[key]), target, place))
          end
        end
      end
    end
  end
end

-- 1. The Org file.
vim.cmd.edit(vim.fs.joinpath(directory, 'sample.org'))
local sample = vim.api.nvim_get_current_buf()
local client
wait('the server', function()
  client = vim.lsp.get_clients({ bufnr = sample, name = 'org_lsp' })[1]
  return client ~= nil and client.initialized
end)
manifest = client.server_capabilities.experimental.org
for _, look in ipairs(manifest.highlights) do
  if #look.vim > 0 then
    table.insert(remaining, look)
  end
end
local looks = #remaining
wait('the tokens of the sample', function()
  return vim.lsp.semantic_tokens.get_at_pos(sample, 0, 0) ~= nil
end)
check_looks(sample, client, 'sample.org')

-- 2. What Org hides is concealed: link brackets and the target of a
-- described link (`org-link-descriptive`), emphasis markers
-- (`org_hide_emphasis_markers`).
check(vim.wo.conceallevel == 2, 'conceallevel 2 in the Org window')
local function concealed(row)
  local namespace = vim.api.nvim_create_namespace('nvim.lsp.semantic_tokens:' .. client.id)
  local found = {}
  for _, mark in ipairs(vim.api.nvim_buf_get_extmarks(sample, namespace, { row, 0 }, { row, -1 }, { details = true })) do
    if mark[4].conceal then
      table.insert(found, vim.api.nvim_buf_get_text(sample, mark[2], mark[3], mark[4].end_row, mark[4].end_col, {})[1])
    end
  end
  return found
end
local lines = vim.api.nvim_buf_get_lines(sample, 0, -1, false)
local function row_of(prefix)
  for row, text in ipairs(lines) do
    if vim.startswith(text, prefix) then
      return row - 1
    end
  end
end
for _, case in ipairs({
  { 'A [[https://orgmode.org]', { '[[https://orgmode.org][', ']]', '[[', ']]' } },
  { 'Text in *bold*', { '*', '*', '/', '/', '_', '_', '+', '+', '=', '=', '~', '~' } },
  { 'Nested: *bold', { '*', '/', '/', '*' } },
}) do
  local row = row_of(case[1])
  vim.api.nvim_win_set_cursor(0, { row + 1, 0 })
  vim.cmd('normal! zz')
  local found
  vim.wait(3000, function()
    vim.cmd.redraw()
    found = concealed(row)
    return vim.deep_equal(found, case[2])
  end, 20)
  check(vim.deep_equal(found, case[2]), ('concealed on %q: %s'):format(case[1], vim.inspect(found)))
end

-- 3. The week's agenda.
local function wait_for_view(title)
  wait('the agenda `' .. title .. '`', function()
    return vim.bo.filetype == 'org-agenda' and vim.startswith(vim.api.nvim_buf_get_lines(0, 0, 1, false)[1] or '', title)
  end)
end
require('org-lsp').run('org.agenda.list')
wait_for_view('Week-agenda')
local agenda = vim.api.nvim_get_current_buf()
wait('the tokens of the agenda', function()
  return vim.lsp.semantic_tokens.get_at_pos(agenda, 0, 0) ~= nil
end)
check_looks(agenda, client, 'the agenda')
-- The line of the clocked entry is highlighted from edge to edge.
local clocked
for row, text in ipairs(vim.api.nvim_buf_get_lines(agenda, 0, -1, false)) do
  if text:find('Clocked in') then
    clocked = row - 1
  end
end
local line_highlights = vim.api.nvim_create_namespace('org_lsp_line_highlights')
wait('the clocked line highlighted', function()
  local marks = vim.api.nvim_buf_get_extmarks(agenda, line_highlights, { clocked, 0 }, { clocked, -1 }, { details = true })
  return marks[1] and marks[1][4].line_hl_group == '@lsp.type.agendaClocking.org-agenda'
end)

-- 4. The TODO list: the clocked entry's line in no other face.
require('org-lsp').run('org.agenda.todos')
wait_for_view('Global list')
check_looks(vim.api.nvim_get_current_buf(), client, 'the TODO list')

-- 5. The date prompt, with the days of the journal and the daily notes.
vim.cmd('only')
vim.cmd.buffer(sample)
vim.api.nvim_win_set_cursor(0, { row_of('* Headline level one') + 1, 0 })
require('org-lsp').run('org.schedule')
local prompt
wait('the date prompt', function()
  vim.api.nvim_feedkeys('', 'x', false)
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local bufnr = vim.api.nvim_win_get_buf(win)
    if vim.bo[bufnr].filetype == 'org-date' then
      prompt = bufnr
    end
  end
  return prompt ~= nil and #vim.api.nvim_buf_get_extmarks(prompt, vim.api.nvim_create_namespace('org_lsp_prompt_highlights'), 0, -1, {}) > 0
end)
check_looks(prompt, client, 'the date prompt')

for _, look in ipairs(remaining) do
  table.insert(failures, look.token .. ': no token to show it on')
end
if #failures > 0 then
  io.stderr:write(('FAIL: %d of %d looks:\n  %s\n'):format(#failures, looks, table.concat(failures, '\n  ')))
  vim.cmd('cquit 1')
end
io.stdout:write(('ok: %d looks shown in their groups, %d checks\n'):format(checked, checks))
vim.cmd('qall!')
