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
  for _, filetype in ipairs({ 'org', manifest.agenda.languageId }) do
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

return M
