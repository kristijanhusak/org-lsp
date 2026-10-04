--- Neovim glue for the org-lsp language server.
---
--- The server does the Org work; this module only starts it, maps its
--- semantic tokens to highlight groups, and binds keys to its code actions.
--- It works without configuration. To change something, either set
--- `vim.g.org_lsp` before the plugin loads or call `setup()`:
---
---     require('org-lsp').setup({
---       todo_keywords = { 'TODO', 'NEXT', '|', 'DONE' },
---       startup_folded = 'overview',
---     })
local M = {}

local SERVER = 'org_lsp'

---@class OrgLspMappings
---@field todo_next string|false
---@field todo_previous string|false
---@field cycle string|false
---@field global_cycle string|false

---@class OrgLspConfig
---@field cmd? string[] Server command. Found automatically when unset.
---@field todo_keywords? string[] Default TODO sequence, e.g. { 'TODO', '|', 'DONE' }.
---@field startup_folded 'showeverything'|'overview' Fold state of a newly opened file.
---@field folding boolean Whether to fold with the server's folding ranges.
---@field replace_syntax boolean Whether to turn off Vim's regex syntax once semantic tokens arrive.
---@field mappings OrgLspMappings|false Buffer-local keys; `false` disables them all.
local defaults = {
  cmd = nil,
  todo_keywords = nil,
  startup_folded = 'showeverything',
  folding = true,
  replace_syntax = true,
  mappings = {
    todo_next = 'cit',
    todo_previous = 'ciT',
    cycle = '<Tab>',
    global_cycle = '<S-Tab>',
  },
}

---@type OrgLspConfig
M.config = vim.tbl_deep_extend('force', defaults, vim.g.org_lsp or {})

--- The directory this plugin is installed in.
local function plugin_root()
  local source = debug.getinfo(1, 'S').source:sub(2)
  return vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(source)))
end

--- The server command: the configured one, a build inside the plugin
--- directory, or `org-lsp` on PATH. Returns nil when none is found.
---@return string[]|nil
function M.server_command()
  if M.config.cmd then
    return M.config.cmd
  end
  local exe = vim.fn.has('win32') == 1 and 'org-lsp.exe' or 'org-lsp'
  local built = vim.fs.joinpath(plugin_root(), 'target', 'release', exe)
  if vim.fn.executable(built) == 1 then
    return { built }
  end
  if vim.fn.executable('org-lsp') == 1 then
    return { 'org-lsp' }
  end
  return nil
end

--- Register the server with Neovim's LSP client and enable it for Org files.
---@return boolean enabled
local function enable()
  local cmd = M.server_command()
  if not cmd then
    return false
  end
  local init_options = nil
  if M.config.todo_keywords then
    init_options = { todoKeywords = M.config.todo_keywords }
  end
  vim.lsp.config(SERVER, { cmd = cmd, filetypes = { 'org' }, init_options = init_options })
  vim.lsp.enable(SERVER)
  return true
end

local function warn_missing_server()
  vim.notify(
    'org-lsp: the server binary was not found. Run :OrgLspBuild (needs cargo), '
      .. 'put `org-lsp` on your PATH, or set `cmd` in the configuration.',
    vim.log.levels.WARN
  )
end

---@param bufnr integer
---@return vim.lsp.Client|nil
local function client_for(bufnr)
  return vim.lsp.get_clients({ bufnr = bufnr, name = SERVER })[1]
end

--- Ask the server for the code action of exactly this kind and apply it.
--- This is how every Org command reaches the server.
---@param kind string e.g. 'org.todo.next'
function M.action(kind)
  local bufnr = vim.api.nvim_get_current_buf()
  local client = client_for(bufnr)
  if not client then
    return
  end
  local params = vim.lsp.util.make_range_params(0, client.offset_encoding)
  params.context = { only = { kind }, diagnostics = {} }
  client:request('textDocument/codeAction', params, function(err, result)
    if err then
      vim.notify('org-lsp: ' .. tostring(err.message), vim.log.levels.ERROR)
      return
    end
    local action = result and result[1]
    if action and action.edit then
      vim.lsp.util.apply_workspace_edit(action.edit, client.offset_encoding)
    end
  end, bufnr)
end

local last_action

--- 'operatorfunc' target, so that `.` repeats the last action.
function M.repeat_action()
  if last_action then
    M.action(last_action)
  end
end

--- An expression mapping that runs `kind` and can be repeated with `.`.
---@param kind string
---@return fun(): string
local function repeatable(kind)
  return function()
    last_action = kind
    vim.o.operatorfunc = "v:lua.require'org-lsp'.repeat_action"
    -- `g@l` runs 'operatorfunc' without moving the cursor, but `l` cannot
    -- move on an empty line; there the action runs directly instead.
    if vim.api.nvim_get_current_line() == '' then
      vim.schedule(M.repeat_action)
      return ''
    end
    return 'g@l'
  end
end

local function set_keymaps(bufnr)
  local mappings = M.config.mappings
  if not mappings then
    return
  end
  local function map(lhs, rhs, desc, opts)
    if lhs then
      opts = vim.tbl_extend('force', { buffer = bufnr, desc = desc }, opts or {})
      vim.keymap.set('n', lhs, rhs, opts)
    end
  end
  map(mappings.todo_next, repeatable('org.todo.next'), 'Org: next TODO state', { expr = true })
  map(mappings.todo_previous, repeatable('org.todo.previous'), 'Org: previous TODO state', { expr = true })
  if M.config.folding then
    local folds = require('org-lsp.folds')
    map(mappings.cycle, folds.cycle, 'Org: cycle visibility of the subtree')
    map(mappings.global_cycle, folds.global_cycle, 'Org: cycle visibility of the whole file')
  end
end

local function set_window_folds(win)
  if vim.w[win].org_lsp_folds then
    return
  end
  vim.w[win].org_lsp_folds = true
  vim.wo[win][0].foldmethod = 'expr'
  vim.wo[win][0].foldexpr = 'v:lua.vim.lsp.foldexpr()'
  vim.wo[win][0].foldlevel = M.config.startup_folded == 'overview' and 0 or 99
end

--- Vim's regex syntax for Org is an approximation and disagrees with the
--- server in places. It stays on until the first semantic tokens arrive, so
--- the buffer is never without highlighting.
local function replace_syntax(bufnr)
  vim.api.nvim_create_autocmd('LspTokenUpdate', {
    buffer = bufnr,
    once = true,
    callback = function()
      if vim.bo[bufnr].syntax == 'org' then
        vim.bo[bufnr].syntax = ''
      end
    end,
  })
end

---@param client vim.lsp.Client
---@param bufnr integer
local function on_attach(client, bufnr)
  require('org-lsp.highlights').define()
  set_keymaps(bufnr)
  if M.config.replace_syntax and client:supports_method('textDocument/semanticTokens/full') then
    replace_syntax(bufnr)
  end
  if M.config.folding and client:supports_method('textDocument/foldingRange') then
    for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
      set_window_folds(win)
    end
    vim.api.nvim_create_autocmd('BufWinEnter', {
      buffer = bufnr,
      group = vim.api.nvim_create_augroup('org_lsp_folds_' .. bufnr, { clear = true }),
      callback = function()
        set_window_folds(vim.api.nvim_get_current_win())
      end,
    })
  end
end

--- Build the server with cargo inside the plugin directory, then start it.
local function build()
  if vim.fn.executable('cargo') == 0 then
    vim.notify('org-lsp: cargo is not installed; see https://rustup.rs', vim.log.levels.ERROR)
    return
  end
  vim.notify('org-lsp: building the server…')
  vim.system({ 'cargo', 'build', '--release', '-p', 'org-lsp' }, { cwd = plugin_root(), text = true }, function(result)
    vim.schedule(function()
      if result.code ~= 0 then
        vim.notify('org-lsp: build failed\n' .. (result.stderr or ''), vim.log.levels.ERROR)
      elseif enable() then
        vim.notify('org-lsp: built; reopen your Org buffers to attach')
      end
    end)
  end)
end

local started = false

--- Called once by plugin/org-lsp.lua.
function M.start()
  if started then
    return
  end
  started = true
  local group = vim.api.nvim_create_augroup('org_lsp', { clear = true })
  vim.api.nvim_create_autocmd('LspAttach', {
    group = group,
    callback = function(args)
      local client = vim.lsp.get_client_by_id(args.data.client_id)
      if client and client.name == SERVER then
        on_attach(client, args.buf)
      end
    end,
  })
  vim.api.nvim_create_autocmd('ColorScheme', {
    group = group,
    callback = function()
      require('org-lsp.highlights').define()
    end,
  })
  vim.api.nvim_create_user_command('OrgLspBuild', build, { desc = 'Build the org-lsp server with cargo' })
  if not enable() then
    vim.api.nvim_create_autocmd('FileType', { group = group, pattern = 'org', once = true, callback = warn_missing_server })
  end
end

--- Change the configuration. May be called before or after the plugin loads.
---@param opts? table
function M.setup(opts)
  M.config = vim.tbl_deep_extend('force', M.config, opts or {})
  if started then
    enable()
  end
end

return M
