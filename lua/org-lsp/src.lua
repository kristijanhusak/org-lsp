--- Code in its own language's colors, as Emacs shows it with
--- `org-src-fontify-natively`: the code of source blocks
--- (`#+begin_src python`), of export blocks (`#+begin_export html`) and of
--- inline source blocks (`src_python{print(1)}`). The server leaves that
--- code to the editor.
---
--- The language's tree-sitter parser and highlight query color it when
--- there is a parser; else Vim's syntax file for the language does (Emacs
--- Lisp has no tree-sitter parser in nvim-treesitter). A language with
--- neither stays as it is.
local M = {}

local ns = vim.api.nvim_create_namespace('org_lsp_src')

--- Org's names for languages (`org-src-lang-modes`, and Emacs's mode
--- names) where the tree-sitter parser has another.
local parsers = {
  ['emacs-lisp'] = 'elisp',
  sh = 'bash',
  shell = 'bash',
  zsh = 'bash',
  ksh = 'bash',
  dash = 'bash',
  ['c++'] = 'cpp',
  js = 'javascript',
  ts = 'typescript',
  py = 'python',
  rb = 'ruby',
  rs = 'rust',
  yml = 'yaml',
  sqlite = 'sql',
  ['shell-script'] = 'bash',
}

--- The same, for Vim's syntax files.
local syntaxes = {
  ['emacs-lisp'] = 'lisp',
  elisp = 'lisp',
  shell = 'sh',
  bash = 'sh',
  ['shell-script'] = 'sh',
  js = 'javascript',
  ts = 'typescript',
  py = 'python',
  rb = 'ruby',
  ['c++'] = 'cpp',
  yml = 'yaml',
  sqlite = 'sql',
  rs = 'rust',
}

--- The tree-sitter parser for Org's name of a language, when there is one.
---@param name string
---@return string?
local function parser_for(name)
  local lower = name:lower()
  local lang = parsers[lower] or vim.treesitter.language.get_lang(lower) or lower
  pcall(vim.treesitter.language.add, lang)
  if not pcall(vim.treesitter.language.inspect, lang) then
    return nil
  end
  return lang
end

--- Vim's syntax file for Org's name of a language, when there is one.
---@param name string
---@return string?
local function syntax_for(name)
  local lower = name:lower()
  local lang = syntaxes[lower] or lower
  if lang:match('^[%w_]+$') and #vim.api.nvim_get_runtime_file('syntax/' .. lang .. '.vim', false) > 0 then
    return lang
  end
  return nil
end

--- Where `text[from]` (`[` or `{`) is closed, counting nested pairs.
---@param text string
---@param from integer
---@return integer?
local function closing(text, from)
  local open = text:sub(from, from)
  local close = open == '[' and ']' or '}'
  local depth = 0
  for at = from, #text do
    local c = text:sub(at, at)
    if c == open then
      depth = depth + 1
    elseif c == close then
      depth = depth - 1
      if depth == 0 then
        return at
      end
    end
  end
  return nil
end

---@class org_lsp.SrcCode
---@field name string The language as written.
---@field row integer Where the code starts (from 0).
---@field col integer
---@field text string

--- The code of the buffer's source and export blocks, and of its inline
--- source blocks.
---@param lines string[]
---@return org_lsp.SrcCode[]
local function code_of(lines)
  local found, open = {}, nil
  for index, line in ipairs(lines) do
    local lower = line:lower()
    if open then
      if lower:match('^%s*#%+end_' .. open.kind .. '%f[^%w_]') then
        if index - 1 > open.start then
          table.insert(found, {
            name = open.name,
            row = open.start,
            col = 0,
            text = table.concat(lines, '\n', open.start + 1, index - 1),
          })
        end
        open = nil
      end
    else
      local kind, at = lower:match('^%s*#%+begin_(%a+)%s+()%S')
      if kind == 'src' or kind == 'export' then
        open = { kind = kind, name = line:match('^(%S+)', at), start = index }
      else
        -- `src_LANG[HEADERS]{CODE}`, at the start of a symbol.
        local from = 1
        while true do
          local start, name_end, name = line:find('src_([^%s%[{]+)', from)
          if not start then
            break
          end
          from = name_end + 1
          local before = start > 1 and line:sub(start - 1, start - 1) or ''
          if not before:match('[%w_%-]') then
            local brace = name_end + 1
            if line:sub(brace, brace) == '[' then
              brace = (closing(line, brace) or #line) + 1
            end
            local close = line:sub(brace, brace) == '{' and closing(line, brace)
            if close then
              if close > brace + 1 then
                table.insert(found, { name = name, row = index - 1, col = brace, text = line:sub(brace + 1, close - 1) })
              end
              from = close + 1
            end
          end
        end
      end
    end
  end
  return found
end

--- Highlight `code` with the tree-sitter parser `lang`.
---@param bufnr integer
---@param code org_lsp.SrcCode
---@param lang string
local function with_parser(bufnr, code, lang)
  local ok, parser = pcall(vim.treesitter.get_string_parser, code.text, lang)
  if not ok or not parser then
    return
  end
  parser:parse(true)
  -- The language's own tree, and those injected into it.
  parser:for_each_tree(function(tree, language_tree)
    local query = vim.treesitter.query.get(language_tree:lang(), 'highlights')
    if not query then
      return
    end
    for id, node in query:iter_captures(tree:root(), code.text) do
      local capture = query.captures[id]
      if not capture:match('^_') and capture ~= 'spell' and capture ~= 'nospell' and capture ~= 'conceal' then
        local start_row, start_col, end_row, end_col = node:range()
        pcall(vim.api.nvim_buf_set_extmark, bufnr, ns, code.row + start_row, (start_row == 0 and code.col or 0) + start_col, {
          end_row = code.row + end_row,
          end_col = (end_row == 0 and code.col or 0) + end_col,
          hl_group = '@' .. capture .. '.' .. language_tree:lang(),
          priority = vim.hl and vim.hl.priorities.treesitter or 100,
        })
      end
    end
  end)
end

--- A hidden buffer per syntax, where Vim's syntax highlighting is read.
---@type table<string, integer>
local scratch = {}
--- What a syntax made of a text: `{row, start, end, group}` spans.
---@type table<string, integer[][]>
local spans_cache, cached = {}, 0

---@param syntax string
---@param text string
---@return table[]
local function syntax_spans(syntax, text)
  local key = syntax .. '\0' .. text
  if spans_cache[key] then
    return spans_cache[key]
  end
  local buf = scratch[syntax]
  if not (buf and vim.api.nvim_buf_is_valid(buf)) then
    buf = vim.api.nvim_create_buf(false, true)
    vim.bo[buf].bufhidden = 'hide'
    scratch[syntax] = buf
    vim.bo[buf].syntax = syntax
  end
  local lines = vim.split(text, '\n', { plain = true })
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  local spans = {}
  vim.api.nvim_buf_call(buf, function()
    vim.cmd('syntax sync fromstart')
    for row, line in ipairs(lines) do
      local span
      for col = 1, #line do
        local group = vim.fn.synIDattr(vim.fn.synID(row, col, 1), 'name')
        if span and span[4] == group and span[3] == col - 1 then
          span[3] = col
        else
          span = group ~= '' and { row - 1, col - 1, col, group } or nil
          if span then
            table.insert(spans, span)
          end
        end
      end
    end
  end)
  if cached > 500 then
    spans_cache, cached = {}, 0
  end
  spans_cache[key], cached = spans, cached + 1
  return spans
end

--- Highlight `code` with Vim's syntax file `syntax`.
---@param bufnr integer
---@param code org_lsp.SrcCode
---@param syntax string
local function with_syntax(bufnr, code, syntax)
  for _, span in ipairs(syntax_spans(syntax, code.text)) do
    local row, start_col, end_col, group = span[1], span[2], span[3], span[4]
    local offset = row == 0 and code.col or 0
    pcall(vim.api.nvim_buf_set_extmark, bufnr, ns, code.row + row, offset + start_col, {
      end_col = offset + end_col,
      hl_group = group,
      priority = vim.hl and vim.hl.priorities.treesitter or 100,
    })
  end
end

--- Highlight the code of every source, export and inline source block of
--- the buffer.
---@param bufnr integer
function M.highlight(bufnr)
  vim.api.nvim_buf_clear_namespace(bufnr, ns, 0, -1)
  for _, code in ipairs(code_of(vim.api.nvim_buf_get_lines(bufnr, 0, -1, false))) do
    local lang = parser_for(code.name)
    if lang then
      with_parser(bufnr, code, lang)
    else
      local syntax = syntax_for(code.name)
      if syntax then
        with_syntax(bufnr, code, syntax)
      end
    end
  end
end

--- Highlight the buffer's code, and again a moment after each change.
---@param bufnr integer
function M.attach(bufnr)
  if vim.b[bufnr].org_lsp_src then
    return
  end
  vim.b[bufnr].org_lsp_src = true
  M.highlight(bufnr)
  local timer = assert(vim.uv.new_timer())
  vim.api.nvim_buf_attach(bufnr, false, {
    on_lines = function()
      timer:stop()
      timer:start(
        100,
        0,
        vim.schedule_wrap(function()
          if vim.api.nvim_buf_is_valid(bufnr) then
            M.highlight(bufnr)
          end
        end)
      )
    end,
    on_detach = function()
      timer:stop()
      timer:close()
    end,
  })
end

M.namespace = ns
M.code_of = code_of

return M
