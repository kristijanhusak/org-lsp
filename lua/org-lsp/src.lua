--- Code in its own language's colors, as Emacs shows it with
--- `org-src-fontify-natively`: the code of source blocks
--- (`#+begin_src python`), of export blocks (`#+begin_export html`) and of
--- inline source blocks (`src_python{print(1)}`). The server says where
--- that code is and in which language (`org-lsp/codeBlocks`), after every
--- change; the editor colors it.
---
--- The language's tree-sitter parser and highlight query color it when
--- there is a parser, as extmarks; else Vim's syntax file for the
--- language does, included in the buffer's own syntax for regions at the
--- code's lines and columns, as the Vim plugin does (Emacs Lisp has no
--- tree-sitter parser in nvim-treesitter). A language with neither stays
--- as it is.
local M = {}

local ns = vim.api.nvim_create_namespace('org_lsp_src')

--- The editor's names of languages (the server's) where Vim's syntax file
--- has another.
local syntaxes = {
  elisp = 'lisp',
  bash = 'sh',
}

--- Whether each tree-sitter parser and Vim syntax file was found, by
--- name: finding them looks through the runtime path, and every block of
--- every `org-lsp/codeBlocks` needs one. A missing one is looked for again
--- after a while, and when the runtime path changed, in case it was
--- installed since.
---@type { runtimepath: string?, [string]: { found: boolean, at: integer } }
local found = {}

--- How long a missing parser or syntax file is taken as missing, in ms.
local missing_for = 10000

--- Whether `look` finds the thing `key` names, from `found` while it holds.
---@param key string
---@param look fun(): boolean
---@return boolean
local function memo(key, look)
  if found.runtimepath ~= vim.o.runtimepath then
    found = { runtimepath = vim.o.runtimepath }
  end
  local now = vim.uv.now()
  local known = found[key]
  if not known or (not known.found and now - known.at > missing_for) then
    known = { found = look(), at = now }
    found[key] = known
  end
  return known.found
end

--- The tree-sitter parser of a language, when there is one.
---@param name string
---@return string?
local function parser_for(name)
  local lang = vim.treesitter.language.get_lang(name) or name
  local there = memo('parser\0' .. lang, function()
    pcall(vim.treesitter.language.add, lang)
    return pcall(vim.treesitter.language.inspect, lang)
  end)
  return there and lang or nil
end

--- Vim's syntax file of a language, when there is one.
---@param name string
---@return string?
local function syntax_for(name)
  local lang = syntaxes[name] or name
  if not lang:match('^[%w_]+$') then
    return nil
  end
  local there = memo('syntax\0' .. lang, function()
    return #vim.api.nvim_get_runtime_file('syntax/' .. lang .. '.vim', false) > 0
  end)
  return there and lang or nil
end

---@class org_lsp.SrcCode
---@field language string The editor's name of the language.
---@field row integer Where the code starts (from 0).
---@field col integer In bytes.
---@field end_row integer Where it ends.
---@field end_col integer
---@field text string

--- Highlight `code` with the tree-sitter parser `lang`.
---@param bufnr integer
---@param code org_lsp.SrcCode
---@param lang string
---@param marks integer[] Where the extmarks made are added.
local function with_parser(bufnr, code, lang, marks)
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
        local set, id = pcall(vim.api.nvim_buf_set_extmark, bufnr, ns, code.row + start_row, (start_row == 0 and code.col or 0) + start_col, {
          end_row = code.row + end_row,
          end_col = (end_row == 0 and code.col or 0) + end_col,
          hl_group = '@' .. capture .. '.' .. language_tree:lang(),
          priority = vim.hl and vim.hl.priorities.treesitter or 100,
        })
        if set then
          table.insert(marks, id)
        end
      end
    end
  end)
end

--- The syntax regions of each buffer's code, by group (`orgLspSrcCode_python`).
---@type table<integer, string[]>
local regions = {}

--- The code of the buffer in the languages of Vim's syntax files, as
--- regions of its own syntax: each language's syntax file included once,
--- as Vim's Markdown does for fenced code, and a region from where the
--- code starts to where it ends, which the next `org-lsp/codeBlocks`
--- replaces. No other buffer is needed.
---@param bufnr integer
---@param codes { syntax: string, code: org_lsp.SrcCode }[]
local function syntax_regions(bufnr, codes)
  if not regions[bufnr] and #codes == 0 then
    return
  end
  vim.api.nvim_buf_call(bufnr, function()
    for _, group in ipairs(regions[bufnr] or {}) do
      pcall(vim.cmd, 'syntax clear ' .. group)
    end
    regions[bufnr] = nil
    if #codes == 0 then
      return
    end
    local groups = {}
    -- The languages included, checked once each.
    local included = {}
    for _, each in ipairs(codes) do
      local cluster = 'orgLspSrc_' .. each.syntax
      -- The syntax file once: Vim's syntax for the buffer forgets it when
      -- it is set again. The listing says whether the cluster is there,
      -- with blanks after a short name; it is no error when the buffer has
      -- no syntax items at all.
      if not included[cluster] then
        local ok, listed = pcall(vim.api.nvim_exec2, 'silent! syntax list @' .. cluster, { output = true })
        if not (ok and listed.output:find('\n' .. cluster .. '%s+cluster=')) then
          local current = vim.b.current_syntax
          vim.b.current_syntax = nil
          pcall(vim.cmd, 'silent! syntax include @' .. cluster .. ' syntax/' .. each.syntax .. '.vim')
          vim.b.current_syntax = current
        end
        included[cluster] = true
      end
      local group = 'orgLspSrcCode_' .. each.syntax
      if not groups[group] then
        groups[group] = true
        table.insert(groups, group)
      end
      local code = each.code
      -- Within the blocks of the syntax for Org (syntax/org.vim), whose
      -- regions take the whole block.
      vim.cmd(
        ('syntax region %s start=/\\%%%dl\\%%%dc/ end=/\\%%%dl\\%%%dc/ keepend contains=@%s containedin=orgSyntaxRawBlock'):format(
          group,
          code.row + 1,
          code.col + 1,
          code.end_row + 1,
          code.end_col + 1,
          cluster
        )
      )
    end
    -- The included files sync as suits their language alone: the sync of
    -- the syntax for Org from the headline before again, as a block ends
    -- before the next headline. From the start, a change far in the file
    -- takes longer than 'redrawtime'.
    vim.cmd('syntax sync clear')
    vim.cmd([[syntax sync match orgSyntaxSync grouphere NONE /^\*\+\s/]])
    regions[bufnr] = groups
  end)
end

---@class org_lsp.SrcDone
---@field marks integer[] The extmarks of the code's colors.
---@field row integer? Where the first one starts, relative to the code.

--- What each buffer's code was colored with, by language, column and
--- text: code that has not changed keeps its extmarks, which move with
--- the lines around it.
---@type table<integer, table<string, org_lsp.SrcDone[]>>
local done = {}

---@param code org_lsp.SrcCode
---@return string
local function key_of(code)
  return code.language .. '\0' .. code.col .. '\0' .. code.text
end

--- Whether the extmarks of `kept` are still where `code` is: an edit that
--- took the code away and put it back leaves them elsewhere.
---@param bufnr integer
---@param code org_lsp.SrcCode
---@param kept org_lsp.SrcDone
local function in_place(bufnr, code, kept)
  if not kept.marks[1] then
    return true
  end
  local at = vim.api.nvim_buf_get_extmark_by_id(bufnr, ns, kept.marks[1], {})
  return at[1] ~= nil and at[1] == code.row + kept.row
end

--- Color `code` with its tree-sitter parser.
---@param bufnr integer
---@param code org_lsp.SrcCode
---@param lang string
---@return org_lsp.SrcDone
local function color(bufnr, code, lang)
  local marks = {}
  with_parser(bufnr, code, lang, marks)
  local first = marks[1] and vim.api.nvim_buf_get_extmark_by_id(bufnr, ns, marks[1], {})
  return { marks = marks, row = first and first[1] and first[1] - code.row or 0 }
end

--- The last blocks the server sent for each buffer, as it sent them: for
--- the tests, and to put the syntax regions back when the buffer's
--- syntax is set again.
---@type table<integer, { version: integer, encoding: string, blocks: table[] }>
M.blocks = {}

--- The code of `block` in the buffer.
---@param bufnr integer
---@param block { range: lsp.Range, language: string }
---@param encoding string
---@return org_lsp.SrcCode?
local function code_of(bufnr, block, encoding)
  local count = vim.api.nvim_buf_line_count(bufnr)
  local start, finish = block.range.start, block.range['end']
  if finish.line >= count then
    return nil
  end
  local function byte(position)
    local line = vim.api.nvim_buf_get_lines(bufnr, position.line, position.line + 1, false)[1] or ''
    local ok, col = pcall(vim.str_byteindex, line, encoding, position.character, false)
    return ok and col or #line
  end
  local col, end_col = byte(start), byte(finish)
  local ok, lines = pcall(vim.api.nvim_buf_get_text, bufnr, start.line, col, finish.line, end_col, {})
  if not ok then
    return nil
  end
  return {
    language = block.language,
    row = start.line,
    col = col,
    end_row = finish.line,
    end_col = end_col,
    text = table.concat(lines, '\n'),
  }
end

--- Color the code of the buffer's blocks: again only the code that
--- changed since the last time.
---@param bufnr integer
---@param blocks table[] As `org-lsp/codeBlocks` has them.
---@param encoding string The client's position encoding.
local function highlight(bufnr, blocks, encoding)
  local before = done[bufnr] or {}
  local now, by_syntax = {}, {}
  for _, block in ipairs(blocks) do
    local code = type(block.language) == 'string' and code_of(bufnr, block, encoding)
    local lang = code and parser_for(code.language)
    if code and lang then
      local key = key_of(code)
      local kept = before[key] and table.remove(before[key], 1)
      if kept and not in_place(bufnr, code, kept) then
        for _, id in ipairs(kept.marks) do
          vim.api.nvim_buf_del_extmark(bufnr, ns, id)
        end
        kept = nil
      end
      now[key] = now[key] or {}
      table.insert(now[key], kept or color(bufnr, code, lang))
    elseif code then
      local syntax = syntax_for(code.language)
      if syntax then
        table.insert(by_syntax, { syntax = syntax, code = code })
      end
    end
  end
  -- The code that is gone or changed.
  for _, list in pairs(before) do
    for _, gone in ipairs(list) do
      for _, id in ipairs(gone.marks) do
        vim.api.nvim_buf_del_extmark(bufnr, ns, id)
      end
    end
  end
  done[bufnr] = now
  syntax_regions(bufnr, by_syntax)
end

--- Take the colors of the buffer's code away.
---@param bufnr integer
function M.clear(bufnr)
  vim.api.nvim_buf_clear_namespace(bufnr, ns, 0, -1)
  done[bufnr] = nil
  syntax_regions(bufnr, {})
  M.blocks[bufnr] = nil
end

--- Where the code of a document is (`org-lsp/codeBlocks`): `blocks` of
--- `{ range, language, name, kind }` for its `version`.
---@param params { uri: string, version: integer, blocks: table[] }
---@param encoding string The client's position encoding.
function M.apply(params, encoding)
  local bufnr = require('org-lsp.buffers').of(params.uri)
  -- For older text than the buffer has: newer blocks are on their way.
  if not bufnr or vim.lsp.util.buf_versions[bufnr] ~= params.version then
    return
  end
  if not require('org-lsp').config.src_highlight then
    M.clear(bufnr)
    return
  end
  if not vim.b[bufnr].org_lsp_src then
    vim.b[bufnr].org_lsp_src = true
    vim.api.nvim_buf_attach(bufnr, false, {
      on_detach = function()
        done[bufnr], regions[bufnr], M.blocks[bufnr] = nil, nil, nil
        pcall(function()
          vim.b[bufnr].org_lsp_src = nil
        end)
      end,
    })
  end
  M.blocks[bufnr] = { version = params.version, encoding = encoding, blocks = params.blocks or {} }
  highlight(bufnr, params.blocks or {}, encoding)
end

--- Put the syntax regions back after the buffer's syntax was set again,
--- which forgets them (`replace_syntax`).
---@param bufnr integer
function M.reapply(bufnr)
  local last = M.blocks[bufnr]
  if last and vim.lsp.util.buf_versions[bufnr] == last.version then
    regions[bufnr] = nil
    highlight(bufnr, last.blocks, last.encoding)
  end
end

M.namespace = ns

return M
