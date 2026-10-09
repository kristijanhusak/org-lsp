--- Downloading the server: the build of a GitHub release for this
--- platform, unpacked into Neovim's data directory
--- (`stdpath('data')/org-lsp/org-lsp`). It needs `curl` and `tar` (Windows
--- 10 and newer have both). A release curl cannot fetch, as one of a
--- private repository, is fetched with the GitHub CLI (`gh`) when it is
--- installed and logged in.
local M = {}
local notify = require('org-lsp.notify')

--- Whether a download runs.
M.running = false

--- Where the release files are: `<base_url>/<release>/org-lsp-<platform>.tar.gz`.
M.base_url = 'https://github.com/kristijanhusak/org-lsp/releases/download'

local REPOSITORY = 'kristijanhusak/org-lsp'

--- The release's name for each system and machine `uv.os_uname()` reports.
local PLATFORMS = {
  Linux = { x86_64 = 'linux-x86_64', aarch64 = 'linux-arm64', arm64 = 'linux-arm64' },
  Darwin = { x86_64 = 'macos-x86_64', arm64 = 'macos-arm64', aarch64 = 'macos-arm64' },
  Windows_NT = { x86_64 = 'windows-x86_64', AMD64 = 'windows-x86_64' },
}

local function windows()
  return vim.fn.has('win32') == 1
end

local function executable()
  return windows() and 'org-lsp.exe' or 'org-lsp'
end

--- The release's name for this platform (`linux-x86_64`), or nil when
--- there is no build for it.
---@return string|nil
function M.platform()
  local uname = vim.uv.os_uname()
  return (PLATFORMS[uname.sysname] or {})[uname.machine]
end

--- The directory the server is downloaded into.
---@return string
function M.dir()
  return vim.fs.joinpath(vim.fn.stdpath('data'), 'org-lsp')
end

--- Where the downloaded server is (whether it is there or not).
---@return string
function M.path()
  return vim.fs.joinpath(M.dir(), executable())
end

--- The release the downloaded server came from and when, as
--- `nightly 2026-10-07 12:00`, or nil when none was downloaded.
---@return string|nil
function M.version()
  local ok, lines = pcall(vim.fn.readfile, vim.fs.joinpath(M.dir(), 'release'))
  return ok and lines[1] or nil
end

---@param cmd string[]
---@param callback fun(err: string|nil)
local function run(cmd, callback)
  vim.system(cmd, { text = true }, function(result)
    if result.code == 0 then
      callback(nil)
    else
      callback(('%s failed: %s'):format(cmd[1], vim.trim(result.stderr or '')))
    end
  end)
end

--- Download `release` (a version tag such as `v0.2.0`, or `nightly`) for
--- this platform, replacing the server downloaded before. `callback` runs
--- on the main loop with an error message, or with the server's path.
---@param release string
---@param callback fun(err: string|nil, path: string|nil)
function M.download(release, callback)
  local done = vim.schedule_wrap(callback)
  local platform = M.platform()
  if not platform then
    local uname = vim.uv.os_uname()
    return done(('there is no build for %s %s; build it with :OrgLspBuild'):format(uname.sysname, uname.machine))
  end
  for _, tool in ipairs({ 'curl', 'tar' }) do
    if vim.fn.executable(tool) == 0 then
      return done(tool .. ' is not installed')
    end
  end
  local archive = ('org-lsp-%s.%s'):format(platform, windows() and 'zip' or 'tar.gz')
  local url = ('%s/%s/%s'):format(M.base_url, release, archive)
  local staging = vim.fs.joinpath(M.dir(), 'staging')
  vim.fn.delete(staging, 'rf')
  vim.fn.mkdir(staging, 'p')
  local file = vim.fs.joinpath(staging, archive)
  local gh = vim.startswith(url, 'https://github.com/') and vim.fn.executable('gh') == 1
  local function fetched(err)
    if err then
      return done(('could not download %s: %s'):format(url, err))
    end
    run({ 'tar', '-xf', file, '-C', staging }, function(unpack_err)
      if unpack_err then
        return done(unpack_err)
      end
      local unpacked = vim.fs.joinpath(staging, executable())
      if not vim.uv.fs_stat(unpacked) then
        return done(('%s has no %s'):format(url, executable()))
      end
      vim.uv.fs_chmod(unpacked, tonumber('755', 8))
      local target = M.path()
      -- A running server cannot be replaced on Windows, but it can be
      -- renamed.
      local old = target .. '.old'
      os.remove(old)
      os.rename(target, old)
      local ok, rename_err = os.rename(unpacked, target)
      if not ok then
        return done(rename_err)
      end
      os.remove(old)
      vim.schedule(function()
        vim.fn.writefile({ release .. ' ' .. os.date('!%Y-%m-%d %H:%M') .. ' UTC' }, vim.fs.joinpath(M.dir(), 'release'))
        vim.fn.delete(staging, 'rf')
        callback(nil, target)
      end)
    end)
  end
  run({ 'curl', '--fail', '--silent', '--show-error', '--location', '--retry', '2', '--output', file, url }, function(err)
    -- A private repository's files need GitHub's login, which `gh` has.
    if err and gh then
      return run({ 'gh', 'release', 'download', release, '--repo', REPOSITORY, '--pattern', archive, '--dir', staging, '--clobber' }, function(gh_err)
        fetched(gh_err and err .. '; ' .. gh_err)
      end)
    end
    fetched(err)
  end)
end

--- `:OrgLspDownload`: download `release` and tell what came of it. Without
--- a UI (`nvim --headless`, a plugin manager's build step) it waits until
--- the download is done.
---@param release string
---@param after? fun(path: string) runs once the server is in place
function M.run(release, after)
  if M.running then
    return notify('org-lsp: the server is being downloaded already')
  end
  M.running = true
  local finished = false
  notify(('org-lsp: downloading the server (%s)…'):format(release))
  M.download(release, function(err, path)
    finished = true
    M.running = false
    if err then
      return notify('org-lsp: ' .. err, vim.log.levels.ERROR)
    end
    notify('org-lsp: downloaded the server to ' .. path)
    if after then
      after(path --[[@as string]])
    end
  end)
  if #vim.api.nvim_list_uis() == 0 then
    vim.wait(300000, function()
      return finished
    end, 100)
  end
end

return M
