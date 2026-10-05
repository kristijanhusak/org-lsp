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
  experimental = { orgDatePrompt = true },
}

--- The manifest's sections for the documents the server owns.
local documents = {}

--- The date prompt pops up in a window at the bottom, as Emacs's calendar
--- does, as high as its text. Returns that window.
---@param bufnr integer
---@return integer
local function prompt_window(bufnr)
  local win = vim.fn.win_findbuf(bufnr)[1]
  if not win then
    vim.cmd('botright split')
    win = vim.api.nvim_get_current_win()
    -- The split takes the folding of the window it came from. The calendar
    -- has nothing to fold, and asking the server for folds of a buffer
    -- that is about to go trips an assertion in Neovim 0.11.0: turn it
    -- off before the calendar is loaded here.
    vim.wo[win].foldmethod = 'manual'
    vim.wo[win].foldenable = false
    vim.api.nvim_win_set_buf(win, bufnr)
    vim.fn.bufload(bufnr)
  end
  vim.api.nvim_win_set_height(win, vim.api.nvim_buf_line_count(bufnr))
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

--- Close the windows that show the document at `uri`, and the buffer.
---@param uri string
function M.close(uri)
  local bufnr = vim.uri_to_bufnr(uri)
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    if #vim.api.nvim_list_wins() > 1 then
      vim.api.nvim_win_close(win, true)
    end
  end
  if vim.api.nvim_buf_is_valid(bufnr) then
    vim.api.nvim_buf_delete(bufnr, { force = true })
  end
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
    else
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
    end
    return { success = true }
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

--- Fetch buffers of the agenda's scheme from the server: the agenda, and
--- the date prompt.
---@param manifest table the server's manifest
function M.setup(manifest)
  documents = { agenda = manifest.agenda, datePrompt = manifest.datePrompt }
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
end

return M
