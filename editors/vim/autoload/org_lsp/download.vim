" Downloading the server: the build of a GitHub release for this
" platform, unpacked into Vim's data directory
" (`~/.local/share/vim/org-lsp/org-lsp`). It needs `curl` and `tar`
" (Windows 10 and newer have both). A release curl cannot fetch, as one of
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
  let platform = org_lsp#download#platform()
  if empty(platform)
    return a:Callback(printf('there is no build for %s; build it with `cargo build --release -p org-lsp`',
          \ has('win32') ? 'this Windows machine' : trim(system('uname -sm'))), '')
  endif
  for tool in ['curl', 'tar']
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
    return s:run(['tar', '-xf', a:state.file, '-C', a:state.staging], {error -> s:unpacked(a:state, error)})
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
