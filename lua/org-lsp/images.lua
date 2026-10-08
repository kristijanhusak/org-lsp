--- Images in place of text: the previews of links to image files
--- (`org.link.preview`, `C-c C-x C-v`) and of LaTeX fragments
--- (`org.latex.preview`, `C-c C-x C-l`), as Org overlays them in Emacs.
---
--- Neovim draws them with `vim.ui.img` (0.13 and later, in a terminal with
--- the Kitty graphics protocol). An image goes below the line of its link
--- or fragment, in virtual lines kept free for it, and follows the line as
--- the window scrolls. The server says which images go where; this module
--- keeps them, as Emacs keeps its overlays, and puts them on the screen.
local M = {}

local namespace = vim.api.nvim_create_namespace('org_lsp_images')

--- The previews of each buffer, by extmark: `{ kind, path, width, height,
--- ids = { [win] = image id } }`.
---@type table<integer, table<integer, table>>
local shown = {}

--- What each image file holds, and its size in pixels.
---@type table<string, { data: string, width: integer, height: integer }|false>
local files = {}

--- The size of a cell of the terminal in pixels, for the size of images:
--- most terminal fonts are about twice as high as wide.
local cell = { width = 10, height = 20 }

function M.supported()
  return vim.ui ~= nil and vim.ui.img ~= nil and require('org-lsp').config.images ~= false
end

local function config()
  local images = require('org-lsp').config.images
  return type(images) == 'table' and images or {}
end

--- A PNG's width and height, from its header.
---@param data string
---@return integer?, integer?
local function png_size(data)
  if data:sub(1, 8) ~= '\137PNG\r\n\26\n' or #data < 24 then
    return nil
  end
  local function number(at)
    local a, b, c, d = data:byte(at, at + 3)
    return ((a * 256 + b) * 256 + c) * 256 + d
  end
  return number(17), number(21)
end

--- The image of `path` as PNG: as it is, or made a PNG with ImageMagick,
--- which Neovim can show.
---@param path string
---@return { data: string, width: integer, height: integer }?
local function load(path)
  local stat = vim.uv.fs_stat(path)
  local key = path .. ':' .. (stat and stat.mtime.sec or 0)
  if files[key] ~= nil then
    return files[key] or nil
  end
  local png = path
  if not path:lower():match('%.png$') then
    local converter = vim.fn.executable('magick') == 1 and 'magick' or vim.fn.executable('convert') == 1 and 'convert'
    if not converter then
      files[key] = false
      return nil
    end
    local directory = vim.fs.joinpath(vim.fn.stdpath('cache'), 'org-lsp', 'images')
    vim.fn.mkdir(directory, 'p')
    png = vim.fs.joinpath(directory, vim.fn.sha256(key) .. '.png')
    if not vim.uv.fs_stat(png) then
      vim.system({ converter, path .. '[0]', png }):wait()
    end
  end
  local ok, data = pcall(vim.fn.readblob, png)
  local width, height
  if ok then
    width, height = png_size(data)
  end
  files[key] = width and { data = data, width = width, height = height } or false
  return files[key] or nil
end

--- How many cells an image of `file` takes in a window `available`
--- columns wide.
local function cells(file, available)
  local width = math.max(1, math.ceil(file.width / cell.width))
  width = math.min(width, config().max_width or 80, math.max(1, available))
  local height = math.max(1, math.ceil(width * cell.width * file.height / file.width / cell.height))
  local most = config().max_height or 20
  if height > most then
    width = math.max(1, math.floor(width * most / height))
    height = most
  end
  return width, height
end

local function hide(entry, win)
  for at, id in pairs(entry.ids) do
    if win == nil or at == win then
      pcall(vim.ui.img.del, id)
      entry.ids[at] = nil
    end
  end
end

--- Put the images of buffer `bufnr` where their lines are in each window
--- that shows it, and take away those whose lines are out of sight.
---@param bufnr integer
function M.render(bufnr)
  local marks = shown[bufnr]
  if not marks or not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  local windows = {}
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    windows[win] = vim.fn.getwininfo(win)[1]
  end
  for id, entry in pairs(marks) do
    local mark = vim.api.nvim_buf_get_extmark_by_id(bufnr, namespace, id, { details = true })
    local ok, text = pcall(function()
      return table.concat(
        vim.api.nvim_buf_get_text(bufnr, mark[1], mark[2], mark[3].end_row, mark[3].end_col, {}),
        '\n'
      )
    end)
    if not mark[1] or not ok or text ~= entry.text then
      -- The link or the fragment changed, or is gone: so is its image, as
      -- Emacs takes an overlay away when its text changes.
      hide(entry)
      marks[id] = nil
      pcall(vim.api.nvim_buf_del_extmark, bufnr, namespace, id)
    else
      local row = mark[1]
      for win in pairs(entry.ids) do
        if not windows[win] then
          hide(entry, win)
        end
      end
      for win, info in pairs(windows) do
        local line = row + 1
        local visible = line >= info.topline and line <= info.botline
          and vim.api.nvim_win_call(win, function()
            return vim.fn.foldclosed(line) == -1
          end)
        -- The screen lines from the top of the window to the end of the
        -- line, other images' among them: the image goes below.
        local top = visible
            and info.winrow + vim.api.nvim_win_text_height(win, { start_row = info.topline - 1, end_row = row }).all
          or 0
        local fits = visible and top + entry.height - 1 <= info.winrow + info.height - 1
        if fits then
          local opts = { row = top, col = info.wincol + info.textoff, width = entry.width, height = entry.height }
          if entry.ids[win] then
            local now = vim.ui.img.get(entry.ids[win])
            if not now or now.row ~= opts.row or now.col ~= opts.col then
              vim.ui.img.set(entry.ids[win], opts)
            end
          else
            local file = load(entry.path)
            if file then
              entry.ids[win] = vim.ui.img.set(file.data, opts)
            end
          end
        else
          hide(entry, win)
        end
      end
    end
  end
end

local scheduled = {}

--- Render buffer `bufnr` once Neovim has drawn its windows.
local function later(bufnr)
  if scheduled[bufnr] then
    return
  end
  scheduled[bufnr] = true
  vim.schedule(function()
    scheduled[bufnr] = nil
    M.render(bufnr)
  end)
end

local function watch(bufnr)
  local group = vim.api.nvim_create_augroup('org_lsp_images_' .. bufnr, { clear = true })
  vim.api.nvim_create_autocmd({ 'TextChanged', 'TextChangedI', 'CursorMoved', 'BufWinEnter' }, {
    group = group,
    buffer = bufnr,
    callback = function()
      later(bufnr)
    end,
  })
  vim.api.nvim_create_autocmd({ 'WinScrolled', 'WinResized', 'VimResized', 'WinClosed', 'TabEnter' }, {
    group = group,
    callback = function()
      later(bufnr)
    end,
  })
  vim.api.nvim_create_autocmd('BufWinLeave', {
    group = group,
    buffer = bufnr,
    callback = function()
      later(bufnr)
    end,
  })
  vim.api.nvim_create_autocmd({ 'BufDelete', 'BufWipeout' }, {
    group = group,
    buffer = bufnr,
    callback = function()
      M.clear(bufnr)
    end,
  })
end

--- The previews of `kind` in buffer `bufnr` overlapping rows and columns
--- `range` (0-based, end exclusive).
local function overlapping(bufnr, kind, range)
  local found = {}
  for id, entry in pairs(shown[bufnr] or {}) do
    local mark = vim.api.nvim_buf_get_extmark_by_id(bufnr, namespace, id, { details = true })
    if entry.kind == kind and mark[1] then
      local start = { mark[1], mark[2] }
      local finish = { mark[3].end_row or mark[1], mark[3].end_col or mark[2] }
      local before = finish[1] < range[1] or (finish[1] == range[1] and finish[2] <= range[2] and start[1] < range[1])
      local after = start[1] > range[3] or (start[1] == range[3] and start[2] >= range[4])
      if not before and not after then
        table.insert(found, id)
      end
    end
  end
  return found
end

local function remove(bufnr, ids)
  for _, id in ipairs(ids) do
    local entry = shown[bufnr] and shown[bufnr][id]
    if entry then
      hide(entry)
      shown[bufnr][id] = nil
    end
    pcall(vim.api.nvim_buf_del_extmark, bufnr, namespace, id)
  end
end

--- Take away every image of buffer `bufnr`.
function M.clear(bufnr)
  for _, entry in pairs(shown[bufnr] or {}) do
    hide(entry)
  end
  shown[bufnr] = nil
  if vim.api.nvim_buf_is_valid(bufnr) then
    vim.api.nvim_buf_clear_namespace(bufnr, namespace, 0, -1)
  end
end

--- Show image `path` at `range` (an LSP range) of buffer `bufnr`.
local function add(bufnr, kind, range, path)
  local file = load(path)
  if not file then
    return false
  end
  local win = vim.fn.bufwinid(bufnr)
  local info = win ~= -1 and vim.fn.getwininfo(win)[1] or { width = vim.o.columns, textoff = 0 }
  local width, height = cells(file, info.width - info.textoff - 1)
  local lines = {}
  for _ = 1, height do
    table.insert(lines, { { '' } })
  end
  local last = vim.api.nvim_buf_line_count(bufnr) - 1
  local function column(row, character)
    return math.min(character, #(vim.api.nvim_buf_get_lines(bufnr, row, row + 1, false)[1] or ''))
  end
  local row, end_row = math.min(range.start.line, last), math.min(range['end'].line, last)
  local id = vim.api.nvim_buf_set_extmark(bufnr, namespace, row, column(row, range.start.character), {
    end_row = end_row,
    end_col = column(end_row, range['end'].character),
    virt_lines = lines,
    undo_restore = false,
  })
  local mark = vim.api.nvim_buf_get_extmark_by_id(bufnr, namespace, id, { details = true })
  local text = table.concat(vim.api.nvim_buf_get_text(bufnr, mark[1], mark[2], mark[3].end_row, mark[3].end_col, {}), '\n')
  shown[bufnr] = shown[bufnr] or {}
  shown[bufnr][id] = { kind = kind, path = path, width = width, height = height, ids = {}, text = text }
  return true
end

--- What a preview command or `org-lsp/images` says: `{ kind, uri, scope,
--- range, remove, includeLinked, items = { { range, path } } }`. `remove`
--- true takes away the previews in `range`; false shows `items` there,
--- anew; null (a link or a fragment at the cursor) toggles them. Says
--- what Emacs says, unless `quiet`.
---@param preview table
---@param quiet? boolean
function M.apply(preview, quiet)
  if not M.supported() then
    return
  end
  local bufnr = vim.uri_to_bufnr(preview.uri)
  if not vim.api.nvim_buf_is_loaded(bufnr) then
    return
  end
  local r = preview.range
  local range = { r.start.line, r.start.character, r['end'].line, r['end'].character }
  local old = overlapping(bufnr, preview.kind, range)
  local function say(message)
    if not quiet then
      require('org-lsp.notify')(message, vim.log.levels.INFO)
    end
  end
  local link = preview.kind == 'link'
  if preview.remove == true or (preview.remove == vim.NIL and #old > 0) then
    remove(bufnr, old)
    if link then
      say(('[%s] Inline link previews turned off (removed %d images)'):format(preview.scope, #old))
    elseif preview.scope == 'buffer' then
      say('LaTeX previews removed from buffer')
    elseif #old > 0 and preview.remove == vim.NIL then
      say('LaTeX preview removed')
    end
  else
    remove(bufnr, old)
    local count = 0
    for _, item in ipairs(preview.items or {}) do
      if add(bufnr, preview.kind, item.range, item.path) then
        count = count + 1
      end
    end
    if link then
      if count > 0 then
        say(('[%s] Displaying %d images inline %s'):format(
          preview.scope,
          count,
          preview.includeLinked and '(including images with description)' or ''
        ))
      elseif preview.scope == 'buffer' then
        say(('[%s] No images to display inline'):format(preview.scope))
      else
        say(('[%s] No images to display inline.  Use `C-u C-u\' or 11 argument to preview the whole buffer'):format(
          preview.scope
        ))
      end
    end
    watch(bufnr)
  end
  later(bufnr)
end

--- The previews of buffer `bufnr`, for tests: `{ kind, path, line, width,
--- height, windows }` each.
function M.list(bufnr)
  local out = {}
  for id, entry in pairs(shown[bufnr] or {}) do
    local mark = vim.api.nvim_buf_get_extmark_by_id(bufnr, namespace, id, {})
    table.insert(out, {
      kind = entry.kind,
      path = entry.path,
      line = mark[1],
      width = entry.width,
      height = entry.height,
      windows = vim.tbl_count(entry.ids),
    })
  end
  table.sort(out, function(a, b)
    return (a.line or 0) < (b.line or 0)
  end)
  return out
end

return M
