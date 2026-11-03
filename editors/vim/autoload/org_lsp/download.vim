" Downloading the server: the build of a GitHub release for this
" platform, unpacked into Vim's data directory
" (`~/.local/share/vim/org-lsp/org-lsp`). It needs `curl` and `tar`
" (Windows 10 and newer have both; on Windows its own `tar` is used, else
" PowerShell). A release curl cannot fetch, as one of
" a private repository, is fetched with the GitHub CLI (`gh`) when it is
" installed and logged in.

" Where the release files are: `<base_url>/<release>/org-lsp-<platform>.tar.gz`.
let g:org_lsp#download#base_url = get(g:, 'org_lsp#download#base_url',
      \ 'https://github.com/kristijanhusak/org-lsp/releases/download')
let s:repository = 'kristijanhusak/org-lsp'
let s:running = 0

" The release's name for each system and machine `uname -sm` reports.
let s:platforms = {
      \ 'Linux': {'x86_64': 'linux-x86_64', 'aarch64': 'linux-arm64', 'arm64': 'linux-arm64'},
      \ 'Darwin': {'x86_64': 'macos-x86_64', 'arm64': 'macos-arm64', 'aarch64': 'macos-arm64'},
      \ }

function! s:executable() abort
  return has('win32') ? 'org-lsp.exe' : 'org-lsp'
endfunction

" The release's name for this platform (`linux-x86_64`), or '' when there
" is no build for it.
function! org_lsp#download#platform() abort
  if has('win32')
    return index([$PROCESSOR_ARCHITECTURE, $PROCESSOR_ARCHITEW6432], 'AMD64') >= 0 ? 'windows-x86_64' : ''
  endif
  let uname = split(system('uname -sm'))
  return len(uname) == 2 ? get(get(s:platforms, uname[0], {}), uname[1], '') : ''
endfunction

" The directory the server is downloaded into.
function! org_lsp#download#dir() abort
  if has('win32')
    return (empty($LOCALAPPDATA) ? expand('~/AppData/Local') : $LOCALAPPDATA) . '/vim/org-lsp'
  endif
  return (empty($XDG_DATA_HOME) ? expand('~/.local/share') : $XDG_DATA_HOME) . '/vim/org-lsp'
endfunction

" Where the downloaded server is (whether it is there or not).
function! org_lsp#download#path() abort
  return org_lsp#download#dir() . '/' . s:executable()
endfunction

" The release the downloaded server came from and when, as
" `nightly 2026-10-07 12:00 UTC`, or '' when none was downloaded.
function! org_lsp#download#version() abort
  let file = org_lsp#download#dir() . '/release'
  return filereadable(file) ? get(readfile(file), 0, '') : ''
endfunction

" The command that unpacks the archive `file` into the directory `dir`. On
" Windows it is Windows' own `tar` (bsdtar, which reads zip files): the
" `tar` found first may be Git's GNU tar, which takes `C:` in a path for a
" remote host and cannot read zip files. Without it, PowerShell's
" `Expand-Archive`.
function! org_lsp#download#unpack_command(file, dir) abort
  if !has('win32')
    return ['tar', '-xf', a:file, '-C', a:dir]
  endif
  let system_tar = (empty($SystemRoot) ? 'C:\Windows' : $SystemRoot) . '\System32\tar.exe'
  if executable(system_tar)
    return [system_tar, '-xf', a:file, '-C', a:dir]
  endif
  let Quote = {text -> "'" . substitute(text, "'", "''", 'g') . "'"}
  return ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command',
        \ printf('Expand-Archive -Force -LiteralPath %s -DestinationPath %s', Quote(a:file), Quote(a:dir))]
endfunction

" Run `cmd`, then `Callback` with '' or what went wrong.
function! s:run(cmd, Callback) abort
  let state = {'errors': []}
  call job_start(a:cmd, {
        \ 'in_io': 'null',
        \ 'out_io': 'null',
        \ 'err_cb': {channel, message -> add(state.errors, message)},
        \ 'exit_cb': {job, status -> a:Callback(status == 0 ? '' :
        \   printf('%s failed: %s', a:cmd[0], trim(join(state.errors, ' '))))},
        \ })
endfunction

" Download `release` (a version tag such as `v0.2.0`, or `nightly`) for
" this platform, replacing the server downloaded before. `Callback` is
" called with what went wrong and '', or with '' and the server's path.
function! org_lsp#download#download(release, Callback) abort
  " The release goes into a URL and into `gh`'s arguments: only a name that
  " can be a release's.
  if type(a:release) != v:t_string || a:release !~# '^\%(nightly\|v[0-9][0-9A-Za-z._-]*\)$'
    return a:Callback(printf('%s is not a release: use nightly or a version tag such as v0.2.0', string(a:release)), '')
  endif
  let platform = org_lsp#download#platform()
  if empty(platform)
    return a:Callback(printf('there is no build for %s; build it with `cargo build --release -p org-lsp`',
          \ has('win32') ? 'this Windows machine' : trim(system('uname -sm'))), '')
  endif
  " Windows has its own way to unpack, see `org_lsp#download#unpack_command`.
  for tool in has('win32') ? ['curl'] : ['curl', 'tar']
    if !executable(tool)
      return a:Callback(tool . ' is not installed', '')
    endif
  endfor
  let archive = printf('org-lsp-%s.%s', platform, has('win32') ? 'zip' : 'tar.gz')
  let url = printf('%s/%s/%s', g:org_lsp#download#base_url, a:release, archive)
  let staging = org_lsp#download#dir() . '/staging'
  call delete(staging, 'rf')
  call mkdir(staging, 'p')
  let state = {'release': a:release, 'url': url, 'archive': archive, 'staging': staging,
        \ 'file': staging . '/' . archive, 'Callback': a:Callback}
  call s:run(['curl', '--fail', '--silent', '--show-error', '--location', '--retry', '2', '--output', state.file, url],
        \ {error -> s:fetched(state, error)})
endfunction

function! s:fetched(state, error) abort
  if empty(a:error)
    return s:fetch_sums(a:state)
  endif
  " A private repository's files need GitHub's login, which `gh` has.
  if executable('gh') && a:state.url =~# '^https://github\.com/' && !get(a:state, 'gh', 0)
    let a:state.gh = 1
    return s:run(['gh', 'release', 'download', a:state.release, '--repo', s:repository,
          \ '--pattern', a:state.archive, '--dir', a:state.staging, '--clobber'],
          \ {error -> s:fetched(a:state, empty(error) ? '' : a:error . '; ' . error)})
  endif
  call a:state.Callback('could not download ' . a:state.url . ': ' . a:error, '')
endfunction

" The release's SHA256SUMS, as `scripts/install.sh` gets them: a release
" without them (not found, or not there in a `file://` release) is not
" checked; any other failure to get them stops the download.
function! s:fetch_sums(state) abort
  let a:state.sums = a:state.staging . '/SHA256SUMS'
  let a:state.sums_url = printf('%s/%s/SHA256SUMS', g:org_lsp#download#base_url, a:state.release)
  let result = {'out': [], 'errors': []}
  call job_start(['curl', '--fail', '--silent', '--show-error', '--location', '--retry', '2',
        \ '--write-out', '%{http_code}', '--output', a:state.sums, a:state.sums_url], {
        \ 'in_io': 'null',
        \ 'out_cb': {channel, message -> add(result.out, message)},
        \ 'err_cb': {channel, message -> add(result.errors, message)},
        \ 'exit_cb': {job, status -> s:got_sums(a:state, status, result)},
        \ })
endfunction

function! s:got_sums(state, status, result) abort
  if a:status == 0
    return s:check(a:state, 1)
  endif
  call delete(a:state.sums)
  " 22: an HTTP error, here 404; 37: no such file (`file://`).
  if !(a:status == 22 && trim(join(a:result.out, '')) ==# '404' || a:status == 37)
    return s:fail(a:state, printf('could not download SHA256SUMS from the %s release (%s): %s',
          \ a:state.release, a:state.sums_url, trim(join(a:result.errors, ' '))))
  endif
  " A private repository's files need GitHub's login, which `gh` has;
  " without it, they are not found.
  if executable('gh') && a:state.sums_url =~# '^https://github\.com/'
    return s:run(['gh', 'release', 'download', a:state.release, '--repo', s:repository,
          \ '--pattern', 'SHA256SUMS', '--dir', a:state.staging, '--clobber'],
          \ {error -> s:check(a:state, empty(error))})
  endif
  call s:check(a:state, 0)
endfunction

" The archive checked against the release's SHA256SUMS (when `found`),
" then unpacked.
function! s:check(state, found) abort
  if !a:found
    call s:warn(printf('the %s release has no SHA256SUMS: %s is not checked', a:state.release, a:state.archive))
  else
    let expected = ''
    for line in readfile(a:state.sums)
      let fields = matchlist(line, '^\(\x\+\)\s\+\*\=\(.\{-}\)\s*$')
      if !empty(fields) && fields[2] ==# a:state.archive
        let expected = tolower(fields[1])
        break
      endif
    endfor
    " The files built together are all in SHA256SUMS: one that is not is
    " left from an older build (a platform whose build failed).
    if empty(expected)
      return s:fail(a:state, printf('%s is not in the SHA256SUMS of the %s release: it is not from the same build, so it is not installed',
            \ a:state.archive, a:state.release))
    endif
    let actual = s:sha256(a:state.file)
    if empty(actual)
      call s:warn(printf('neither sha256() nor a sha256sum, shasum or certutil can hash files: %s is not checked', a:state.archive))
    elseif actual !=# expected
      return s:fail(a:state, printf('%s does not match its checksum in SHA256SUMS: the download is broken', a:state.archive))
    endif
  endif
  call s:run(org_lsp#download#unpack_command(a:state.file, a:state.staging),
        \ {error -> s:unpacked(a:state, error)})
endfunction

" The SHA-256 of `file`, in lowercase hex: Vim's own `sha256()` where it
" takes a Blob, else the system's tool; '' when there is none.
function! s:sha256(file) abort
  try
    return sha256(readblob(a:file))
  catch
  endtry
  if has('win32')
    let lines = systemlist('certutil -hashfile ' . shellescape(a:file) . ' SHA256')
    return v:shell_error == 0 && len(lines) > 1 ? tolower(substitute(lines[1], '\s', '', 'g')) : ''
  endif
  for tool in ['sha256sum', 'shasum -a 256']
    if executable(split(tool)[0])
      let out = system(tool . ' ' . shellescape(a:file))
      return v:shell_error == 0 ? tolower(matchstr(out, '^\x\{64}')) : ''
    endif
  endfor
  return ''
endfunction

function! s:fail(state, error) abort
  call delete(a:state.staging, 'rf')
  call a:state.Callback(a:error, '')
endfunction

function! s:warn(message) abort
  echohl WarningMsg
  echomsg 'org-lsp: ' . a:message
  echohl None
endfunction

function! s:unpacked(state, error) abort
  if !empty(a:error)
    return a:state.Callback(a:error, '')
  endif
  let unpacked = a:state.staging . '/' . s:executable()
  if !filereadable(unpacked)
    return a:state.Callback(printf('%s has no %s', a:state.url, s:executable()), '')
  endif
  call setfperm(unpacked, 'rwxr-xr-x')
  let target = org_lsp#download#path()
  " A running server cannot be replaced on Windows, but it can be renamed.
  let old = target . '.old'
  call delete(old)
  call rename(target, old)
  if rename(unpacked, target) != 0
    return a:state.Callback('could not move the server to ' . target, '')
  endif
  call delete(old)
  call writefile([a:state.release . ' ' . strftime('%Y-%m-%d %H:%M', localtime()) . ' ' . strftime('%Z')],
        \ org_lsp#download#dir() . '/release')
  call delete(a:state.staging, 'rf')
  call a:state.Callback('', target)
endfunction

" Whether a download runs.
function! org_lsp#download#running() abort
  return s:running
endfunction

" `:OrgLspDownload`: download `release` and say what came of it; `After` is
" called with the server's path once it is in place.
function! org_lsp#download#run(release, After) abort
  if s:running
    echomsg 'org-lsp: the server is being downloaded already'
    return
  endif
  let s:running = 1
  echomsg printf('org-lsp: downloading the server (%s)…', a:release)
  call org_lsp#download#download(a:release, {error, path -> s:done(error, path, a:After)})
endfunction

function! s:done(error, path, After) abort
  let s:running = 0
  if !empty(a:error)
    echohl ErrorMsg
    echomsg 'org-lsp: ' . a:error
    echohl None
    return
  endif
  echomsg 'org-lsp: downloaded the server to ' . a:path
  call a:After(a:path)
endfunction
