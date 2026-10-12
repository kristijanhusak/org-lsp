# Audit findings

A review of org-lsp for bugs, security issues and performance problems.
Missing features and parity gaps with Emacs are left out on purpose.
Every item names the file and line where it was found. "Verified" means the
trigger was run and the failure observed; "unverified" means it was
established by reading the code only.

The server runs on one thread with no `catch_unwind` (`server/org-lsp/src/main.rs:95`,
`server/org-lsp/src/server.rs:1412`), so every panic below ends the whole
language server, and every hang blocks every request until the editor kills
it. Fixing that one thing (finding S1) turns most "high" items into
"medium".

Severity key: **High** = crash, hang or data loss from ordinary document
content, or a security problem; **Medium** = crash or hang from unusual
content or configuration, or a clear performance cliff; **Low** = hardening,
minor bugs, or performance that only matters on hostile input.

---

## 1. Crashes and hangs from document content

### C1. High — Stack overflow (abort) tangling a self-referencing noweb block
`server/org-core/src/tangle.rs:942-1020` (`Expander::expand` → `reference` → `expand_body`).
No cycle guard: a block with `:noweb yes` that references itself by name or
`:noweb-ref`, or two blocks referencing each other, recurses without bound.
A stack overflow is an abort, not a panic, so nothing could catch it.
Reached through `org.babel.tangle` and detangle (`server.rs:8771`, `8802`).
Emacs raises a catchable `max-lisp-eval-depth` error.
**Verified**: SIGABRT with
```
#+name: a
#+begin_src sh :noweb yes :tangle out.sh
<<a>>
#+end_src
```
Fix: track the blocks being expanded in `Expander` and return an error when a
block is already on the stack, or cap the depth.

### C2. High — Infinite loop in `org-todo` on a `++0d` repeater
`server/org-core/src/todo.rs:1294-1311` (`Editor::auto_repeat`).
The `count == 0` guard at line 1220 covers only the first repeater of the
entry. For every later timestamp the `++` branch loops until the stamp is in
the future, but with `count == 0` the stamp never moves.
**Verified**: 100% CPU for over 90 s on
```
* TODO a
SCHEDULED: <2026-01-01 Thu +1w>
body <2020-01-01 Wed ++0d>
```
when marking the entry DONE.
Fix: `continue` when the repeater count is 0 (Org treats a 0 shift as a
no-op), and cap the loop.

### C3. High — Hang on `@+N`/`@-N` row descriptors with a huge N
`server/org-core/src/tblfm.rs:540-555` (`row_type`).
The inner loop breaks when `i` leaves the table but the outer `for _ in 0..n`
keeps running for all `n` iterations; `n` is parsed from `[0-9]+` into an
i64 without a limit.
**Verified**: `#+TBLFM: @2$1=@+999999999999$1` on a two-row table did not
return within 60 s. Emacs errors immediately.
Fix: return `Err("Row descriptor ... leads outside table")` as soon as
`i < 0 || i >= l`.

### C4. High — Panic on a multi-byte unit in a match-string time operand
`server/org-core/src/agenda/matcher.rs:303` (`time_operand`).
`rest[..rest.len() - 1]` assumes the unit character is one byte.
**Verified**: `Matcher::new("SCHEDULED<\"<+5日>\"")` panics with
"end byte index 3 is not a char boundary". Reached from any tags/agenda
match typed by the user or in a custom command.
Fix: `rest[..rest.len() - unit.len_utf8()]` or `rest.strip_suffix(unit)`.

### C5. Medium — Panic archiving to a datetree with an out-of-range CLOSED date
`server/org-core/src/archive.rs:829-830` (`MONTH_NAMES[date.month as usize - 1]`,
`WEEKDAY_NAMES[weekday(day)]`), fed by `timestamp_date` at line 839, which
takes the digits of `[YYYY-MM-DD` without range checks (the parser at
`org-syntax/src/objects.rs:872` accepts any digits).
**Verified**: `#+ARCHIVE: a.org::datetree/` plus
`* DONE x\nCLOSED: [2024-13-01 Mon 10:00]` panics with
"index out of bounds: the len is 12 but the index is 12". Month `00` would
underflow.
Fix: normalise through `Date::from_day(date.to_day())` or validate and fall
back to today's date.

### C6. Medium — Hang on mutually referencing table cells
`server/org-core/src/tblfm.rs:990-1004` and `1024-1036` (`eval_formula`).
References are substituted by the cell's text in a `while let` loop whose
only guard is `replacement.contains(&form)`. Two cells pointing at each
other alternate forever.
**Verified**: `| @2$1 |\n| @1$1 |\n| x |\n#+TBLFM: @3$1=@1$1+1` timed out at
40 s. Emacs has the same check but `C-g` works there; the server cannot be
interrupted.
Fix: cap the substitution iterations and return a spreadsheet error.

### C7. Medium — Unbounded client `count` hangs the server or overflows
`server/org-lsp/src/server.rs` (`Context.count: Option<i64>`, passed through
unclamped); `server/org-core/src/structure.rs:44`;
`server/org-core/src/motion.rs:56`; `agenda/mod.rs:90` (`Span::shift`).
**Verified**: `org.paste.subtree` with `"count": 4000000000` hung for more
than 20 s (the level shifts one step per iteration); a motion command with
`count = i64::MIN` panics on `-count` in a debug build (wraps in release).
`org.copy.subtree`, `META_UP` (`server.rs:10890`, `11279`) and `7 * steps`
in `Span::shift` have the same shape.
Fix: clamp `count` when deserialising (for example to ±10 000) and use
`checked_neg`/saturating arithmetic.

### C8. Medium — Panic on an empty `:ancestor-with-todo` list in super-agenda config
`server/org-core/src/agenda/super_agenda.rs:846` (`&list[1..]` on a possibly
empty list).
**Verified**: group `{ "ancestor-with-todo": [[]] }` panics with "slice index
1 out of range" when any view is built with org-super-agenda on. Only user
config, but it kills the server.
Fix: `list.get(1..).unwrap_or(&[])`.

### C9. Low — Following a long timestamp range renders one agenda block per day
`server/org-core/src/open.rs:437`, `server/org-lsp/src/server.rs:3824`.
**Verified**: `<2026-10-12>--<2126-10-12>` takes 0.8 s and produces 73 051
lines; a range to 9999 would block the server for roughly a minute and push
tens of megabytes to the client. Emacs behaves the same, but there it is
interruptible.
Fix: clamp the span (a year, say) with a message.

### C10. Low — Huge widths and precisions in formats allocate gigabytes
`server/org-core/src/tblfm.rs:1342-1356`, `905-907`; `calc.rs:1025-1029`;
`agenda/item.rs:274`, `280-290`.
`%9999999999d` in a `#+TBLFM:` format, `;f999999999`/`;n999999999` modes, or
a prefix format `%999999999999c` reach `" ".repeat(width)` /
`format!("{:.p$}")` with the raw number. Unverified. Emacs errors on these.
Fix: clamp widths and precisions.

### C11. Low — Integer overflow with absurd repeater or relative values
`server/org-core/src/timestr.rs:125` (`repeater()` saturates to `i64::MAX`,
then `closest_date` does `7 * value` and `n1 + value`);
`read_date.rs:444-449`, `703-712` (`(n - 1) * 7`, `year + n`). Wraps in
release and yields garbage dates; panics in debug. Unverified.
Fix: `checked_*` or reject, as the `w` unit in `read_date.rs` already does.

### C12. Low — Deep recursion on pathological nesting
`server/org-core/src/calc.rs` (expression parser and `write_expr`),
`clocktable.rs` (`Reader::read` on nested lists), `cite.rs`
(`JsonReader::value` on CSL-JSON arrays), `agenda/diary.rs` (`read_form`).
Tens of thousands of nesting levels overflow the stack. The Org parser
itself caps nesting at 64; these readers do not. Unverified.
Fix: a depth limit in each reader.

### C13. Low — `Span::Days(n)` with a huge configured `n` iterates n days per request
`server/org-core/src/agenda/mod.rs:1323`; `config.rs` only checks `days > 0`.
Config-controlled. Fix: clamp.

---

## 2. Security

### S1. Medium — No panic boundary: any handler panic ends the server
`server/org-lsp/src/server.rs:1412` (`run` loop), `main.rs:95-105`.
Requests and notifications run directly on the server thread; the main
function only prints "the server thread panicked". Every panic in sections
1 and 3 therefore ends the editor's Org session.
Fix: wrap each request and notification in
`std::panic::catch_unwind(AssertUnwindSafe(..))`, answer `InternalError`,
and keep looping (an abort from stack overflow, C1, still needs its own fix).

### S2. Medium — Predictable, world-readable temp files; writes follow symlinks
`server/org-lsp/src/server.rs:1747-1766` (`server_directory()`), and the
writers at `7347-7355` (capture buffers), `9687-9688` (special-edit
buffers with the user's source blocks), `10789` (exports), `4438`, `4602`,
`1782` (agenda file), `5544` (date prompt), `server/roam/buffer.rs:113`.
Paths are `$TMPDIR/org-lsp-{kind}-{pid}-{n}`; directories come from
`create_dir_all` with the default umask and files from `fs::write`, which
follows an existing symlink and has no `O_EXCL`. On a shared machine another
user can read the user's notes and exports, pre-create the directory with a
symlink so the server writes over one of the victim's files, or replace a
special-edit file, whose content `check_special_edits` (`9789-9811`) reads
back by mtime and writes into the victim's Org file. Unverified by
execution; the code path is unambiguous. `port.rs:260-281` already does
this correctly (0700 directory, `create_new`, rename).
Fix: one per-server root created with `DirBuilder::mode(0o700)` and a random
component (fail if it exists), or `$XDG_RUNTIME_DIR`; open files with
`create_new(true)` and mode 0600.

### S3. Medium — VS Code: `org-lsp.settings` is workspace-overridable and names the Emacs program
`editors/vscode/package.json:1409-1420`, `2366-2380` (generated from
`editors/vscode/scripts/generate.mjs:370`). `org-lsp.server.path` has
`"scope": "machine-overridable"`, but `org-lsp.settings` has no scope, so a
repository's `.vscode/settings.json` can set `emacs` (the program spawned at
`server/org-lsp/src/emacs.rs:173`), `emacs_config` (Lisp evaluated at
start), `emacs_config_file`, `config_file`, and capture targets of kind
`function`. In a trusted workspace a cloned repo can set
`"org-lsp.settings": {"emacs": "./payload"}`, and the payload runs the first
time Emacs is needed, which happens without any user command: startup
previews run `org.latex.preview` on open, and a file with
`#+STARTUP: latexpreview` plus one `$x$` fragment starts "Emacs"
(`server.rs:10461-10475`). The extension declares no
`capabilities.untrustedWorkspaces`, so Restricted Mode disables it.
Unverified by execution. Zed's `lsp.org-lsp.initialization_options` from a
project's `.zed/settings.json` has the same shape.
Fix: give `org-lsp.settings` `"scope": "machine-overridable"`, or split the
executable-bearing keys out of workspace scope and list them in
`capabilities.untrustedWorkspaces.restrictedConfigurations`.

### S4. High (Windows only) — Vim plugin: command injection when opening a link
`editors/vim/autoload/org_lsp.vim:2109-2117` (`s:open_external`).
Without Vim's `:Open` (the plugin requires only patch 9.0.1485) it runs
`job_start(['cmd', '/c', 'start', '', uri])`. Vim joins the list into one
command line and `cmd.exe` re-parses it, so `&`, `|`, `^` and `%VAR%` in a
link are interpreted: `[[https://example.com/?a=1&calc]]` runs `calc` when
the user follows it, and any ordinary URL with a query string is cut at
`&`. Neovim is not affected (`vim.ui.open` uses `rundll32`). Unverified
(no Windows here).
Fix: use `['rundll32', 'url.dll,FileProtocolHandler', uri]` or
`['explorer', uri]`; never pass document text through `cmd /c start`.

### S5. Medium — Neovim: ImageMagick runs synchronously, unbounded, on every linked image
`lua/org-lsp/images.lua:153-158` (`load`). Every non-PNG image the server
reports (jpeg, gif, tiff, xbm, xpm, pbm/pgm/ppm/pnm, svg, webp per
`org-core/src/link_preview.rs:27`) is converted with
`vim.system({converter, path .. '[0]', png}):wait()` on the main loop with no
timeout, inside the LSP handler, and `#+STARTUP: inlineimages` triggers it
on open. A large or multi-frame file, a decompression bomb, or a slow
delegate freezes Neovim for the whole conversion, and a crafted file in a
shared Org directory exercises ImageMagick's legacy decoders with no policy
or `-limit`. The PNG data is cached forever in `files` (keyed by path and
mtime, never evicted).
Fix: convert asynchronously with a timeout, pass `-limit memory/area/time`,
pin the input coder from the extension (`jpeg:`, `svg:`), bound the cache.

### S6. Low — Tangle writes anywhere the document says, with `:shebang` making it executable
`server/org-core/src/tangle.rs:182-216`, `server/org-lsp/src/server.rs:1981-1996`.
`..`, absolute paths and `~/` are resolved; `:mkdirp yes` creates
directories; `:shebang` sets mode 0755.
**Verified**: `:tangle ../../../../etc/evil.sh :shebang #!/bin/sh :mkdirp yes`
from `/home/u/notes/notes.org` resolves to `/etc/evil.sh` with mode 0755;
`:tangle ~/.bashrc.d/x` lands in the home directory. This is what Emacs
does and it needs the explicit tangle command, so it is listed for
awareness. A safer-than-Emacs option: confirm when the target is outside
the file's directory or workspace, as `org-link-shell-confirm-function` does
for shell links. Also, `write_tangled` skips `set_permissions` when the text
is unchanged, so a changed `:tangle-mode` alone is not applied.

### S7. Low — Server download: no integrity check; release string spliced into the URL
`lua/org-lsp/download.lua:84-86`, `editors/vim/autoload/org_lsp/download.vim:70-72`,
`editors/sublime/LSP-org/plugin.py:148-151`, `editors/zed/src/lib.rs:44-56`.
All four fetch over HTTPS from the hardcoded repository with list-form
commands, so the mechanics are sound. But none verifies a checksum or
signature, the default `nightly` tag is mutable, and in Neovim and Vim the
user's `release` option goes into the URL unvalidated (`../../other/...`
reaches another repository's asset; a value starting with `-` becomes a
`gh release download` flag). Only the user's own config reaches it.
Fix: validate `release` against `^(nightly|v[0-9][\w.-]*)$`; publish and
check `SHA256SUMS` (see also B3).

### S8. Low — Command port: no read timeout; token visible in process arguments
`server/org-lsp/src/port.rs:198` (`receive`) and `244` (the script passes
`TOKEN` on the `org-lsp command` command line). The accept loop is single
threaded and reads with no `set_read_timeout`, so any local process that
connects and sends nothing blocks the port forever, after which no key
command reaches the server. While a command runs, the token is readable by
other local users in `ps` or `/proc/*/cmdline`, and the token is all that
authorises commands. Unverified. Only clients announcing command actions
(Zed) open the port.
Fix: `set_read_timeout`; pass the token through stdin or the environment.

### S9. Low — Release workflow: unverified third-party binaries run with a write token
`.github/workflows/release.yml:123-127`, `.github/workflows/ci.yml:150-152`.
The `zed` job downloads `zed-extension` from a DigitalOcean Spaces URL and
CI downloads `tree-sitter-linux-x64.gz`, and both run them with no checksum.
`release.yml` grants `contents: write` at workflow level, and checkout
persists credentials, so an unverified binary runs with a write-capable
token in `.git/config`. Also, every third-party action is pinned by major
tag (`actions/checkout@v4` and so on), not by commit.
Fix: pin a sha256 for each download, move `contents: write` to the
`publish` job, add `persist-credentials: false` to the build checkouts, pin
actions by SHA.

### S10. Low — `org-site --out` deletes the target directory first
`site/src/main.rs:207` (`Site::write` calls `remove_dir_all(out)`).
`--out .` deletes the checkout. Unverified (deliberately not run).
Fix: refuse a non-empty `out` without the `.nojekyll` marker the tool itself
writes, or delete only what it wrote.

### S11. Low — `install.sh` is not wrapped in a function
`scripts/install.sh:19` (the `curl | bash` form). A truncated download
executes a prefix of the script; `rm -rf "$target"` (lines 302 and 372) can
run before the replacement is unpacked.
Fix: `main() { ... }; main "$@"`.

---

## 3. Data loss and correctness

### D1. Medium — An archive, journal or roam file that is not valid UTF-8 is treated as absent and overwritten
`server/org-lsp/src/server.rs:6660` (`read_to_string(..).ok()`), consumed by
`server/org-core/src/archive.rs:203-211` (`Files::get`: `None` means new
file) and written at `server.rs:6704`. Same pattern in
`server/journal.rs:335`/`377` and `server/roam.rs:444`,
`server/roam/buffer.rs:71`, `roam/capture.rs:130`. Any read error (a
Latin-1 archive from years ago, EACCES, EIO) is conflated with "does not
exist": the server starts an empty buffer, adds the header, and writes it
over the existing file. Elsewhere the server reads with `from_utf8_lossy`
(`workspace.rs:242`, `server.rs:3915`). Unverified by execution.
Fix: distinguish `ErrorKind::NotFound` from other errors and refuse the
command otherwise; read lossily like `workspace::read`.

### D2. Low — Archive, journal and tangle writes are not atomic
`server/org-lsp/src/server.rs:6704`, `1990` (`write_tangled`),
`server/journal.rs:377`. `fs::write` truncates then writes, so a crash
mid-write leaves an empty or partial file. The archive file is also written
before the source edit is requested from the client, so a refused edit
leaves the subtree in both places (Emacs has the same ordering).
Fix: write to a temporary file and `rename`, as `port.rs::write_script` does.

### D3. Medium — Vim plugin: URIs for non-ASCII file names are wrong both ways
`editors/vim/autoload/org_lsp.vim:294-307` (`org_lsp#uri`, `org_lsp#uri_to_path`).
`org_lsp#uri` percent-encodes each character's code point instead of its
UTF-8 bytes: `/tmp/café.org` becomes `file:///tmp/caf%E9.org` and
`/tmp/日記.org` becomes garbage, while the LSP client's own `didOpen` sends
`caf%C3%A9.org`, so every request the plugin sends names a document the
server does not have ("This file is not open"; keys do nothing).
`org_lsp#uri_to_path` decodes `%XX` with `nr2char()`, so the server's URI
becomes `/tmp/cafÃ©.org`, `bufnr()` is -1, and `org_lsp#apply_edit` creates
a new empty buffer under the wrong name and applies the agenda, capture or
refile edit there; with `auto_save` on, `org_lsp#save` writes that buffer
out as a bogus file. **Verified** with `vim -es`. On Windows the function
also encodes backslashes and never produces `file:///C:/...` (unverified).
Fix: encode and decode bytes (`str2list(path, 1)` / `list2str`), normalise
Windows paths, or ask the adapter for the client's own URI of a buffer.

### D4. Low — Neovim: carried edits applied to a buffer that may be gone or changed
`lua/org-lsp/init.lua:740-751` (`apply_carried`) and the `handle` closure at
`258-266`; `images.apply` via `vim.uri_to_bufnr`. `bufnr` is captured when
the command is sent; the asynchronous response applies
`vim.lsp.util.apply_text_edits` with no `nvim_buf_is_valid` or version
check. A wiped buffer errors in the handler; text typed before the next
`didChange` flush makes the edit land at stale positions. Unverified.
Fix: guard with `nvim_buf_is_valid`, and compare
`vim.lsp.util.buf_versions[bufnr]` with the version the server built on
(code-action edits are already versioned; carry `version` in the result).

### D5. Medium — Emacs bridge: synchronous calls block the server; a hung Emacs never recovers
`server/org-lsp/src/server.rs:2655` (`encrypt_before_save` on every
`willSaveWaitUntil` of a file with `:crypt:`, 10 s; the first call also
starts Emacs and loads the user's config), `1224` (`diary-sexp` during
agenda rendering, 30 s per sexp per day), `3963` (`expand-abbrev`, 30 s),
`10591`, `12075` (`attach-expand`/`attach-files`, 60 s each, once per
attachment link when link previews are on at startup).
While blocked, the select loop cannot service `Event::Ask` from Emacs, so if
Emacs asks `y-or-n-p` during a queued job both sides wait for the timeout.
After a timeout the Emacs thread is still inside `request()` for the old
job (`emacs.rs:150-170`, `213-250`), so every later job queues behind it and
also times out; nothing kills or restarts a hung Emacs. Related:
`emacs.rs:233` blocks in `receiver.recv()` forever if the client never
answers `window/showMessageRequest`.
Fix: on timeout kill the child and reset `process`; answer Ask events while
waiting; `recv_timeout` with a default "no".

### D6. Low — Site generator: a missing image aborts the whole run; a broken link is reported twice; unescaped `id`
`site/src/main.rs:223-227`, `500-505` (`resolve` records `no image` but still
copies it, so `write` fails on `std::fs::copy` before any problem is
printed; **verified** with `[[file:nope.png][desc]]`);
`site/src/html.rs:229-249`, `489-495` (a lone link in a paragraph is
resolved twice, so `org-site: 2 broken links` for one; **verified**);
`site/src/html.rs:110`, `main.rs:271-276` (`CUSTOM_ID` goes into `id="…"`
and `href="#…"` without `escape`).

---

## 4. Performance

### P1. Medium — Diagnostics on every keystroke re-walk the workspace and re-read includes
`server/org-lsp/src/server.rs:11679` (`publish_diagnostics`): when the text
contains `id:`, `known_ids()` → `id_paths()` → `agenda_files()` →
`Workspace::files()` re-walks every workspace folder (`workspace.rs:118`, up
to 2000 files, depth 8), stats every file, and rebuilds
`Outline::with_inheritance` for every open document. `lint_diagnostics`
(`11740`) runs org-lint over the whole file with a stat per `file:` link
and a read plus parse of each `#+INCLUDE` target
(`org-core/src/lint.rs:1810-1820`). No debounce, no cache of the listing.
`didSave` also re-renders the whole agenda. Unverified timing.
Fix: cache the workspace listing (invalidate on `didChangeWatchedFiles` or a
short TTL), debounce diagnostics, cache include reads by mtime.

### P2. Medium — Parser: quadratic paths on hostile input
`server/org-syntax`, all verified with release-build timings at N and 4N:
- `parser.rs:556-576` (`find_closing`): the memo of failed searches is keyed
  by closer name and scanned linearly, so many unclosed openers with
  distinct names (`#+begin_b0`, `#+begin_b1`, ...; `\begin{e0}`, ...) or
  many sibling containers each holding one rescan to the section end.
  20 000 lines: 2.4 s (289 KB); control 8 ms for 420 KB of plain text.
  Fix: index the closer lines once per section and binary-search.
- `parser.rs:843-855`: an orphaned run of affiliated keywords is rescanned
  from each line. `"#+NAME: x\n"` × 20 000: 3.2 s.
  Fix: remember the end of the orphaned run.
- `objects.rs:882-903` (`first_closer`) and `907-916` (diary timestamps):
  unclosed `<2024-01-01 ` repeated on one line rescans to the line end for
  each. 20 000 repeats: 1.9 s. Fix: a memo like `absent`.
- `parser.rs:745`: three `find`s per chunk on a planning line; many
  `SCHEDULED:` on one line: 2.6 s at 20 000.
- `config.rs:124-126`, `176-178`: `#+TODO:` with k words costs O(k²) to
  build and O(k) per headline to look up. Fix: a `HashMap`.
No panics were found in the parser: 300 000 random documents with
multi-byte, CRLF and lone-CR snippets ran clean, and nesting is capped at 64.

### P3. Medium — `org-indent-region` re-parses the whole document per changed line
`server/org-core/src/indent.rs:868-874`, `934-937`. `Pass::snapshot()`
rebuilds `Snapshot::new(self.lines.join("\n"))` whenever any line changed,
`Pass::found` is called per line, and each call recomputes
`split('\n').take(line).sum()`. Indenting N lines in a document of M lines is
O(N·M). Reached from promote/demote with `org-adapt-indentation`, clock-in
drawers and `indent_element`. Unverified timing.

### P4. Medium — `Elements::at` is linear and lint calls it per regex hit
`server/org-core/src/indent.rs:534-560`; `lint.rs:218-225`, `816-871`,
`873-910`. `Snapshot::element_at` scans the roots linearly; six lint
checkers call it per match, and lint runs on every edit. A file with
thousands of headlines each with a drawer or planning line is
O(matches × roots). Fix: `partition_point` on `begin`.

### P5. Medium — Quadratic BibTeX parsing
`server/org-core/src/cite.rs:682-719`. `Bib::entries` calls `at_lines()`
(a full scan) inside its inner loop every time `valid_entry` fails
(`@comment`, `@preamble`, unknown types, malformed entries). Fix: compute it
once and binary-search.

### P6. Low — Per-request directory walks and uncached reads in roam and journal
`server/org-lsp/src/server/roam.rs:84-100`, `243-279` (every roam command
recursively lists the roam directory and stats each file; parsing is cached
by mtime and length, the listing is not); `server/journal.rs:320-328`
(`entry_days` reads every journal file on every journal command, no cache).
`org-core/src/journal.rs:414`, `681` and `journal/carryover.rs:224` build a
`Regex` per call; so do `lint.rs:240-262` (`duplicate_name`, per named
element) and `sort.rs:296-303`, `354`, `379` (per record).

### P7. Low — `workspace/symbol` and backlinks re-parse every known file per call
`server/org-lsp/src/server.rs:3677` (`workspace_symbols`), `3534`
(`backlinks`): `known_files()` clones every file's text and
`org_syntax::parse` runs on each, ignoring the cached parse from
`Workspace::read`. VS Code sends `workspace/symbol` per keystroke of the
picker. Also `folding.rs:154` is O(headlines × folds) per open.

### P8. Low — Neovim: whole-buffer work on hot events
- `lua/org-lsp/src.lua:276-305`: 100 ms after any keystroke anywhere,
  `M.highlight` clears the namespace, rescans all lines, and re-parses every
  `#+begin_src` block with a fresh tree-sitter string parser (or `synID()`
  per column for the syntax fallback). Fix: only blocks intersecting the
  changed range; reuse parsers.
- `lua/org-lsp/images.lua:296-308`, `208-275`: `render` runs on every
  `CursorMoved` and does several API calls per image per window; with
  hundreds of previews that is thousands of calls per cursor move.
  `measured_cell()` re-runs a synchronous `ps` on macOS after every
  `VimResized`. Fix: render on `WinScrolled`/`TextChanged`/`BufWinEnter`.
- Synchronous requests on keys: `init.lua:727` (1 s on every insert-mode
  `<CR>`), `indent.lua:36` (500 ms per `o`/`O`/Enter via `indentexpr`),
  `folds.lua:16` (1 s per `<Tab>`), `view.lua:538` (2 s on agenda load);
  Vim mirrors this (`org_lsp.vim:248-262` polls with `sleep 10m`). While
  the server is busy (agenda rebuild, a bridge call to Emacs) Enter and Tab
  block for the timeout. Deliberate, but a shorter timeout with the native
  key as fallback would keep typing responsive.

### P9. Low — Site generator panics on non-ASCII whitespace in an indented block
`site/src/html.rs:610` (`block_contents`). `indent` is the minimum leading
byte count over non-empty lines; whitespace-only lines (`trim()` treats
U+00A0 as whitespace) are skipped for that minimum but still sliced
`&line[indent..]`. **Verified**: a `#+begin_src` indented by one space with
a line holding a single NBSP aborts `org-site` with "start byte index 1 is
not a char boundary", which takes down `check.sh`, `site.sh` and the docs
workflow with a panic instead of a message.

---

## 5. Build and release

### B1. Medium — A tagged release is published even when a platform's build failed
`.github/workflows/release.yml:168-191` (`publish` runs with
`if: !cancelled() && (...)` after a `fail-fast: false` matrix). For a `v*`
tag it then creates a final, non-draft release missing one or more
platforms, with `SHA256SUMS` listing only what is there; `install.sh` then
fails with "Could not download" on the missing archive. Unverified at
runtime. Fix: require every build job to succeed for tags, or publish as a
draft when any failed.

### B2. Medium — Nightly can serve a stale binary that passes verification silently
`.github/workflows/release.yml:187-191`, `scripts/install.sh:109-126`.
`gh release upload nightly dist/* --clobber` replaces only the files that
were built; a platform that failed keeps its previous archive while
`SHA256SUMS` is replaced with a list that no longer names it. `install.sh`
then downloads the stale archive, finds no line for it, and silently skips
verification (line 116). The Neovim and Vim downloaders never check
`SHA256SUMS` at all. Fix: delete the nightly assets before uploading, and
make `install.sh` fail when `SHA256SUMS` exists but has no entry.

### B3. Low — Emacs reference scripts and fuzzers have no timeout and never run in CI
`scripts/fuzz-*.py`, `scripts/*_reference.py` (except cite and journal):
`subprocess.run` of Emacs with no `timeout`, so a batch-mode prompt hangs a
run forever; CI runs only by `workflow_dispatch` and never calls the
fuzzers, so "checked by differential fuzzing" is a manual-only guarantee.

---

## What was checked and found sound

- Parser: all `&str` slicing on ASCII or `char_indices` boundaries; every
  `unwrap` preceded by its predicate; loops advance; element and object
  nesting capped at 64; lossless `split_lines` with CRLF and lone CR; the
  `kind_from_raw` transmute is bounds-checked.
- LSP: position encoding negotiated (utf-8, utf-16, utf-32) and converted
  through `LineIndex` with clamping (surrogate-middle rounds up, reversed
  ranges clamp); incremental and full `didChange` applied in order;
  versions checked before Emacs results are applied; malformed params give
  `InvalidParams`, unknown methods `MethodNotFound`; every request gets a
  response (probed for every handler and manifest command); non-`file:` URIs
  rejected without panicking; Windows drive letters handled.
- Emacs bridge: everything crosses as JSON, `bridge.el` never builds Lisp from
  strings, only allowlisted commands run, `read-*`/`y-or-n-p` are advised,
  `enable-local-variables :safe`, Emacs started with `--batch -Q` and an argv
  array. `%(sexp)` in capture templates comes only from the user's config.
- Links: `shell:` and `elisp:` links are refused ("Links of type ... are not
  supported"), never executed; external URLs go to the client through
  `window/showDocument`; the server downloads nothing.
- Refile never writes to disk itself (a `WorkspaceEdit` with `version: None`
  for closed files); archive and refile use the open document's in-memory
  text, so unsaved edits are respected; tangle refuses tangling into the
  Org file itself.
- Workspace walk bounded (2000 files, depth 8, hidden/`node_modules`/`target`
  skipped); reads cached by mtime and length; line diff capped at 4M cells;
  semantic tokens avoid quadratic column counting.
- `cargo audit` on both lockfiles: no advisories (chrono 0.4.45, regex
  1.13.1, lsp-server 0.10.0, lsp-types 0.97.0, serde_json 1.0.151).
  `npm audit` in `editors/vscode`: 0 vulnerabilities. Workspace builds with
  no warnings; clippy clean on `org-syntax`.
- CI: no `pull_request_target`, no `${{ github.event.* }}` in `run:` steps,
  no secrets to forks; docs publish only from `main` with `--check`; release
  gated to tag pushes or `main` with `--verify-tag`; all shell scripts use
  `set -euo pipefail` and quote their variables; `check.sh` detects stale
  and untracked generated files.
- Editors: Neovim and Vim spawn only argv lists, augroups are per buffer and
  cleared, offset encoding passed consistently; VS Code uses no
  `child_process` or webviews, disposables are registered, decorations are
  debounced; Sublime escapes HTML and extracts a single named tar member;
  Zed uses the hardcoded repository and sanitised release names.
- Site generator escapes `& < > "` everywhere else, normalises `file:` links
  so image copying cannot escape the checkout, and the real docs build with
  no broken links.
- Date and time code: `to_day`/`from_day`, ISO weeks, `+0d` in
  `closest_date` (returns the base date), `date_ordinal_to_time` rejects
  month 13, clock sums and ranges, quarter arithmetic.
