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

--- The size of a cell of the terminal in pixels, for the size of images,
--- unless the terminal says it (`cell_size()`): most terminal fonts are
--- about twice as high as wide.
local guess = { width = 10, height = 20 }

--- The terminals Neovim may run in: its own, and with a UI (`nvim` in a
--- terminal runs the editor in a child process, which has none) the one
--- of the UI, its parent.
---@return string[]
local function terminals()
  local paths = { '/dev/tty' }
  local parent = vim.uv.os_getppid()
  local linked = vim.uv.fs_readlink('/proc/' .. parent .. '/fd/0')
  if linked and linked:match('^/dev/') then
    table.insert(paths, linked)
  elseif vim.fn.executable('ps') == 1 then
    local found = vim.system({ 'ps', '-o', 'tty=', '-p', tostring(parent) }, { text = true }):wait()
    local name = vim.trim(found.stdout or '')
    if found.code == 0 and name ~= '' and name ~= '?' and name ~= '??' then
      table.insert(paths, '/dev/' .. name)
    end
  end
  return paths
end

--- The terminal's size in pixels and in cells (`TIOCGWINSZ`): the size of a
--- cell, so that images take the cells they show in. None where the
--- terminal does not say (zero pixels) or cannot be asked.
---@return { width: number, height: number }?
local function measured_cell()
  local ok, ffi = pcall(require, 'ffi')
  if not ok or ffi.os == 'Windows' then
    return nil
  end
  pcall(
    ffi.cdef,
    [[
      struct org_lsp_winsize { unsigned short rows, cols, xpixel, ypixel; };
      int open(const char *path, int flags, ...);
      int close(int fd);
      int ioctl(int fd, unsigned long request, ...);
    ]]
  )
  local TIOCGWINSZ = ffi.os == 'OSX' and 0x40087468 or 0x5413
  -- O_RDONLY, O_NOCTTY: asking, not taking the terminal.
  local flags = ffi.os == 'OSX' and 0x20000 or 0x100
  for _, path in ipairs(terminals()) do
    local fd = ffi.C.open(path, flags)
    if fd >= 0 then
      local size = ffi.new('struct org_lsp_winsize')
      local got = ffi.C.ioctl(fd, TIOCGWINSZ, size)
      ffi.C.close(fd)
      if got == 0 and size.cols > 0 and size.rows > 0 and size.xpixel > 0 and size.ypixel > 0 then
        return { width = size.xpixel / size.cols, height = size.ypixel / size.rows }
      end
    end
  end
  return nil
end

--- The size of a cell in pixels: `images.cell` if set, else as the
--- terminal says, else a guess. Asked again when the editor's size changes.
local cell
-- A font of another size, as the editor's size in cells changes with it.
vim.api.nvim_create_autocmd('VimResized', {
  group = vim.api.nvim_create_augroup('org_lsp_images_cell', { clear = true }),
  callback = function()
    cell = nil
  end,
})
local function cell_size()
  local images = require('org-lsp').config.images
  local set = type(images) == 'table' and images.cell
  if type(set) == 'table' and tonumber(set.width) and tonumber(set.height) then
    return { width = tonumber(set.width), height = tonumber(set.height) }
  end
  if not cell then
    cell = measured_cell() or guess
  end
  return cell
end

--- The size of a cell in pixels that images are made for.
M.cell_size = cell_size

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

--- The extensions of the image files the server previews that are not PNG
--- (`image-file-name-extensions`), each the name of ImageMagick's coder
--- for its files.
local coders = {}
for _, extension in ipairs({ 'jpeg', 'jpg', 'gif', 'tiff', 'tif', 'xbm', 'xpm', 'pbm', 'pgm', 'ppm', 'pnm', 'svg', 'webp' }) do
  coders[extension] = true
end

--- The images read, by file and time of change, the last read last: at
--- most `KEPT`, so that a session that shows many does not keep them all.
local order, KEPT = {}, 64

---@param key string
---@param file table|false
local function remember(key, file)
  if files[key] == nil then
    table.insert(order, key)
    if #order > KEPT then
      files[table.remove(order, 1)] = nil
    end
  end
  files[key] = file
end

--- Remove the images ImageMagick made that no preview asked for in a
--- month, once a session.
local pruned = false
---@param directory string
local function prune(directory)
  if pruned then
    return
  end
  pruned = true
  local old = os.time() - 30 * 24 * 60 * 60
  for name, kind in vim.fs.dir(directory) do
    local file = vim.fs.joinpath(directory, name)
    local stat = kind == 'file' and name:match('%.png$') and vim.uv.fs_stat(file)
    if stat and stat.atime.sec < old and stat.mtime.sec < old then
      os.remove(file)
    end
  end
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
      remember(key, false)
      return nil
    end
    local directory = vim.fs.joinpath(vim.fn.stdpath('cache'), 'org-lsp', 'images')
    vim.fn.mkdir(directory, 'p')
    prune(directory)
    png = vim.fs.joinpath(directory, vim.fn.sha256(key) .. '.png')
    if not vim.uv.fs_stat(png) then
      -- Read as what its extension says (`svg:pic.svg`), not as what
      -- ImageMagick makes of its bytes, and within limits: the editor
      -- waits for it.
      local extension = (path:match('%.(%w+)$') or ''):lower()
      local input = (coders[extension] and extension .. ':' or '') .. path .. '[0]'
      local limits = { '-limit', 'memory', '256MiB', '-limit', 'area', '128MP', '-limit', 'time', '10' }
      local made = vim.system(vim.list_extend({ converter, unpack(limits) }, { input, png }), { timeout = 10000 }):wait()
      if made.code ~= 0 then
        -- Not kept as no image: the next preview tries again.
        os.remove(png)
        return nil
      end
    end
  end
  local ok, data = pcall(vim.fn.readblob, png)
  local width, height
  if ok then
    width, height = png_size(data)
  end
  remember(key, width and { data = data, width = width, height = height } or false)
  return files[key] or nil
end

--- How an image of `file` shows in a window `available` columns wide:
--- the cells it takes, and the one side the terminal is told to scale it
--- to (`{ width = n }` or `{ height = n }`), when it is larger than
--- `max_width` and `max_height` allow. An image is never given both: the
--- terminal keeps its proportions, at its own size in pixels where it fits,
--- rather than stretching it over whole cells.
---@return integer width, integer height, table scale
local function cells(file, available)
  local cell = cell_size()
  local columns, rows = file.width / cell.width, file.height / cell.height
  local most_columns = math.max(1, math.min(config().max_width or 80, available))
  local most_rows = config().max_height or 20
  if columns <= most_columns and rows <= most_rows then
    return math.max(1, math.ceil(columns)), math.max(1, math.ceil(rows)), {}
  end
  if most_columns / columns <= most_rows / rows then
    local width = math.max(1, math.floor(most_columns))
    return width, math.max(1, math.ceil(rows * width / columns)), { width = width }
  end
  local height = math.max(1, math.floor(most_rows))
  return math.max(1, math.ceil(columns * height / rows)), height, { height = height }
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
local indent_namespace = vim.api.nvim_create_namespace('org_lsp_virtual_indent')

--- Where the text of line `row` starts on the screen, from the window's
--- text: after its virtual indentation (`org-indent-mode`, the prefix the
--- server says) and its own leading blanks. An image goes below its line
--- from there, as the line's text does.
---@param bufnr integer
---@param row integer from 0
---@return integer
local function indentation(bufnr, row)
  local width = 0
  local marks = vim.api.nvim_buf_get_extmarks(bufnr, indent_namespace, { row, 0 }, { row, 0 }, { details = true })
  for _, mark in ipairs(marks) do
    local details = mark[4]
    if details.virt_text and details.virt_text_pos == 'inline' then
      for _, chunk in ipairs(details.virt_text) do
        width = width + vim.fn.strdisplaywidth(chunk[1])
      end
    end
  end
  local line = vim.api.nvim_buf_get_lines(bufnr, row, row + 1, false)[1] or ''
  return width + vim.fn.strdisplaywidth(line:match('^%s*'))
end

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
          -- As far in as the line's text, as long as the image fits.
          local text_width = info.width - info.textoff
          local offset = math.max(0, math.min(indentation(bufnr, row), text_width - entry.width))
          local opts = vim.tbl_extend('force', { row = top, col = info.wincol + info.textoff + offset }, entry.scale)
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

--- What the windows showing buffer `bufnr` show of it: the lines, where
--- and how high (folds count), and the buffer's version. The images
--- stay where they are while it stays the same.
---@param bufnr integer
---@return string
local function view_of(bufnr)
  local parts = { vim.api.nvim_buf_get_changedtick(bufnr) }
  for _, win in ipairs(vim.fn.win_findbuf(bufnr)) do
    local info = vim.fn.getwininfo(win)[1]
    local height = vim.api.nvim_win_text_height(win, { start_row = info.topline - 1, end_row = info.botline - 1 }).all
    table.insert(
      parts,
      table.concat({ win, info.topline, info.botline, info.winrow, info.wincol, info.width, info.height, info.textoff, height }, ',')
    )
  end
  return table.concat(parts, ';')
end

--- The view each buffer's images were last put in.
---@type table<integer, string>
local rendered = {}

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
    if vim.api.nvim_buf_is_valid(bufnr) then
      rendered[bufnr] = view_of(bufnr)
    end
  end)
end

local function watch(bufnr)
  local group = vim.api.nvim_create_augroup('org_lsp_images_' .. bufnr, { clear = true })
  vim.api.nvim_create_autocmd({ 'TextChanged', 'TextChangedI', 'BufWinEnter' }, {
    group = group,
    buffer = bufnr,
    callback = function()
      later(bufnr)
    end,
  })
  -- A move of the cursor moves the images only when it scrolls, opens or
  -- closes a fold, or changes the text.
  vim.api.nvim_create_autocmd('CursorMoved', {
    group = group,
    buffer = bufnr,
    callback = function()
      if shown[bufnr] and next(shown[bufnr]) and rendered[bufnr] ~= view_of(bufnr) then
        later(bufnr)
      end
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
      rendered[bufnr] = nil
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
  local start = math.min(range.start.line, vim.api.nvim_buf_line_count(bufnr) - 1)
  local available = info.width - info.textoff - 1
  local width, height, scale = cells(file, math.max(available - indentation(bufnr, start), math.floor(available / 2)))
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
  shown[bufnr][id] = { kind = kind, path = path, width = width, height = height, scale = scale, ids = {}, text = text }
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
  local bufnr = require('org-lsp.buffers').of(preview.uri)
  if not bufnr then
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
