--- What the server asks the editor to do: show documents, among them the
--- ones the server owns and the editor only displays (the agenda and the
--- date prompt), and apply edits.
---
--- For those this is the client side of LSP 3.18's
--- `workspace/textDocumentContent`, which Neovim does not have built in. A
--- buffer named with the server's scheme is filled with the text the server
--- returns, and filled again when the server says that the text has changed.
local M = {}

--- Tells the server that this client fetches such documents, and shows
--- its date prompt.
M.capabilities = {
  workspace = { textDocumentContent = { dynamicRegistration = false } },
  experimental = { orgDatePrompt = true, orgVirtualIndent = true },
}

--- The manifest's sections for the documents the server owns, and the
--- name capture buffers start with.
local documents = {}

---@param win integer
---@return boolean
local function is_float(win)
  return vim.api.nvim_win_get_config(win).relative ~= ''
end

--- Whether `win` shows nothing: a buffer without a name, unchanged and
--- empty.
---@param win integer
---@return boolean
local function is_empty(win)
  local buf = vim.api.nvim_win_get_buf(win)
  return vim.api.nvim_buf_get_name(buf) == ''
    and vim.bo[buf].buftype == ''
    and not vim.bo[buf].modified
    and vim.api.nvim_buf_line_count(buf) == 1
    and vim.api.nvim_buf_get_lines(buf, 0, 1, false)[1] == ''
end

--- The window an entry's file opens in from the agenda's window `from`
--- with TAB (`org-agenda-goto`): one that shows the file, else an empty
--- one, else a new one beside the window the agenda was opened from.
---@param from integer
---@param bufnr integer
---@return integer
local function other_window(from, bufnr)
  local wins = vim.tbl_filter(function(win)
    return win ~= from and not is_float(win)
  end, vim.api.nvim_tabpage_list_wins(0))
  for _, win in ipairs(wins) do
    if vim.api.nvim_win_get_buf(win) == bufnr then
      return win
    end
  end
  for _, win in ipairs(wins) do
    if is_empty(win) then
      return win
    end
  end
  -- Split the previous window, or the agenda's when it is the only one:
  -- side by side when it is wide, as Emacs's `split-window-sensibly`.
  local previous = vim.fn.win_getid(vim.fn.winnr('#'))
  local base = vim.tbl_contains(wins, previous) and previous or wins[1] or from
  local new
  vim.api.nvim_win_call(base, function()
    vim.cmd(vim.api.nvim_win_get_width(base) >= 160 and 'belowright vsplit' or 'belowright split')
    new = vim.api.nvim_get_current_win()
  end)
  return new
end

--- Open a window for `bufnr` as `mode` says, make it the current one and
--- return it. `mode` is `'float'`, `'current'` (the current window), an Ex
--- command that opens a window (`'split'`, `'vsplit'`, `'botright split'`,
--- `'tabnew'`), or a function that is given the buffer and returns the
--- window it opened for it.
---@param bufnr integer
---@param mode string|fun(bufnr: integer): integer?
---@return integer
local function open_window(bufnr, mode)
  local win
  local from = vim.api.nvim_get_current_win()
  local size = require('org-lsp').config.win_split_size
  if type(size) ~= 'number' or size <= 0 then
    size = nil
  end
  --- `size` of `total`: a fraction of it, or lines or columns.
  local function sized(total, default)
    local value = size or default
    if not value then
      return nil
    end
    return math.max(1, math.min(total, value < 1 and math.floor(total * value) or math.floor(value)))
  end
  if type(mode) == 'function' then
    win = mode(bufnr)
    if type(win) ~= 'number' or not vim.api.nvim_win_is_valid(win) then
      win = vim.api.nvim_get_current_win()
    end
  elseif mode == 'float' then
    -- A fraction for both sides; lines for the height, the width then 80%.
    local height = sized(vim.o.lines - vim.o.cmdheight - 2, 0.8)
    local width = math.max(1, math.floor(vim.o.columns * (size and size < 1 and size or 0.8)))
    win = vim.api.nvim_open_win(bufnr, true, {
      relative = 'editor',
      row = math.floor((vim.o.lines - vim.o.cmdheight - height) / 2) - 1,
      col = math.floor((vim.o.columns - width) / 2),
      width = width,
      height = height,
      border = 'rounded',
    })
  elseif mode == 'current' then
    win = vim.api.nvim_get_current_win()
  else
    vim.cmd(mode)
    win = vim.api.nvim_get_current_win()
    if size then
      -- `vsplit`, `vnew`, `vertical …`: the width; else the height.
      if mode:match('^%s*%S*%s*v') or mode:match('vert') then
        vim.api.nvim_win_set_width(win, sized(vim.o.columns))
      else
        vim.api.nvim_win_set_height(win, sized(vim.o.lines - vim.o.cmdheight - 1))
      end
    end
  end
  -- Closing the document closes a window opened for it too.
  if mode ~= 'current' then
    vim.w[win].org_lsp_opened = true
    if win ~= from then
      vim.w[win].org_lsp_from = from
    end
  end
  if vim.api.nvim_win_get_buf(win) ~= bufnr then
    vim.api.nvim_win_set_buf(win, bufnr)
  end
  vim.api.nvim_set_current_win(win)
  return win
end

--- Whether the document at `uri` is a capture buffer.
---@param uri string
---@return boolean
local function is_capture(uri)
  local prefix = documents.capture and documents.capture.prefix
  return prefix ~= nil and vim.startswith(uri, 'file:') and vim.startswith(vim.fs.basename(vim.uri_to_fname(uri)), prefix)
end

--- Whether the document at `uri` is one the plugin opens in a window of
--- its own: the agenda or a capture buffer.
---@param uri string
---@return boolean
local function opens_apart(uri)
  if documents.agenda and vim.startswith(uri, documents.agenda.scheme .. ':') then
    return not (documents.datePrompt and uri == documents.datePrompt.uri)
  end
  return is_capture(uri)
end

--- The date prompt pops up in a window at the bottom, as Emacs's calendar
--- does, as high as its text. Returns that window.
---@param bufnr integer
---@return integer
local function prompt_window(bufnr)
  local win = vim.fn.win_findbuf(bufnr)[1]
  local from = vim.api.nvim_get_current_win()
  -- Asked from a floating window, such as the agenda in one, a split at the
  -- bottom would be under it: the calendar floats at the bottom instead.
  local bottom = function(height)
    return { relative = 'editor', row = vim.o.lines - vim.o.cmdheight - height - 1, col = 0, width = vim.o.columns, height = height }
  end
  if not win and is_float(vim.api.nvim_get_current_win()) then
    win = vim.api.nvim_open_win(bufnr, true, vim.tbl_extend('force', bottom(1), { zindex = 100 }))
    vim.w[win].org_lsp_opened = true
    vim.w[win].org_lsp_from = from
    vim.fn.bufload(bufnr)
  elseif not win then
    vim.cmd('botright split')
    win = vim.api.nvim_get_current_win()
    vim.w[win].org_lsp_opened = true
    vim.w[win].org_lsp_from = from
    -- The split takes the folding of the window it came from. The calendar
    -- has nothing to fold, and asking the server for folds of a buffer
    -- that is about to go trips an assertion in Neovim 0.11.0: turn it
    -- off before the calendar is loaded here.
    vim.wo[win].foldmethod = 'manual'
    vim.wo[win].foldenable = false
    vim.api.nvim_win_set_buf(win, bufnr)
    vim.fn.bufload(bufnr)
  end
  local height = vim.api.nvim_buf_line_count(bufnr)
  if is_float(win) then
    vim.api.nvim_win_set_config(win, bottom(height))
  else
    vim.api.nvim_win_set_height(win, height)
  end
  vim.wo[win][0].winfixheight = true
  vim.wo[win][0].number = false
  vim.wo[win][0].relativenumber = false
  vim.wo[win][0].signcolumn = 'no'
  vim.wo[win][0].wrap = false
  -- A calendar to move about in, never to type into.
  vim.schedule(function()
    if vim.api.nvim_get_current_win() == win then
      vim.cmd.stopinsert()
    end
  end)
  return win
end

--- Write the files at `uris` that have changes.
---@param uris string[]
function M.save(uris)
  for _, uri in ipairs(uris) do
    local bufnr = vim.uri_to_bufnr(uri)
    if vim.api.nvim_buf_is_loaded(bufnr) and vim.bo[bufnr].modified then
      vim.api.nvim_buf_call(bufnr, function()
        vim.cmd('silent write')
      end)
    end
  end
end

--- Close the buffer `bufnr` and the windows the plugin opened for it. A
--- window that was there before goes back to the buffer it showed.
---@param bufnr integer
function M.close_buffer(bufnr)
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    if vim.w[win].org_lsp_opened and #vim.api.nvim_list_wins() > 1 then
      -- Back to the window it was opened from, where the cursor was, not
      -- the one that takes its place on the screen.
      local from = vim.w[win].org_lsp_from
      local current = vim.api.nvim_get_current_win() == win
      vim.api.nvim_win_close(win, true)
      if current and from and from ~= win and vim.api.nvim_win_is_valid(from) then
        vim.api.nvim_set_current_win(from)
      end
    else
      vim.api.nvim_win_call(win, function()
        local previous = vim.fn.bufnr('#')
        if previous > 0 and previous ~= bufnr and vim.api.nvim_buf_is_valid(previous) then
          vim.api.nvim_win_set_buf(win, previous)
        else
          vim.cmd.enew()
        end
      end)
    end
  end
  if vim.api.nvim_buf_is_valid(bufnr) then
    vim.api.nvim_buf_delete(bufnr, { force = true })
  end
end

--- Close the document at `uri`, as `close_buffer` does.
---@param uri string
function M.close(uri)
  M.close_buffer(vim.uri_to_bufnr(uri))
end

---@param bufnr integer
---@param text string
local function set_text(bufnr, text)
  local lines = vim.split(text, '\n', { plain = true })
  if lines[#lines] == '' then
    table.remove(lines)
  end
  -- Replacing every line would move the cursor and the scroll position.
  local saved = {}
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    saved[win] = vim.api.nvim_win_call(win, vim.fn.winsaveview)
  end
  vim.bo[bufnr].modifiable = true
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, lines)
  vim.bo[bufnr].modifiable = false
  vim.bo[bufnr].modified = false
  for win, view in pairs(saved) do
    vim.api.nvim_win_call(win, function()
      vim.fn.winrestview(view)
    end)
  end
  -- Highlight the new text now, not after the usual delay. This also sends
  -- the change to the server at once, which is what the server waits for
  -- before it moves the cursor to a line of the new text.
  vim.lsp.semantic_tokens.force_refresh(bufnr)
end

--- Fill a new buffer of the server's scheme and attach the server to it,
--- so that highlighting and commands work in it as in any document.
---@param bufnr integer
---@param language string the filetype of the document
local function load(bufnr, language)
  local client = vim.lsp.get_clients({ name = 'org_lsp' })[1]
  if not client then
    return
  end
  vim.bo[bufnr].buftype = 'nofile'
  vim.bo[bufnr].swapfile = false
  local params = { uri = vim.uri_from_bufnr(bufnr) }
  local response = client:request_sync('workspace/textDocumentContent', params, 2000)
  set_text(bufnr, response and response.result and response.result.text or '')
  vim.bo[bufnr].filetype = language
  vim.lsp.buf_attach_client(bufnr, client.id)
end

M.handlers = {
  --- The server edits a file for a command. Neovim reports changes to a
  --- buffer after a delay; the server should hear of this one at once,
  --- because the next press of the same key builds on it.
  ['workspace/applyEdit'] = function(err, params, ctx)
    local result = vim.lsp.handlers['workspace/applyEdit'](err, params, ctx)
    local edit = params.edit or {}
    local uris = vim.tbl_keys(edit.changes or {})
    for _, change in ipairs(edit.documentChanges or {}) do
      table.insert(uris, change.textDocument and change.textDocument.uri)
    end
    for _, uri in ipairs(uris) do
      -- Asking for the new highlighting sends the change first.
      vim.lsp.semantic_tokens.force_refresh(vim.uri_to_bufnr(uri))
    end
    return result
  end,

  --- The server asks for a document to be shown: the agenda, or the file
  --- of an entry. Neovim's own handler fails when the cursor is on an empty
  --- line (it asks for the word under the cursor), so this one does the
  --- same without it.
  ['window/showDocument'] = function(err, params, ctx)
    local client = vim.lsp.get_client_by_id(ctx.client_id)
    if params.external or not client then
      return vim.lsp.handlers['window/showDocument'](err, params, ctx)
    end
    local bufnr = vim.uri_to_bufnr(params.uri)
    local win
    if documents.datePrompt and params.uri == documents.datePrompt.uri then
      win = prompt_window(bufnr)
    elseif opens_apart(params.uri) then
      -- The agenda and capture buffers open as `win_split_mode` says, or
      -- in the window that already shows them.
      win = vim.fn.win_findbuf(bufnr)[1]
      if not win then
        vim.bo[bufnr].buflisted = true
        win = open_window(bufnr, require('org-lsp').config.win_split_mode or 'split')
      end
    elseif params.orgOtherWindow and not is_float(vim.api.nvim_get_current_win()) then
      -- An entry's file in another window (TAB in the agenda).
      vim.cmd("normal! m'")
      win = other_window(vim.api.nvim_get_current_win(), bufnr)
      vim.bo[bufnr].buflisted = true
      vim.api.nvim_win_set_buf(win, bufnr)
    else
      -- A file, such as an entry's from the agenda. From a floating
      -- window, it opens in the window under it.
      local current = vim.api.nvim_get_current_win()
      if is_float(current) and #vim.api.nvim_list_wins() > 1 then
        vim.api.nvim_win_close(current, false)
      end
      vim.cmd("normal! m'")
      win = vim.fn.win_findbuf(bufnr)[1] or vim.api.nvim_get_current_win()
      vim.bo[bufnr].buflisted = true
      vim.api.nvim_win_set_buf(win, bufnr)
    end
    if params.takeFocus ~= false then
      vim.api.nvim_set_current_win(win)
    end
    local start = params.selection and params.selection.start
    if start then
      local text = vim.api.nvim_buf_get_lines(bufnr, start.line, start.line + 1, false)[1] or ''
      local column = vim.str_byteindex(text, client.offset_encoding, start.character, false)
      pcall(vim.api.nvim_win_set_cursor, win, { start.line + 1, column })
      -- A capture buffer is for typing in, from where `%?` was.
      if is_capture(params.uri) and vim.api.nvim_get_current_win() == win then
        vim.cmd(column >= #text and 'startinsert!' or 'startinsert')
      end
    end
    return { success = true }
  end,

  --- The server made changes to files the user is not editing (from the
  --- agenda, a capture or a refile) and `autoSave` is on: write them.
  ['org-lsp/save'] = function(_, params)
    M.save(params and params.uris or {})
  end,

  --- How far the lines of a file with `org-indent-mode` are shown indented.
  ['org-lsp/virtualIndent'] = function(_, params)
    if require('org-lsp').config.virtual_indent then
      require('org-lsp.indent').virtual(params)
    end
  end,

  --- A clock started or stopped: for the statusline.
  ['org-lsp/clock'] = function(_, params)
    require('org-lsp.clock').update(params)
  end,

  --- The server's text for a document has changed: fetch it again.
  ['workspace/textDocumentContent/refresh'] = function(_, params, ctx)
    local client = vim.lsp.get_client_by_id(ctx.client_id)
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      if client and vim.api.nvim_buf_is_loaded(bufnr) and vim.uri_from_bufnr(bufnr) == params.uri then
        client:request('workspace/textDocumentContent', { uri = params.uri }, function(_, result)
          if result and vim.api.nvim_buf_is_loaded(bufnr) then
            set_text(bufnr, result.text)
            if documents.datePrompt and params.uri == documents.datePrompt.uri then
              prompt_window(bufnr)
            end
          end
        end)
      end
    end
    return vim.NIL
  end,
}

--- The width last told to the server.
local told_width

--- Tell the server how many columns of text the window that shows the
--- agenda has, for `agenda.tagsColumn` `"auto"`: the tags then end at its
--- right edge. When the window changes size, the server builds the agenda
--- again.
---@param language string the agenda's filetype
local function tell_width(language)
  local client = vim.lsp.get_clients({ name = 'org_lsp' })[1]
  if not client then
    return
  end
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == language then
      local info = vim.fn.getwininfo(win)[1]
      local width = info.width - info.textoff
      if width ~= told_width then
        told_width = width
        client:notify('org-lsp/agendaWidth', { width = width })
      end
      return
    end
  end
end

--- Fetch buffers of the agenda's scheme from the server: the agenda, and
--- the date prompt.
---@param manifest table the server's manifest
function M.setup(manifest)
  documents = { agenda = manifest.agenda, datePrompt = manifest.datePrompt, capture = manifest.capture }
  vim.api.nvim_create_autocmd('BufReadCmd', {
    group = vim.api.nvim_create_augroup('org_lsp_view', { clear = true }),
    -- With a slash, the pattern is matched against the whole name.
    pattern = manifest.agenda.scheme .. ':/*',
    callback = function(args)
      local prompt = documents.datePrompt
      if prompt and vim.uri_from_bufnr(args.buf) == prompt.uri then
        -- Gone with its window: closing it is cancelling the prompt.
        vim.bo[args.buf].bufhidden = 'wipe'
        load(args.buf, prompt.languageId)
      else
        load(args.buf, manifest.agenda.languageId)
      end
    end,
  })
  -- Windows that open, close or change size, once their sizes are set.
  vim.api.nvim_create_autocmd({ 'BufWinEnter', 'WinNew', 'WinClosed', 'WinResized', 'VimResized' }, {
    group = 'org_lsp_view',
    callback = function()
      vim.schedule(function()
        tell_width(manifest.agenda.languageId)
      end)
    end,
  })
end

return M
