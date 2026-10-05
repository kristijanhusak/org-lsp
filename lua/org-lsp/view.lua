--- Showing documents on the server's request, among them the ones the
--- server owns and the editor only displays: the agenda.
---
--- For those this is the client side of LSP 3.18's
--- `workspace/textDocumentContent`, which Neovim does not have built in. A
--- buffer named with the server's scheme is filled with the text the server
--- returns, and filled again when the server says that the text has changed.
local M = {}

--- Tells the server that this client fetches such documents.
M.capabilities = { workspace = { textDocumentContent = { dynamicRegistration = false } } }

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
    vim.cmd("normal! m'")
    local win = vim.fn.win_findbuf(bufnr)[1] or vim.api.nvim_get_current_win()
    vim.bo[bufnr].buflisted = true
    vim.api.nvim_win_set_buf(win, bufnr)
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
          end
        end)
      end
    end
    return vim.NIL
  end,
}

--- Fetch buffers of the agenda's scheme from the server.
---@param agenda table the `agenda` section of the server's manifest
function M.setup(agenda)
  vim.api.nvim_create_autocmd('BufReadCmd', {
    group = vim.api.nvim_create_augroup('org_lsp_view', { clear = true }),
    -- With a slash, the pattern is matched against the whole name.
    pattern = agenda.scheme .. ':/*',
    callback = function(args)
      load(args.buf, agenda.languageId)
    end,
  })
end

return M
