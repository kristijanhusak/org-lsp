--- Neovim glue for the org-lsp language server.
---
--- The server does the Org work, and it says what an editor needs to know
--- about it: its commands with their default keys, and how its tokens
--- should look. This module applies those tables. What is written here is
--- only what an editor has to do itself: start the server, show the
--- documents it serves, and open and close folds.
---
--- It works without configuration. To change something, set `vim.g.org_lsp`
--- before the plugin loads or call `setup()`:
---
---     require('org-lsp').setup({
---       settings = { agendaFiles = { '~/org' } },
---       mappings = { org = { ['org.todo.next'] = '<Leader>t' } },
---     })
local M = {}

local SERVER = 'org_lsp'

--- The plugin's options, for `setup()` and `vim.g.org_lsp`. Every one may
--- be left out. Annotate a table of your own with `---@type org_lsp.Config`
--- to have them completed and checked.
---@class org_lsp.Config
---@field cmd? string[] Server command. Found automatically when unset.
---@field settings? org_lsp.Settings The server's options, such as `agendaFiles` and `todoKeywords`; see docs/neovim/configuration.md.
---@field startup_folded? 'showeverything'|'overview' Fold state of a newly opened file.
---@field folding? boolean Whether to fold with the server's folding ranges.
---@field replace_syntax? boolean Whether to turn off Vim's regex syntax once semantic tokens arrive.
---@field conceal? boolean Whether to conceal what the server marks hidden (link brackets and targets; emphasis markers with `hideEmphasisMarkers`), with 'conceallevel' 2 in Org windows.
---@field indent? boolean Whether the server indents as Org does: new lines through 'indentexpr' (`org-indent-line`), and `=` (`org-indent-region`), following `adaptIndentation`.
---@field virtual_indent? boolean Whether files with `org-indent-mode` (`startupIndented`, `#+STARTUP: indent`) show indented by level, without being changed, and with hidden leading stars (`hideLeadingStars`, `#+STARTUP: hidestars`).
---@field win_split_mode? 'split'|'vsplit'|'float'|'current'|string|fun(bufnr: integer): integer? How the agenda and capture buffers open: 'split', 'vsplit', 'float', 'current', another Ex command that opens a window ('botright split', 'tabnew'), or a function that opens a window for the buffer and returns it.
---@field mappings? org_lsp.Mappings|false Keys by scope; `false` sets no keys at all. See docs/neovim/keys.md.

--- Keys by scope. In a scope, a command (`org.todo`, or one of the plugin's
--- own: `cycle`, `global_cycle`, `quit`) maps to a key, a list of keys, or
--- `false`; commands that are not listed keep the server's default keys.
--- `false` instead of a scope's table sets no keys in that scope.
---@class org_lsp.Mappings
---@field global? table<string, string|string[]|false>|false Keys that work anywhere.
---@field org? table<string, string|string[]|false>|false Keys in Org files.
---@field agenda? table<string, string|string[]|false>|false Keys in the agenda.
---@field calendar? table<string, string|string[]|false>|false Keys in the date prompt's calendar.
---@field capture? table<string, string|string[]|false>|false Keys in a capture or note buffer.

local defaults = {
  cmd = nil,
  settings = {},
  startup_folded = 'showeverything',
  folding = true,
  replace_syntax = true,
  conceal = true,
  indent = true,
  virtual_indent = true,
  win_split_mode = 'split',
  mappings = {
    -- The keys that must work before the server has said anything.
    global = {
      ['org.agenda.dispatch'] = '<Leader>oa',
      ['org.capture'] = '<Leader>oc',
      ['org.clock.goto'] = '<Leader>oxj',
    },
    org = { cycle = '<Tab>', global_cycle = '<S-Tab>' },
    agenda = { quit = 'q' },
  },
}

---@type org_lsp.Config
M.config = vim.tbl_deep_extend('force', defaults, vim.g.org_lsp or {})

--- What the server announced about itself: commands, keys, looks. Set when
--- the server has started.
---@type table?
M.manifest = nil

--- Commands that only the editor can carry out, with their titles.
local own_commands = {
  cycle = { 'Org: cycle visibility of the subtree', function()
    require('org-lsp.folds').cycle()
  end },
  global_cycle = { 'Org: cycle visibility of the whole file', function()
    require('org-lsp.folds').global_cycle()
  end },
  quit = { 'Agenda: close', function()
    require('org-lsp.view').close_buffer(vim.api.nvim_get_current_buf())
  end },
}

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

local function warn_missing_server()
  vim.notify(
    'org-lsp: the server binary was not found. Run :OrgLspBuild (needs cargo), '
      .. 'put `org-lsp` on your PATH, or set `cmd` in the configuration.',
    vim.log.levels.WARN
  )
end

---@return vim.lsp.Client|nil
local function running_client()
  return vim.lsp.get_clients({ name = SERVER })[1]
end

-- ----- commands

--- Functions waiting for the server to start.
local waiting = {}

--- Call `fn` with the client, starting the server first if need be. The
--- server can run without an Org file being open: the agenda is asked for
--- from anywhere.
---@param fn fun(client: vim.lsp.Client)
local function with_client(fn)
  local client = running_client()
  if client and client.initialized then
    return fn(client)
  end
  if not client and not M.server_command() then
    return warn_missing_server()
  end
  table.insert(waiting, fn)
  if not client then
    vim.lsp.start(vim.lsp.config[SERVER], { attach = false })
  end
end

---@param id string
---@return table? # the manifest's entry for the command
local function server_command(id)
  for _, command in ipairs(M.manifest and M.manifest.commands or {}) do
    if command.command == id then
      return command
    end
  end
end

--- Run a command: one of the editor's own, or one of the server's, which
--- is told where the cursor is.
---@param name string e.g. 'org.todo.next'
---@param opts? { count?: integer, key?: string } the count typed before the
--- key, and the key to give back its own meaning if the command does not
--- apply at the cursor
function M.run(name, opts)
  if own_commands[name] then
    return own_commands[name][2]()
  end
  opts = opts or {}
  local count = opts.count and opts.count > 0 and opts.count or nil
  with_client(function(client)
    local context = vim.lsp.util.make_position_params(0, client.offset_encoding)
    context.count = count
    local command = { title = name, command = name, arguments = { context } }
    local bufnr = vim.api.nvim_get_current_buf()
    local function handle(err, result)
      if err then
        if opts.key and type(err.data) == 'table' and err.data.notApplicable then
          -- Not an Org matter here: the key does what it does elsewhere.
          vim.api.nvim_feedkeys((count or '') .. vim.keycode(opts.key), 'n', false)
        else
          vim.notify('org-lsp: ' .. tostring(err.message), vim.log.levels.WARN)
        end
        return
      end
      -- The server names files to save; those with changes are written.
      if type(result) == 'table' and type(result.save) == 'table' then
        require('org-lsp.view').save(result.save)
      end
      -- What the command has to say, as Emacs says it in the echo area.
      if type(result) == 'table' and type(result.message) == 'string' then
        vim.notify(result.message, vim.log.levels.INFO)
      end
      -- The command is done with a document the server showed for it.
      if type(result) == 'table' and result.close then
        require('org-lsp.view').close(result.close)
      end
      -- The command asks for something first: ask, and run it again with
      -- the answer.
      local input = type(result) == 'table' and result.input or nil
      if input then
        local function answered(answer)
          if answer then
            -- What the server asked to have back, and the answer.
            for key, value in pairs(input.context or {}) do
              context[key] = value
            end
            context.input = answer
            client:exec_cmd(command, { bufnr = bufnr }, handle)
          end
        end
        if input.kind == 'key' then
          return answered(M.read_key(input.prompt, input.choices))
        end
        if input.choices then
          -- One of a list, such as the targets of a refile.
          local format = function(choice)
            return choice.label
          end
          return vim.ui.select(input.choices, { prompt = input.prompt .. ': ', format_item = format }, function(choice)
            answered(choice and choice.key)
          end)
        end
        vim.ui.input({ prompt = input.prompt .. ': ', default = input.value }, function(answer)
          -- An input that runs in insert mode may leave it behind in the
          -- calendar it was asked from, which is never typed into.
          vim.schedule(function()
            local prompt = M.manifest and M.manifest.datePrompt
            if prompt and vim.bo.filetype == prompt.languageId then
              vim.cmd.stopinsert()
            end
          end)
          answered(answer)
        end)
      end
    end
    client:exec_cmd(command, { bufnr = bufnr }, handle)
  end)
end

--- The window that lists the choices of `read_key`, as Emacs lists them
--- in a window of its own at the bottom of the frame.
---@param prompt string
---@param choices { key: string, label: string }[]
---@return integer window
local function show_choices(prompt, choices)
  local lines, width = {}, vim.fn.strdisplaywidth(prompt) + 2
  for _, choice in ipairs(choices) do
    local line = ('[%s] %s'):format(choice.key == ' ' and 'SPC' or choice.key, choice.label)
    table.insert(lines, line)
    width = math.max(width, vim.fn.strdisplaywidth(line))
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  local namespace = vim.api.nvim_create_namespace('org_lsp_choices')
  for row, line in ipairs(lines) do
    vim.api.nvim_buf_set_extmark(buf, namespace, row - 1, 0, { end_col = line:find(']', 1, true), hl_group = 'Special' })
  end
  vim.bo[buf].bufhidden = 'wipe'
  -- Above the command line and the status line, inside the borders.
  local height = math.max(1, math.min(#lines, vim.o.lines - vim.o.cmdheight - 4))
  return vim.api.nvim_open_win(buf, false, {
    relative = 'editor',
    row = vim.o.lines - vim.o.cmdheight - height - 3,
    col = 0,
    width = math.min(width, vim.o.columns - 2),
    height = height,
    style = 'minimal',
    border = 'rounded',
    title = ' ' .. prompt .. ' ',
    focusable = false,
    noautocmd = true,
  })
end

--- Read one key, as Emacs does at its fast selection prompts: the
--- choices are shown in a window at the bottom, `<Esc>` or `<C-c>`
--- cancels.
---@param prompt string
---@param choices? { key: string, label: string }[]
---@return string?
function M.read_key(prompt, choices)
  local win = choices and #choices > 0 and show_choices(prompt, choices) or nil
  vim.api.nvim_echo({ { prompt .. ': ' } }, false, {})
  vim.cmd.redraw()
  local ok, key = pcall(vim.fn.getcharstr)
  vim.api.nvim_echo({ { '' } }, false, {})
  if win and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_win_close(win, true)
  end
  if not ok or key == vim.keycode('<Esc>') or key == vim.keycode('<C-c>') then
    return nil
  end
  return key
end

--- The editing command that `.` repeats, with its options.
local last_edit

--- 'operatorfunc' target, so that `.` repeats the last editing command.
--- A count typed before `.` replaces the one the command had.
function M.repeat_edit()
  if last_edit then
    M.run(last_edit.name, { count = vim.v.count, key = last_edit.key })
  end
end

--- The right-hand side of an expression mapping that runs `name` and can
--- be repeated with `.`.
---@param name string
---@param key? string the mapped key, if it has a meaning of its own
---@return fun(): string
local function repeatable(name, key)
  return function()
    last_edit = { name = name, key = key }
    vim.o.operatorfunc = "v:lua.require'org-lsp'.repeat_edit"
    -- `g@l` runs 'operatorfunc' without moving the cursor, but `l` cannot
    -- move on an empty line; there the command runs directly instead.
    if vim.api.nvim_get_current_line() == '' then
      local count = vim.v.count
      vim.schedule(function()
        M.run(name, { count = count, key = key })
      end)
      -- Use up the count, which would otherwise wait for a command.
      return count > 0 and vim.keycode('<Esc>') or ''
    end
    return 'g@l'
  end
end

-- ----- keys

--- The keys of a scope: the server's defaults for modal editors, with the
--- configuration's entries taking their place command by command.
---@param scope 'global'|'org'|'agenda'|'calendar'|'capture'
---@return table<string, string[]> # command to keys
---@return table<string, boolean> # the keys that keep a meaning of their own
local function keys_in(scope)
  if not M.config.mappings or M.config.mappings[scope] == false then
    return {}, {}
  end
  local keys, native = {}, {}
  for _, command in ipairs(M.manifest and M.manifest.commands or {}) do
    for _, key in ipairs(command.keys) do
      if key.preset == 'vim' and key['in'] == scope then
        keys[command.command] = keys[command.command] or {}
        table.insert(keys[command.command], key.key)
        native[key.key] = key.native or nil
      end
    end
  end
  for name, key in pairs(M.config.mappings[scope] or {}) do
    keys[name] = type(key) == 'table' and key or key and { key } or {}
  end
  return keys, native
end

--- Global keys that are set, so that a change of configuration can take
--- them back.
local global_keys = {}

---@param scope 'global'|'org'|'agenda'|'calendar'|'capture'
---@param bufnr? integer the buffer the keys are for; none for global keys
local function set_keys(scope, bufnr)
  if scope == 'global' then
    for _, lhs in ipairs(global_keys) do
      pcall(vim.keymap.del, 'n', lhs)
    end
    global_keys = {}
  end
  local keys, native = keys_in(scope)
  -- The agenda and the calendar are read-only, with keys of one letter:
  -- they run at once, not after waiting to see whether a longer mapping
  -- of the user's (`tn`) is typed. A key that starts a longer one of
  -- these (`v` of `v d`) still waits.
  local nowait = {}
  if scope == 'agenda' or scope == 'calendar' then
    local all = vim.iter(vim.tbl_values(keys)):flatten():totable()
    for _, lhs in ipairs(all) do
      nowait[lhs] = not vim.iter(all):any(function(other)
        return other ~= lhs and vim.startswith(other, lhs)
      end)
    end
  end
  for name, list in pairs(keys) do
    local command = server_command(name)
    local title = own_commands[name] and own_commands[name][1] or command and command.title or name
    for _, lhs in ipairs(list) do
      if scope == 'global' then
        table.insert(global_keys, lhs)
      end
      local key = native[lhs] and lhs or nil
      if command and command.edits and scope == 'org' then
        vim.keymap.set('n', lhs, repeatable(name, key), { buffer = bufnr, desc = title, expr = true })
      else
        vim.keymap.set('n', lhs, function()
          M.run(name, { count = vim.v.count, key = key })
        end, { buffer = bufnr, desc = title, nowait = nowait[lhs] })
      end
    end
  end
end

-- ----- attaching

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

--- Text the server marks hidden (link brackets and targets, emphasis
--- markers) is concealed, as Org hides it. The marks go in the semantic
--- tokens' own namespace, which Neovim clears whenever new tokens arrive,
--- so they come and go with the highlighting.
---@param bufnr integer
local function conceal(bufnr)
  vim.api.nvim_create_autocmd('LspTokenUpdate', {
    buffer = bufnr,
    group = vim.api.nvim_create_augroup('org_lsp_conceal_' .. bufnr, { clear = true }),
    callback = function(args)
      local token = args.data.token
      if not token.modifiers.hidden then
        return
      end
      local namespace = vim.api.nvim_create_namespace('nvim.lsp.semantic_tokens:' .. args.data.client_id)
      pcall(vim.api.nvim_buf_set_extmark, bufnr, namespace, token.line, token.start_col, {
        end_row = token.end_line or token.line,
        end_col = token.end_col,
        conceal = '',
        strict = false,
      })
    end,
  })
  local function set_window(win)
    vim.wo[win][0].conceallevel = 2
  end
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    set_window(win)
  end
  vim.api.nvim_create_autocmd('BufWinEnter', {
    buffer = bufnr,
    group = vim.api.nvim_create_augroup('org_lsp_conceal_windows_' .. bufnr, { clear = true }),
    callback = function()
      set_window(vim.api.nvim_get_current_win())
    end,
  })
end

--- The server has started: take over what it announced.
---@param client vim.lsp.Client
local function on_init(client)
  M.manifest = (client.server_capabilities.experimental or {}).org
  if not M.manifest then
    return vim.notify('org-lsp: the server is older than this plugin; run :OrgLspBuild', vim.log.levels.WARN)
  end
  require('org-lsp.highlights').define(M.manifest)
  require('org-lsp.view').setup(M.manifest)
  set_keys('global')
  for _, fn in ipairs(waiting) do
    fn(client)
  end
  waiting = {}
end

---@param client vim.lsp.Client
---@param bufnr integer
local function on_attach(client, bufnr)
  if not M.manifest then
    return
  end
  if vim.bo[bufnr].filetype == M.manifest.agenda.languageId then
    return set_keys('agenda', bufnr)
  end
  if M.manifest.datePrompt and vim.bo[bufnr].filetype == M.manifest.datePrompt.languageId then
    return set_keys('calendar', bufnr)
  end
  set_keys('org', bufnr)
  -- A capture buffer is an Org file with keys of its own on top.
  local prefix = M.manifest.capture and M.manifest.capture.prefix
  if prefix and vim.startswith(vim.fs.basename(vim.api.nvim_buf_get_name(bufnr)), prefix) then
    set_keys('capture', bufnr)
  end
  if M.config.replace_syntax and client:supports_method('textDocument/semanticTokens/full') then
    replace_syntax(bufnr)
  end
  if M.config.conceal then
    conceal(bufnr)
  end
  if M.config.indent and client:supports_method('textDocument/onTypeFormatting') then
    require('org-lsp.indent').attach(bufnr)
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

--- Register the server with Neovim's LSP client and enable it for Org files.
---@return boolean enabled
local function enable()
  local cmd = M.server_command()
  if not cmd then
    return false
  end
  local view = require('org-lsp.view')
  vim.lsp.config(SERVER, {
    cmd = cmd,
    filetypes = { 'org' },
    init_options = M.config.settings,
    capabilities = view.capabilities,
    handlers = view.handlers,
    on_init = on_init,
  })
  vim.lsp.enable(SERVER)
  return true
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
        vim.notify('org-lsp: built; restart Neovim to use the new server')
      end
    end)
  end)
end

--- `:Org agenda list` runs `org.agenda.list`. A group of commands stands
--- for its dispatcher: `:Org agenda` runs `org.agenda.dispatch`.
---@param words string[]
local function user_command(words)
  local id = 'org.' .. table.concat(#words > 0 and words or { 'agenda' }, '.')
  with_client(function()
    M.run(server_command(id) and id or id .. '.dispatch')
  end)
end

---@param lead string what has been typed of the argument
---@return string[]
local function complete_user_command(lead)
  local names = {}
  for _, command in ipairs(M.manifest and M.manifest.commands or {}) do
    local name = command.command:gsub('^org%.', '')
    if name:sub(1, #lead) == lead then
      table.insert(names, name)
    end
  end
  return names
end

local started = false

--- What Emacs's mode line shows while a clock runs, for the statusline:
--- `[0:25/1:00] (Write the report)`, or an empty string.
---
--- ```lua
--- vim.o.statusline = '%f %=%{v:lua.require("org-lsp").clock_status()}'
--- ```
---@return string
function M.clock_status()
  return require('org-lsp.clock').status()
end

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
      if M.manifest then
        require('org-lsp.highlights').define(M.manifest)
      end
    end,
  })
  vim.api.nvim_create_user_command('OrgLspBuild', build, { desc = 'Build the org-lsp server with cargo' })
  vim.api.nvim_create_user_command('Org', function(opts)
    user_command(opts.fargs)
  end, { nargs = '*', complete = complete_user_command, desc = 'Run an Org command' })
  set_keys('global')
  if not enable() then
    vim.api.nvim_create_autocmd('FileType', { group = group, pattern = 'org', once = true, callback = warn_missing_server })
  end
end

--- Change the configuration. May be called before or after the plugin loads.
---@param opts? org_lsp.Config
function M.setup(opts)
  M.config = vim.tbl_deep_extend('force', M.config, opts or {})
  if started then
    set_keys('global')
    enable()
  end
end

return M
