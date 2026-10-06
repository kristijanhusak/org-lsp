--- Source blocks (`#+begin_src python`) with their code highlighted by the
--- tree-sitter parser of its language and that parser's highlight query,
--- as Emacs shows them with `org-src-fontify-natively`. The server leaves
--- a block's code to the editor. A language without a parser stays as it
--- is.
local M = {}

local ns = vim.api.nvim_create_namespace('org_lsp_src')

--- Org's names for languages (`org-src-lang-modes`, and Emacs's mode
--- names) where the parser has another.
local aliases = {
  ['emacs-lisp'] = 'elisp',
  sh = 'bash',
  shell = 'bash',
  zsh = 'bash',
  ksh = 'bash',
  dash = 'bash',
  c = 'c',
  ['c++'] = 'cpp',
  js = 'javascript',
  ts = 'typescript',
  py = 'python',
  rb = 'ruby',
  rs = 'rust',
  yml = 'yaml',
  sqlite = 'sql',
  dot = 'dot',
  ocaml = 'ocaml',
  ['shell-script'] = 'bash',
}

--- The parser for Org's name of a language, when there is one.
---@param name string
---@return string?
local function language(name)
  local lower = name:lower()
  local lang = aliases[lower] or vim.treesitter.language.get_lang(lower) or lower
  pcall(vim.treesitter.language.add, lang)
  if not pcall(vim.treesitter.language.inspect, lang) then
    return nil
  end
  return lang
end

--- The source blocks of the buffer: their language as written, and the
--- first and last lines of their code (from 0).
---@param lines string[]
---@return { name: string, first: integer, last: integer }[]
local function blocks(lines)
  local found, open = {}, nil
  for index, line in ipairs(lines) do
    local lower = line:lower()
    if not open then
      local at = lower:match('^%s*#%+begin_src%s+()%S')
      if at then
        open = { name = line:match('^(%S+)', at), start = index }
      end
    elseif lower:match('^%s*#%+end_src') then
      table.insert(found, { name = open.name, first = open.start, last = index - 2 })
      open = nil
    end
  end
  return found
end

--- Highlight the code of every source block of the buffer.
---@param bufnr integer
function M.highlight(bufnr)
  vim.api.nvim_buf_clear_namespace(bufnr, ns, 0, -1)
  local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
  for _, block in ipairs(blocks(lines)) do
    local lang = block.last >= block.first and language(block.name)
    if lang then
      local text = table.concat(lines, '\n', block.first + 1, block.last + 1)
      local ok, parser = pcall(vim.treesitter.get_string_parser, text, lang)
      if ok and parser then
        parser:parse(true)
        -- The language's own tree, and those injected into it.
        parser:for_each_tree(function(tree, language_tree)
          local query = vim.treesitter.query.get(language_tree:lang(), 'highlights')
          if not query then
            return
          end
          for id, node in query:iter_captures(tree:root(), text) do
            local capture = query.captures[id]
            if not capture:match('^_') and capture ~= 'spell' and capture ~= 'nospell' and capture ~= 'conceal' then
              local start_row, start_col, end_row, end_col = node:range()
              pcall(vim.api.nvim_buf_set_extmark, bufnr, ns, block.first + start_row, start_col, {
                end_row = block.first + end_row,
                end_col = end_col,
                hl_group = '@' .. capture .. '.' .. language_tree:lang(),
                priority = vim.hl and vim.hl.priorities.treesitter or 100,
              })
            end
          end
        end)
      end
    end
  end
end

--- Highlight the buffer's source blocks, and again a moment after each
--- change.
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

return M
