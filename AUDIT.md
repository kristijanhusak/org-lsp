# Audit findings, round five (final, completed)

The fifth and last review of org-lsp, after the fourth round's fixes landed
on `main` (PR #228). Earlier rounds are in this file's git history. This
file completes the version that was cut short: the server-side fixes of
#228 are now re-verified, and the three reviews that had been stopped (lint
and indentation, `org-read-date` and capture templates, display, cycling
and `RET`) have run. `check.sh` passes on the merged tree. No further
audits will follow.

No Emacs or Org sources were on this machine at any point; "what Emacs
does" comes from the reviewers' knowledge of Org 9.8 and the recorded cases
under `tests/`, except where a reviewer fetched a source file and says so.
"Verified" means the trigger was run here on org-lsp's side.

Two corrections to earlier rounds, both settled by recorded Emacs output:
**round three's E3** (shrink cookies) was wrong, the first cookie wins as
#227 implements; and **round four's J3** was wrong: the new cases in
`tests/roam/capture.json` show that org-roam itself writes the node's ID at
the file level when capturing to an existing headline node, so the
duplicated ID is Emacs's behaviour, not the port's bug.

Deliberately left as they are, per the maintainer (unchanged list):
diagnostics debounce, caching the workspace file list, Sublime
file-and-position paths, coc.nvim on Windows and the Neovim edit version
check, roam finalize and jump-to-captured after a refiled capture, the
encryption docs, tangle paths, the synchronous Enter/o/Tab waits, and the
`zed-extension` hash (the host answers 403 from this environment).

---

## 1. Status of the round-four findings after #228

| ID | Finding | Status |
| --- | --- | --- |
| J1 | journal formats with `%A`/`%b` disabled org-journal | Fixed, verified (`%Y-%m-%d-%A.org`, `%a %d %b %Y.org`, `%B %Y.org`, case folding, `Čtvrtek`, `pátek`, CREATED with `%A`). Residue: `Files::new` (`journal.rs:302-305`) still swallows a regex build error into a never-matching regex; only the new unit test guards it |
| J2 | `id:` links to roam nodes | Fixed, verified over LSP (open, definition, hover, no "Unknown ID"; not stale after the ID is removed on disk). Cost: see 2.1 |
| J3 | capture to an existing node duplicates the ID | Not a bug (see above); three cases recorded from Emacs |
| J4 | `immediate_finish` on roam templates | Fixed, verified (`entry` template with `immediate_finish`, `clock_in`, `clock_keep` to a file neither open nor on disk: file written with the CLOCK line, clock announced) |
| J5 | carryover deletes an open journal file | Fixed, verified (the emptying edit carries the current version). Residue: see 2.2 |
| 2.1 | persistent marks lost on adjacent rewrites | Fixed, verified (existing test; 13 hand cases plus 2000 random line-diff pairs reproduce the new text with ordered, non-overlapping edits; EOF without newline right) |
| 2.2 | prefix forms block the loop | Fixed, verified with a stand-in Emacs (view shown at once, marks kept across the rebuild, back-to-back rebuilds serialised; with a nonexistent `emacs` the view returns in 10 ms). `org_text` (the agenda as text, `server.rs:6883`) still uses the synchronous 10 s call by design |
| 2.3 | hard links written in place | Fixed, verified (both names updated, same inode, mode kept, no temp left). Residue: see 2.3 |
| 2.5 | flat LCS table | Fixed, verified (edges, empty sides, covered by the random pairs) |
| R6 | view directory made eagerly | Fixed, verified (no directory after `initialize`; appears with 0700 on the first prompt). Residue: see 2.4 |
| A1 | `<+2h>`, `<yesterday>` as times | Fixed, verified (strings now; `<now>`, `<today>`, `<tomorrow>`, `<+3d>`, dates still times). Residue: `[+2d]` and `<+2dx>` are time 0 where Org signals "Not an Org time string"; `TimeValue::Hours` is dead code |
| L2 | bare `%` in URLs | Fixed, verified (`100%` → `100%25`, `a%20b` kept, `%zz` → `%25zz`, trailing `%`) |
| L5 | `~name/` | Fixed, verified (`~root/r.org` → `/root/r.org`; unknown user left literal, relative to the file's directory). Residue: see 2.5 |
| W1 | ID files by spelt path | Fixed, verified (test passes; `id_paths` and `ids_on_disk` compare by canonical path). The canonical-path cache (`server.rs:4403-4410`) is never invalidated for the server's life |
| 3b | SCHEDULED/DEADLINE as properties; priorities with `org_auto_align_tags` off | Fixed, verified |
| 3b | `::/regexp/` searches | Partly: a sparse tree only in the link's own file (`server.rs:4276-4291`); `file:other.org::/re/` still refused (`open.rs:670`). The new parity row says exactly this |
| 3b | `[#AB]`/`[#100]` cookies on `S-up` | Not fixed, not claimed (`priority.rs:174-184`) |
| 3b | LOGBOOK checkbox as the reference box | Not fixed, and the new test at `checkbox.rs:410-424` asserts that `C-c C-c` on a headline skips an item inside `:LOGBOOK:`; the reviewer's memory of `org-toggle-checkbox` is the opposite (items in drawers count). No recording exists either way; the test should not be taken as evidence until one is made |
| 2.6, 2.7, 3c | VS Code deactivate, image temp names, site highlighting, logo, alt text | Fixed, verified (see the earlier version of this round in git history for details) |
| 3a, 3b | undocumented gaps | 28 options and commands now in the parity table as missing, each confirmed absent; `org-journal-carryover-headings-only` and `org-journal-encrypt-on` were dropped from the list and are neither implemented nor listed |

Still open from earlier rounds: **S5** (Neovim image conversion is still synchronous on the main loop, `images.lua`, `vim.system(...):wait()` with 10 s; not on the left-as-is list) and **mixed line endings** in one file still putting `\r` into a table cell (pure-CRLF files are fine).

---

## 2. Residue from #228

### 2.1 Low-medium — `id:` lookups through roam walk the whole roam directory every time
`server.rs:4544-4552` (`find_id`) → `id_paths` (`4478-4491`) → `roam_paths` (`roam.rs:254-262`) lists the roam directory, canonicalises every path, then reads and parses files one by one until the ID is found, with no cache. This runs on every hover over an `id:` link, every definition, every `org.open` and refile-to-ID. Lint's `ids_on_disk` is bounded by freshness and mtimes and is fine. Verified by reading.
Fix: resolve through the roam database cache, or keep a path-to-IDs map keyed by mtime as `ids_on_disk` does.

### 2.2 Low — Carryover deletes the disk file before the emptying edit is answered
`journal.rs:380-393`: `remove_file` runs unconditionally; if the editor refuses the edit, the old entries exist only in the buffer, which a save writes back beside today's copies. Verified by reading. Fix: delete in the `Pending::Edit` answer when applied.

### 2.3 Low — The temporary beside a hard-linked file has default permissions
`server.rs:2298-2318`: the temporary is created with the umask, not the target's mode, so if the in-place write fails the kept copy of a private file is 0644; and the write fails where the directory is unwritable but the file is. Verified by reading. Fix: `set_permissions` on the temporary before the in-place write, as the rename path does.

### 2.4 Low — The view directory is never removed once a date prompt was shown
`server.rs:1615-1617` with `6282-6300`: the prompt file is never deleted and the cleanup is `remove_dir`, so the directory stays behind (three leftovers from earlier processes were found in `/tmp`). Pre-existing, not new in #228. Fix: remove the prompt file at exit after making it writable, as `remove_view_file` does.

### 2.5 Low (unverified, platform) — `~name/` reads only `/etc/passwd`
`server.rs:1918-1932` (`user_home`): on macOS users live in Directory Services and on NSS or LDAP systems elsewhere, so `~alice/` for a real user stays literal where Emacs's `getpwnam` finds the home; with `USER` unset even the current user goes through passwd. Acceptable if documented; `getpwnam_r` through libc would match Emacs.

Informational: `immediate_finish` with `jump_to_captured` on `org.roam.node.find` sends two `showDocument` for the same file; racing prefix-form rebuilds drop the older answer but the dropped job still ran in Emacs.

---

## 3. New findings

### 3a. Parser and display

#### D1. Medium — Emphasis can span any number of lines inside a paragraph
`server/org-syntax/src/objects.rs:500-527` (`match_emphasis`): the closing-marker search runs to the end of the text with no newline count. `org-emph-re` allows at most one newline in the body (the fifth element of `org-emphasis-regexp-components` is 1), and org-element uses the same regexp, so Emacs neither highlights nor parses a three-line emphasis.
**Verified**: `*c\nd\ne f*` in one paragraph gives one bold token over three lines, with both markers hidden under `org_hide_emphasis_markers`; expected plain text. `*two\nline*` is correctly bold and `*a\n\nb*` correctly not. Since the parser is shared, every consumer (export, lint, structure commands) sees the bogus object.
Fix: count newlines between the markers and give up at the second.

#### D2. Low-medium — `org.return` on a headline re-aligns tags even with `org_auto_align_tags` off
`server/org-core/src/newline.rs:196-201` (`split_headline`) always calls `tags_padding` with the tags column; `newline::Settings` has no `auto_align` and `server.rs:9563-9564` passes none, unlike `priority.rs` and `tags.rs`. Emacs's `org-return` keeps the tags where they were when the option is nil.
**Verified**: `* TODO Write the report      :work:` (tags at column 29), RET at column 12 with the option off: the tags move to column 71. Expected: column 29.

#### D3. Low — DONE headlines are not shaded
`server/org-lsp/src/highlight.rs:632-660`: the `done` modifier goes only on the keyword token. `org-fontify-done-headline` has defaulted to t since Org 9.4, so Emacs puts `org-headline-done` on everything after a done keyword (title, priority, cookies, tags), the span the `archived` handling already covers. **Verified**: `* DONE t [2/2] [100%] :a:` has no done marking on the title, cookie or tag. Needs the modifier documented in `protocol.org` and mapped in the editors.

#### D4. Low — Statistics cookies all get one face
`highlight.rs:596`: every cookie is `cookie` with no modifier. `org-get-checkbox-statistics-face` gives `[100%]` and `[n/n]` (including `[0/0]`) the done face. **Verified**. Fix: a `done` modifier when the text is `[100%]` or the two numbers are equal.

#### D5. Low — `#+TITLE:`, `#+AUTHOR:`, `#+EMAIL:`, `#+DATE:` values have no face
`highlight.rs:522`: the key is `keyword`, the value nothing; Emacs gives the title `org-document-title` and the others `org-document-info`. **Verified**. Needs a legend addition, so arguably a parity gap; not in `parity.org`.

#### D6. Very low (unverified) — `#+STARTUP:` is read from inside blocks
`server.rs:1705-1713` (`startup_words`) and `folding.rs:118-131` scan every line; `org-collect-keywords` only takes keyword elements, so a `#+STARTUP: overview` inside an example block is ignored by Emacs and honoured here.

### 3b. Capture, indentation, lint

#### C1. Medium — `%i` repeats the unexpanded template as the lead of its continuation lines
`server/org-core/src/capture.rs:462-467`: the lead before `%i` is taken from the raw template. `org-capture-fill-template` takes the current line of the capture buffer, where earlier escapes on the line are already expanded.
**Verified**: template `- %t %i` with a three-line region gives `- <2026-10-13 Tue> first line\n- %t   second line\n- %t third\n`, a literal `%t` on every continuation line. Expected the date on each. Same with two `%i` on a line.
Fix: compute the lead from the output built so far.

#### C2. Low-medium — A blank line inside a src block is pre-indented without the minimum with the first code line
`server/org-core/src/indent.rs:740-744` (`line_column`, src block arm) returns the begin line's indentation plus `org-edit-src-content-indentation`. `org-indent-line` takes the minimum of that and the indentation of the first non-blank line after the begin line (or the content indentation when that line is the end line). The recorded indent cases skip code lines.
**Verified**: `#+begin_src python\nx = 1\n\n#+end_src`, TAB or RET on the blank line gives column 2; Emacs gives 0. An indented block gives 4 where Emacs gives 2. Code written at column 0 is affected on every RET. Also, Emacs treats a whitespace-only line as blank here; the port only an empty one.

#### C3. Low — `spurious-colons` misses an empty tag at the edge of the group
`server/org-core/src/lint.rs:968-981`: `trim_matches(':')` strips all edge colons, while Emacs splits with one separator skipped at each end, so `:a::` and `::a:` contain an empty tag and are reported. **Verified**: `* Task :a::` and `* Task ::a:` give no report. Fix: strip exactly one colon at each end.

#### C4. Low (unverified) — Capture ignores `org-extend-today-until`
`capture.rs` never reads the setting (only `journal.rs` and `clock_range.rs` do). Emacs moves the time behind `%t`, `%T`, `%u`, `%U`, `%<…>` to the previous day before that hour, and datetree targets use `org-today`. A user who sets it (the journal honours it) gets the wrong day from an early capture.

#### C5. Very low — Small lint drifts
`lint.rs:127-133` strips a trailing `+` from property keys, so `:EFFORT+:` is checked by `invalid-effort-property` and `:CUSTOM_ID+:` counted by `duplicate-custom-id`, where Emacs sees different keys (port verified, Emacs from memory). `link-to-local-file` reports remote paths (`/ssh:host:/x`) that Emacs skips (unverified). `capture.rs:656-674` (`tidy`) drops whole blank lines at the ends where Emacs trims only newlines (unverified).

### 3c. Neovim plugin and docs (from the earlier version of this round)

#### N1. Medium — Neovim's and Vim's `S-TAB` have no CONTENTS state, but the docs claim it
`lua/org-lsp/folds.lua:128-143`: two states, overview and everything; `tests/nvim/e2e.lua:143-146` asserts that. `scripts/features.mjs:138` renders `docs/parity.org:82` as ✅ "overview, contents, everything" and `editors/neovim/TESTING.org:44` likewise. Emacs's `org-global-cycle` has three. The Vim plugin does the same two (`org_lsp.vim:2210-2220`). Fix: add a CONTENTS step, or change the row and check text.

#### N2. Low-medium (unverified) — Starting the server from a non-Org buffer can split the plugin over two servers
`lua/org-lsp/init.lua:162-174` (`with_client`) calls `vim.lsp.start` directly for `<Leader>oa` and `:Org` from a non-Org buffer. With a `vim.lsp.config('*', { root_markers = { '.git' } })`, Neovim 0.12's `lsp.start` ignores `root_markers`, so this client has no root; when an Org file opens later `vim.lsp.enable`'s path starts a second `org_lsp`, and `running_client()` (`init.lua:149`) returns the first, which never got `didOpen` for Org buffers. Fix: start through the same path `restart()` uses, or prefer the client attached to the current buffer.

#### N3. Low — Smaller Neovim items
- `setup()` after start deep-merges through `vim.lsp.config(SERVER, {...})` (`init.lua:1324-1330`) and never re-sends settings; `docs/neovim.org:42-43` says it can be called at any time while `configuration.org:320-323` says a restart is needed. Say so in `neovim.org`.
- Hidden leading stars reappear on the cursor line (`indent.lua:107-114` uses `conceal` marks that follow the user's `concealcursor`); a doc sentence.
- `init.lua:1035` depends on Neovim's private semantic-token namespace name; `indent.lua:118-124` never restores `breakindent` and `formatlistpat`; `view.lua:602` reuses an agenda window in another tabpage and switches to it; `.org.gpg` is not an Org filetype in Neovim (no doc promises it).

### 3d. Options named in the manual with no setting here, not in `parity.org`
`org-return-follows-link`; `org-cycle-emulate-tab`, `org-cycle-include-plain-lists`, `org-cycle-global-at-bob`, `org-cycle-max-level`, `org-cycle-level-after-item/entry-creation`; `org-cycle-level-faces`, `org-n-level-faces` (levels always wrap after 8); `org-fontify-done-headline`, `org-fontify-todo-headline`, `org-fontify-whole-heading-line`, `org-fontify-quote-and-verse-blocks`, `org-hidden-keywords`, `org-hide-macro-markers`; `org-emphasis-alist`, `org-emphasis-regexp-components`; `org-highlight-latex-and-related` (fragments always get the `latex` token; Emacs's default gives none); `org-show-context-detail`; `org-fold-catch-invisible-edits`; `org-ellipsis`; `org-script-display`; `org-src-tab-acts-natively`; `org-src-lang-modes` and the Babel language set (lint's language and header tables are fixed, so a user's extra modes are reported as unknown); `org-time-stamp-rounding-minutes` exists as a setting but `read_date.rs` and `capture.rs` never round.

---

## 4. Checked and found sound this round

- Every #228 server fix as listed in section 1, by re-running its trigger; the `timep` regexp, `url.parse()` keeping valid URLs byte for byte, the sparse-tree refactor leaving existing tests green, edits from `todo::edits` ordered and non-overlapping under 2000 random pairs.
- Display: link concealment with `literallinks`/`descriptivelinks`; emphasis pre/post rules with CJK, quotes and parentheses, `*bold*` inside a word, `=v*b*v=`, two-line emphasis, nested markers in body, title and cells; entities (`\alpha`, `\alpha{}`, `\alphabet`, not on comment lines, hidden token with correct UTF-16 offsets); sub/superscripts under t/`{}`/nil; `$` fragment rules, `\(`, `\[`, `$$` across lines, environments; level faces; COMMENT and ARCHIVE; TODO/done keywords; priorities, checkboxes, counters; planning, clocks, timestamps; blocks, fixed width, macros, footnotes, targets, radio links; tokens never overlapping conceal ranges.
- Folding: subtree ends under `org_cycle_separator_lines` 2/1/0/-1 and at EOF, items, blocks, LaTeX environments, drawers, unclosed blocks; startup folds for every `#+STARTUP` value and `VISIBILITY`; `hideblocks`/`nohideblocks`; `reveal.rs` matches `org-fold-show-set-visibility`; Neovim's TAB states right.
- `RET`: tables (next row, new row after the last or a rule, blank field, bare newline before the first bar and after the last, `#+TBLFM:` and formula fields), headlines (start, in priority, tags, keyword, `* ` alone, COMMENT), items (empty, around checkboxes, continuation lines, description tags, nested, ordered), property drawers, planning, keywords, fixed width, footnote definitions, example blocks, src code refused, EOF, empty file.
- Lint: 60 probes across the priority checkers (misplaced headings in blocks and cells, duplicates with case rules, block and drawer ends, local file links with `~`, spaces, `::` options and links inside comments and blocks skipped, affiliated keywords, every listed header argument and value, timestamp syntax, footnotes, empty headlines, effort and ID properties, languages, `#+OPTIONS`, bibliographies): consistent with Emacs's checkers. 61 names present in `checkers()`.
- Indentation: lists, example/quote/verse blocks, block end lines, drawers and property alignment, footnote definitions, keywords, comments, EOF, tabs never emitted, `org-adapt-indentation` t/nil/headline-data per line and region match Emacs and the recorded cases.
- `org-read-date`: 130 answers (`+3d`, `++1w`, `-2m`, `fri`, `Fri 14:00`, `15:30` under each `prefer-future` value, `2026-10`, `10/13`, `13.10.`, time ranges, `tue 2w`, `+0`, `.`, `w42`, `w42 fri`, `w1`/`w53` at year boundaries, `--` ranges, `10:00+1:30`, `1/2`, two-digit years, compatible dates on and off, defaults from an existing stamp) agree with `org-read-date-analyze` step by step.
- Capture: `%t %T %u %U %<fmt> %a %A %l %L %:link %:description %n %f %F %k %K %c %x %%`, backslash escapes, `%^{p|default|c1|c2}`, `%^t/T/u/U`, `%^g`/`%^G`, `%^{PROP}p`, `%^C`/`%^L`, `%\n` back references, expansion order; placement covered by 240 recorded cases.
- Neovim: key mapping from the manifest, insert-mode `<CR>` with completion popups, `g?` help, `:Org` completion, `vim.lsp.config`/`enable` for 0.12, `indentexpr`, `foldexpr` with startup folds, sparse trees, the agenda view buffer handling, the date prompt, `display.lua`, `highlights.lua`, `clock.lua`, `columns.lua`, `buffers.lua`.
