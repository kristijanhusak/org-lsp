# Audit findings, round two

A second review of org-lsp after the first audit's fixes landed on `main`
(PRs #212 to #220). This file replaces the first round's list; the earlier
text is in this file's git history. It has three parts:

1. the status of every first-round finding, with what was only partly fixed,
2. new findings: regressions from the fixes, and logic bugs in the stateful
   flows (capture, clocking, agenda) that the first round did not look for,
3. the parity check: where the docs claim something the code does not do,
   or the other way round.

"Verified" means the trigger was run here and the failure observed;
"unverified" means it was established by reading the code. `check.sh`
passes on the merged tree (formatting, clippy, every test, generated files).

The hash of the `zed-extension` binary could not be computed from this
environment either: the host answers 403 through the proxy here as well.

---

## 1. Status of the first-round findings

| ID | Finding | Status |
| --- | --- | --- |
| C1 | noweb self-reference stack overflow | Fixed (`tangle.rs:936-955`, test added) |
| C2 | `++0d` repeater infinite loop | Fixed (`todo.rs:1278-1283`, `1311-1314`) |
| C3 | `@+N` row descriptor hang | Fixed (`tblfm.rs:549-552`) |
| C4 | multi-byte unit panic in match string | Fixed (`matcher.rs:303`) |
| C5 | datetree archive panic on month 13 | Fixed (`archive.rs:826-829`) |
| C6 | mutually referencing cells hang | Fixed as a hang, but see R2 below |
| C7 | unbounded client `count` | Fixed (`server.rs:341-352`, clamp ±100 000) |
| C8 | empty `:ancestor-with-todo` panic | Fixed (`super_agenda.rs:846`) |
| C9, C13 | long timestamp range / huge agenda span | Fixed by clamping to 3660 days, silently (no message to the user) |
| C10 | huge format widths | Fixed (`FORMAT_MAX = 1000`) |
| C11 | repeater/relative overflow | Fixed (clamped and saturating) |
| C12 | deep recursion in readers | Fixed (depth cap 200) |
| S1 | no panic boundary | Fixed (`guarded`, `server.rs:1437-1457`), but see R3 |
| S2 | predictable temp files | Fixed (0700 directory with a random component, `server.rs:1788-1812`) |
| S3 | VS Code settings from the workspace | Fixed via `capabilities.untrustedWorkspaces`; Zed half still open (its project settings have no trust prompt, by Zed's design) |
| S4 | Vim `cmd /c start` injection | Fixed (`rundll32`) |
| S5 | ImageMagick synchronous, unbounded | **Partly fixed**: limits and a 10 s timeout added, but still synchronous on the main loop (`images.lua:159-169`), the memory cache is still unbounded, the disk cache never pruned, and a transient failure is cached as "no image" for the session |
| S6 | tangle paths | Left on purpose (matches Emacs) |
| S7 | download `release` unvalidated | Fixed (regex, with tests) |
| S8 | command port timeout and token in argv | Fixed (2 s read timeout, token in the environment), but see R7 |
| S9 | release workflow | Fixed except the `zed-extension` hash (host blocked) |
| S10 | `org-site --out` deletes the target | Fixed (refuses a non-empty directory without `.nojekyll`) |
| S11 | `install.sh` not wrapped | Fixed; `install_sublime` still removes the old package before fetching (`install.sh:253-258`), and `--help` through a pipe prints `sed: can't read main` (`install.sh:33`) |
| D1 | non-UTF-8 archive overwritten | Fixed (`write_whole` refuses, `server.rs:2062-2075`) |
| D2 | non-atomic writes | **Partly fixed**: archive and journal go through `write_whole`; `write_tangled` (`server.rs:2096-2112`), Emacs `run` results for closed files (`server.rs:9627`) and the agenda export (`server.rs:6256`) still use `fs::write` |
| D3 | Vim URI encoding | Fixed for yegappan/lsp, vim-lsp and ALE; coc.nvim has no `uri` hook and uses vscode-uri, which writes `%3A` for `:` and lower-cases the drive letter, so on Windows the plugin's `file:///C:/...` never matches coc's `file:///c%3A/...` and every key is dead (unverified, no Windows here) |
| D4 | Neovim edits to a gone buffer | Fixed (validity checks); no version check was added, so an edit can still land on text typed before the next `didChange` flush |
| D5 | Emacs calls block; hung Emacs never recovers | **Partly fixed**, and the fix regressed: see R1 and N1 |
| D6 | site generator bugs | Fixed |
| P1 | diagnostics re-walk the workspace per keystroke | **Partly fixed**: IDs cached 5 s and includes by mtime, but `publish_diagnostics` still runs straight from `didChange` with no debounce, org-lint still stats every `file:` link, `Workspace::files()` itself is still uncached (`workspace.rs:176`), and `didSave` still re-renders the whole agenda |
| P2 | parser quadratic paths | Fixed; all five hostile shapes now scale linearly (≤45 ms at 20 000 lines), parse results unchanged |
| P3-P5, P7 | indent, `Elements::at`, BibTeX, symbols | Fixed |
| P6 | roam and journal per-request work | **Partly fixed**: regexes cached, journal reads reduced; every roam command still lists the roam directory recursively following directory symlinks with only a depth cap (`server/roam.rs:84-98`) |
| P8 | synchronous Enter/o/Tab | Left on purpose |
| P9 | site NBSP panic | Fixed |
| B1, B2 | release publishing | Fixed |
| B3 | Emacs scripts without timeout | Fixed except `scripts/update-lint-tables.py:37`; the fuzzers still never run in CI |

---

## 2. New findings

### 2a. Regressions from the fixes

#### R1. High — A slow-starting Emacs is killed by every short call; org-crypt can never work
`server/org-lsp/src/emacs.rs:84-92` (`call` sets `deadline: Some(Instant::now() + timeout)` when the job is queued), `:149-176` (`serve` runs `start()`, up to `SETUP_TIME` = 120 s loading the user's init, before `request()`), `:243-270` (`request` with an already-expired deadline returns `LATE` at once, so `serve` drops the process, which kills the fresh Emacs).
`encrypt_before_save` (`server.rs:2776-2795`) calls with a 10 s timeout on every `willSaveWaitUntil` of a file with `:crypt:`; `appt-check` and `timer-fire` use 10 s too. With an init that takes longer than 10 s, every save spawns an Emacs, kills it, saves the file in plain text and shows an error, forever. Before the fix the first call timed out but Emacs stayed up and the next call worked.
**Verified** with a stand-in Emacs whose setup sleeps 2 s: a 500 ms call fails, a second call 3 s later fails again (new Emacs, new sleep), only a 10 s call succeeds.
Fix: keep the timeout in the `Job` and compute the deadline after `start()` returns (when the request is written to Emacs); have the caller wait `timeout + SETUP_TIME` (or plain `recv`) and let the Emacs thread enforce the deadline, so start-up time is never charged to a job.

#### R2. Medium-low — The table-formula cycle cap still costs ~1.6 s per formula
`server/org-core/src/tblfm.rs:573` (`REFERENCES_MAX = 10_000`), loop at `:1002-1012`. Each iteration of the `@`-reference loop calls `get_range` (~160 µs), so a two-cell cycle takes 1.6 s in release per formula (16 s in debug; the committed `stops_on_formulas_that_never_end` is why `tests/tblfm.rs` takes 39 s). Ten such field formulas hold the server for 16 s. **Verified** (release timings).
Fix: detect the cycle exactly with a set of `form` values seen, or lower the cap to ~100; a legitimate formula never substitutes more than a handful of references.

#### R3. Medium-low — `catch_unwind` can leave a document's text, parse and version out of step
`server/org-lsp/src/server.rs:1879` (`guarded`) with `document.rs:28-48` (`Document::apply`): the text is replaced and the index rebuilt, then `version` and `parse` are set last. A parser panic, now survivable, leaves new text with the old parse and old version; every later request on the document runs on the mismatch, and the client refuses the server's own edits on the version check with no explanation, until the triggering text is edited away. Unverified (no known parser panic to trigger it).
Fix: compute `parse` into a local first and commit all four fields together, or roll back on panic.

#### R4. Low — `write_whole` replaces a symlinked archive or journal file with a regular file
`server/org-lsp/src/server.rs:2086` renames the temporary over `path` without canonicalising. An archive that is a symlink (dotfiles repos, `~/org -> /data/org`) becomes a plain file at the link's location and the target is left untouched; hard links are broken likewise. Emacs writes through the link. Unverified by execution.
Fix: `canonicalize` the path first (falling back to the path when it does not exist), as `temp_dir()` already does.

#### R5. Low — `call()` jobs answer every `y-or-n-p` from Emacs with a silent "no"
`server/org-lsp/src/emacs.rs:281-286`: with a deadline set, `answer = false` and the user is never told (`refile-verify`, `capture-target`, `crypt-before-save` can all ask). Better than the old deadlock, but the user never learns a question was declined.
Fix: send the prompt as a `window/showMessage`, or include it in the error.

#### R6. Low — Six private temp directories are created at every start
`server/org-lsp/src/server.rs:1299-1385`: capture, edit, export, journal, roam and view directories are made eagerly, not on first use; a server the editor kills leaves them all in `$TMPDIR`. Cosmetic.

#### R7. Low — The command port's read timeout is per read, not per connection
`server/org-lsp/src/port.rs:192`: `set_read_timeout(2 s)` with `read_line` over `take(4096)` keeps a connection that sends one byte every 1.9 s alive for up to two hours, still blocking the single-threaded accept loop.
Fix: a deadline for the whole receive, or accept on a thread.

#### R8. Low — `src.lua` incremental highlighting
`lua/org-lsp/src.lua:318`: `table.remove(before[key])` pops the last entry, so two blocks with identical name, column and text swap marks, `in_place` fails and both are recoloured on every change (correct output, performance win lost). Fix: `table.remove(before[key], 1)`. Also, a line deleted inside a block and undone within the 100 ms debounce leaves collapsed extmarks that are not restored, since the key (the text) is unchanged (unverified, no Neovim here).

### 2b. Logic bugs in capture, clocking and the agenda (new)

All verified by driving the server over LSP with throwaway integration tests. Line numbers are in `server/org-lsp/src/server.rs` unless stated.

#### N1. High — Capture with `:clock-in` or `:jump-to-captured` writes the CLOCK line into the wrong entry when the target changed during the capture
`finish_capture` (7979-8009) passes `capture.placed.begin` to `file_capture` (8030+), but `captured()` (7924-7952) re-places the template into the current text when `now != capture.snapshot`, returning a `done` whose entry is at a different offset. `file_capture` then clocks in or jumps at the stale offset. `refile_capture` (8128-8129) uses the same stale offset and would refile the wrong subtree.
Trigger: template `file+headline` "Inbox" with `clock_in` and `jump_to_captured`, file `* Inbox\n* Other\n`; start the capture, insert `* Added one\n* Added two\n` at the top through `didChange`, finish. Result: the LOGBOOK with the CLOCK line lands under `* Added one`, the captured entry is under Inbox, and `showDocument` selects line 0.
Fix: make `captured()` return the `Placed` it used and use that offset in `finish_capture` and `refile_capture`.

#### N2. Medium — `org.clock.in.last`, clock history and `:clock-resume` pick the first entry with the same title
`clock_history`, `clock_interrupted`, `clock_default_task` are `(Uri, title)` (1004-1015), resolved with `find_title` (12449-12460: first `plain_heading == title`); also at 8079 (resume after capture) and `clock_in_selected` (12605+). Emacs keeps markers.
Trigger: `* Project A\n** TODO Review\n* Project B\n** TODO Review\n`; clock in on line 3, clock out, `org.clock.in.last`: the CLOCK line lands under Project A's Review. Same for `:clock-resume` after a capture and the `i`/`d`/`c` choices of `C-u C-c C-x C-x`.
Fix: store `(Uri, offset, head)` like `EntryRef` and resolve with `views::resolve`, or by `ID`.

#### N3. Medium — Agenda bulk actions on entries with the same headline skip one while reporting success
`server/org-lsp/src/server/bulk.rs:436+` (`run_bulk`) resolves each mark against the staged text with `views::resolve` (`views.rs:652-661`): exact offset, else the nearest entry with the same head. After the first entry gains text (a SCHEDULED line, a LOGBOOK, CLOSED), the second mark's stale offset is nearer the first entry than its own.
Trigger: `* TODO Review\n* TODO Review\n`, todo view, mark all, bulk `s` with a date. Only the first entry is scheduled; the message says "Acted on 2 entries". Same for `d`, `S`, `t` without a keyword change, `+`/`-` and `$` of large subtrees.
Fix: re-map the remaining marks through each step's edit (as `roam.rs::moved` does), or at least count a mark resolved to an already-processed entry as skipped.

#### N4. Medium — `S-right`/`S-left` on an agenda line after an unsaved source edit shifts the wrong timestamp
`edit_with` 8248-8256 locates the stamp as `heading + (reference.marker - reference.offset)`, a byte distance recorded when the view was built; the view is rebuilt only on save or on server-made edits (2574-2578). A user edit between the headline and the stamp without saving leaves the distance stale.
Trigger: `* Meeting\nNotes that got much longer\n<2026-05-19 Tue>\n<2026-05-25 Mon>\n`, week agenda, shorten the body line through `didChange` without saving, `org.date.later` on the Meeting line: the 25th becomes the 26th and the 19th is untouched. With text inserted instead, the command fails with "Cannot find time stamp".
Fix: refresh the view on any `didChange` of a file it shows (the server already refreshes on save), or re-locate the stamp by its recorded text within the entry.

#### N5. Medium — `org.capture.abort` with `:clock-in :clock-resume` leaves the interrupted clock stopped
`CAPTURE_ABORT` (5283-5287) only drops `self.capture`; `stop_clock_for_capture` (8011-8028) had already clocked the running entry out. In Emacs `org-capture-kill` runs the finalize path, whose `:clock-resume` branch runs on abort too.
Trigger: clock in on `* Work`, start a template with `clock_in` and `clock_resume`, abort: the file ends with a closed `CLOCK:` line, no clock notification, no edit.
Fix: on abort, if `clock_resume` and an interrupted entry is set, clock it in again (with N2's fix).

#### N6. Medium — `org.capture.refile` with `:clock-in` drops the capture's clock and never resumes the interrupted one
`refile_capture` (8113-8148) calls `refile_from` with the placed text but never goes through `file_capture`, so the captured entry gets no `CLOCK:` line and `:clock-resume`, `:jump-to-captured` and the org-roam finalize are skipped. Emacs's `org-capture-refile` finalizes first.
Fix: build `done` through the same clock and roam-finalize path as `finish_capture`, then refile from that text.

#### N7. Low — A rejected `workspace/applyEdit` leaves the server believing the edit happened
`response` (2272-2286) on `Pending::Edit` only refreshes the view: `self.edited` keeps the key, so the next user `didChange` is treated as the server's own edit; `clock_in_after` has already set `self.running`, `clock_history` and `clock_interrupted` (12813-12824) before the client answers. Unverified by execution.
Fix: on `applied == Some(false)` clear the file from `edited` and undo the clock state, or set it only in the response handler.

### 2c. Other new findings

- **Low** — VS Code: `org-lsp.restartServer` and the `onDidChangeConfiguration` restart both do `stopClient(); startClient()` with no in-flight guard (`editors/vscode/src/extension.ts:2012-2027`, `669-673`); a config change during a manual restart, or a trust grant changing two settings, can start two clients. Fix: serialise through a promise.
- **Low** — Sublime `show_in_other_group` (`plugin.py:432`) formats `path:line:col` for `ENCODED_POSITION`; a path with `:` is split wrongly. `image_size` (`plugin.py:483-485`) reads the whole image file on the UI thread to parse a header; read the first 64 KB.
- **Low** — Neovim `view.lua:415`, `images.lua:473`, `columns.lua:180` use `vim.uri_to_bufnr`, which creates an unlisted buffer for an unknown URI before the `is_loaded` check; phantom buffers for server-named URIs that are not open.
- **Low** — The `-limit` flags in `images.lua` are valid for ImageMagick 6 `convert` (tested) and the same syntax for 7's `magick` (unverified). On Windows `executable('convert')` can match the NTFS `convert.exe`; it fails harmlessly.
- **Checked and fine** (new areas): org-crypt handling (plaintext goes only where Emacs's buffers would put it, nothing logged, the passphrase never passes through the server since `read-passwd` is advised to error); `bridge.el` (`eval` only for the user's own capture templates, no shell or file writes beyond Org's own); config loading (all errors become problem messages, no unwraps); `highlight.rs`, `completion.rs`, `folding.rs`, `views.rs`, `link_preview.rs`, column view, export, LaTeX preview (slicing on char boundaries, cached lookups); Zed `lib.rs`, `extension.toml`, `tasks.json`, `keymap.json`; Sublime phantoms and downloads; `clock.lua`, `columns.lua`, `preview.lua`, `help.lua`, `health.lua`; the Vim adapters; action SHA pins (all seven match their tags); the Neovim 0.12 requirement is stated consistently everywhere except `CLAUDE.md:115`, which still says the Neovim tests run on 0.11.0.

---

## 3. Parity check: do the docs say what the code does?

### 3a. Org features (`docs/parity.org`, README, `docs/plugins.org`, options)

All 245 rows of `docs/parity.org` across its 30 sections were traced to an implementation, a command handler and, where one exists, a recorded test case; every row marked as working has code behind it, and every row marked missing has none. All 263 manifest commands reach a handler; all 208 options in `settings.rs` are read by something. The 61-of-61 org-lint claim, the super-agenda selector list, the roam and journal specifics, and the "through Emacs" routes all match. The discrepancies are in the surrounding docs:

1. **`settings.rs:391`** (copied into `docs/configuration.org:996` and the JSON block): the `org_link_abbrev_alist` doc says "A function (`%(…)`) is not supported", but `server.rs:4083-4091` expands them through the bridge and `parity.org:385` claims that as working. The option text is stale.
2. **`docs/keys/helix.org:11-28`** lists 11 code actions; the server offers Helix many more with no capability needed (`server.rs:2968-3290`, `actions.rs:23-36`): promote/demote/move for headlines and items, table row/hline/field edits, tangle, execute, edit special, column width, archive subtree/sibling/tag, citation fixes. `parity.org`'s Helix notes are right; the keys page undersells it.
3. **`docs/configuration.org:241-247`**: the custom agenda command "views" table lists five types; `custom.rs:95-105` also accepts `search` and `stuck`, and its refusal message names all seven.
4. **`CLAUDE.md:115`** says the Neovim tests run on 0.11.0; everything else says 0.12.

Not verifiable here: the Emacs-message parity spot check (no Org sources or Emacs on this machine), the behaviour of "through Emacs" rows, and whether Org 9.8.10's `org-set-tags-command` sets `#+FILETAGS` before the first headline, where org-lsp refuses with "Setting file tags is not supported yet" (`tags.rs:530`).

### 3b. Per-editor claims (`scripts/features.mjs` and the editor docs)

180 area × editor cells were checked against the editor code, the capabilities each editor announces, and 14 protocol shapes in `docs/protocol.org` against the server. About 165 cells are accurate as written. The generators pass in `--check` mode, so the generated tables match `features.mjs`; the inaccuracies are in `features.mjs` itself and the hand-written pages:

1. **Zed: several "left out" claims are stale.** `editors/zed/generate.mjs:157-188` writes a task and a key for every manifest command with a non-native Emacs key, and the server runs them through the command port with `message_prompts` on, so key-choice menus come as buttons. Bound in `keymap.json` yet claimed absent in `features.mjs`: export (`:779`, marked ❌ with "its menu would come as buttons", which is exactly what happens), citations (`:804`), footnotes (`:644`), completion helpers (`:876`), Babel file/subtree evaluation and block navigation (`:744`), link navigation (`:615`). Rendered at `docs/zed.org:95-130`, `parity.org` and `editors/zed/TESTING.org`. Unverified in Zed itself.
2. **Zed and Helix "cannot ask for text" omits the rename mechanism.** `docs/zed.org:84-86`, `docs/keys/zed.org:20-24` and the `ZED_TEXT` note in 15 areas of `features.mjs` say such commands "say so and do nothing". For a `message_prompts` client the server instead answers "PROMPT: answer with a rename at the cursor" and takes the next `textDocument/rename` (`server.rs:4936-4950`, `3698-3725`), which `docs/any-editor.org:57-60` documents for generic clients and Zed has (`F2`). Helix does not set `message_prompts`, so for Helix the claim stands.
3. **`docs/keys/helix.org:14-27`**: same as 3a.2.
4. **Vim indentation checks exclude Vim although the plugin implements them**: `features.mjs:842-843` (`not: ['vim', ...]` for the indented new line and re-indenting a selection), but `org_lsp.vim:963` sets `indentexpr`, `:985` runs `onTypeFormatting`, `:1016` `rangeFormatting`, and `editors/vim/test/test.vim` ("indents new lines and = as Org does") covers both.
5. **`docs/keys/vscode.org:820`** says `j` in the agenda opens the calendar; the binding is `shift+j` (`docs/vscode.org:94` has it right). The example at `:901-902` removes `"j"`, never bound, and re-adds the default.
6. **`docs/protocol.org` is missing `org-lsp/display` and `experimental.orgDisplay`**: the server reads the capability (`server.rs:90-94`, `1162`) and sends `{ uri, version, items: [[line, char, line, char, shown]...] }` (`server.rs:2700-2722`); Neovim and VS Code announce and handle it. The parity text says these show as inlay hints, which is only true for clients without it.
7. **`docs/protocol.org` manifest section omits** the per-command `clipboard` and `folds` fields (`commands.rs:1692-1762`) and the context's `folded: [[first, last]...]` (`server.rs:400-403`), which `org.export.dispatch` uses for "only the visible part"; the manifest example at `:1513-1546` shows neither.
8. **Sublime property completion**: `features.mjs:329` claims name and value completion when setting a property for Sublime, but `plugin.py:935-957` shows a plain input panel and ignores `completions`. The tags row (`:303`) correctly limits this to Neovim, Vim and VS Code.
9. **Vim column view**: `features.mjs:328` limits moving, narrowing, widening, deleting a column and moving an entry to Neovim and VS Code, but `org_lsp.vim:1232` binds every `columns`-scope key and handles `org-lsp/columns`. Possibly "untested in Vim" (the test covers only `l`, `n`, `q`); as written it reads as not working.
10. **`features.mjs:198`, `201`** use `org.table.recalculate.line` for "turning a line into a heading", which has only an Emacs key, so the Neovim and Vim checklists show a `:Org` command while `docs/keys/neovim.org:376` tells users `<Leader>o*` on `org.table.recalculate`, which does the same off a table.

Conditions the docs state but only in one place: Vim with ALE never shows the inlay hints for shrunk columns and clocked time (only in the TESTING setup bullet); encryption is marked working for every editor but depends on the client sending `willSaveWaitUntil`, which no editor code touches, so Vim with ALE or vim-lsp is unverified.

Not verifiable here: anything needing the editors themselves (Zed tasks end to end, rename-as-answer in Zed, Helix's `window.showMessage` and `window/showDocument` capabilities, which if announced would change the Helix agenda, capture, roam and journal rows).
