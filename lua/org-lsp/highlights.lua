--- Maps the server's semantic tokens to highlight groups.
---
--- Neovim turns a token of type `T` with modifier `M` into the groups
--- `@lsp.type.T.org`, `@lsp.mod.M.org` and `@lsp.typemod.T.M.org`. Each is
--- linked here to the group nvim-orgmode uses for the same thing when a
--- colorscheme defines it, and to a standard group otherwise. All links are
--- defaults: define any of these groups yourself to override them.
local M = {}

local LEVELS = { 'Title', 'Constant', 'Identifier', 'Statement', 'PreProc', 'Type', 'Special', 'String' }

-- { our group, nvim-orgmode's group, fallback }
local LINKS = {
  { '@lsp.type.todoKeyword.org', '@org.keyword.todo', 'DiagnosticError' },
  { '@lsp.typemod.todoKeyword.done.org', '@org.keyword.done', 'DiagnosticOk' },
  { '@lsp.type.priority.org', '@org.priority.highest', '@comment.error' },
  { '@lsp.type.tag.org', '@org.tag', '@tag.attribute' },
  { '@lsp.type.planning.org', '@org.plan', 'Constant' },
  { '@lsp.type.timestamp.org', '@org.timestamp.active', '@keyword' },
  { '@lsp.typemod.timestamp.inactive.org', '@org.timestamp.inactive', '@comment' },
  { '@lsp.type.drawer.org', '@org.drawer', '@property' },
  { '@lsp.type.property.org', '@org.properties.name', '@property' },
  { '@lsp.type.keyword.org', '@org.directive', '@comment' },
  { '@lsp.type.block.org', '@org.block', '@comment' },
  { '@lsp.type.comment.org', '@org.comment', '@comment' },
  { '@lsp.type.bullet.org', '@org.bullet', '@markup.list' },
  { '@lsp.type.checkbox.org', '@org.checkbox', '@markup.list.unchecked' },
  { '@lsp.typemod.checkbox.checked.org', '@org.checkbox.checked', '@markup.list.checked' },
  { '@lsp.typemod.checkbox.partial.org', '@org.checkbox.halfchecked', '@markup.list.unchecked' },
  { '@lsp.type.link.org', '@org.hyperlink', '@markup.link' },
  { '@lsp.type.code.org', '@org.code', '@markup.raw' },
  { '@lsp.type.verbatim.org', '@org.verbatim', '@markup.raw' },
  { '@lsp.type.tableDelimiter.org', '@org.table.delimiter', '@punctuation.special' },
  { '@lsp.type.footnote.org', '@org.footnote', '@markup.link.url' },
  { '@lsp.type.latex.org', '@org.latex', '@markup.math' },
  { '@lsp.type.entity.org', '@org.latex', '@markup.math' },
  { '@lsp.type.cookie.org', '@org.priority.highest', '@number' },
  { '@lsp.type.target.org', '@org.hyperlink.desc', '@markup.link.label' },
  { '@lsp.type.macro.org', '@org.directive', '@function.macro' },
  { '@lsp.mod.bold.org', '@org.bold', '@markup.strong' },
  { '@lsp.mod.italic.org', '@org.italic', '@markup.italic' },
  { '@lsp.mod.underline.org', '@org.underline', '@markup.underline' },
  { '@lsp.mod.strikethrough.org', '@org.strikethrough', '@markup.strikethrough' },
}

for level, fallback in ipairs(LEVELS) do
  local org = '@org.headline.level' .. level
  table.insert(LINKS, { ('@lsp.typemod.headline.level%d.org'):format(level), org, fallback })
  table.insert(LINKS, { ('@lsp.typemod.stars.level%d.org'):format(level), org, fallback })
end

--- Whether a highlight group has been given a definition.
local function is_defined(name)
  return not vim.tbl_isempty(vim.api.nvim_get_hl(0, { name = name }))
end

--- (Re)define the default links. Runs on attach and after a colorscheme change.
function M.define()
  for _, entry in ipairs(LINKS) do
    local name, org, fallback = entry[1], entry[2], entry[3]
    vim.api.nvim_set_hl(0, name, { link = is_defined(org) and org or fallback, default = true })
  end
end

return M
