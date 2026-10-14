--- Links the server's semantic tokens to highlight groups.
---
--- Neovim turns a token of type `T` with modifier `M` in a buffer of
--- filetype `ft` into the groups `@lsp.type.T.ft`, `@lsp.mod.M.ft` and
--- `@lsp.typemod.T.M.ft`. The server says which groups each should look
--- like: the one nvim-orgmode colorschemes define for the same thing, and
--- a standard group to fall back on. All links are defaults: define any of
--- these groups yourself to override them.
local M = {}

--- Whether a highlight group has been given a definition.
local function is_defined(name)
  return not vim.tbl_isempty(vim.api.nvim_get_hl(0, { name = name }))
end

--- The highlight group of a selector: `type`, `type.modifier` or `*.modifier`.
---@param selector string
---@param filetype string
---@return string
local function group(selector, filetype)
  local type, modifier = selector:match('^([^.]+)%.?(.*)$')
  if type == '*' then
    return ('@lsp.mod.%s.%s'):format(modifier, filetype)
  elseif modifier ~= '' then
    return ('@lsp.typemod.%s.%s.%s'):format(type, modifier, filetype)
  end
  return ('@lsp.type.%s.%s'):format(type, filetype)
end

--- (Re)define the default links. Runs when the server starts and after a
--- colorscheme change.
---@param manifest table what the server announced
function M.define(manifest)
  -- Org files, the agenda, and the date prompt's calendar.
  local filetypes = { 'org', manifest.agenda.languageId }
  if manifest.datePrompt then
    table.insert(filetypes, manifest.datePrompt.languageId)
  end
  for _, filetype in ipairs(filetypes) do
    for _, look in ipairs(manifest.highlights) do
      -- The first group that exists, or else the last one, the fallback.
      local target = look.vim[#look.vim]
      for _, candidate in ipairs(look.vim) do
        if is_defined(candidate) then
          target = candidate
          break
        end
      end
      if target then
        vim.api.nvim_set_hl(0, group(look.token, filetype), { link = target, default = true })
      end
    end
  end
end

--- Faces of particular TODO keywords and priorities
--- (`org_todo_keyword_faces`, `org_priority_faces`), as the server
--- announced them: `{ todo = { WAIT = face }, priority = { A = face } }`.
local faces = { todo = {}, priority = {} }

--- The highlight group of the face of keyword or priority `name`.
---@param kind 'todo'|'priority'
---@param name string
---@return string
local function face_group(kind, name)
  local prefix = ({ todo = 'Todo', priority = 'Priority', tag = 'Tag' })[kind] or 'Priority'
  return ('OrgLsp%sFace_%s'):format(prefix, (name:gsub('[^%w_]', '_')))
end

--- Define highlight group `name` as `face` looks, unless it is defined.
---@param name string
---@param face table
local function define_face(name, face)
  pcall(vim.api.nvim_set_hl, 0, name, {
    fg = face.foreground,
    bg = face.background,
    bold = face.bold,
    italic = face.italic,
    underline = face.underline,
    strikethrough = face.strikethrough,
    default = true,
  })
end

--- (Re)define the groups of the faces; with `given`, those faces from now
--- on. Runs when the server starts and after a colorscheme change.
---@param given? table
function M.define_faces(given)
  faces = given or faces
  for kind, list in pairs(faces) do
    for name, face in pairs(list) do
      define_face(face_group(kind, name), face)
    end
  end
end

--- The highlight group of a choice the server offers with its look: the
--- face of its keyword (`face`), or else the group of the token it is
--- highlighted as (`highlight`, a selector such as `todoKeyword.done`).
---@param choice { label: string, highlight?: string, face?: table }
---@return string?
function M.choice_group(choice)
  if choice.face then
    local name = face_group('todo', choice.label)
    define_face(name, choice.face)
    return name
  end
  return choice.highlight and group(choice.highlight, 'org') or nil
end

--- Give the TODO keywords and priorities of buffer `bufnr` that have a
--- face of their own that face, over the server's highlighting.
---@param bufnr integer
--- `org-tag-faces`: each tag of the headline's `:a:b:` that has a face
--- of its own in that face, its name without the colons, as Org gives it.
---@param bufnr integer
---@param token table
---@param client_id integer
---@param priority integer
local function tag_faces(bufnr, token, client_id, priority)
  local ok, text = pcall(vim.api.nvim_buf_get_text, bufnr, token.line, token.start_col, token.line, token.end_col, {})
  -- In Org files only: the agenda shows tags in `org-tag` alone.
  if not ok or vim.tbl_isempty(faces.tag or {}) or vim.bo[bufnr].filetype ~= 'org' then
    return
  end
  -- The marks go in the semantic tokens' namespace, cleared with them.
  local namespace = vim.api.nvim_create_namespace('nvim.lsp.semantic_tokens:' .. client_id)
  local line = text[1] or ''
  local at = 1
  while true do
    local start, finish, name = line:find(':([^:%s]+):', at)
    if not start then
      break
    end
    if faces.tag[name] then
      pcall(vim.api.nvim_buf_set_extmark, bufnr, namespace, token.line, token.start_col + start, {
        end_col = token.start_col + finish - 1,
        hl_group = face_group('tag', name),
        priority = priority,
        strict = false,
      })
    end
    -- The colon after a tag starts the next.
    at = finish
  end
end

function M.attach_faces(bufnr)
  if vim.tbl_isempty(faces.todo or {}) and vim.tbl_isempty(faces.priority or {}) and vim.tbl_isempty(faces.tag or {}) then
    return
  end
  local priority = (vim.hl or vim.highlight).priorities.semantic_tokens + 3
  vim.api.nvim_create_autocmd('LspTokenUpdate', {
    buffer = bufnr,
    group = vim.api.nvim_create_augroup('org_lsp_faces_' .. bufnr, { clear = true }),
    callback = function(args)
      local token = args.data.token
      if token.type == 'tag' then
        return tag_faces(bufnr, token, args.data.client_id, priority)
      end
      local kind = (token.type == 'todoKeyword' and 'todo') or (token.type == 'priority' and 'priority') or nil
      if not kind then
        return
      end
      local ok, text = pcall(vim.api.nvim_buf_get_text, bufnr, token.line, token.start_col, token.line, token.end_col, {})
      if not ok then
        return
      end
      local name = text[1] or ''
      if kind == 'priority' then
        name = name:match('%[#(.-)%]') or name
      end
      if faces[kind] and faces[kind][name] then
        vim.lsp.semantic_tokens.highlight_token(token, bufnr, args.data.client_id, face_group(kind, name), {
          priority = priority,
        })
      end
    end,
  })
end

return M
