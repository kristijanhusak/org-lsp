--- What the server asks the editor to do: show documents, among them the
--- ones the server owns and the editor only displays (the agenda and the
--- date prompt), and apply edits.
---
--- For those this is the client side of LSP 3.18's
--- `workspace/textDocumentContent`, which Neovim does not have built in. A
--- buffer named with the server's scheme is filled with the text the server
--- returns, and filled again when the server says that the text has changed.
local M = {}
local notify = require('org-lsp.notify')

--- Tells the server that this client fetches such documents, and shows
--- its date prompt; and that it is org-lsp's own (its token types, what a
--- command's result asks for).
M.capabilities = {
  workspace = { textDocumentContent = { dynamicRegistration = false } },
  experimental = {
    orgClient = true,
    orgDatePrompt = true,
    orgVirtualIndent = true,
    orgLineHighlights = true,
    orgDisplay = true,
    orgHiddenLines = true,
    orgRunCommand = true,
  },
}

--- The manifest's sections for the documents the server owns, and the
--- name capture buffers start with.
local documents = {}

---@param win integer
---@return boolean
local function is_float(win)
  return vim.api.nvim_win_get_config(win).relative ~= ''
end

--- The lines the windows the plugin opens came from start with.
local view_namespace = vim.api.nvim_create_namespace('org_lsp_view')

--- Remember the view of the window `from` (`winsaveview()`), for when the
--- window about to open from it closes. A split takes lines from it, and
--- with 'splitkeep' "cursor" or 'scrolloff' Neovim scrolls it then and
--- moves its cursor, and again when the split closes; lines a command adds
--- above the cursor in between leave both off. Emacs keeps the line a
--- window starts with and the cursor on its text. The line the window
--- starts with and, with `cursor`, the cursor are marks, which move with
--- the edits; the rest of the view is kept as it was.
---@param from integer
---@param cursor? boolean
---@return table
local function keep_view(from, cursor)
  local bufnr = vim.api.nvim_win_get_buf(from)
  local view = vim.api.nvim_win_call(from, vim.fn.winsaveview)
  local kept = { bufnr = bufnr, view = view }
  -- Lines put in where a mark is go before it: Emacs puts a planning line
  -- in at the end of the headline's line, before the next line's start,
  -- where the edit's difference puts it.
  kept.top = vim.api.nvim_buf_set_extmark(bufnr, view_namespace, view.topline - 1, 0, {})
  if cursor then
    kept.cursor = vim.api.nvim_buf_set_extmark(bufnr, view_namespace, view.lnum - 1, view.col, {})
  end
  return kept
end

--- The window `from` shows what `keep_view` remembered again
--- (`winrestview()`), the line it starts with and the cursor where the
--- edits since took them.
---@param from integer
---@param kept? table
local function restore_view(from, kept)
  if not kept or not vim.api.nvim_buf_is_valid(kept.bufnr) then
    return
  end
  local function take(mark)
    if not mark then
      return nil
    end
    local position = vim.api.nvim_buf_get_extmark_by_id(kept.bufnr, view_namespace, mark, {})
    vim.api.nvim_buf_del_extmark(kept.bufnr, view_namespace, mark)
    return position[1] and position or nil
  end
  local top, cursor = take(kept.top), take(kept.cursor)
  if not top or not vim.api.nvim_win_is_valid(from) or vim.api.nvim_win_get_buf(from) ~= kept.bufnr then
    return
  end
  local view = { topline = top[1] + 1 }
  if cursor then
    view = vim.tbl_extend('force', kept.view, view, { lnum = cursor[1] + 1, col = cursor[2] })
    if cursor[2] ~= kept.view.col then
      view.curswant = cursor[2]
    end
  end
  vim.api.nvim_win_call(from, function()
    vim.fn.winrestview(view)
  end)
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
--- with TAB (`org-agenda-goto`) and in follow mode: one that shows the
--- file, else an empty one, else the previous or another window; a new
--- one only when the agenda's is the only window.
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
  -- Another window there is: the previous one, else the first, as
  -- Emacs's `display-buffer` uses some window rather than split again (or
  -- follow mode would split once for each entry).
  local previous = vim.fn.win_getid(vim.fn.winnr('#'))
  if vim.tbl_contains(wins, previous) then
    return previous
  end
  if wins[1] then
    return wins[1]
  end
  -- The agenda's window is the only one: split it, side by side when it
  -- is wide, as Emacs's `split-window-sensibly`.
  local base = from
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
---@param cursor? boolean whether the cursor of the window it opens from is
--- put back where its text went when it closes (a capture or note buffer)
---@return integer
local function open_window(bufnr, mode, cursor)
  local win
  local from = vim.api.nvim_get_current_win()
  local kept = keep_view(from, cursor)
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
  end
  if mode ~= 'current' and win ~= from then
    vim.w[win].org_lsp_from = from
    vim.w[win].org_lsp_from_view = kept
  else
    restore_view(from, kept)
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

--- Whether the document at `uri` is a file of the org-roam buffer.
---@param uri string
---@return boolean
local function is_roam_buffer(uri)
  local prefix = documents.roamBuffer and documents.roamBuffer.prefix
  return prefix ~= nil and vim.startswith(uri, 'file:') and vim.startswith(vim.fs.basename(vim.uri_to_fname(uri)), prefix)
end

--- Whether a window shows the persistent org-roam buffer (not a
--- dedicated one).
---@return boolean
function M.roam_buffer_shown()
  local prefix = documents.roamBuffer and documents.roamBuffer.prefix
  if not prefix then
    return false
  end
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.fs.basename(vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(win))) == prefix .. '.org' then
      return true
    end
  end
  return false
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

--- How many lines the text of `bufnr` has, and how wide its widest is.
---@param bufnr integer
---@return integer, integer
local function text_size(bufnr)
  local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
  local width = 1
  for _, line in ipairs(lines) do
    width = math.max(width, vim.fn.strdisplaywidth(line))
  end
  return #lines, width
end

--- Where the floating window of the date prompt goes, as high and as wide
--- as `height` and `width`: at the bottom, in the middle, or below the
--- cursor of the window it was asked from (above it when there is no room
--- below), `anchor` saying where that cursor was on the screen.
---@param place 'bottom'|'float'|'cursor'
---@param height integer
---@param width integer
---@param anchor? { row: integer, col: integer }
---@return vim.api.keyset.win_config
local function prompt_float(place, height, width, anchor)
  local border = require('org-lsp').config.calendar_border
  if border == nil then
    border = 'rounded'
  end
  local framed = border ~= 'none' and border ~= '' and border ~= false
  local edge = framed and 2 or 0
  local lines = vim.o.lines - vim.o.cmdheight
  -- Room after the widest line, which would touch the border otherwise.
  width = math.min(width + 2, vim.o.columns - edge)
  height = math.min(height, lines - edge - 1)
  if place == 'bottom' then
    -- Along the whole bottom, as the split it stands for.
    return { relative = 'editor', row = lines - height - 1, col = 0, width = vim.o.columns, height = height, border = 'none' }
  end
  local config = { relative = 'editor', width = width, height = height, border = framed and border or 'none' }
  if place == 'cursor' and anchor then
    local below = anchor.row
    if below + height + edge > lines - 1 then
      -- No room below: above the cursor's line.
      below = math.max(0, anchor.row - 1 - height - edge)
    end
    config.row = below
    config.col = math.max(0, math.min(anchor.col, vim.o.columns - width - edge))
  else
    config.row = math.max(0, math.floor((lines - height - edge) / 2) - 1)
    config.col = math.floor((vim.o.columns - width - edge) / 2)
  end
  return config
end

--- The date prompt pops up in a window as `calendar_win_mode` says: by
--- default at the bottom, as Emacs's calendar does, as high as its text.
--- Returns that window.
---@param bufnr integer
---@return integer
local function prompt_window(bufnr)
  local win = vim.fn.win_findbuf(bufnr)[1]
  local from = vim.api.nvim_get_current_win()
  local mode = require('org-lsp').config.calendar_win_mode or 'bottom'
  if not win then
    local kept = keep_view(from, true)
    -- Asked from a floating window, such as the agenda in one, a split at
    -- the bottom would be under it: the calendar floats at the bottom
    -- instead.
    local place = mode
    if place == 'bottom' and is_float(from) then
      place = 'bottom float'
    end
    if type(mode) == 'function' then
      win = mode(bufnr)
      if type(win) ~= 'number' or not vim.api.nvim_win_is_valid(win) then
        win = vim.api.nvim_get_current_win()
      end
    elseif place == 'bottom float' or place == 'float' or place == 'cursor' then
      local anchor
      if place == 'cursor' then
        local cursor = vim.api.nvim_win_get_cursor(from)
        local screen = vim.fn.screenpos(from, cursor[1], cursor[2] + 1)
        -- The row below the cursor's, counted from 0.
        anchor = { row = screen.row, col = math.max(0, screen.col - 1) }
      end
      local config = prompt_float(place == 'bottom float' and 'bottom' or place, 1, 1, anchor)
      win = vim.api.nvim_open_win(bufnr, true, vim.tbl_extend('force', config, { zindex = 100, style = 'minimal' }))
      vim.w[win].org_lsp_prompt_place = place == 'bottom float' and 'bottom' or place
      vim.w[win].org_lsp_prompt_anchor = anchor
    else
      vim.cmd(mode == 'bottom' and 'botright split' or mode)
      win = vim.api.nvim_get_current_win()
    end
    if win ~= from then
      vim.w[win].org_lsp_opened = true
      vim.w[win].org_lsp_from = from
      vim.w[win].org_lsp_from_view = kept
    else
      restore_view(from, kept)
    end
    -- The window takes the folding of the window it came from. The
    -- calendar has nothing to fold, and the server is not asked for folds
    -- of a buffer that is about to go: turn it off before the calendar is
    -- loaded here.
    vim.wo[win].foldmethod = 'manual'
    vim.wo[win].foldenable = false
    if vim.api.nvim_win_get_buf(win) ~= bufnr then
      vim.api.nvim_win_set_buf(win, bufnr)
    end
    vim.fn.bufload(bufnr)
  end
  local height, width = text_size(bufnr)
  local place = vim.w[win].org_lsp_prompt_place
  if is_float(win) and place then
    vim.api.nvim_win_set_config(win, prompt_float(place, height, width, vim.w[win].org_lsp_prompt_anchor))
  elseif not is_float(win) and type(mode) == 'string' and not mode:match('v') then
    -- A split as high as the calendar, but one beside the window
    -- (`vsplit`, `vertical …`).
    vim.api.nvim_win_set_height(win, height)
  end
  vim.wo[win][0].winfixheight = true
  vim.wo[win][0].number = false
  vim.wo[win][0].relativenumber = false
  vim.wo[win][0].signcolumn = 'no'
  vim.wo[win][0].wrap = false
  -- The calendar shows whole: 'scrolloff' would scroll its months' names
  -- away to keep the selected day off the window's edge, and the text of
  -- another month may be longer than the window was.
  vim.wo[win][0].scrolloff = 0
  vim.wo[win][0].sidescrolloff = 0
  if height <= vim.api.nvim_win_get_height(win) then
    vim.api.nvim_win_call(win, function()
      vim.fn.winrestview({ topline = 1, leftcol = 0 })
    end)
  end
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
    local bufnr = require('org-lsp.buffers').of(uri)
    if bufnr and vim.bo[bufnr].modified then
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
      local kept = vim.w[win].org_lsp_from_view
      local current = vim.api.nvim_get_current_win() == win
      vim.api.nvim_win_close(win, true)
      if from and from ~= win then
        restore_view(from, kept)
      end
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
  local bufnr = require('org-lsp.buffers').of(uri)
  if bufnr then
    M.close_buffer(bufnr)
  end
end

--- Tell the server of the changes to `bufnr` now, not after Neovim's
--- usual delay. Neovim's change tracking does that without touching the
--- highlighting; asking for new highlighting does it too, but drops the
--- old highlighting until the new arrives, which flickers.
---@param client vim.lsp.Client?
---@param bufnr integer
local function send_changes(client, bufnr)
  local ok, changetracking = pcall(require, 'vim.lsp._changetracking')
  if client and ok and type(changetracking) == 'table' and type(changetracking.flush) == 'function' then
    changetracking.flush(client, bufnr)
  else
    vim.lsp.semantic_tokens.force_refresh(bufnr)
  end
end

--- Change line `row` (from 0) of `bufnr` from `old` to `new`, only the
--- part between what they start and end with alike.
---@param bufnr integer
---@param row integer
---@param old string
---@param new string
local function change_line(bufnr, row, old, new)
  local function continues(text, at)
    local byte = text:byte(at)
    return byte ~= nil and byte >= 0x80 and byte < 0xC0
  end
  local start = 0
  while start < #old and start < #new and old:byte(start + 1) == new:byte(start + 1) do
    start = start + 1
  end
  -- Not within a character.
  while start > 0 and continues(old, start + 1) do
    start = start - 1
  end
  local tail = 0
  while tail < #old - start and tail < #new - start and old:byte(#old - tail) == new:byte(#new - tail) do
    tail = tail + 1
  end
  while tail > 0 and continues(old, #old - tail + 1) do
    tail = tail - 1
  end
  vim.api.nvim_buf_set_text(bufnr, row, start, row, #old - tail, { new:sub(start + 1, #new - tail) })
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
  -- Only what changed: the other lines keep their highlighting until the
  -- new arrives, and marks on them, such as where a window opened from
  -- here puts the cursor back, stay where they are. A line changed where
  -- it is changes from where it differs, as Emacs changes an agenda line
  -- (a new date at its end), and the marks before that stay on it.
  local old = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
  vim.bo[bufnr].modifiable = true
  if #old == 1 and old[1] == '' then
    vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, lines)
  else
    local hunks = (vim.text and vim.text.diff or vim.diff)(
      table.concat(old, '\n') .. '\n',
      table.concat(lines, '\n') .. '\n',
      { result_type = 'indices' }
    )
    -- From the last, so that the line numbers of the others still hold.
    for i = #hunks, 1, -1 do
      local at, count, from, new_count = unpack(hunks[i])
      if count == new_count then
        for k = 0, count - 1 do
          change_line(bufnr, at + k - 1, old[at + k], lines[from + k])
        end
      else
        -- With no old lines, the new ones go after line `at`.
        local start = count == 0 and at or at - 1
        vim.api.nvim_buf_set_lines(bufnr, start, start + count, false, vim.list_slice(lines, from, from + new_count - 1))
      end
    end
  end
  vim.bo[bufnr].modifiable = false
  vim.bo[bufnr].modified = false
  for win, view in pairs(saved) do
    vim.api.nvim_win_call(win, function()
      vim.fn.winrestview(view)
    end)
  end
  -- Send the change to the server at once, which is what the server waits
  -- for before it moves the cursor to a line of the new text; the new
  -- highlighting follows.
  send_changes(vim.lsp.get_clients({ bufnr = bufnr, name = 'org_lsp' })[1], bufnr)
end

local prompt_highlights = vim.api.nvim_create_namespace('org_lsp_prompt_highlights')

---@param bufnr integer
---@return boolean
local function is_prompt(bufnr)
  return documents.datePrompt ~= nil and vim.uri_from_bufnr(bufnr) == documents.datePrompt.uri
end

--- Highlight the date prompt with the server's tokens `data`, as Neovim
--- highlights semantic tokens.
---@param bufnr integer
---@param client vim.lsp.Client
---@param data integer[]
local function paint(bufnr, client, data)
  vim.api.nvim_buf_clear_namespace(bufnr, prompt_highlights, 0, -1)
  local provider = client.server_capabilities.semanticTokensProvider
  if not provider then
    return
  end
  local types, modifiers = provider.legend.tokenTypes, provider.legend.tokenModifiers
  local filetype = vim.bo[bufnr].filetype
  local priority = (vim.hl or vim.highlight).priorities.semantic_tokens
  local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
  local line, start = 0, 0
  local function mark(from, to, group, level)
    pcall(vim.api.nvim_buf_set_extmark, bufnr, prompt_highlights, line, from, {
      end_col = to,
      hl_group = group,
      priority = level,
      strict = false,
    })
  end
  for i = 1, #data, 5 do
    line = line + data[i]
    start = data[i] == 0 and start + data[i + 1] or data[i + 1]
    local type = types[data[i + 3] + 1]
    if type then
      local text = lines[line + 1] or ''
      local from = vim.str_byteindex(text, client.offset_encoding, start, false)
      local to = vim.str_byteindex(text, client.offset_encoding, start + data[i + 2], false)
      mark(from, to, ('@lsp.type.%s.%s'):format(type, filetype), priority)
      local bits, index = data[i + 4], 1
      while bits > 0 do
        if bits % 2 == 1 and modifiers[index] then
          mark(from, to, ('@lsp.mod.%s.%s'):format(modifiers[index], filetype), priority + 1)
          mark(from, to, ('@lsp.typemod.%s.%s.%s'):format(type, modifiers[index], filetype), priority + 2)
        end
        bits, index = math.floor(bits / 2), index + 1
      end
    end
  end
end

--- Show the date prompt's new text, if any, and its highlighting at the
--- same time. Neovim's own highlighting is off in it: that drops the
--- highlighting of a line the moment the line changes and draws it again
--- only once the new tokens arrive, so the calendar flickers at each key
--- and may keep the old field marked.
---@param bufnr integer
---@param client vim.lsp.Client
---@param text string?
local function update_prompt(bufnr, client, text)
  local params = { textDocument = { uri = documents.datePrompt.uri } }
  client:request('textDocument/semanticTokens/full', params, function(_, result)
    if not vim.api.nvim_buf_is_loaded(bufnr) then
      return
    end
    if text then
      set_text(bufnr, text)
      prompt_window(bufnr)
    end
    paint(bufnr, client, result and result.data or {})
  end, bufnr)
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
  local prompt = is_prompt(bufnr)
  if prompt then
    pcall(vim.lsp.semantic_tokens.enable, false, { bufnr = bufnr })
  end
  vim.lsp.buf_attach_client(bufnr, client.id)
  if prompt then
    update_prompt(bufnr, client)
  end
end

local line_highlights = vim.api.nvim_create_namespace('org_lsp_line_highlights')

--- Text put in after the last line of a file (an archived subtree, a
--- refiled entry at the end) goes in at the end of the last line instead,
--- its last newline moved before it: the same text. Neovim 0.12.0 adds an
--- empty line after text put in past the last line.
---@param uri string?
---@param edits table[]?
---@param client_id integer
local function at_last_line(uri, edits, client_id)
  local client = vim.lsp.get_client_by_id(client_id)
  if not uri or not edits or not client then
    return
  end
  local bufnr = vim.uri_to_bufnr(uri)
  vim.fn.bufload(bufnr)
  local count = vim.api.nvim_buf_line_count(bufnr)
  local last = vim.api.nvim_buf_get_lines(bufnr, count - 1, count, false)[1] or ''
  for _, text_edit in ipairs(edits) do
    local range = text_edit.range
    if
      range.start.line == count
      and range.start.character == 0
      and range['end'].line == count
      and range['end'].character == 0
      and vim.endswith(text_edit.newText, '\n')
    then
      local character = vim.str_utfindex(last, client.offset_encoding)
      text_edit.range = {
        start = { line = count - 1, character = character },
        ['end'] = { line = count - 1, character = character },
      }
      text_edit.newText = '\n' .. text_edit.newText:sub(1, -2)
    end
  end
end

M.handlers = {
  --- A message of the server, titled `Org`. Neovim's own handler sends it
  --- without a title, as `LSP[org-lsp] …`.
  ['window/showMessage'] = function(_, params)
    local levels = vim.log.levels
    local level = ({ levels.ERROR, levels.WARN, levels.INFO, levels.DEBUG, levels.DEBUG })[params.type]
    notify(params.message, level or levels.INFO)
    return vim.NIL
  end,

  --- The server edits a file for a command. Neovim reports changes to a
  --- buffer after a delay; the server should hear of this one at once,
  --- because the next press of the same key builds on it.
  ['workspace/applyEdit'] = function(err, params, ctx)
    local edit = params.edit or {}
    local uris = vim.tbl_keys(edit.changes or {})
    for _, change in ipairs(edit.documentChanges or {}) do
      table.insert(uris, change.textDocument and change.textDocument.uri)
      at_last_line(change.textDocument and change.textDocument.uri, change.edits, ctx.client_id)
    end
    for uri, edits in pairs(edit.changes or {}) do
      at_last_line(uri, edits, ctx.client_id)
    end
    local result = vim.lsp.handlers['workspace/applyEdit'](err, params, ctx)
    local client = vim.lsp.get_client_by_id(ctx.client_id)
    for _, uri in ipairs(uris) do
      local bufnr = require('org-lsp.buffers').of(uri)
      if bufnr then
        send_changes(client, bufnr)
      end
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
    local from = vim.api.nvim_get_current_win()
    if documents.datePrompt and params.uri == documents.datePrompt.uri then
      win = prompt_window(bufnr)
    elseif is_roam_buffer(params.uri) then
      -- The org-roam buffer beside the file, as Emacs shows it in a window
      -- of its own; gone with its window, which tells the server.
      win = vim.fn.win_findbuf(bufnr)[1]
      if not win then
        vim.bo[bufnr].bufhidden = 'wipe'
        win = open_window(bufnr, 'botright vsplit')
      end
      if params.takeFocus == false and vim.api.nvim_win_is_valid(from) then
        vim.api.nvim_set_current_win(from)
      end
    elseif opens_apart(params.uri) then
      -- The agenda and capture buffers open as `win_split_mode` says, or
      -- in the window that already shows them.
      win = vim.fn.win_findbuf(bufnr)[1]
      if not win then
        vim.bo[bufnr].buflisted = true
        win = open_window(bufnr, require('org-lsp').config.win_split_mode or 'split', is_capture(params.uri))
      end
    elseif params.orgOtherWindow and not is_float(vim.api.nvim_get_current_win()) then
      -- An entry's file in another window (TAB in the agenda, and follow
      -- mode, which keeps the cursor in the agenda).
      if params.takeFocus ~= false then
        vim.cmd("normal! m'")
      end
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
      if params.orgOtherWindow then
        -- The entry unfolded, as `org-agenda-show` shows its context.
        vim.api.nvim_win_call(win, function()
          vim.cmd('normal! zv')
        end)
      end
      -- A capture buffer is for typing in, from where `%?` was.
      if is_capture(params.uri) and vim.api.nvim_get_current_win() == win then
        vim.cmd(column >= #text and 'startinsert!' or 'startinsert')
      end
    end
    return { success = true }
  end,

  --- The server made changes to files the user is not editing (from the
  --- agenda, a capture or a refile) and `auto_save` is on: write them.
  ['org-lsp/save'] = function(_, params)
    M.save(params and params.uris or {})
  end,

  --- A command to run as from its key, where the cursor is, with the
  --- argument's fields the server gives: a capture an org-protocol URL
  --- started.
  ['org-lsp/runCommand'] = function(_, params)
    if params and type(params.command) == 'string' then
      vim.schedule(function()
        require('org-lsp').run(params.command, { argument = params.argument })
      end)
    end
  end,

  --- The headlines of a document org-journal showed, folded as it folds
  --- them, or the subtrees a yank or paste folded.
  ['org-lsp/folds'] = function(_, params)
    require('org-lsp').journal_folds(params)
  end,

  --- How far the lines of a file with `org-indent-mode` are shown indented.
  ['org-lsp/virtualIndent'] = function(_, params)
    if require('org-lsp').config.virtual_indent then
      require('org-lsp.indent').virtual(params)
    end
  end,

  --- What stands in for hidden text: entities, sub- and superscripts,
  --- shrunk table fields.
  ['org-lsp/display'] = function(_, params, ctx)
    local client = vim.lsp.get_client_by_id(ctx.client_id)
    if client then
      require('org-lsp.display').show(client, params)
    end
  end,

  --- Images in place of LaTeX fragments, made by Emacs, and of links to
  --- images downloaded first; quiet for those a file opens with.
  ['org-lsp/images'] = function(_, params)
    require('org-lsp.images').apply(params, params.startup == true)
  end,

  --- Lines highlighted from edge to edge, as Emacs's overlays and faces
  --- that extend cover them: the agenda's entry the clock runs on, and
  --- headlines with `org_fontify_whole_heading_line`.
  ['org-lsp/lineHighlights'] = function(_, params)
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      if vim.api.nvim_buf_is_loaded(bufnr) and vim.uri_from_bufnr(bufnr) == params.uri then
        vim.api.nvim_buf_clear_namespace(bufnr, line_highlights, 0, -1)
        local count = vim.api.nvim_buf_line_count(bufnr)
        for _, line in ipairs(params.lines or {}) do
          if line.line < count then
            -- The group of the token, or token and modifier, it names.
            local group = require('org-lsp.highlights').group(line.highlight, vim.bo[bufnr].filetype)
            vim.api.nvim_buf_set_extmark(bufnr, line_highlights, line.line, 0, { line_hl_group = group })
          end
        end
      end
    end
  end,

  --- What column view shows (`org-columns`), or that it is off.
  ['org-lsp/columns'] = function(_, params)
    require('org-lsp.columns').show(params)
  end,

  --- A clock started or stopped: for the statusline.
  ['org-lsp/clock'] = function(_, params)
    require('org-lsp.clock').update(params)
  end,

  --- The relative timer or the countdown started, paused or stopped: for
  --- the statusline.
  ['org-lsp/timer'] = function(_, params)
    require('org-lsp.clock').update_timer(params)
  end,

  --- The server's text for a document has changed: fetch it again.
  ['workspace/textDocumentContent/refresh'] = function(_, params, ctx)
    local client = vim.lsp.get_client_by_id(ctx.client_id)
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      if client and vim.api.nvim_buf_is_loaded(bufnr) and vim.uri_from_bufnr(bufnr) == params.uri then
        client:request('workspace/textDocumentContent', { uri = params.uri }, function(_, result)
          if not (result and vim.api.nvim_buf_is_loaded(bufnr)) then
            return
          end
          if is_prompt(bufnr) then
            update_prompt(bufnr, client, result.text)
          else
            set_text(bufnr, result.text)
          end
        end)
      end
    end
    return vim.NIL
  end,

  --- The server's highlighting has changed: Neovim fetches it again for
  --- the documents it highlights, and the date prompt is highlighted here.
  ['workspace/semanticTokens/refresh'] = function(err, result, ctx)
    local client = vim.lsp.get_client_by_id(ctx.client_id)
    for bufnr in pairs(client and client.attached_buffers or {}) do
      if is_prompt(bufnr) and vim.api.nvim_buf_is_loaded(bufnr) then
        update_prompt(bufnr, client)
      end
    end
    return vim.lsp.handlers['workspace/semanticTokens/refresh'](err, result, ctx)
  end,
}

--- The width last told to the server.
local told_width

--- Tell the server how many columns of text the window that shows the
--- agenda has, for `org_agenda_tags_column` `"auto"`: the tags then end at its
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
  documents = {
    agenda = manifest.agenda,
    datePrompt = manifest.datePrompt,
    capture = manifest.capture,
    roamBuffer = manifest.roamBuffer,
  }
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
  -- The line of the agenda the cursor is on, each time it changes, for
  -- follow mode (`F`), which the server keeps, and the outline path of
  -- its entry.
  vim.api.nvim_create_autocmd('CursorMoved', {
    group = 'org_lsp_view',
    pattern = manifest.agenda.scheme .. ':/*',
    callback = function(args)
      if vim.bo[args.buf].filetype ~= manifest.agenda.languageId then
        return
      end
      local line = vim.api.nvim_win_get_cursor(0)[1]
      if vim.b[args.buf].org_lsp_cursor_line == line then
        return
      end
      vim.b[args.buf].org_lsp_cursor_line = line
      local client = vim.lsp.get_clients({ name = 'org_lsp' })[1]
      if client then
        local params = vim.lsp.util.make_position_params(0, client.offset_encoding)
        params.width = vim.o.columns
        -- `org-agenda-show-outline-path`: the entry's outline path, in the
        -- echo area and not in the message history, as Emacs shows it.
        -- The answer comes later: echoed only if the user is still in the
        -- agenda, and not typing.
        client:request('org-lsp/agendaCursor', params, function(_, result)
          if vim.api.nvim_get_current_buf() ~= args.buf or vim.api.nvim_get_mode().mode ~= 'n' then
            return
          end
          if type(result) == 'table' and type(result.echo) == 'string' then
            vim.api.nvim_echo({ { result.echo } }, false, {})
          end
        end, args.buf)
      end
    end,
  })
  -- The cursor on another line of an Org file while the org-roam buffer
  -- shows: the buffer follows the node it is in.
  vim.api.nvim_create_autocmd('CursorMoved', {
    group = 'org_lsp_view',
    callback = function(args)
      if vim.bo[args.buf].filetype ~= 'org' or is_roam_buffer(vim.uri_from_bufnr(args.buf)) then
        return
      end
      local line = vim.api.nvim_win_get_cursor(0)[1]
      if vim.b[args.buf].org_lsp_roam_line == line or not M.roam_buffer_shown() then
        return
      end
      vim.b[args.buf].org_lsp_roam_line = line
      local client = vim.lsp.get_clients({ bufnr = args.buf, name = 'org_lsp' })[1]
      if client then
        client:notify('org-lsp/cursor', vim.lsp.util.make_position_params(0, client.offset_encoding))
      end
    end,
  })
  -- Windows that open, close or change size, once their sizes are set;
  -- and their columns before the text (line numbers, signs, folds), which
  -- a configuration may turn on once the window is there.
  local function retell()
    vim.schedule(function()
      tell_width(manifest.agenda.languageId)
    end)
  end
  vim.api.nvim_create_autocmd({ 'BufWinEnter', 'BufEnter', 'WinEnter', 'WinNew', 'WinClosed', 'WinResized', 'VimResized' }, {
    group = 'org_lsp_view',
    callback = retell,
  })
  vim.api.nvim_create_autocmd('OptionSet', {
    group = 'org_lsp_view',
    pattern = { 'number', 'relativenumber', 'numberwidth', 'signcolumn', 'foldcolumn', 'statuscolumn' },
    callback = retell,
  })
end

return M
