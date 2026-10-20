# Audit findings, round six

A full review of org-lsp after the 127 commits that followed the fifth
round (bcc77f5 to df7bcbc: #229 to #355, 138 000 lines). Earlier rounds
are in this file's git history. Nine reviewers covered the parser, a
security pass on the new external surfaces, the two halves of the Org
logic, the server crate, the editor plugins, scripts, CI, site and
dependencies, and the two parity checks (Org features and per-editor
claims). `check.sh` passes on the merged tree.

"Verified" means the trigger was run here; "unverified" means established
by reading. No Emacs is installed. Copies of Org 9.8.10's `org.el`,
`org-agenda.el` and `org-table.el` were available to two reviewers, who
checked claims touching those files against the source; everything else
about Emacs is from memory and the recorded cases under `tests/`.

Deliberately left as they are, per the maintainer (unchanged list):
diagnostics debounce, caching the workspace file list, Sublime
file-and-position paths, coc.nvim on Windows and the Neovim edit version
check, roam finalize and jump-to-captured after a refiled capture, the
encryption docs, tangle paths, the synchronous Enter/o/Tab waits, and the
`zed-extension` hash. Still open from round five, not re-examined here:
Neovim image conversion on the main loop (S5), and mixed line endings in
one file putting `\r` into a table cell.

---

## 1. Performance

### P1. High — Building the outline is quadratic in document size since inline tasks (#319)
`server/org-core/src/inlinetask.rs:62-67` (`min_stars_in` → `structure::odd_levels_in` → `text::startup_words`, a full scan of the text allocating a lowercased prefix per line), called by `is_task_line` (`:111`), `is_end_line` (`:105`) and `is_task_level` (`:67`) on every invocation, and run even when inline tasks are off because `min_stars(odd)` takes the scan's result eagerly. `server/org-core/src/outline.rs:518-524` calls these three to four times per headline, so `Outline::new`, which runs on every document change and per agenda file, is O(headlines × bytes).
**Verified** by two reviewers independently: `* a\nb\n` × 4000 (24 KB) takes 873 ms in release and 43 s in debug for 3000 headlines over 24 000 lines; agenda views on the same outline take under 0.6 s, so the outline is the cliff.
Fix: compute `min_stars_in(text)` once per text (the private `Tasks { text, min }` already does) and pass the number to per-line tests; make the odd-levels scan lazy; take `odd_levels` from the outline, which already has it.

### P2. High — `inlinetask::hides_stars` is O(lines²)
`server/org-core/src/inlinetask.rs:39-41`: `text.split('\n').any(|line| is_task_line(text, line) || is_end_line(text, line))`, each call rescanning the whole text as in P1, and never short-circuiting when the file has no inline task (the common case). Called from `server.rs:15253` (`publish_virtual_indent`) after every change for clients with virtual indent.
**Verified**: 4000 lines 1.56 s, 16 000 lines 26 s in release; 24 000 lines 334 s in debug.
Fix: compute `min_stars_in` once, return `false` at once when it is `None`.

### P3. Medium — Remote image downloads run on the main loop with no size cap
`server/org-lsp/src/server/remote_images.rs:150-187` (`download_image`): `curl --max-time 30` runs synchronously, once per distinct URL, serially, inside `org.link.preview` (`server.rs:13552`) and at file open with `org_startup_with_link_previews`; no `--max-filesize`, no content-type check; in `download` mode every preview re-downloads into a new numbered file and old ones are never removed. A file with N unreachable images stalls every request for up to 30 s × N. Consent is as in Emacs (`prompt` by default, asked once per URL; startup never downloads nor asks). TLS verification is curl's default, protocols limited to http and https including redirects. Verified by reading.
Fix: a worker thread with the result delivered as an event, as LaTeX previews do; `--max-filesize`; remove the previous file of the same URL.

### P4. Medium — Duration parsing compiles three or four regexes per call
`server/org-core/src/duration.rs:232-277` (`to_minutes_with`, `is_duration`): `Regex::new` for `one`, `full`, `mixed` and `bare` on every call, and `current()` clones the unit list. Reached per agenda line from sorting keys (`sorting.rs:352`), effort filters, super-agenda, per field of a column sum (`table.rs:1344`), clock tables. **Verified**: about 160 µs per call in debug, so an agenda of a few thousand lines spends hundreds of milliseconds here.
Fix: cache the compiled regexes keyed by the unit list beside the thread-local `CURRENT`.

### P5. Medium — Following an `id:` link parses every known file until the hit
`server/org-lsp/src/server.rs:5724` (`find_id`) and `:5594` (`Open::Id`): `id_paths()` walks the roam directory, re-reads `org-id-locations-file` and globs `org_id_extra_files`, then every path is read from disk and fully parsed in order until the ID is found, on every `C-c C-o`, hover and definition. `ids_on_disk()` keeps `self.id_files: path → (mtime, ids)` but `find_id` never consults it. Verified by reading.
Fix: look the ID up in the cached map to get the one path, refresh on a miss, parse only that file.

### P6. Medium — Parser: quadratic scans on three new object kinds
`server/org-syntax/src/objects.rs`, all verified at N vs 4N in release:
- `:733-738` (`match_citation`): after `balanced_end`, each `[cite:` opener rescans its body for a `@key` with `match_indices`; nested unclosed openers: 448 KB 0.97 s, 1.8 MB 14.7 s.
- `:656-660` (`inline_name_len`) from `match_inline_src`/`match_inline_call`: every `src_`/`call_` at a word start scans to the next blank; `(src_` × n: 80 KB 409 ms.
- `:948` (`match_diary_timestamp`): `closes_sexp` scans to the memoised closer for every `<%%(` before the cheap `>` test; 64 KB 261 ms.
Fix: memoise as `absent`/`scan_to` do (the first position from which no valid key or blank exists), and test `bytes[close] == b'>'` first.

### P7. Medium — The site's search index is 557 KB and loads on every page
`site/src/main.rs:308` (`<script defer src="assets/search.js">`), `site/src/search.rs` (`TEXT_LIMIT` 4000 per section). 224 KB of it comes from the 432 option sections of `configuration.html`. **Verified** by building.
Fix: load the script when the search dialog first opens, or shrink option entries and split the index.

### P8. Low — Subprocesses without a deadline on the main loop
`server.rs:16395` (`shell_output`, reads to EOF with no timeout) runs `ctags -R` over the file's directory when an internal link's tag is missing with org-ctags on (`server/manual.rs:146`) and for `org.ctags.create`; `man`/`info` (`manual.rs:36`) and Graphviz `dot` for `org.roam.graph` (`roam/graph.rs:58`) likewise. Emacs blocks there too. Fix: wait with a deadline, as `emacs.rs` does.

### P9. Low — Neovim per-redraw costs
- `lua/org-lsp/preview.lua:224-245`: the global `BufWinEnter` autocmd dumps all window options (`nvim_get_all_options_info`, about 400 entries) for every float of every plugin, not only Neovim's hover. Fix: test the buffer first (`buftype` and the ```` ```org ```` fence).
- `lua/org-lsp/folds.lua:458-528`: `foldtext` calls `vim.lsp.semantic_tokens.get_at_pos` once per character of a closed fold's headline, re-evaluated on every redraw; the function already fetches all marks of the row. Fix: derive the groups from those marks.

---

## 2. Security

Sound and gated as Emacs gates them, verified over LSP: `shell:` links ask `Execute … in shell?` and run only on an explicit `y` (a missing or wrong answer aborts, `org_link_shell_skip_confirm_regexp` honoured, hover and startup never follow); `elisp:` links are refused; org-protocol listens on a 0600 Unix socket in a 0700 directory with bounded reads, nothing from a URL is executed, and captures go through template validation; the command port accepts no `input`/`step`, so it cannot answer a confirmation; `org-id-locations-file` is read only; tangle and BibTeX writes are atomic beside the target; the Windows unpack is argv to the system `tar` or PowerShell; the Zed and Helix runners use argv lists; the Emacs bridge's `run` allowlist gained only the two plot functions; `cargo audit` and `npm audit` are clean. Remaining findings:

### S1. Medium — `man:` link words reach `man` as options
`server/org-core/src/manual.rs:23-43` (`man_arguments`) splits the link's page on spaces and `server/org-lsp/src/server/manual.rs:36-47` runs `man` with those words as argv, nothing before them. **Verified**: `[[man:-l /etc/hostname]]` arrives as `-l /etc/hostname` and shows any readable file as a man page; `[[man:-Hxterm ls]]` makes man-db run `xterm` as the browser; `-C file` picks another config that can define `pager`/`browser`. Needs `C-c C-o` on the link, or `RET` with `org-return-follows-link`; no confirmation, as in Emacs, whose `Man-translate-references` puts an unmatched reference into a shell string, so Emacs is at least as exposed. The argv here still lets a document choose man's options.
Fix: pass `--` before the page words, or refuse words starting with `-`. `info --file FILE` is safe already.

### S2. Medium (Windows only, unverified) — `org-file-apps` commands are quoted for POSIX but run through `cmd /C`
`server/org-core/src/file_apps.rs:211-226` (`shell_quote`) backslash-escapes as `shell-quote-argument` does on POSIX; `server.rs:16370-16380` (`run_shell`) runs the result with `cmd /C` on Windows, where backslashes quote nothing. A linked file named `x&calc.pdf` with a settings entry `pdf → "viewer %s"` runs `calc` when followed; `shell_output` (`:16395`, org-ctags) has the same shape. On Linux this is sound (verified: a file named ``x;touch$(echo _)INJ`id`.pdf`` reached the viewer as one literal argument).
Fix: on Windows quote as Emacs's w32 `shell-quote-argument` does, or run the viewer with argv when the command is a plain program followed by `%s`.

### S3. Low — org-protocol `open-source` follows `..` out of the project directory
`server/org-core/src/protocol.rs:427-447` collapses literal `..` as emacsclient does, but `link_decode` runs afterwards (`parse_query`, `:294-305`), so `url=https%3A%2F%2Fexample.com%2F%2e%2e%2Fsecret.txt` becomes `WORKDIR/../secret.txt` (`map_source`, `:481-515`) and `server.rs:6452-6470` sends `window/showDocument` for it. **Verified**: the editor was told to open `site/../secret.txt`. Any page can navigate to `org-protocol://` once the browser always opens the scheme, so with `org_protocol_project_alist` configured a page can open any readable file in the editor; nothing is read back, and `org-protocol.el` behaves the same.
Fix, as a deliberate difference worth writing down: canonicalise and refuse a file outside the project's working directory.

### S4. Low — `org-clock-persist` file written non-atomically with default permissions
`server/org-lsp/src/server/clock_persist.rs:243-246` (`clock_save`): `create_dir_all` plus `fs::write` of `org-clock-save.el`, umask permissions, no temp-and-rename, so a crash mid-write leaves Emacs a truncated file to `load`. Reading is a hand-written scanner with no evaluation, and file names are written with `"` and `\` escaped, so org-lsp cannot inject Lisp into the file Emacs loads. Fix: write through `replace_whole`.

### S5. Low — The org-protocol listener fails quietly when the state path is long
`server/org-lsp/src/protocol.rs:134`: `UnixListener::bind` fails with "path must be shorter than SUN_LEN" for a long `XDG_STATE_HOME` (**verified** with the scratchpad path); `server.rs:6364-6367` prints to stderr only, and `org-lsp protocol` then finds no entry. Fix: fall back to a short socket in a 0700 `/tmp` directory, or show a message once.

### S6. Low — `install.sh` silently skips checksum verification on a transient failure
`scripts/install.sh:70-74` (`fetch`): the release file is fetched with `--retry 2` but `SHA256SUMS` once without retry; any failure writes `SHA256SUMS.none` and nothing is verified or said for the rest of the run. Also `:~495`: a Helix checksum mismatch, since `fetch` runs in a subshell, becomes "The release has no colors for Helix". Verified by reading. Fix: retry, treat only a 404 as "no sums", say when verification is skipped, and let a mismatch fail.

Informational: `shell:` link output goes into one `window/showMessage` with no cap (`server.rs:5482-5500`); Emacs puts long output in a buffer.

---

## 3. Correctness

### C1. Low — Debug-build overflow panics on document and formula content (release wraps)
- `server/org-core/src/todo.rs:1617`: the repeater count is folded with `n * 10 + digit` into i64; `<2026-01-01 Thu ++99999999999999999999d>` then marking DONE panics "attempt to multiply with overflow" (**verified**). Emacs reads a float. Fix: `checked_mul`/`checked_add` or saturate.
- `server/org-core/src/calc.rs:624-629` → `date.rs:71` (`from_day`, `day - 1`): any one-argument call converts the number to a day before the function name is matched, so `year(-1e30)`, `weekday(-1e300)`, `trunc(-1e40)`, `abs(-1.7e38)` in a `#+TBLFM:` panic in debug (**verified**) and return garbage in release for the date functions where Emacs errors. Fix: build the civil date only for the date-part functions and bound the day.
- `calc.rs:668`: `(root.round() as i128).pow(2)` overflows for `sqrt(170141183460469231731687303715884105727)` (**verified**). Fix: `checked_pow`.
- `server/org-lsp/src/server/roam/dailies.rs:319-326` (`roam_dailies_step`): `-n` and `position + n` with a client `count` of `i64::MIN` (unverified by execution). Fix: checked arithmetic.

### C2. Low — Messages to the client before `initialized`
`server.rs:1943-1948` sends `org-lsp/clock`, `window/showMessage` for option problems and, with `org_clock_persist` and `query_resume`, the `window/showMessageRequest` "Resume clock (…)?" (`clock_persist.rs:160`) right after `initialize`, before the client's `initialized`. Clients that install handlers on `initialized` drop them, and an unanswered request leaves `Pending::ResumeClock` dangling. Verified by reading. Fix: queue until `initialized`.

### C3. Low — `org-roam-extract-new-file-path` is not filled as a capture template
`server/org-core/src/roam/edit.rs:321-345` (`extract_file_name`) expands only `%<…>`, `${title}`, `${slug}` and `${id}`; org-roam runs `org-capture-fill-template` first, so `%t`, `%U`, `%n`, `%^{prompt}` expand too. Verified on the org-lsp side. Fix: run `capture::fill_counting` before the `${…}` pass, as `roam/capture.rs::fill_template` does.

### C4. Low (unverified) — Smaller divergences
- `server/org-core/src/open.rs:917-934` (`search_with`) with `org-link-search-must-match-exact-headline = nil`: the headline match runs before the text search; in `org-link-search` the headline branch is guarded by `(or starred headline-search)`, so with the option nil and no `*` only the text search from the start runs. Differs only when the words also occur before the headline.
- `server/org-core/src/roam/graph.rs:91-118`: a `cite:` link is an edge to the bare key; org-roam's SQL joins it with `refs`, so it reaches a node only through a `ROAM_REFS` entry.
- `server/org-core/src/agenda/sorting.rs:521-650`: Emacs's timsort is reproduced without galloping; since `org-entries-lessp` is not consistent under `category-keep`, views of 64 or more lines can order equal-time mixed-category lines differently in rare patterns.
- `server/org-core/src/ctags.rs:66-83` (`capitalize`): `'` breaks a word (`It'S`); Emacs keeps the apostrophe inside a word, affecting `%t` in the ctags topic template.
- `sorting.rs` `cookie`: `[3/]` yields 3; Emacs's empty denominator gives 0.
- `server.rs` (`fit_outline_path`): width counted in characters, Emacs in display columns, so CJK paths overflow the echo area.
- `server/org-syntax/src/objects.rs:850` (`plain_link_end`): a non-char-boundary offset panics in `prev_char`; every current caller guards it. Fix: `is_char_boundary` check or `text.get(..i)`.

### C5. Low — Editor items
- `lua/org-lsp/view.lua:992-1000` and Vim's `s:echo_path`: the outline path from the async `agendaCursor` answer is echoed whatever mode or buffer the user is in by then (over the command line, or another buffer). Fix: check the current buffer and normal mode in the callback.
- `editors/vscode/src/extension.ts:661-669`: `sendRequest('org-lsp/agendaCursor')` has no `.catch`, so a restart surfaces an unhandled rejection. `:117-136`: `commandsRun` grows without bound (kept for the tests).
- `editors/vscode/test/run.mjs`: two `mkdtemp` directories per scenario run, never removed (dozens per full run, each with VS Code state). `editors/zed/test/run.py:542-546`: `zed.log` handle never closed. `editors/vim/test/run.sh:35-37`: unquoted paths in the string tmux runs, so a checkout with a space fails.
- `server/org-lsp/tests/e2e_scenarios.rs:864`: `assert!(ran > 0 || only.is_some())` lets `ORG_LSP_E2E=typo` pass with nothing run. Otherwise the coverage gate is sound: a missing snapshot fails, stale exceptions fail, and each editor runner fails a step whose command has no key unless excused.
- `scripts/inlinetask_reference.py:102`, `num_reference.py:120`, `startup_align_reference.py:59`: Emacs run without `timeout=`, unlike the other reference scripts. `scripts/highlight-screenshots.py:197` hard-codes `/root/zed/org.wasm` as a default.

---

## 4. Parity: do the docs say what the code does?

### 4a. Org features and options
All 31 areas and 370 rows of `docs/parity.org` were read, every option in a ✅ row has a setting, the 12 "done another way" notes hold (four of them checked against the Org source), the options table's numbers match `scripts/org-options.json` exactly, all 225 new settings trace to a consumer (one, `org_agenda_add_entry_text_descriptive_links`, is read nowhere, and its own doc string says truthfully why), all 313 manifest commands reach a handler, and every command and key the docs cite exists. 26 claims in `plugins.org` hold in code. Discrepancies:

1. **`shell:` prompt lacks Emacs's quotes.** `server.rs:5351` formats `Execute {command} in shell? `; `org-link--open-shell` uses `%S`, so Emacs shows `Execute "echo hi" in shell? `. The test at `agenda_lsp.rs:2623` pins the unquoted form. From memory of `ol.el`, fairly sure.
2. **The remote-resource prompt is shorter than Org's when the file is long.** `server/org-core/src/remote_resource.rs:99`: `Please type y, n{}, d, or !: `; `org.el:4992-4997` adds `, or C-v/M-v to scroll` when the buffer is taller than the window. Verified against source; window-bound, minor.
3. **`Resume clock (…)?`** (`clock_persist.rs:158`): the recollection of `org-clock-load` is `Resume clock (%s) ` with no `?`. Doubtful, unverified.
4. **`org-fold-show-context-detail`** is triaged `emacs-only` in `org-options.json` (keeping the options table at 100%), but `parity.org:94` lists it under its pre-9.6 name `org-show-context-detail` as a cycling option org-lsp lacks, and the server does reveal context after agenda jumps itself. Either the row or the triage is wrong; the row's name is stale either way.
5. **`org-table-auto-blank-field`** is counted as "done another way" (`parity.org:822`: TAB leaves the cursor at the field's start and the first character typed does not blank it), but `org.el:17030-17036` does blank the field after `org-cycle`/`org-return`, so the behaviour is not done. Verified against source; a judgment call on how to count it.

### 4b. Per-editor claims
192 cells (32 areas × 6 editors) were examined; 185 are accurate; every capability each editor announces matches its "needs the capability" rows; the six key tables regenerate clean; the five scenario adapters read `tests/e2e/exceptions.json` and no editor is marked covered for a scenario it skips; 20 of 20 protocol shapes match the code. Discrepancies:

1. **Helix's TESTING.org names code actions the server never offers Helix.** `scripts/generate-docs.mjs:787` renders every check for Zed and Helix as "the code action /TITLE/", but Helix only gets `actions.rs:23-37` plus the structure, table, tangle, archive, todo, refile, schedule, agenda and capture actions of `server.rs:4440-4530`. Checks with no such action: `editors/helix/TESTING.org:52` (inline task insert), `:84` (todo next/previous set), `:104-105` (shift on a property line, property delete), `:107`, `:116` (schedule and deadline remove), `:142` (`agenda.appt`, which has no key in any preset), `:154`, `:156`, `:201`, `:212`, `:234`, `:242`, `:246`, `:248`, `:256`, `:288`, `:317-318` (crypt); the five `org.open` checks say "code action" where the route is `gd`. Verified.
2. **Helix ✅ for Table formulas and Encryption** (`docs/helix.org:266`, `:276`): `table.recalculate.line`, `iterate`, `formula.debugger` and the four `crypt` commands are neither code actions nor keys in Helix. Should be `part`/`no` with a reason.
3. **Zed's TESTING.org** uses the same wording for all 184 checks, while nearly all run from `keymap.json` tasks generated only for commands with an Emacs-preset key. Commands with no Emacs key are reachable by no route in Zed yet listed, and their areas marked ✅: `docs/zed.org:111` Refile and archive (`archive.done`), `:115` Table formulas (`table.iterate`), `:116` Babel (`babel.detangle`), `:125` Encryption; and in `part` areas without a word: property delete, schedule/deadline remove, `agenda.appt`, `id.create`, `num.mode`, `todo.yesterday`, the bibtex commands, several roam and journal commands. Verified against `keymap.json` and the manifest.
4. `docs/vim.org:6` says Vim 9.1; `editors/vim/plugin/org_lsp.vim:3` accepts 9.0.1485. `docs/sublime.org`'s hand-written text says wrapped lines are not indented while the generated table on the same page says they are indented by the line's own leading whitespace; unverifiable here.

---

## 5. Checked and found sound

- Parser: 2.6 million random documents (CRLF, lone CR, NUL, BOM, ZWJ, emoji, combining marks, CJK, every new construct, four list syntaxes, user entities shadowing built-ins): no panic, every tree lossless, ranges nested; linear at N vs 4N for every element and object kind except P6; lettered items match `org-item-re` (single letter, terminator honoured); inline tasks follow `org-inlinetask.el` line by line; one documented difference: `org-list-allow-alphabetical` also affects where paragraphs end, which Emacs fixes at load time.
- Server: all 315 command ids × 21 argument shapes (none, wrong types, positions past the line and file, negative, unknown documents, garbage input fields, 4-byte characters, CRLF) under UTF-8 and UTF-16 clients, plus every `org-lsp/*` method with wrong-shaped params and the standard requests at wild positions: no `InternalError`, clean shutdown; new options at 0, negative, huge and wrong-typed values are clamped or dropped one at a time. `guarded` wraps requests, notifications, responses, org-protocol, the port, Emacs events and every timer. All 20 protocol methods match the docs; every `initialize` capability has a handler; every `WorkspaceEdit` carries a version except created files; Emacs calls on the loop have deadlines and exports run asynchronously; clock persistence writes only at exit; ids are read with a freshness window; `docs/config.toml` matches all 399 defaults and the schema, verified by mutating one.
- Org logic: agenda sorting strategies and limits, todo-ignore, other-date handling, time display, inherited tags, priorities, log notes, clock report parameters, deadline faces, persistent filters, columns, agenda date, effective time, extend-today-until and odd levels match the 9.8 source and 82 recorded cases, and views of 6000 entries build in under 0.6 s; clock ranges with malformed keys, durations with NaN and huge values, repeaters at the 100 000 cap, lists at every cursor position, 45 malformed `#+TBLFM:` forms in three modes, huge numbers, level conversion of 2000 headlines in 11 ms, num mode, inline task edits, BibTeX on 20 000 `@`s and 5000 nested braces, odd-level targets, custom properties, ORDERED tags: no panics or hangs beyond C1. Roam refile, extract, promote and demote are recorded (97 cases) and edit open text through versioned edits with the source deleted only after the target edit; dailies, graph distances, journal calendar (110 cases), protocol parsing (93 cases), file-apps (8856-line recorded set), IDs by every method, sparse-tree case folding and date kinds, display options, line keys, entities: sound.
- Editors: no plugin runs a program with document text, downloads images or registers a URL scheme; Neovim's one-server change, sparse highlights, looks, hidden lines, bullets, agenda text diffing, folds retry, hover float conversion, wrap prefix, ellipsis; Vim's agenda reload, text edits, text properties, popup; VS Code's habit decorations, level colors, theme contributions (all 17 colors have the four theme kinds), journal folds; Sublime's hidden lines and run command; Zed's generator, tasks, runner and proxy; Helix's queries against the grammar's node types and its runner.
- Scripts, CI, dependencies: every action pinned by SHA including the new `protocol.yml`, minimal permissions, no `pull_request_target`, tree-sitter and zed-extension checksummed, nightly assets pruned; `cargo audit` and `npm audit` clean; the only new crate is `regex` for org-lsp; no TLS crate (remote images use system curl); all shell scripts `set -euo pipefail` with quoted paths; `install.sh`'s `main` wrapper intact; `test-all.sh` propagates every runner's failure; reference scripts stub Emacs's process launchers. The site builds 21 pages with no broken links, 471 unique ids on the configuration page, every option name escaped, 394 TOML blocks highlighted.
