# Audit findings, round three

A third review of org-lsp after the second round's fixes landed on `main`
(PRs #221 to #225). This file replaces the earlier rounds; their text is in
this file's git history. Parts:

1. the status of every round-two finding,
2. regressions from the round-two fixes,
3. new findings, in areas no round had examined for correctness before:
   the editing commands, agenda rendering, the standard LSP features, and
   files outside the workspace,
4. what was checked and found sound.

"Verified" means the trigger was run here; "unverified" means established
by reading. No Emacs or Org sources are on this machine, so "what Emacs
does" comes from the reviewers' knowledge of Org 9.8 and from the recorded
cases under `tests/`. `check.sh` passes on the merged tree; one test
(`offers_commands_as_actions_and_asks_with_buttons`) failed once under
heavy load and passes alone, see 3e.

Deliberately left as they are, per the maintainer: diagnostics debounce,
caching the workspace file list, Sublime file-and-position paths, coc.nvim
on Windows and the Neovim edit version check, roam finalize and
jump-to-captured after a refiled capture, the encryption docs, tangle
paths, the synchronous Enter/o/Tab waits, and the `zed-extension` hash
(still 403 from here).

---

## 1. Status of the round-two findings

| ID | Finding | Status |
| --- | --- | --- |
| R1 | Emacs killed by short calls during a slow start | Fixed, verified with a stand-in Emacs: short calls after a slow start succeed, a hung call is killed and the next call restarts Emacs, parallel calls during start-up share one Emacs |
| R2 | formula cycle cap costs 1.6 s | Fixed, verified: legitimate repeated references all evaluate, real cycles refused in milliseconds |
| R3 | `catch_unwind` leaves a document inconsistent | Fixed. Note `Document::apply` now clones the whole text and `LineIndex` per `didChange` (`document.rs:71-101`); fine for normal files, a copy per keystroke on a 10 MB one |
| R4 | `write_whole` replaces a symlink | Fixed for live symlinks and symlinked parents. A dangling symlink still becomes a regular file (`canonicalize` fails), and a hard link is still broken by the rename |
| R5 | silent "no" to Emacs questions | Fixed ("prompt (y or n) n" messages) |
| R6 | temp directories created eagerly | Fixed for five of six; `view_directory` (`server.rs:1234`) is still made at start |
| R7 | port read timeout per read | Fixed (whole-line deadline) |
| R8 | `src.lua` duplicate blocks | Fixed; the undo-within-debounce case was not addressed (not verifiable here) |
| N1 | capture clocks the wrong entry after the target changed | Fixed, verified |
| N2 | clock history by title | Fixed (`ClockTask { uri, offset, title }`) |
| N3 | bulk actions on same-headline entries | **Partly fixed**: the loop re-maps the marks, but the "kept marked" path still collapses, see 2.1 |
| N4 | agenda `S-right` after an unsaved edit | Fixed, verified with two extra cases |
| N5 | capture abort leaves the clock stopped | Fixed |
| N6 | capture refile drops the clock | Fixed except the deliberate part, but see 2.2 |
| N7 | rejected `applyEdit` believed applied | Fixed for `edited` and `running`; see 2.4 for what is left |
| D2 | non-atomic writes | Fixed: tangle, Emacs results and the agenda export go through `replace_whole` |
| D5 | hung Emacs never recovers | Fixed for `call()` jobs; **`start()` jobs still hang forever**, see 2.3 |
| P6 | roam directory walk | Fixed (each canonical directory once) |
| S5 | ImageMagick | **Still synchronous** (`images.lua:202`, `vim.system(...):wait()` with 10 s). Cache bounded to 64 entries by count, not bytes; disk cache pruned after 30 days; failures no longer cached |
| S11 | `install.sh` | Fixed |
| B3 | Emacs script timeouts | Fixed; the fuzzers still never run in CI |
| 2c | VS Code double restart, Sublime `image_size`, Neovim phantom buffers | Fixed; see 2.6 and 2.7 for residue |
| 3a.1-4, 3b.1-10 | parity docs | Fixed and regenerated; every new claim was checked against the code and holds, except 3a.3's schema half (2.9) and the Zed checks in 2.8 |

---

## 2. Regressions and residue from the round-two fixes

### 2.1 Medium — Persistent agenda marks are lost after any bulk action
`server/org-lsp/src/server.rs:2651-2665` (the new `follow` on `didChange`) with `server/bulk.rs:567-585`. `kept` is resolved against the staged (new) text, so its offsets are in new-text coordinates; the view rendered at the end of `run_bulk` is still from the old text, and when the editor's `didChange` arrives the new follow shifts the kept marks again, so they match no line (marks match by `EntryRef` equality, `views.rs:280`).
**Verified**: `* TODO Alpha\n* TODO Beta\n* TODO Gamma\n`, persistent marks, mark all, bulk `s`, apply the edit: only Alpha stays marked. On the commit before the fix all three stayed marked.
Also, `kept` is built from the original `agenda_marks` resolved by nearest same head, so with `* TODO Review\n* TODO Review\n` a bulk `s` then bulk `d` says "Acted on 2 entries (kept marked)" but only the first gets the DEADLINE (the N3 residue).
Fix: keep `kept` in old-text coordinates (the followed marks' original offsets, with the head from the staged entry) so the `didChange` follow brings them to the new text; or skip following `agenda_marks` for a `didChange` the server caused itself (`self.edited` holds the key).

### 2.2 Medium — `org.capture.refile` with `:clock-in :clock-keep` leaves the running clock pointing at the wrong file
`server.rs:8257-8290` (`refile_capture` → `clock_capture` sets `self.running` with the capture target's URI at 8159), then `refile_from` moves the entry to another file.
**Verified** on disk files: capture into `notes.org`, refile to `other.org`: the CLOCK line is correctly under the entry in `other.org`, but `org.clock.out` then fails with "Clock start time is gone" and the clock display stays pending. Same-file refile works.
Fix: after a successful refile set `running.uri` and `last_clocked` to the destination, or `self.running = self.find_running()` after staging.

### 2.3 Medium-low — A hung `start()` job holds Emacs forever
`server/org-lsp/src/emacs.rs:216-224`, `322-325`: `Reply::Later` jobs (export, publish, tangle through Emacs, column view, LaTeX preview) have `timeout: None`, so `request` does `process.lines.recv()` with no deadline. The Emacs thread blocks for the session, every later `call()` reports "did not answer in time", no `Done` event comes, Emacs is never killed.
**Verified** with the stand-in Emacs: `start("hang")`, then `call("ping", 300 ms)` is LATE, still LATE 2 s later, no events.
Fix: a generous deadline (minutes) for `Later` jobs with `Done { Err }` on expiry, or keep the child handle where the server loop can kill it.

### 2.4 Low — Rejected `applyEdit` residue
`server.rs:2335-2360`: the clock state undone is `running` and `clock_pending` only; `clock_history`, `clock_interrupted`, `last_clocked` and `clock_out_time` set in `clock_in_after` (12967-12985) stay, so `org.clock.in.last` afterwards clocks into an entry that was never clocked. And `self.running = find_running()` runs for any refused edit, not only clock ones: a running clock in a file that is neither open nor an agenda file is dropped. Unverified.

### 2.5 Low — `Caller::call` can abandon a job Emacs is about to run
`emacs.rs:149-162` vs `194-216`: the thread clears `starting` before it records `begun`; a caller polling in that window sees "not sent, not starting" and gives up, and the result goes to a dropped receiver. Microsecond window, unverified. Fix: record `begun` before `starting.store(false)`.

### 2.6 Low — VS Code restart chain gaps
`editors/vscode/src/extension.ts:671-681`: the chain serialises restarts correctly, but if `stopClient` rejects (stopping a dead server) `startClient` is skipped and no client runs until the next manual restart; and the config listener (`:2028`) is registered before activation's own `startClient` (`:2051`), which is outside the chain, so a settings change during activation can start two clients. Unverified.

### 2.7 Low — Neovim buffer lookup and image cache details
- `lua/org-lsp/buffers.lua:11` matches by exact string, while the old `vim.uri_to_bufnr` went through `bufadd`, whose name comparison is case-insensitive on Windows and macOS. A server-composed URI (agenda files from settings, journal, roam) differing in drive-letter or path case now finds no buffer, so `org-lsp/columns`, `folds`, `images` and `view.save` silently do nothing for it. Fix: `vim.fn.bufnr(name)` (finds without creating) or compare normalised paths.
- `images.lua:166-168`: the prune condition needs `atime`; on `noatime` mounts a PNG shown daily but converted over 30 days ago is deleted and reconverted synchronously once a month. A partial PNG left by a Neovim killed mid-conversion fails `png_size` and is treated as "no image" for the session without being removed (`:211-215`).

### 2.8 Low — The Zed parity fix over-claims three checks
Changing `ZED_TEXT` and dropping `not: ['zed']` in `scripts/features.mjs` turned three areas to `part` for Zed, so checks now render in `editors/zed/TESTING.org` that Zed cannot pass: journal "the other entries of the file are folded" (`features.mjs:504`, rendered at `TESTING.org:209`) while the same area's Zed note says Zed lets no server fold; the notes area (`:859-861`, rendered at `:330-333`) says a note buffer opens, contradicting its own Zed note that a note is one line; `org.schedule` with `org_log_reschedule = "note"` (`:671`). Fix: restore `not: ['zed']` on those checks.

### 2.9 Low — `stuck` is missing from the custom-command schema
`server/org-lsp/src/settings.rs:2116` and `2121` still have `"enum": ["agenda", "alltodo", "todo", "tags", "tags-todo", "search"]`, so `docs/config.schema.json` and VS Code's `package.json` flag `type = "stuck"` invalid although `custom.rs:102` accepts it and `configuration.org` now lists it. Fix: add `"stuck"` and regen.

---

## 3. New findings

### 3a. Editing commands (tables, lists, structure, links, timestamps)

#### E1. Medium — TAB, S-TAB, `C-c RET` and `C-c C-c` are refused on any table row marked `#`
`server/org-core/src/table.rs:670` (`maybe_recalculate_line` returns an error), called from `next_field` (`:681`), `previous_field` (`:722`), `hline_and_move` (`:1018`) and `Command::Align` (`:103`); `server.rs:11210` turns it into a refusal with no fallback. In Emacs `org-table-maybe-recalculate-line` recalculates the line and the command continues, so TAB moves on. The `#` mark is what the manual recommends for automatic recalculation; in such tables the main navigation key is dead.
**Verified**: `| # | 1¦ |  |\n#+TBLFM: $3=$2*2\n` with next field gives the error; expected `| # | 1 | ¦2 |`.
Fix: on that error run the line recalculation the server already has and re-run the table command on the result.

#### E2. Low — Table alignment ignores `org_link_descriptive`, `literallinks` and `org_hide_emphasis_markers`
`server/org-core/src/table.rs:136` (`hidden_parts`, unconditional) and `format.rs:133`; `Server::table` (`server.rs:11195`) passes no display settings. Links are always measured at the description's width and emphasis markers always counted, while Emacs measures the fontified text: with `org-link-descriptive` nil the column is the full link width, with `org-hide-emphasis-markers` t `*bold*` counts 4. The result is a table Emacs realigns on `C-c C-c`. Unverified against Emacs, certain from its mechanism.

#### E3. Low — A column with several width cookies uses the first; Emacs uses the last of each kind
`server/org-core/src/shrink.rs:84`: the first cookie of a column wins, with alignment and width from that one cookie. `org-table--shrink-columns` lets every later `<l>`/`<r>`/`<c>` and `<N>` overwrite, separately.
**Verified**: `| <l> |\n| <5> |\n| <r3> |\n| abcdefghijkl |` shrinks to a bare `…` here; Emacs shrinks to 3 characters, right-aligned. Rare, cosmetic.

#### E4. Low — CRLF files: formatting corrupts tables and table edits mix line endings
`server/org-core/src/format.rs:38` (`text.split('\n')`) and `:102-114` (`row()` reads up to `|`, so the trailing `\r` becomes an extra cell), reached from `textDocument/formatting` and `rangeFormatting` (`server.rs:3415`, `3440`).
**Verified**: formatting `| a | b |\r\n|-\r\n| 1 | 2 |\r\n` returns edits making every row `| a | b | \r  |` and the rule three columns; the `org.table.recalculate` action drops the `\r`s, leaving mixed endings; `:t:\r` is not seen as a tag group. LF text is correct and idempotent; parsing, tokens, folds and symbols on CRLF are right. Windows editors default to CRLF, so format-on-save mangles every table there.
Fix: strip a trailing `\r` per line in the table and format paths and put it back, or normalise to LF for org-core and map edits back.

Options named in the Org manual that have no setting here, so the defaults always apply (not in `parity.org`): `org-list-allow-alphabetical`, `org-list-indent-offset`, `org-M-RET-may-split-line`, `org-insert-heading-respect-content`, `org-table-tab-jumps-over-hlines`, `org-list-demote-modify-bullet`. A formula typed into a cell (`=$1*2`) is refused with a message pointing at `#+TBLFM`, deliberate but undocumented.

### 3b. Agenda rendering and matching

#### A1. Medium — `"<+Nm>"` and `"<+Ny>"` in a match string use 30 and 365 days; Emacs uses 31 and 365.25
`server/org-core/src/agenda/matcher.rs:309-310`. `org-matcher-time` uses 2 678 400 s (31 days) and 31 557 600 s (365.25 days); `clocktable.rs:838-839` already has those values, so the two disagree.
**Verified** on the org-lsp side: `SCHEDULED<"<-1m>"` on 2026-10-12 lists an entry scheduled 2026-09-11; Emacs's threshold is 09-11 00:00, so it does not. Also, Emacs only treats `[dmwy]` units as times, so `"<+2h>"` is a string comparison there.
Fix: share `clocktable::matcher_time`.

#### A2. Low-medium — `PRIORITY="5"` never matches with numeric priority cookies
`matcher.rs:429-433`: the cookie's value is converted through `priority::value` and `char::from_u32`, so `[#5]` yields `"\u{5}"`. Emacs's `PRIORITY` special property is the cookie's text; only the default falls back to the character.
**Verified**: `#+PRIORITIES: 1 9 5`, `* TODO [#5] five`, match `PRIORITY="5"` prints nothing; Emacs lists it. Related: `item.rs:647-659` reads cookies as one capital or 0 to 64, while `org-priority-regexp` is `[A-Z0-9]+`, so `[#100]` and `[#AB]` sort differently.

#### A3. Low-medium (unverified) — A multi-day range whose ends carry the same time shows no time on the days between
`agenda/mod.rs:794-799`: `dotime` is `None` when neither end has a time flag. In `org-agenda-get-blocks` the first branch is `(string= hhmm1 hhmm2)`, so when both ends have the same time the full `<a>--<b>` text is the `dotime` on every day and the first stamp's time goes in the prefix. `<2026-10-12 Mon 10:00>--<2026-10-14 Wed 10:00>` on 10-13 shows `(2/3):` with no `10:00......` here. With no times on either end nothing differs, which is why the recorded cases pass.

#### A4. Low — `%%(org-class …)` text is not split on `"; "`
`agenda/diary.rs:243-248` returns `Value::Entry`, one line. `org-class` returns a bare string, which `org-diary-sexp-entry` splits, so `Lecture; Room 5` is two agenda lines in Emacs. Fix: return `Value::Str`.

#### A5. Low — Minor matcher and format divergences (verified on the org-lsp side)
- `matcher.rs:286-289`: `!=` and `/=` are accepted as operators; Emacs's regexp is `[<>=]{1,2}`, so `PRIORITY!="A"` is a tag there. `LEVEL="2"` matches here; `LEVEL` is not a string property in Emacs.
- `mod.rs:1510` (search view `!`/`S`): the open-keyword test uses a word boundary; Emacs's `org-not-done-regexp` against the entry text has none, so `* TODOX` and a deeper `TODO` child under `org-agenda-search-view-max-outline-level` pass there.
- `mod.rs:917-922` (log mode): the note is placed before any tag group; Emacs's regexp only matches a single tag, so with `:a:b:` the note lands after the tags, then the tags are re-added (`Title :a:b: - note   :a:b:`). Worth a recording; unverified.
- `mod.rs:776` (`blocks`): a range inside a comment or src block is shown by Emacs and skipped here (arguably Emacs's bug).
- `diary.rs:170-190` (`format_count`): `%d`/`%s` taken in order of appearance; Emacs's `format` is positional.
- Clock sums (`clock.rs:422`) and `<+Nh>` work in wall-clock minutes; Emacs in epoch seconds, so a clock across the autumn DST change is an hour longer there.

Agenda options with no setting here, so the defaults always apply (not in `parity.org`, whose agenda section reads as complete): `org-agenda-sorting-strategy`, `org-agenda-todo-ignore-*`, `org-agenda-skip-scheduled-if-deadline-is-shown`, `-skip-deadline-prewarning-if-scheduled`, `-skip-scheduled-delay-if-deadline`, `org-agenda-prefer-last-repeat`, `org-agenda-category-icon-alist` (`%i` is always empty), `org-agenda-include-diary`, `org-agenda-remove-times-when-in-prefix 'beg`, `org-sort-agenda-notime-is-late nil`.

### 3c. Standard LSP features

#### L1. Low-medium — Diagnostics of a closed document are never cleared
`server.rs:2699-2712` (`DidCloseTextDocument`): the document is removed, `self.diagnosed` is left, no empty `publishDiagnostics` is sent. **Verified**: open `[[*Missing]]`, two lint items published, close, nothing follows. VS Code, Sublime and Neovim's diagnostic list keep them, so the Problems panel shows lint of closed files, stale after the file is edited elsewhere. Fix: on close, if `diagnosed.remove(key)`, publish an empty list.

#### L2. Low-medium — Links whose URL has a non-ASCII character are dropped from `documentLink` and go-to-definition
`server.rs:3626`: `Ok(Open::External(url)) => (url.parse::<Uri>().ok(), None)`; `fluent-uri` refuses `https://example.com/é`, so target and data are both `None` and the link is skipped at `:3634`. **Verified**: of `[[https://example.com/é]]`, `https://example.com/日本` and `[[https://example.com/a%20b]]` only the third is returned, and definition on the first is `null`. Emacs opens all three. Fix: percent-encode bytes outside RFC 3986's set before parsing, as `uri::from_path` does for paths.

#### L3. Low — A message between `shutdown` and `exit` kills the server without a response or clean-up
`server.rs:1462-1464`: `lsp_server::Connection::handle_shutdown` returns an error for anything after `shutdown` that is not `exit` (a request, which the spec says to answer with `InvalidRequest`; a `$/cancelRequest` or `didClose`, which some clients send while stopping), and the `?` leaves `run` before the clean-up at `:1492-1509`. **Verified** with the binary: `shutdown`, then `foldingRange`: no reply, exit code 1, the view directory left behind. Also `exit` without `shutdown` exits 0 (spec: 1). Fix: handle `shutdown` in the loop with a flag, answer later requests with `InvalidRequest`, ignore notifications, break on `exit`.

#### L4. Low — `codeActionProvider.codeActionKinds` lists 13 kinds; `code_actions` returns many more
`server.rs:2006-2008` vs `:3200+`: `org.meta.right`, `org.shiftmeta.*`, table row/hline/width actions, `org.archive.*`, `org.clock.in` and, with `more_actions`, every command are returned but not announced. A client building menus or `only` filters from the announcement never sees them. Fix: announce what is offered, or the `org` prefix.

#### L5. Low — `file:` link targets are not normalised
`server.rs:4128-4137` (`link_path`) joins `./rel.org` onto the directory as is, so `documentLink` for `[[./rel.org]]` in `/tmp/x.org` has `target: file:///tmp/./rel.org`; clients comparing URIs do not match it to the open buffer. Also `~` expands only with `HOME`, not `USERPROFILE` (`Workspace::expand` handles both), and `~user/` is unsupported (Emacs expands it). Fix: lexically normalise `.` and `..`, keep symlinks.

### 3d. Files outside the workspace and symlinks

#### W1. Medium-low — An open file is not recognised as an agenda file when the folder and the editor spell its path through different symlinks
`server.rs:4345-4357` matches open documents to `workspace.files()` by decoded path, not canonical path; `id_paths`, `ids_on_disk` (`4202-4255`) and `file_text` (`4141-4151`) compare the same way. **Verified**: the root folder given as a symlink to the real directory, the editor opens `work.org` by the real path with an unsaved extra entry: the agenda shows the disk copy, and an agenda command on such an entry edits the disk-path URI with `version: null` while the buffer is open, so the editor opens a second buffer or applies to stale text. Same for `~/org -> /data/org` the other way round. Fix: canonicalise both sides, cached per document.

#### W2. Low — `org_agenda_files` globs with `**` follow directory symlink loops
`workspace.rs:143-153` (`glob`) has no depth cap or seen set (`org_files_below` has `MAX_DEPTH`). **Verified**: `sub/up -> root` with `root/**/*.org` yields 123 paths for 3 files; each entry shows about 41 times. Fix: canonicalise and skip seen directories, as `roam::walk_files` now does.

#### W3. Low — `Workspace::read` cache keyed by the path as spelt
`workspace.rs:283-298`: the same file reached by two spellings is parsed and cached twice and never evicted; `org_files_below` dedups by spelt path, so symlinked duplicates inside a folder appear twice.

#### W4. Note — No `didChangeWorkspaceFolders`, `didChangeWatchedFiles` or rename handling
Nothing is announced, so not a bug: folders added after `initialize` (`server.rs:1229-1232`) are invisible until restart, and `ClockTask`/`EntryRef` URIs go stale on a rename. Worth a line in `parity.org`.

### 3e. Tests and tooling

- **Low** — `offers_commands_as_actions_and_asks_with_buttons` (`server/org-lsp/tests/agenda_lsp.rs:4360-4377`) waits about a second (200 × 5 ms) for the agenda file to be rewritten through the command port. It failed once in the full `check.sh` run while five other processes were compiling, and passes alone. A longer wait would make it robust under load.
- **Low** — `editors/vim/test/run.sh:27-30` waits at most 120 s per client, while `test.vim` has about 45 tests with waits up to 15 s each, so a slow CI run fails as "Vim did not finish" rather than naming the failing test. Fail-closed, so safe.

---

## 4. Checked and found sound this round

- Emacs bridge after #221: no reply delivered twice, reader threads end with the pipe, a job behind a long one still gets its own LATE, `Declined` events become messages, `Process::drop` kills and waits.
- `document.rs`: `Change::between` respects char boundaries, `moved` is right at both edit boundaries, incremental changes apply in order, UTF-16 untouched, `catch_unwind` leaves text, index, version and parse consistent.
- `bulk.rs` `moved`: sorted non-overlapping edits, marks processed by file then ascending offset; `Private` directories: every use site creates on demand, cleanup uses `made()`.
- `write_whole` through a live symlink and a symlinked parent (merged test); `replace_whole` used by tangle, Emacs results and the agenda export.
- Semantic tokens: full equals the union of range requests in UTF-8, UTF-16 and UTF-32 on text with emoji and CJK on every kind of line, in order, never overlapping, correct after incremental changes and on CRLF; every legend type is produced.
- Folding, startup folds for every `#+STARTUP` visibility value, document and workspace symbols, completion ranges after multi-byte text, hover escaping, definition and references for every link form, rename rules and edits across files, code action filtering and versions, LF formatting idempotence, diagnostic columns in all encodings, the `initialize` result (every claimed capability has a handler and every unclaimed method answers `MethodNotFound`, all 264 commands announced), proper shutdown and exit.
- Tables: alignment with emoji, combining marks, ZWJ sequences and CJK; TAB at the last cell; `S-TAB` from the first field; row and column insert, delete and move with formula references rewritten as `org-table-fix-formulas` does; marker rows `*`/`$`/`!`/`_`; sort with ties, accents, durations and timestamps; forty formula shapes including `$<`, `@>`, `@I..@II`, `::` lists, field-over-column precedence, `;f3`, `;t`, `;U`, `;N`, `;E`, `;L`.
- Lists, headlines, links and timestamps: every case in the reviewer's 150-input probe matched Emacs, including M-RET variants at every cursor position, promote and demote with each `org-adapt-indentation` value, subtree moves and paste, checkbox cookies, `org-link-escape`, and S-up/down on every stamp field at its boundary.
- Agenda: which days scheduled and deadline entries appear with every repeater and warning form, the `org-closest-date` quirks, week and year boundaries, source order and `org-entries-lessp`, the time grid, `org-agenda-format-item` with every `%` field, log mode, the tag matcher, search view parsing, stuck projects, habits, diary sexps, clock tables (138 recorded cases), block agendas.
- Zed and Helix docs after #224: every command now claimed for Zed is in `tasks.json` and `keymap.json`, and the server turns its choices into buttons; every Helix action listed is offered without a capability; the protocol doc's `org-lsp/display`, `clipboard`, `folds` and `folded` shapes match the code.
- Tests that cannot fail: none found in the VS Code, Sublime, Neovim or Vim suites; `test-all.sh` propagates exit codes.
- `release.yml` checksum step: correct `sha256sum -c` syntax, fails the step on mismatch, right platform file.
