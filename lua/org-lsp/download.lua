--- Downloading the server: the build of a GitHub release for this
--- platform, unpacked into Neovim's data directory
--- (`stdpath('data')/org-lsp/org-lsp`). It needs `curl` and `tar` (Windows
--- 10 and newer have both; on Windows its own `tar` is used, else
--- PowerShell). A release curl cannot fetch, as one of a
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

--- The command that unpacks the archive `file` into the directory `dir`.
--- On Windows it is Windows' own `tar` (bsdtar, which reads zip files):
--- the `tar` found first may be Git's GNU tar, which takes `C:` in a path
--- for a remote host and cannot read zip files. Without it, PowerShell's
--- `Expand-Archive`.
---@param file string
---@param dir string
---@return string[]
function M.unpack_command(file, dir)
  if not windows() then
    return { 'tar', '-xf', file, '-C', dir }
  end
  local system_tar = vim.fs.joinpath(os.getenv('SystemRoot') or 'C:\\Windows', 'System32', 'tar.exe')
  if vim.fn.executable(system_tar) == 1 then
    return { system_tar, '-xf', file, '-C', dir }
  end
  local quote = function(text)
    return "'" .. text:gsub("'", "''") .. "'"
  end
  return {
    'powershell.exe',
    '-NoProfile',
    '-NonInteractive',
    '-Command',
    ('Expand-Archive -Force -LiteralPath %s -DestinationPath %s'):format(quote(file), quote(dir)),
  }
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

--- The release's `SHA256SUMS` into `file`, as `scripts/install.sh` gets
--- them. `callback` gets what went wrong, or nil and whether the release
--- has them: one without them (not found, or not there in a `file://`
--- release) is not checked; any other failure stops the download.
---@param release string
---@param file string
---@param callback fun(err: string|nil, found: boolean|nil)
local function fetch_sums(release, file, callback)
  local url = ('%s/%s/SHA256SUMS'):format(M.base_url, release)
  local cmd = { 'curl', '--fail', '--silent', '--show-error', '--location', '--retry', '2' }
  vim.list_extend(cmd, { '--write-out', '%{http_code}', '--output', file, url })
  vim.system(cmd, { text = true }, function(result)
    if result.code == 0 then
      return callback(nil, true)
    end
    os.remove(file)
    -- 22: an HTTP error, here 404; 37: no such file (`file://`).
    local missing = result.code == 22 and vim.trim(result.stdout or '') == '404' or result.code == 37
    if not missing then
      return callback(('could not download SHA256SUMS from the %s release (%s): %s'):format(release, url, vim.trim(result.stderr or '')))
    end
    -- A private repository's files need GitHub's login, which `gh` has;
    -- without it, they are not found.
    if vim.startswith(url, 'https://github.com/') and vim.fn.executable('gh') == 1 then
      local dir = vim.fs.dirname(file)
      return run({ 'gh', 'release', 'download', release, '--repo', REPOSITORY, '--pattern', 'SHA256SUMS', '--dir', dir, '--clobber' }, function(err)
        callback(nil, err == nil)
      end)
    end
    callback(nil, false)
  end)
end

--- Check the archive `file` (named `name` in the release) against the
--- release's `SHA256SUMS` in `sums`: what is wrong with it, or nil. Runs on
--- the main loop (`sha256()`).
---@param file string
---@param name string
---@param sums string
---@param release string
---@return string|nil
function M.check(file, name, sums, release)
  local expected
  for line in io.lines(sums) do
    local sum, listed = line:match('^(%x+)%s+%*?(.-)%s*$')
    if listed == name then
      expected = sum:lower()
      break
    end
  end
  -- The files built together are all in SHA256SUMS: one that is not is
  -- left from an older build (a platform whose build failed).
  if not expected then
    return ('%s is not in the SHA256SUMS of the %s release: it is not from the same build, so it is not installed'):format(name, release)
  end
  local handle = assert(io.open(file, 'rb'))
  local data = handle:read('*a')
  handle:close()
  if vim.fn.sha256(data) ~= expected then
    return ('%s does not match its checksum in SHA256SUMS: the download is broken'):format(name)
  end
end

--- Download `release` (a version tag such as `v0.2.0`, or `nightly`) for
--- this platform, replacing the server downloaded before. `callback` runs
--- on the main loop with an error message, or with the server's path.
---@param release string
---@param callback fun(err: string|nil, path: string|nil)
function M.download(release, callback)
  local done = vim.schedule_wrap(callback)
  -- The release goes into a URL and into `gh`'s arguments: only a name
  -- that can be a release's.
  if type(release) ~= 'string' or release ~= 'nightly' and not release:match('^v%d[%w._-]*$') then
    return done(('%s is not a release: use nightly or a version tag such as v0.2.0'):format(vim.inspect(release)))
  end
  local platform = M.platform()
  if not platform then
    local uname = vim.uv.os_uname()
    return done(('there is no build for %s %s; build it with :OrgLspBuild'):format(uname.sysname, uname.machine))
  end
  -- Windows has its own way to unpack, see `unpack_command`.
  for _, tool in ipairs(windows() and { 'curl' } or { 'curl', 'tar' }) do
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
  local function unpack()
    run(M.unpack_command(file, staging), function(unpack_err)
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
  -- The archive checked against the release's SHA256SUMS before it is
  -- unpacked, when the release has them.
  local function fetched(err)
    if err then
      return done(('could not download %s: %s'):format(url, err))
    end
    local sums = vim.fs.joinpath(staging, 'SHA256SUMS')
    fetch_sums(release, sums, function(sums_err, found)
      if sums_err then
        return done(sums_err)
      end
      vim.schedule(function()
        if not found then
          notify(('org-lsp: the %s release has no SHA256SUMS: %s is not checked'):format(release, archive), vim.log.levels.WARN)
          return unpack()
        end
        local ok, wrong = pcall(M.check, file, archive, sums, release)
        if not ok or wrong then
          vim.fn.delete(staging, 'rf')
          return callback(ok and wrong or tostring(wrong))
        end
        unpack()
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
