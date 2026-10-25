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
---       settings = { org_agenda_files = { '~/org' } },
---       mappings = { org = { ['org.todo.next'] = '<Leader>t' } },
---     })
local M = {}
local notify = require('org-lsp.notify')

local SERVER = 'org_lsp'

--- The plugin's options, for `setup()` and `vim.g.org_lsp`. Every one may
--- be left out. Annotate a table of your own with `---@type org_lsp.Config`
--- to have them completed and checked.
---@class org_lsp.Config
---@field cmd? string[] Server command. Found automatically when unset: a build in the plugin's directory, `org-lsp` on PATH, or the one `download` fetched.
---@field download? boolean Whether a server is downloaded when none is found: the build of `release` for this platform, into `stdpath('data')/org-lsp` (needs `curl` and `tar`). `:OrgLspDownload` downloads it again, to update it.
---@field release? string The release `download` and `:OrgLspDownload` fetch the server from: 'nightly' (built from the latest `main`), or a version tag such as 'v0.2.0'.
---@field settings? org_lsp.Settings The server's options, such as `org_agenda_files` and `org_todo_keywords`; see docs/configuration.org.
---@field startup_folded? 'showeverything'|'overview' How a newly opened file shows when neither the settings' `org_startup_folded` nor its `#+STARTUP:` say: everything open or every subtree folded. The file's `#+STARTUP:` and `VISIBILITY` properties always decide for it.
---@field folding? boolean Whether to fold with the server's folding ranges.
---@field replace_syntax? boolean Whether to turn off Vim's regex syntax once semantic tokens arrive.
---@field src_highlight? boolean Whether the code of source and export blocks and of inline source blocks (`#+begin_src python`, `src_python{…}`) is highlighted where the server says it is (`org-lsp/codeBlocks`) by the tree-sitter parser of its language, or its Vim syntax file in regions of the buffer's syntax when there is no parser, as Emacs's `org-src-fontify-natively` shows it.
---@field conceal? boolean Whether to conceal what the server marks hidden (link brackets and targets; emphasis markers with `org_hide_emphasis_markers`), with 'conceallevel' 2 in Org windows (a level set already, as by a `FileType` autocommand, stays).
---@field indent? boolean Whether the server indents as Org does: new lines through 'indentexpr' (`org-indent-line`), and `=` (`org-indent-region`), following `org_adapt_indentation`.
---@field virtual_indent? boolean Whether files with `org-indent-mode` (`org_startup_indented`, `#+STARTUP: indent`) show indented by level, without being changed, and with hidden leading stars (`org_hide_leading_stars`, `#+STARTUP: hidestars`).
---@field win_split_mode? 'split'|'vsplit'|'float'|'current'|string|fun(bufnr: integer): integer? How the agenda and capture buffers open: 'split', 'vsplit', 'float', 'current', another Ex command that opens a window ('botright split', 'tabnew'), or a function that opens a window for the buffer and returns it.
---@field win_split_size? number The size of the window `win_split_mode` opens: lines for a split, columns for a vertical one, or with a number below 1 that part of the editor ('float' takes the part for both sides, 0.8 when unset). Unset, a split takes half of the window it splits.
---@field calendar_win_mode? 'bottom'|'float'|'cursor'|string|fun(bufnr: integer): integer? Where the date prompt's calendar opens: 'bottom' in a split along the bottom, as Emacs shows it; 'float' in a floating window in the middle; 'cursor' in a floating window below the cursor (above it when there is no room); another Ex command that opens a window ('topleft split'); or a function that opens a window for the buffer and returns it.
---@field calendar_border? string|string[] The border of the calendar's floating window ('float', 'cursor'), as `nvim_open_win()` takes it; 'rounded' when unset, 'none' for none.
---@field images? { max_width?: integer, max_height?: integer, cell?: { width: number, height: number } }|false Previews of links to images and of LaTeX fragments (`<Leader>oxv`, `<Leader>oxl`), drawn with `vim.ui.img` (Neovim 0.13 and later, in a terminal with the Kitty graphics protocol): the most columns and lines an image takes (the window's width, `org_image_max_width` for one at its own width, and 20), the size of a cell in pixels where the terminal does not say it (for the lines an image takes; 10 by 20 then), or `false` for no previews.
---@field mappings? org_lsp.Mappings|false Keys by scope; `false` sets no keys at all. See docs/keys/neovim.org.

--- Keys by scope. In a scope, a command (`org.todo`, or one of the plugin's
--- own: `cycle`, `global_cycle`, `quit`, `help`, `hover`) maps to a key, a list of keys, or
--- `false`; commands that are not listed keep the server's default keys.
--- `false` instead of a scope's table sets no keys in that scope.
---@class org_lsp.Mappings
---@field global? table<string, string|string[]|false>|false Keys that work anywhere.
---@field org? table<string, string|string[]|false>|false Keys in Org files.
---@field agenda? table<string, string|string[]|false>|false Keys in the agenda.
---@field calendar? table<string, string|string[]|false>|false Keys in the date prompt's calendar.
---@field capture? table<string, string|string[]|false>|false Keys in a capture or note buffer.
---@field columns? table<string, string|string[]|false>|false Keys in column view (`org.columns`); `h` and `l` pick the column.

local defaults = {
  cmd = nil,
  download = true,
  release = 'nightly',
  settings = {},
  folding = true,
  replace_syntax = true,
  src_highlight = true,
  conceal = true,
  indent = true,
  virtual_indent = true,
  win_split_mode = 'split',
  calendar_win_mode = 'bottom',
  -- Previews of images and LaTeX (`vim.ui.img`, Neovim 0.13): the most
  -- cells an image takes, or false for none.
  images = { max_height = 20 },
  mappings = {
    -- The keys that must work before the server has said anything.
    global = {
      ['org.agenda.dispatch'] = '<Leader>oa',
      ['org.capture'] = '<Leader>oc',
      ['org.clock.goto'] = '<Leader>oxj',
      ['org.journal.new.entry'] = '<Leader>jj',
    },
    org = { cycle = '<Tab>', global_cycle = '<S-Tab>', help = 'g?' },
    agenda = { quit = 'q', help = 'g?' },
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
  help = { 'Show the keys', function()
    require('org-lsp.help').show()
  end },
  quit = { 'Agenda: close', function()
    require('org-lsp.view').close_buffer(vim.api.nvim_get_current_buf())
  end },
  hover = { 'Agenda: show the entry of the line', function()
    require('org-lsp.preview').show()
  end },
}

--- The directory this plugin is installed in.
local function plugin_root()
  local source = debug.getinfo(1, 'S').source:sub(2)
  return vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(source)))
end

--- The server command: the configured one, a build inside the plugin
--- directory, `org-lsp` on PATH, or the downloaded one. Returns nil when
--- none is found.
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
  local downloaded = require('org-lsp.download').path()
  if vim.fn.executable(downloaded) == 1 then
    return { downloaded }
  end
  return nil
end

local function warn_missing_server()
  notify(
    'org-lsp: the server binary was not found. Run :OrgLspDownload, :OrgLspBuild (needs cargo), '
      .. 'put `org-lsp` on your PATH, or set `cmd` in the configuration.',
    vim.log.levels.WARN
  )
end

--- The server: the one of the current buffer, else the one that runs.
---@return vim.lsp.Client|nil
local function running_client()
  return vim.lsp.get_clients({ bufnr = 0, name = SERVER })[1] or vim.lsp.get_clients({ name = SERVER })[1]
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
---@param opts? { count?: integer, key?: string, append?: boolean, argument?: table } the
--- count typed before the key, the key to give back its own meaning if the
--- command does not apply at the cursor, whether the command acts below
--- the cursor's line and typing goes on in insert mode, and fields of the
--- argument the server gave (`org-lsp/runCommand`)
function M.run(name, opts)
  if own_commands[name] then
    return own_commands[name][2]()
  end
  opts = opts or {}
  -- Rebuilding reads the files again: those changed on disk while open
  -- here, and not changed here, are read again first (`autoread`), as the
  -- server takes an open file's text from the editor.
  if name == 'org.agenda.redo' then
    local autoread = vim.o.autoread
    vim.o.autoread = true
    vim.cmd('silent! checktime')
    vim.o.autoread = autoread
  end
  local count = opts.count and opts.count > 0 and opts.count or nil
  -- The register the key was typed with (`"a`), as `y` and `p` take it.
  local register = vim.v.register
  -- In Visual mode, the selection: the region the command works on; the
  -- lines of a range given to `:Org`.
  local selection = opts.region and M.selection() or nil
  if opts.lines then
    selection = { start = { opts.lines[1] - 1, 0 }, ['end'] = { opts.lines[2], 0 }, cursor_at_end = false }
  end
  if selection and not opts.lines then
    vim.api.nvim_feedkeys(vim.keycode('<Esc>'), 'nx', false)
  end
  with_client(function(client)
    local context = vim.lsp.util.make_position_params(0, client.offset_encoding)
    context.count = count
    -- In column view, the column picked.
    if vim.startswith(name, 'org.columns.') then
      context.column = require('org-lsp.columns').column(vim.api.nvim_get_current_buf())
    end
    if selection then
      local function position(at)
        local line = vim.api.nvim_buf_get_lines(0, at[1], at[1] + 1, false)[1] or ''
        local character = at[2] > 0 and vim.str_utfindex(line, client.offset_encoding, math.min(at[2], #line), false) or 0
        return { line = at[1], character = character }
      end
      context.range = { start = position(selection.start), ['end'] = position(selection['end']) }
      -- The point at the end of the selection the cursor was at.
      context.position = selection.cursor_at_end and context.range['end'] or context.range.start
    end
    -- What the command reads as Emacs's latest kill: the register.
    if (server_command(name) or {}).clipboard then
      context.clipboard = vim.fn.getreg(register)
    end
    -- The closed folds, for what is visible.
    if (server_command(name) or {}).folds then
      context.folded = M.closed_folds()
    end
    -- LaTeX is drawn in the colors of the text.
    if name == 'org.latex.preview' then
      local normal = vim.api.nvim_get_hl(0, { name = 'Normal', link = false })
      context.foreground = normal.fg and ('#%06x'):format(normal.fg) or nil
      context.background = 'Transparent'
    end
    -- Whether the org-roam buffer shows, which the command hides then.
    if name == 'org.roam.buffer.toggle' then
      context.visible = require('org-lsp.view').roam_buffer_shown()
    end
    if opts.append then
      -- Below the cursor's line, as nvim-orgmode's `<Leader><CR>` puts it:
      -- never above it, and nothing split.
      context.below = true
    end
    -- What the server asked the command to be run with (a capture an
    -- org-protocol URL started).
    for key, value in pairs(opts.argument or {}) do
      context[key] = value
    end
    local command = { title = name, command = name, arguments = { context } }
    local bufnr = vim.api.nvim_get_current_buf()
    --- Run `command`. With keys typed ahead (a mapping or a macro that
    --- goes on after the command's key), the answer is waited for, so that
    --- those keys answer what it asks (`<Leader>oaa`) or go where it leaves
    --- the cursor, as Emacs reads the next key only once the command is
    --- done. (An `<Esc>` alone, which ends what a mapping started, is not
    --- an answer.)
    local function exec(command, callback)
      local ahead = vim.fn.getchar(1)
      if ahead == 0 or ahead == 27 then
        return client:exec_cmd(command, { bufnr = bufnr }, callback)
      end
      local params = { command = command.command, arguments = command.arguments }
      local response, err = client:request_sync('workspace/executeCommand', params, 10000, bufnr)
      if not response then
        return callback({ message = tostring(err) }, nil)
      end
      callback(response.err, response.result)
    end
    local function handle(err, result)
      if err then
        if opts.key and type(err.data) == 'table' and err.data.notApplicable then
          -- Not an Org matter here: the key does what it does elsewhere.
          vim.api.nvim_feedkeys((count or '') .. vim.keycode(opts.key), 'n', false)
        else
          -- Emacs's text, as its echo area shows a `user-error`.
          notify(tostring(err.message), vim.log.levels.WARN)
        end
        return
      end
      -- The edit and the cursor, carried in the result (`org.meta.return`);
      -- from its key, typing goes on there, in insert mode, as with
      -- nvim-orgmode.
      if type(result) == 'table' and type(result.edits) == 'table' then
        local at_end = M.apply_carried(result, bufnr, client)
        if opts.append and vim.api.nvim_get_current_buf() == bufnr then
          -- Normal mode keeps the cursor off the end of the line.
          vim.cmd(at_end and 'startinsert!' or 'startinsert')
        end
      end
      -- What was cut or copied goes to the register, by lines, or by
      -- characters for a kill of part of a line (`org.kill.line`).
      if type(result) == 'table' and type(result.copy) == 'string' then
        vim.fn.setreg(register, result.copy, result.copyCharacters and 'c' or 'l')
      end
      -- A sparse tree: everything folded but the lines shown, and the
      -- matches highlighted.
      if type(result) == 'table' and type(result.sparse) == 'table' then
        M.show_sparse(bufnr, result.sparse, client)
      end
      -- The server names files to save; those with changes are written.
      if type(result) == 'table' and type(result.save) == 'table' then
        require('org-lsp.view').save(result.save)
      end
      -- Images to show in place of links or LaTeX, or to take away.
      if type(result) == 'table' and type(result.preview) == 'table' then
        require('org-lsp.images').apply(result.preview)
      end
      -- Lines to hide (the custom properties), those hidden before shown.
      if type(result) == 'table' and type(result.hiddenLines) == 'table' then
        require('org-lsp.hidden').apply(result.hiddenLines)
      end
      -- What the command has to say, as Emacs says it in the echo area.
      if type(result) == 'table' and type(result.message) == 'string' then
        notify(result.message, vim.log.levels.INFO)
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
          -- (The buffer may be gone while the question was open.)
          if answer and vim.api.nvim_buf_is_valid(bufnr) then
            -- What the server asked to have back, and the answer.
            for key, value in pairs(input.context or {}) do
              context[key] = value
            end
            context.input = answer
            exec(command, handle)
          end
        end
        if input.kind == 'key' then
          return answered(M.read_key(input.prompt, input.choices, input.expert))
        end
        -- A priority is one key, as `org-priority` reads it: a letter, or
        -- SPC to remove it.
        if input.kind == 'priority' then
          return answered(M.read_key(input.prompt))
        end
        if input.choices then
          -- One of a list, such as the targets of a refile.
          local format = function(choice)
            return choice.label
          end
          return vim.ui.select(input.choices, { prompt = M.prompt_text(input.prompt), format_item = format }, function(choice)
            answered(choice and choice.key)
          end)
        end
        -- Text that may be typed, completed from a list (Emacs's
        -- `completing-read`).
        local completion
        if input.completions and #input.completions > 0 then
          M.completions = input.completions
          M.completion_separator = input.separator
          completion = 'customlist,v:lua.OrgLspCompleteInput'
        end
        -- What a date typed so far means, shown as it is typed, as
        -- `org-read-date` shows it in the minibuffer: the server writes it
        -- on the calendar's last line.
        local previewing
        if input.kind == 'date' then
          previewing = vim.api.nvim_create_autocmd('CmdlineChanged', {
            group = vim.api.nvim_create_augroup('org_lsp_date_preview', { clear = true }),
            callback = function()
              local arguments = vim.tbl_extend('force', context, { command = name, input = vim.fn.getcmdline() })
              local params = { command = 'org.date.preview', arguments = { arguments } }
              client:request('workspace/executeCommand', params, function()
                vim.cmd.redraw()
              end, bufnr)
            end,
          })
        end
        vim.ui.input({ prompt = M.prompt_text(input.prompt), default = input.value, completion = completion }, function(answer)
          if previewing then
            pcall(vim.api.nvim_del_autocmd, previewing)
          end
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
    exec(command, handle)
  end)
end

--- A prompt as it is shown: Emacs's own words (`Execute this? `, ending in
--- a space) as they are, a title (`Select a TODO state`) followed by `: `.
---@param prompt string
---@return string
function M.prompt_text(prompt)
  if prompt:match('%s$') then
    return prompt
  end
  return prompt .. (prompt:match('[:?]$') and ' ' or ': ')
end

--- The completions of the text a command asks for, beginning with what is
--- typed, without regard to case.
---@param lead string
---@return string[]
function _G.OrgLspCompleteInput(lead)
  -- With a separator (the `:` between tags), what follows the last one is
  -- completed, what comes before it kept.
  local before = ''
  local separator = M.completion_separator
  if separator and separator ~= '' then
    local last = lead:match('.*()' .. vim.pesc(separator))
    if last then
      before, lead = lead:sub(1, last), lead:sub(last + 1)
    end
  end
  local lower = lead:lower()
  local out = {}
  for _, candidate in ipairs(M.completions or {}) do
    if candidate:lower():sub(1, #lower) == lower then
      table.insert(out, before .. candidate)
    end
  end
  return out
end

--- The closed folds of the current window, as their first and last
--- lines, from 0.
---@return integer[][]
function M.closed_folds()
  local folds, line, last = {}, 1, vim.api.nvim_buf_line_count(0)
  while line <= last do
    if vim.fn.foldclosed(line) == line then
      local finish = vim.fn.foldclosedend(line)
      table.insert(folds, { line - 1, finish - 1 })
      line = finish + 1
    else
      line = line + 1
    end
  end
  return folds
end

local sparse_namespace = vim.api.nvim_create_namespace('org_lsp_sparse')
-- By buffer, how many times highlights were shown: a change clears only
-- those it was waiting for.
local sparse_shown = {}

--- Show a sparse tree (`org-sparse-tree`) in the windows of `bufnr`: all
--- folds closed, then those around each of `sparse.lines` opened, as
--- `org-occur` shows its matches with everything else hidden; the matches
--- highlighted (`OrgLspSparseMatch`) until the text changes, or with
--- `keepHighlights` until they are replaced. The cursor stays. With `keep`
--- (`org-reveal`), only the folds that start on the lines of `unfold` open,
--- outer ones first; the others stay as they are, and `highlights`, when
--- given (`C-c C-c`), replace those shown.
---@param bufnr integer
---@param sparse { lines: integer[], highlights: table[]?, keepHighlights: boolean?, keep: boolean?, unfold: integer[]? }
---@param client vim.lsp.Client
function M.show_sparse(bufnr, sparse, client)
  if not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  vim.api.nvim_set_hl(0, 'OrgLspSparseMatch', { link = 'Search', default = true })
  if not sparse.keep or sparse.highlights then
    vim.api.nvim_buf_clear_namespace(bufnr, sparse_namespace, 0, -1)
    sparse_shown[bufnr] = (sparse_shown[bufnr] or 0) + 1
  end
  local function column(position)
    local text = vim.api.nvim_buf_get_lines(bufnr, position.line, position.line + 1, false)[1] or ''
    return vim.str_byteindex(text, client.offset_encoding, position.character, false)
  end
  for _, range in ipairs(sparse.highlights or {}) do
    pcall(vim.api.nvim_buf_set_extmark, bufnr, sparse_namespace, range.start.line, column(range.start), {
      end_row = range['end'].line,
      end_col = column(range['end']),
      hl_group = 'OrgLspSparseMatch',
    })
  end
  if sparse.keep then
    for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
      if vim.wo[win].foldenable then
        vim.api.nvim_win_call(win, function()
          for _, line in ipairs(sparse.unfold or {}) do
            if vim.fn.foldclosed(line + 1) == line + 1 then
              vim.cmd((line + 1) .. 'foldopen')
            end
          end
        end)
      end
    end
    return
  end
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    if vim.wo[win].foldenable then
      vim.api.nvim_win_call(win, function()
        local cursor = vim.api.nvim_win_get_cursor(0)
        vim.cmd('normal! zM')
        for _, line in ipairs(sparse.lines or {}) do
          pcall(vim.api.nvim_win_set_cursor, 0, { line + 1, 0 })
          vim.cmd('normal! zv')
        end
        vim.api.nvim_win_set_cursor(0, cursor)
      end)
    end
  end
  -- Org takes the highlighting away at the next change, unless the server
  -- says it stays (`org-remove-highlights-with-change`).
  if sparse.keepHighlights then
    return
  end
  local shown = sparse_shown[bufnr]
  vim.api.nvim_buf_attach(bufnr, false, {
    on_lines = function()
      vim.schedule(function()
        if vim.api.nvim_buf_is_valid(bufnr) and sparse_shown[bufnr] == shown then
          vim.api.nvim_buf_clear_namespace(bufnr, sparse_namespace, 0, -1)
        end
      end)
      return true
    end,
  })
end

--- How a key of a key input shows: Emacs's names for the blank ones, and
--- `C-b` for a control character.
---@param key string
---@return string
function M.key_name(key)
  local names = { [' '] = 'SPC', ['\r'] = 'RET', ['\t'] = 'TAB' }
  if names[key] then
    return names[key]
  end
  local code = #key == 1 and key:byte() or -1
  return (code > 0 and code < 32) and ('C-' .. string.char(code + 96)) or key
end

--- The window that lists the choices of `read_key`, as Emacs lists them
--- in a window of its own at the bottom of the frame.
---@param prompt string
---@param choices { key: string, label: string, highlight?: string, face?: table }[]
---@return integer window
local function show_choices(prompt, choices)
  local lines, width = {}, vim.fn.strdisplaywidth(prompt) + 2
  for _, choice in ipairs(choices) do
    local line = ('[%s] %s'):format(M.key_name(choice.key), choice.label)
    table.insert(lines, line)
    width = math.max(width, vim.fn.strdisplaywidth(line))
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  local namespace = vim.api.nvim_create_namespace('org_lsp_choices')
  for row, line in ipairs(lines) do
    local key_end = line:find(']', 1, true)
    vim.api.nvim_buf_set_extmark(buf, namespace, row - 1, 0, { end_col = key_end, hl_group = 'Special' })
    -- A TODO keyword in its face, as `org-fast-todo-selection` shows it.
    local group = require('org-lsp.highlights').choice_group(choices[row])
    if group then
      vim.api.nvim_buf_set_extmark(buf, namespace, row - 1, key_end + 1, { end_col = #line, hl_group = group })
    end
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
---@param expert? boolean the keys in the prompt alone, no window
---@return string?
function M.read_key(prompt, choices, expert)
  -- `expert` (`org-use-fast-todo-selection`): the keys are in the prompt,
  -- which shows alone, as Emacs's message does.
  local text = expert and prompt or M.prompt_text(prompt)
  -- A prompt longer than the echo area would scroll it and wait for Enter
  -- (the hit-enter prompt), which then takes the prompt away before the
  -- key is read: the command line grows to hold it while it is asked, as
  -- Emacs's minibuffer does.
  local cmdheight = vim.o.cmdheight
  local width = vim.fn.strdisplaywidth(text)
  if width >= vim.v.echospace then
    vim.o.cmdheight = cmdheight + math.ceil((width - vim.v.echospace + 1) / vim.o.columns)
  end
  local win = not expert and choices and #choices > 0 and show_choices(prompt, choices) or nil
  vim.api.nvim_echo({ { text } }, false, {})
  vim.cmd.redraw()
  local ok, key = pcall(vim.fn.getcharstr)
  vim.api.nvim_echo({ { '' } }, false, {})
  if win and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_win_close(win, true)
  end
  if vim.o.cmdheight ~= cmdheight then
    vim.o.cmdheight = cmdheight
  end
  if not ok or key == vim.keycode('<Esc>') or key == vim.keycode('<C-c>') then
    return nil
  end
  return key
end

--- The Visual selection, as Emacs's region: its start and the position
--- after its end, `{ line, byte }` from 0; whole lines from Visual line
--- mode. Nil out of Visual mode.
---@return { start: integer[], end: integer[], cursor_at_end: boolean }?
function M.selection()
  local mode = vim.fn.mode()
  if mode ~= 'v' and mode ~= 'V' and mode ~= '\22' then
    return nil
  end
  local cursor = vim.fn.getpos('.')
  local other = vim.fn.getpos('v')
  local first, last = other, cursor
  if cursor[2] < other[2] or (cursor[2] == other[2] and cursor[3] < other[3]) then
    first, last = cursor, other
  end
  local cursor_at_end = last == cursor
  if mode == 'V' then
    return { start = { first[2] - 1, 0 }, ['end'] = { last[2], 0 }, cursor_at_end = cursor_at_end }
  end
  -- The last character is in the selection: the end is after it.
  local line = vim.fn.getline(last[2])
  local after = last[3] - 1
  if after < #line then
    after = after + #vim.fn.strcharpart(line:sub(last[3]), 0, 1)
  end
  return { start = { first[2] - 1, first[3] - 1 }, ['end'] = { last[2] - 1, after }, cursor_at_end = cursor_at_end }
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

--- The commands whose keys are typed in insert mode (`org.return`).
local insert_keys = {}

--- The commands whose keys act below the cursor's line and go on in
--- insert mode (`org.meta.return`).
local append_keys = {}

--- The buffer-local insert-mode mappings the plugin's keys took the place
--- of, by buffer and key.
---@type table<integer, table<string, table>>
local replaced = {}

--- Set while the key's own meaning runs, so that a mapping of it that
--- maps the key again (`imap <CR> …<CR>`) reaches Vim's own key and not
--- the plugin's again.
local passing = false

--- What `key` does without the plugin: the buffer's mapping that the
--- plugin's took the place of, else the global mapping (an autopairs
--- plugin's `<CR>`), else the key itself.
---@param bufnr integer
---@param key string
local function key_without_plugin(bufnr, key)
  local code = vim.keycode(key)
  local map = (replaced[bufnr] or {})[code]
    or vim.iter(vim.api.nvim_get_keymap('i')):find(function(global)
      return vim.keycode(global.lhs) == code
    end)
  -- Ahead of what is typed after it.
  if not map then
    return vim.api.nvim_feedkeys(code, 'ni', false)
  end
  local keys
  if map.callback then
    local result = map.callback()
    if map.expr ~= 1 or type(result) ~= 'string' then
      return
    end
    keys = map.replace_keycodes == 1 and vim.keycode(result) or result
  elseif map.expr == 1 then
    keys = vim.api.nvim_eval((map.rhs:gsub('<SID>', '<SNR>' .. (map.sid or 0) .. '_')))
  else
    keys = vim.keycode(map.rhs)
  end
  if map.noremap ~= 1 then
    passing = true
    vim.schedule(function()
      passing = false
    end)
  end
  vim.api.nvim_feedkeys(keys, (map.noremap == 1 and 'n' or 'm') .. 'i', false)
end

--- Whether a completion menu is open: Vim's, or nvim-cmp's or blink.cmp's.
local function completing()
  if vim.fn.pumvisible() == 1 then
    return true
  end
  local cmp_ok, cmp = pcall(require, 'cmp')
  if cmp_ok and type(cmp) == 'table' and type(cmp.visible) == 'function' and cmp.visible() then
    return true
  end
  local blink_ok, blink = pcall(require, 'blink.cmp')
  return blink_ok and type(blink) == 'table' and type(blink.is_visible) == 'function' and blink.is_visible() or false
end

--- Close the completion menu that is open, nvim-cmp's or blink.cmp's.
--- Returns whether none is open now: Vim's own closes only once the
--- mapping is done.
local function close_completion()
  local cmp_ok, cmp = pcall(require, 'cmp')
  if cmp_ok and type(cmp) == 'table' and type(cmp.visible) == 'function' and cmp.visible() then
    cmp.close()
  end
  local blink_ok, blink = pcall(require, 'blink.cmp')
  if blink_ok and type(blink) == 'table' and type(blink.is_visible) == 'function' and blink.is_visible() then
    blink.hide()
  end
  return vim.fn.pumvisible() == 0
end

--- Run `name`, whose result carries edits and a cursor (`org.return`), from
--- insert mode: at once, so that what is typed next goes after it. The key
--- keeps its own meaning (another plugin's mapping of it included) with no
--- server, or where the command does not apply. While a completion menu is
--- open, `<CR>` is the menu's; another key for the command (`<S-CR>`)
--- closes the menu and runs it.
---@param name string
---@param key string
function M.insert_command(name, key)
  local bufnr = vim.api.nvim_get_current_buf()
  local client = running_client()
  local function native()
    key_without_plugin(bufnr, key)
  end
  if passing then
    -- The key mapped again by its own meaning: Vim's key, once.
    passing = false
    return vim.api.nvim_feedkeys(vim.keycode(key), 'ni', false)
  end
  if not client or completing() and vim.keycode(key) == vim.keycode('<CR>') then
    return native()
  end
  if completing() and not close_completion() then
    -- Vim's menu takes its entry with `<C-y>`, then the key again, ahead
    -- of what is typed after it.
    vim.api.nvim_feedkeys(vim.keycode(key), 'mi', false)
    return vim.api.nvim_feedkeys(vim.keycode('<C-y>'), 'ni', false)
  end
  local context = vim.lsp.util.make_position_params(0, client.offset_encoding)
  local response = client:request_sync('workspace/executeCommand', { command = name, arguments = { context } }, 1000, bufnr)
  local result = response and not response.err and response.result
  if type(result) ~= 'table' or type(result.edits) ~= 'table' then
    return native()
  end
  M.apply_carried(result, bufnr, client)
end

--- Apply the edits a command's result carries to `bufnr`, and put the
--- cursor where the result says.
---@param result { edits: table[], cursor?: { line: integer, character: integer } }
---@param bufnr integer
---@param client vim.lsp.Client
---@return boolean # whether the cursor goes at the end of its line
function M.apply_carried(result, bufnr, client)
  -- The answer may come after the buffer is gone.
  if not vim.api.nvim_buf_is_valid(bufnr) then
    return false
  end
  vim.lsp.util.apply_text_edits(result.edits, bufnr, client.offset_encoding)
  local cursor = result.cursor
  if cursor and vim.api.nvim_get_current_buf() == bufnr then
    local text = vim.api.nvim_buf_get_lines(bufnr, cursor.line, cursor.line + 1, false)[1] or ''
    local column = vim.str_byteindex(text, client.offset_encoding, cursor.character, false)
    pcall(vim.api.nvim_win_set_cursor, 0, { cursor.line + 1, column })
    return column >= #text
  end
  return false
end

--- The keys of a scope: the server's defaults for modal editors, with the
--- configuration's entries taking their place command by command.
---@param scope 'global'|'org'|'agenda'|'calendar'|'capture'|'columns'
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
        insert_keys[command.command] = key.mode == 'insert' or nil
        append_keys[command.command] = key.mode == 'append' or nil
      end
    end
  end
  -- The keys of the mappings, the plugin's own (`q` for `quit`) and the
  -- user's, are theirs: a command of the server's with the same key goes
  -- without it. `K` in the agenda stays Neovim's hover, which shows the
  -- line's entry (Emacs's `K`, `org.agenda.habits`, goes without a key).
  local mapped = scope == 'agenda' and { [vim.keycode('K')] = 'hover' } or {}
  for name, key in pairs(M.config.mappings[scope] or {}) do
    keys[name] = type(key) == 'table' and key or key and { key } or {}
    for _, lhs in ipairs(keys[name]) do
      mapped[vim.keycode(lhs)] = name
    end
  end
  for name, list in pairs(keys) do
    keys[name] = vim.tbl_filter(function(lhs)
      local owner = mapped[vim.keycode(lhs)]
      return owner == nil or owner == name
    end, list)
  end
  return keys, native
end

--- The keys of a scope as they are set, for the help: in the order of the
--- server's commands, then the plugin's own.
---@param scope 'global'|'org'|'agenda'|'calendar'|'capture'|'columns'
---@return { command: string, title: string, keys: string[] }[]
function M.keys(scope)
  local keys = keys_in(scope)
  local out = {}
  local function add(name, title)
    if keys[name] and #keys[name] > 0 then
      table.insert(out, { command = name, title = title, keys = keys[name] })
      keys[name] = nil
    end
  end
  for _, command in ipairs(M.manifest and M.manifest.commands or {}) do
    add(command.command, command.title)
  end
  for _, name in ipairs({ 'cycle', 'global_cycle', 'quit', 'hover', 'help' }) do
    add(name, own_commands[name][1])
  end
  -- Keys of the user's for commands neither knows of.
  for name, list in pairs(keys) do
    if #list > 0 then
      table.insert(out, { command = name, title = name, keys = list })
    end
  end
  return out
end

--- Global keys that are set, so that a change of configuration can take
--- them back.
local global_keys = {}

---@param scope 'global'|'org'|'agenda'|'calendar'|'capture'|'columns'
---@param bufnr? integer the buffer the keys are for; none for global keys
local set_keys

--- Sets the keys of `scope` in buffer `bufnr`; for column view (see
--- org-lsp.columns), which comes and goes. The keys set.
---@param scope string
---@param bufnr integer
---@return string[]
function M._set_keys(scope, bufnr)
  return set_keys(scope, bufnr)
end

set_keys = function(scope, bufnr)
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
  if scope == 'agenda' or scope == 'calendar' or scope == 'columns' then
    -- A key waits for more when it starts a longer one of the buffer's,
    -- or a global one, with the leader as it is set: `,` (priority) and
    -- `<Leader>ois` (schedule) with `,` as the leader.
    local all = vim.iter(vim.tbl_values(keys)):flatten():totable()
    local longer = vim.list_extend(vim.list_extend({}, all), global_keys)
    for _, lhs in ipairs(all) do
      local code = vim.keycode(lhs)
      nowait[lhs] = not vim.iter(longer):any(function(other)
        local other_code = vim.keycode(other)
        return other_code ~= code and vim.startswith(other_code, code)
      end)
    end
  end
  local set = {}
  for name, list in pairs(keys) do
    local command = server_command(name)
    local title = own_commands[name] and own_commands[name][1] or command and command.title or name
    for _, lhs in ipairs(list) do
      table.insert(set, lhs)
      if scope == 'global' then
        table.insert(global_keys, lhs)
      end
      local key = native[lhs] and lhs or nil
      if insert_keys[name] then
        -- A mapping of the buffer's that this one takes the place of (a
        -- completion plugin's `<CR>`) is what the key does where the
        -- command does not apply.
        local before = vim.fn.maparg(lhs, 'i', false, true)
        if before.buffer == 1 and before.desc ~= title then
          replaced[bufnr] = replaced[bufnr] or {}
          replaced[bufnr][vim.keycode(lhs)] = before
        end
        vim.keymap.set('i', lhs, function()
          M.insert_command(name, lhs)
        end, { buffer = bufnr, desc = title })
      elseif append_keys[name] then
        vim.keymap.set('n', lhs, function()
          M.run(name, { append = true })
        end, { buffer = bufnr, desc = title })
      elseif command and command.edits and scope == 'org' then
        vim.keymap.set('n', lhs, repeatable(name, key), { buffer = bufnr, desc = title, expr = true })
      else
        vim.keymap.set('n', lhs, function()
          M.run(name, { count = vim.v.count, key = key })
        end, { buffer = bufnr, desc = title, nowait = nowait[lhs] })
      end
      -- On a selection, the commands Org gives a region.
      if command and command.region and scope == 'org' then
        vim.keymap.set('x', lhs, function()
          M.run(name, { region = true })
        end, { buffer = bufnr, desc = title })
      end
    end
  end
  return set
end

-- ----- attaching

--- Open the folds starting at the lines of `open` and close those of
--- `close` (0-based `startLine`s, innermost first) in window `win` while
--- it shows buffer `bufnr`, once it has the server's folds, which come a
--- moment after the buffer, or after an edit of the server's. A fold that
--- closes other lines than the server's range is not the server's yet: it
--- is opened again, and tried again a moment later (the last time it stays
--- closed).
---@param win integer
---@param bufnr integer
---@param open table[]
---@param close table[]
local function set_folds(win, bufnr, open, close)
  local first = close[1] or open[1]
  if not first then
    return
  end
  local tries = 0
  local function apply()
    if not vim.api.nvim_win_is_valid(win) or vim.api.nvim_win_get_buf(win) ~= bufnr then
      return
    end
    local ready = vim.api.nvim_win_call(win, function()
      return vim.fn.foldlevel(first.startLine + 1) > 0
    end)
    if not ready and tries < 40 then
      tries = tries + 1
      return vim.defer_fn(apply, 50)
    end
    vim.api.nvim_win_call(win, function()
      for _, fold in ipairs(open) do
        vim.cmd(('silent! %dfoldopen'):format(fold.startLine + 1))
      end
      open = {}
      local stale = {}
      for _, fold in ipairs(close) do
        local line = fold.startLine + 1
        -- (A line in a fold closed already stays as it is.)
        if vim.fn.foldclosed(line) == -1 then
          vim.cmd(('silent! %dfoldclose'):format(line))
          local ends = fold.endLine and vim.fn.foldclosedend(line) ~= fold.endLine + 1
          if tries < 40 and (vim.fn.foldclosed(line) ~= line or ends) then
            vim.cmd(('silent! %dfoldopen'):format(line))
            table.insert(stale, fold)
          end
        end
      end
      close = stale
    end)
    if #close > 0 and tries < 40 then
      tries = tries + 1
      first = close[1]
      vim.defer_fn(apply, 50)
    end
  end
  apply()
end

--- The folds the server says a file opens with closed (`org_startup_folded`,
--- `#+STARTUP:`, `VISIBILITY` properties), closed in window `win` once it
--- has the server's folds, innermost first.
---@param win integer
local function close_startup_folds(win)
  local bufnr = vim.api.nvim_win_get_buf(win)
  local client = vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1]
  if not client then
    return
  end
  local params = { textDocument = vim.lsp.util.make_text_document_params(bufnr) }
  client:request('org-lsp/startupFolds', params, function(err, result)
    set_folds(win, bufnr, {}, not err and result and result.folds or {})
  end, bufnr)
end

--- The headlines' folds the server asks for (`org-lsp/folds`): as
--- org-journal leaves them, or of the subtrees a yank or paste folds, in
--- the windows that show the document.
---@param params { uri: string, open: table[], close: table[] }
function M.journal_folds(params)
  local bufnr = require('org-lsp.buffers').of(params.uri)
  if not bufnr then
    return
  end
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    set_folds(win, bufnr, params.open or {}, params.close or {})
  end
end

--- org-journal's files need not have a name that says they are Org files
--- (`org-journal-is-journal`): those of its directory without an extension
--- are, here and in the buffers already loaded.
---@param client vim.lsp.Client
local function journal_files(client)
  local journal = vim.tbl_get(client.server_capabilities, 'experimental', 'orgJournal')
  local directory = type(journal) == 'table' and journal.directory
  if type(directory) ~= 'string' then
    return
  end
  local pattern = vim.pesc(vim.fs.normalize(directory)) .. '/[^.]+'
  vim.filetype.add({ pattern = { [pattern] = 'org' } })
  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    local name = vim.api.nvim_buf_get_name(bufnr)
    if vim.bo[bufnr].filetype == '' and name:match('^' .. pattern .. '$') then
      vim.bo[bufnr].filetype = 'org'
    end
  end
end

--- The server's folds in window `win`, for the buffer it shows: once per
--- buffer, as the options are local to it there and later changes of the
--- user's are kept.
local function set_window_folds(win)
  local key = 'org_lsp_folds_' .. vim.api.nvim_win_get_buf(win)
  if vim.w[win][key] then
    return
  end
  vim.w[win][key] = true
  vim.wo[win][0].foldmethod = 'expr'
  vim.wo[win][0].foldexpr = 'v:lua.vim.lsp.foldexpr()'
  -- A closed fold shows its first line and `org-ellipsis`, nothing after.
  vim.wo[win][0].foldtext = "v:lua.require'org-lsp.folds'.foldtext()"
  local fillchars = vim.api.nvim_get_option_value('fillchars', { win = win })
  if not fillchars:find('fold:') then
    vim.wo[win][0].fillchars = fillchars .. (fillchars == '' and '' or ',') .. 'fold: '
  end
  -- Everything open, then closed as the server says the file opens
  -- (`#+STARTUP:`, `VISIBILITY`, the settings): the user's 'foldlevel'
  -- does not decide for an Org file.
  vim.wo[win][0].foldlevel = 99
  close_startup_folds(win)
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
        -- The regions of code in the syntax of its language go with it.
        require('org-lsp.src').reapply(bufnr)
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
  -- A level of the user's own (a `FileType` autocommand's) stays.
  local function set_window(win)
    if vim.wo[win][0].conceallevel == 0 then
      vim.wo[win][0].conceallevel = 2
    end
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
    return notify('org-lsp: the server is older than this plugin; run :OrgLspDownload or :OrgLspBuild', vim.log.levels.WARN)
  end
  require('org-lsp.highlights').define(M.manifest)
  require('org-lsp.highlights').define_faces((client.server_capabilities.experimental or {}).orgFaces)
  require('org-lsp.folds').ellipsis = (client.server_capabilities.experimental or {}).orgEllipsis or '...'
  require('org-lsp.view').setup(M.manifest)
  set_keys('global')
  for _, fn in ipairs(waiting) do
    fn(client)
  end
  waiting = {}
end

---@param client vim.lsp.Client
---@param bufnr integer
--- Whether Neovim's hover floats are watched for Org text (`preview.watch`).
local hover_watched = false

local function on_attach(client, bufnr)
  -- Neovim's own hover shows an agenda entry highlighted, as Org files are.
  if not hover_watched then
    hover_watched = true
    require('org-lsp.preview').watch()
  end
  if not M.manifest then
    return
  end
  require('org-lsp.highlights').attach_faces(bufnr)
  require('org-lsp.highlights').attach_looks(bufnr, M.manifest)
  journal_files(client)
  if vim.bo[bufnr].filetype == M.manifest.agenda.languageId then
    return set_keys('agenda', bufnr)
  end
  if M.manifest.datePrompt and vim.bo[bufnr].filetype == M.manifest.datePrompt.languageId then
    return set_keys('calendar', bufnr)
  end
  -- An agenda entry in a float (`K`): highlighted as any Org file, with
  -- no keys, indenting or folds.
  local preview = vim.b[bufnr].org_lsp_preview
  if not preview then
    set_keys('org', bufnr)
    -- The times of `org-clock-display` (`<Leader>oxd`) are inlay hints.
    if vim.lsp.inlay_hint and client:supports_method('textDocument/inlayHint') then
      vim.lsp.inlay_hint.enable(true, { bufnr = bufnr })
    end
  end
  -- A capture buffer is an Org file with keys of its own on top.
  local prefix = M.manifest.capture and M.manifest.capture.prefix
  if not preview and prefix and vim.startswith(vim.fs.basename(vim.api.nvim_buf_get_name(bufnr)), prefix) then
    set_keys('capture', bufnr)
  end
  if M.config.replace_syntax and client:supports_method('textDocument/semanticTokens/full') then
    replace_syntax(bufnr)
  end
  if M.config.conceal then
    conceal(bufnr)
  end
  if preview then
    return
  end
  -- The previews a file opens with (`#+STARTUP: inlineimages`).
  if require('org-lsp.images').supported() then
    local params = vim.lsp.util.make_position_params(0, client.offset_encoding)
    params.textDocument = { uri = vim.uri_from_bufnr(bufnr) }
    params.position = { line = 0, character = 0 }
    params.startup = true
    client:request('workspace/executeCommand', { command = 'org.link.preview', arguments = { params } }, function(_, result)
      if type(result) == 'table' and type(result.preview) == 'table' then
        require('org-lsp.images').apply(result.preview, true)
      end
    end, bufnr)
    -- And of LaTeX (`#+STARTUP: latexpreview`), in the colors of the text:
    -- they come in `org-lsp/images` once Emacs has made them.
    local latex = vim.deepcopy(params)
    local normal = vim.api.nvim_get_hl(0, { name = 'Normal', link = false })
    latex.foreground = normal.fg and ('#%06x'):format(normal.fg) or nil
    latex.background = 'Transparent'
    client:request('workspace/executeCommand', { command = 'org.latex.preview', arguments = { latex } }, function() end, bufnr)
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

--- The settings the server starts with: `settings`, with `startup_folded`
--- as `org_startup_folded` when the settings do not say.
---@return table
function M.server_settings()
  local settings = vim.deepcopy(M.config.settings or {})
  if settings.org_startup_folded == nil and M.config.startup_folded then
    settings.org_startup_folded = M.config.startup_folded
  end
  return settings
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
    init_options = M.server_settings(),
    -- Images in place of text, where Neovim can draw them; code in its
    -- language's colors, where the server says it is.
    capabilities = vim.tbl_deep_extend('force', view.capabilities, {
      experimental = {
        orgImages = require('org-lsp.images').supported(),
        orgCodeBlocks = M.config.src_highlight and true or false,
      },
    }),
    handlers = view.handlers,
    on_init = on_init,
    -- One server for every Org file, whatever its root, and the one
    -- started for the agenda with no file open: the server reads the
    -- agenda files from its settings, not from a workspace.
    reuse_client = function(client, config)
      return client.name == config.name and not client:is_stopped()
    end,
  })
  vim.lsp.enable(SERVER)
  return true
end

--- Start the server (again) in the Org buffers that are open: stop the
--- one that runs, then attach them as opening them would have.
local function restart()
  local client = running_client()
  if client then
    client:stop()
  end
  local function attach()
    if client and not client:is_stopped() then
      return vim.defer_fn(attach, 100)
    end
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      if vim.api.nvim_buf_is_loaded(bufnr) and vim.bo[bufnr].filetype == 'org' then
        vim.api.nvim_exec_autocmds('FileType', { group = 'nvim.lsp.enable', buffer = bufnr, modeline = false })
      end
    end
  end
  attach()
end

--- Download the server, then start it (or restart it, when one runs).
local function download()
  require('org-lsp.download').run(M.config.release, function()
    if enable() then
      restart()
    end
  end)
end

--- Build the server with cargo inside the plugin directory, then start it.
local function build()
  if vim.fn.executable('cargo') == 0 then
    notify('org-lsp: cargo is not installed; see https://rustup.rs', vim.log.levels.ERROR)
    return
  end
  notify('org-lsp: building the server…')
  vim.system({ 'cargo', 'build', '--release', '-p', 'org-lsp' }, { cwd = plugin_root(), text = true }, function(result)
    vim.schedule(function()
      if result.code ~= 0 then
        notify('org-lsp: build failed\n' .. (result.stderr or ''), vim.log.levels.ERROR)
      elseif enable() then
        notify('org-lsp: built; restart Neovim to use the new server')
      end
    end)
  end)
end

--- `:Org agenda list` runs `org.agenda.list`. A group of commands stands
--- for its dispatcher: `:Org agenda` runs `org.agenda.dispatch`. With a
--- range (`:'<,'>Org toggle heading`), its lines are the region.
---@param words string[]
---@param range? integer[] the first and last line
local function user_command(words, range)
  local id = 'org.' .. table.concat(#words > 0 and words or { 'agenda' }, '.')
  with_client(function()
    if not server_command(id) and server_command(id .. '.dispatch') then
      id = id .. '.dispatch'
    end
    M.run(id, { lines = range })
  end)
end

--- The next word of `:Org`'s command after those typed: `:Org agenda <Tab>`
--- offers `list`, `todos`, `dispatch`, ….
---@param lead string what has been typed of the word
---@param line string the command line
---@param pos integer where the cursor is in it
---@return string[]
local function complete_user_command(lead, line, pos)
  local typed = line:sub(1, pos - #lead):gsub('^%s*%S+', '', 1)
  local before = vim.split(vim.trim(typed), '%s+', { trimempty = true })
  local prefix = table.concat(vim.list_extend({ 'org' }, before), '.') .. '.'
  local words, seen = {}, {}
  for _, command in ipairs(M.manifest and M.manifest.commands or {}) do
    if vim.startswith(command.command, prefix) then
      local word = command.command:sub(#prefix + 1):match('^[^.]+')
      if word and vim.startswith(word, lead) and not seen[word] then
        seen[word] = true
        table.insert(words, word)
      end
    end
  end
  table.sort(words)
  return words
end

local started = false

--- What Emacs's mode line shows while a clock runs, for the statusline:
--- `[0:25/1:00] (Write the report)`, then the relative timer or the
--- countdown while one is on (`<0:05:23>`); or an empty string.
---
--- ```lua
--- vim.o.statusline = '%f %=%{v:lua.require("org-lsp").clock_status()}'
--- ```
---@return string
function M.clock_status()
  local clock = require('org-lsp.clock')
  local parts = vim.tbl_filter(function(part)
    return part ~= ''
  end, { clock.status(), clock.timer_status() })
  return table.concat(parts, ' ')
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
        require('org-lsp.highlights').define_faces()
      end
    end,
  })
  vim.api.nvim_create_user_command('OrgLspBuild', build, { desc = 'Build the org-lsp server with cargo' })
  vim.api.nvim_create_user_command('OrgLspDownload', download, { desc = 'Download the org-lsp server' })
  vim.api.nvim_create_user_command('Org', function(opts)
    user_command(opts.fargs, opts.range > 0 and { opts.line1, opts.line2 } or nil)
  end, { nargs = '*', range = true, complete = complete_user_command, desc = 'Run an Org command' })
  set_keys('global')
  if not enable() then
    vim.api.nvim_create_autocmd('FileType', {
      group = group,
      pattern = 'org',
      once = true,
      callback = function()
        if M.config.download and not M.server_command() then
          download()
        elseif not enable() then
          warn_missing_server()
        end
      end,
    })
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
