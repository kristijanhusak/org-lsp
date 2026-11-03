" The plugin downloading the server when none is found. Sourced by vimrc
" in place of test.vim when download.sh runs the tests: the plugin is a
" copy without a build of its own, and the "release" a directory with the
" server packed as CI packs it, reached through a file:// URL.
let s:work = $ORG_LSP_TEST_WORK
let s:results = []

function! s:until(what, Probe, ...) abort
  let timeout = get(a:, 1, 15000)
  let waited = 0
  while !a:Probe()
    if waited >= timeout
      throw 'timed out waiting for ' . a:what
    endif
    sleep 20m
    let waited += 20
    call feedkeys('', 'x')
  endwhile
endfunction

function! s:test(name, Fn) abort
  try
    call a:Fn()
    call add(s:results, 'ok - ' . a:name)
  catch
    call add(s:results, 'not ok - ' . a:name . ': ' . v:exception . ' (' . v:throwpoint . ')')
  endtry
endfunction

function! s:messages() abort
  return execute('messages')
endfunction

" A server in the current directory is never taken, as Windows would take
" it for a bare name, even with `.` or an empty entry in PATH.
function! s:not_from_here() abort
  let here = s:work . '/here'
  call mkdir(here, 'p')
  call writefile(['#!/bin/sh'], here . '/org-lsp')
  call setfperm(here . '/org-lsp', 'rwxr-xr-x')
  let [path, cwd] = [$PATH, getcwd()]
  execute 'cd' fnameescape(here)
  try
    let $PATH = '.::' . path
    call assert_equal([], org_lsp#server_command(), 'no server is taken from the current directory')
    let $PATH = here . ':' . path
    call assert_equal([here . '/org-lsp'], org_lsp#server_command(), 'a server on PATH is taken by its absolute path')
  finally
    let $PATH = path
    execute 'cd' fnameescape(cwd)
  endtry
endfunction

function! s:first_file() abort
  call assert_equal([], org_lsp#server_command(), 'no server before the download')
  call mkdir(s:work . '/notes', 'p')
  call writefile(['* TODO Call the client'], s:work . '/notes/work.org')
  execute 'edit' fnameescape(s:work . '/notes/work.org')
  call s:until('the downloaded server', {-> org_lsp#ready(bufnr())}, 30000)
  call assert_equal([org_lsp#download#path()], org_lsp#server_command(), 'the downloaded server is found')
  call assert_equal('rwxr-xr-x', getfperm(org_lsp#download#path()))
  call assert_match('^nightly \d\{4}-\d\d-\d\d', org_lsp#download#version())
  call assert_false(isdirectory(org_lsp#download#dir() . '/staging'), 'the unpacked archive is cleaned up')
  call assert_match('downloading the server (nightly)', s:messages())
  call assert_match('downloaded the server to', s:messages())
  " The plugin attached: its keys work.
  call cursor(1, 3)
  call feedkeys('cit', 'x')
  call s:until('DONE', {-> getline(1) ==# '* DONE Call the client'})
endfunction

function! s:again() abort
  messages clear
  OrgLspDownload
  call s:until('the download', {-> !org_lsp#download#running()})
  call assert_match('the new server runs the next time Vim starts', s:messages())
endfunction

function! s:missing() abort
  let g:org_lsp.release = 'v0.0.0-missing'
  messages clear
  OrgLspDownload
  call s:until('the failure', {-> !org_lsp#download#running()})
  call assert_match('could not download .*v0.0.0-missing', s:messages())
  call assert_true(executable(org_lsp#download#path()), 'the server downloaded before stays')
endfunction

" A release that is no release's name (it would go into the URL and `gh`'s
" arguments) is refused before anything is fetched.
function! s:not_a_release() abort
  let g:org_lsp.release = '../nightly'
  messages clear
  OrgLspDownload
  call s:until('the refusal', {-> !org_lsp#download#running()})
  call assert_match("'../nightly' is not a release", s:messages())
  call assert_notmatch('could not download', s:messages())
  call assert_true(executable(org_lsp#download#path()), 'the server downloaded before stays')
endfunction

" With no `release`, the release of the plugin's checkout: the version tag
" of its commit, or nightly.
function! s:checkout_release() abort
  let dir = tempname()
  call mkdir(dir, 'p')
  let Git = {args -> system('git -C ' . shellescape(dir) . ' -c user.name=t -c user.email=t@t ' . args)}
  call assert_equal('nightly', org_lsp#checkout_release(dir), 'not a git checkout')
  call Git('init -q')
  call Git('commit -q --allow-empty -m one')
  call assert_equal('nightly', org_lsp#checkout_release(dir), 'a commit without a tag')
  call Git('tag v0.3.1')
  call assert_equal('v0.3.1', org_lsp#checkout_release(dir), 'a commit with a version tag')
  call Git('commit -q --allow-empty -m two')
  call assert_equal('nightly', org_lsp#checkout_release(dir), 'a commit after the tag')
  let g:org_lsp.release = 'v0.2.0'
  call assert_equal('v0.2.0', org_lsp#release(), 'the release option first')
endfunction

" `:OrgLspDownload` of `release`, once it is over.
function! s:download_release(release) abort
  let g:org_lsp.release = a:release
  messages clear
  OrgLspDownload
  call s:until('the download of ' . a:release, {-> !org_lsp#download#running()}, 30000)
endfunction

" An archive that does not match the release's SHA256SUMS, or that they do
" not list, is not installed: the server downloaded before stays.
function! s:checked() abort
  let platform = org_lsp#download#platform()
  let before = org_lsp#download#version()
  call s:download_release('v9.0.0-broken')
  call assert_match('org-lsp-' . platform . '.tar.gz does not match its checksum in SHA256SUMS: the download is broken', s:messages())
  call assert_equal(before, org_lsp#download#version(), 'the server downloaded before stays')
  call assert_false(isdirectory(org_lsp#download#dir() . '/staging'), 'the refused archive is cleaned up')
  call s:download_release('v9.0.0-unlisted')
  call assert_match('is not in the SHA256SUMS of the v9.0.0-unlisted release', s:messages())
  call assert_equal(before, org_lsp#download#version(), 'the server downloaded before stays')
  " A release without SHA256SUMS is installed unchecked, as
  " scripts/install.sh does, and says so.
  call s:download_release('v9.0.0-unchecked')
  call assert_match('the v9.0.0-unchecked release has no SHA256SUMS: org-lsp-' . platform . '.tar.gz is not checked', s:messages())
  call assert_match('^v9.0.0-unchecked ', org_lsp#download#version())
endfunction

" From a timer: vimrc sources this file with execute(), which is silent,
" and a silent message is not kept in the message history.
function! s:main(...) abort
  set cmdheight=20
  call s:test('never takes the server from the current directory', function('s:not_from_here'))
  call s:test('downloads the server for the first Org file and starts it', function('s:first_file'))
  call s:test(':OrgLspDownload downloads it again', function('s:again'))
  call s:test('a release that does not exist is an error', function('s:missing'))
  call s:test('a release that is no release name is refused', function('s:not_a_release'))
  call s:test('the release of the checkout', function('s:checkout_release'))
  call s:test('the archive is checked against SHA256SUMS', function('s:checked'))
  for error in v:errors
    call add(s:results, 'not ok - ' . error)
  endfor
  call writefile(s:results, $ORG_LSP_TEST_RESULT . '.part')
  call rename($ORG_LSP_TEST_RESULT . '.part', $ORG_LSP_TEST_RESULT)
  qa!
endfunction
call timer_start(0, function('s:main'))
