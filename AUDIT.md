# Audit findings, round four

A fourth review of org-lsp after the third round's fixes landed on `main`
(PR #227) together with #226 (`%(…)` in `org-agenda-prefix-format` through
Emacs). This file replaces the earlier rounds; their text is in this file's
git history. Parts:

1. the status of every round-three finding,
2. regressions and residue from those fixes, and a review of #226,
3. new findings in areas no round had examined for correctness: tags,
   properties, priorities, sparse trees, motion and link search; org-roam
   and org-journal; the docs website,
4. what was checked and found sound.

"Verified" means the trigger was run here; "unverified" means established
by reading. `check.sh` passes on the merged tree (the VS Code type check
needs `npm ci` in `editors/vscode` first in a fresh container). One
reviewer fetched Org 9.8.10's `org.el` and `org-table.el` to settle three
questions by source; everything else about Emacs comes from memory and the
recorded cases.

A correction to round three: **E3 was wrong.** `org-table--shrink-columns`
(Org 9.8.10, `org-table.el:4176-4183`) sets `align` and `width` only when
unset, so the *first* alignment and the *first* width of a column win, each
on its own, and its regexp requires digits so `<l>` alone is not a cookie.
The fix in #227 does exactly this.

Deliberately left as they are, per the maintainer (unchanged list):
diagnostics debounce, caching the workspace file list, Sublime
file-and-position paths, coc.nvim on Windows and the Neovim edit version
check, roam finalize and jump-to-captured after a refiled capture, the
encryption docs, tangle paths, the synchronous Enter/o/Tab waits, and the
`zed-extension` hash.

---

## 1. Status of the round-three findings

| ID | Finding | Status |
| --- | --- | --- |
| 2.1 | persistent marks lost after bulk actions | Fixed for `s`/`d`; **still lost for actions that rewrite adjacent headlines**, see 2.1 below |
| 2.2 | refiled capture's clock points at the wrong file | Fixed, test passes |
| 2.3 | hung `start()` job holds Emacs forever | Fixed (`LATER_TIME` 1800 s, `Done { Err }` delivered, Emacs restarted) |
| 2.4 | rejected `applyEdit` clock residue | Fixed (`ClockState` restored only when the command changed the clock) |
| 2.5 | `Caller::call` abandons a job | Fixed (`begun` set before `starting` cleared) |
| 2.6 | VS Code restart chain | Fixed; `deactivate` is still outside the chain, see 2.6 below |
| 2.7 | Neovim buffer lookup, image cache | Fixed; two small residues, see 2.7 below |
| 2.8 | Zed over-claimed checks | Fixed (exactly the three named) |
| 2.9 | `stuck` in the schema | Fixed, generated |
| R4 | `write_whole` and symlinks | Fixed for dangling links; hard links are written in place and that write is not atomic, see 2.3 below |
| R6 | view directory made eagerly | Still made at `initialize` for clients without virtual documents (`server.rs:1269`, `prompt_file`); verified with the binary, removed at exit |
| E1 | `#` rows refuse TAB | Fixed, five Emacs cases recorded, cursor back in its column |
| E3 | shrink cookies | Fixed; the audit was wrong, see above |
| E4 | CRLF tables | Fixed for pure-CRLF files (TAB, `C-c RET`, `#` rows, `#+TBLFM`, recalculation, formatting all restore `\r`); **mixed endings still mangled**, see 2.4 below |
| A1 | `<+Nm>`/`<+Ny>` | Fixed (31 and 365.25 days); `<+Nh>` is still a time comparison where Emacs compares strings (`matcher.rs:309`), and an unknown unit becomes time 0 |
| A2 | `PRIORITY` as text | Fixed, matches 9.8.10's `org-priority-to-string` |
| A4 | `org-class` split | Fixed |
| L1 | diagnostics on close | Fixed |
| L2 | non-ASCII URLs | Fixed; a bare `%` (`https://example.com/100%`) is still dropped since `%` is kept as is and then fails to parse |
| L3 | shutdown handling | Fixed, verified with the binary (codes, `InvalidRequest`, cleanup) |
| L4 | code action kinds | Fixed (`org` and `quickfix` announced; `only: ["org"]` works) |
| L5 | link path normalisation | Fixed for `./`, `../`, `~/`, `USERPROFILE`; `~name/` for an unknown user becomes `<home parent>/name` (`server.rs:4281-4284`) where Emacs leaves it alone, and real homes outside the parent directory are wrong |
| W1 | symlink spellings | Fixed for `file_text` and `agenda_files` with a per-document cache; `id_paths`/`ids_on_disk` (`server.rs:4381-4418`) still compare by spelt path, so an ID removed in the editor stays known from disk until the next scan |
| W2 | `**` glob loops | Fixed |
| 3e | test waits | Fixed (10 s and 5 min) |
| S5 | ImageMagick synchronous | **Still synchronous** (`images.lua:208`, `:wait()` with 10 s on the main loop). Not on the maintainer's left-as-is list |

---

## 2. Regressions and residue from the round-three fixes, and #226

### 2.1 Medium — Persistent marks are still lost by any bulk action that rewrites adjacent headline lines
`server/org-core/src/todo.rs:1753-1795` (`edits`) with `server/org-lsp/src/document.rs:50-58` (`Change::moved`) and the follow at `server.rs:2776-2790`. When a run of changed lines has no common line, `edits` emits one block edit covering all of them; the server's own `applyEdit` for bulk `t`, `+tag` and the like is therefore a single edit spanning every marked headline. The editor applies it as one change, `Change::moved` sends every offset inside the block to its start, and the follow puts the second and later marks mid-way through the first headline.
**Verified** over LSP: `#+TODO: TODO NEXT | DONE`, three `* TODO` headlines, persistent marks, mark all, bulk `t` to NEXT, apply the edit as whole text or incrementally: only Alpha stays marked, and the next bulk `s` says "Acted on 1 entries, skipped 2 (disappeared before their turn)". The merged test uses only `s` and `d`, which insert lines after headlines.
Fix: in `todo::edits`, when a block replaces n lines by m, pair them one to one as separate edits (the edits get smaller for editors too); or re-attach kept marks by `views::resolve` after the `edited` refresh.

### 2.2 Medium-low — `%(…)` prefix forms (#226) block the main loop on every view build
`server.rs:4845`: `render_with`/`org_text` call `self.emacs.call("agenda-prefix", …, 10 s)` synchronously on every agenda build: every `didSave` of any open file (`refresh_view`), every server-caused `didChange`, every agenda command that re-renders, and the first call also waits for Emacs to start. Emacs creates a buffer per agenda file and runs `org-mode` on the full text (`bridge.el:514-524`). A slow form over many entries past 10 s is LATE, which kills Emacs and empties every field with a warning. The view is also built twice whenever any form is missed. Unverified by execution (no Emacs here).
Suggest a `Later` job with the view rebuilt on `Done`, as exports do, or a size-dependent deadline.
Otherwise #226 is sound: the form comes only from settings (never from a document), crosses as JSON and is `read` and evaluated in Emacs with no string interpolation, one batched call per render deduplicated by form and entry, one failing form empties its line only, no Emacs or a timeout gives one warning. The trust boundary is the same as `emacs_config` and diary sexps already have. One fragility: the `Evaluator` is attached to `self.agenda` at start (`server.rs:1314`), so any future rebuild of `AgendaConfig` from options must carry it over or forms silently go empty.

### 2.3 Low — Hard links are written in place, non-atomically
`server.rs:2221-2229` (`replace_whole` for `nlink > 1`): `truncate(true)`, then `write_all`, then `sync_all`. A crash or full disk between the truncate and the write loses the file's contents, where every other write is now atomic. The commit calls it "as Emacs writes it", which is true. Acceptable if documented; otherwise write a temp file first and copy its bytes back on success, which keeps both names.

### 2.4 Low — Mixed line endings are still mangled
`text::crlf` is true only when every line ends in `\r\n`. **Verified**: `| a | bb |\r\n|-\n| ccc | 1 |\r\n` still gets `\r` as a cell from `table::run` and `format::format`. Suggest converting whenever any `\r\n` is present and remembering per line, or treating a file as CRLF when most lines are.

### 2.5 Low — Whole-text `didChange` cost
`todo::edits` builds an LCS table up to 4 000 000 cells; a 2 000-line file whose first and last lines both change takes about 200 ms per `didChange` in a debug build, then falls back to one edit above the cap. Only clients that send full text pay it (the server announces incremental sync). The `Vec<Vec<u32>>` could be one flat `Vec`.

### 2.6 Low — `deactivate` is outside the VS Code restart chain
`editors/vscode/src/extension.ts:2131-2133`: `deactivate` calls `stopClient()` directly. If it runs while a restart is between its stop and its start, it stops nothing and the chained `startClient` starts a server after deactivation. Unverified. Fix: `restarting = restarting.catch(() => undefined).then(stopClient); return restarting;`.

### 2.7 Low — Image cache residue
`lua/org-lsp/images.lua:205-214`: the temporary name is `png .. '.part'`, the same for every converter of a key, so two Neovim instances previewing the same new image can leave a truncated PNG that `png_size` then rejects and `remember(key, false)` keeps for the session without removing the file. Orphaned `.part` files are never pruned (`prune` matches `%.png$` only, `:168`). Fix: a per-process suffix, prune `.part` too, remove a cached PNG when `png_size` fails on it.

---

## 3. New findings

### 3a. org-journal and org-roam

#### J1. High — Any journal file format with `%a`, `%A`, `%b` or `%B` turns org-journal off
`server/org-core/src/journal.rs:264` emits `\p{Alphabetic}` for Emacs's `[[:alpha:]]`, but `server/org-core/Cargo.toml:12` builds `regex` without the `unicode-bool` feature, so the pattern fails to compile ("Unicode property not found") and `Files::new` (`journal.rs:300-303`) silently falls back to a regex that never matches. The same happens to `created_pattern` (`journal.rs:460`) when the CREATED timestamp format has a day or month name.
**Verified** (the regex failure reproduced independently): with `org_journal_file_format = "%Y-%m-%d-%A.org"`, no file is a journal file; a new entry fails with "Failed to extract year from file", previous-entry says "called outside calendar/org-journal mode", search and the agenda see nothing; with `%A` in the CREATED format every entry of a weekly file errors "doesn't match CREATED property value". Emacs matches these names.
Fix: add `"unicode-bool"` to the regex features, or emit `[\p{L}\p{M}\p{Nl}]`, and make `Files::new` and `created_pattern` report a build error instead of swallowing it.

#### J2. Medium — `id:` links to org-roam nodes cannot be followed unless the node's file is open or an agenda file
`server/org-lsp/src/server.rs:4379-4389` (`id_paths`: open documents, then agenda files) and `:4438-4446` (`find_id`), used by `open_location`, hover and go-to-definition; nothing consults the roam database. In Emacs org-roam's `org-roam-id-open` finds the node in the database, and `org-roam-db-sync` registers every node in `org-id-locations`.
**Verified** over LSP: `a.org` holds `[[id:b-id][B]]`, `sub/b.org` holds the headline node; `org.open` answers `Cannot find entry with ID "b-id"`, definition is `null`. `known_ids` uses the same lookup, so lint likely flags such links as unknown (unverified).
Fix: in `find_id` and `known_ids`, when roam is on, look the ID up in the roam database.

#### J3. Medium — Capturing to an existing headline node with a `file`/`file+head` template duplicates the node's ID at the file level
`server/org-lsp/src/server/roam.rs:468-492` (existing node: `file` is the node's file, `id` the headline's ID) and `server/org-core/src/roam/capture.rs:124-187` (for `file` targets `id_point = 0`, and the ID is written there when the file has none).
**Verified** (org-core): `#+title: Notes\n\n* Alpha\n:PROPERTIES:\n:ID: alpha-1\n:END:\n`, capture to node `alpha-1`: the file gains a file-level `:ID: alpha-1` while `* Alpha` keeps its own; `Db::add` then keeps the file node and drops Alpha, so backlinks and finds point at the wrong node. The Emacs side is from memory (org-roam creates a fresh ID at the target point rather than copying the node's); either way Emacs does not make two entries with one ID. Worth a recorded case; the capture recorder only covers new nodes.

#### J4. Low-medium — `immediate_finish`, `jump_to_captured` and `clock_in` are ignored on roam capture templates
`server/org-lsp/src/server/roam.rs:644-669` (`roam_template` drops them) and `:510-528` (always `show_capture`). `plugins.org` says the templates are written as `org_capture_templates`, where `immediate_finish` is documented; in Emacs org-roam's templates go through `org-capture`, so `:immediate-finish t` files the node at once.
**Verified** over LSP: a template with `immediate_finish` and a new title shows a capture buffer and writes no file.

#### J5. Low — Carryover can delete a journal file the editor has open
`server/org-lsp/src/server/journal.rs:381-384`: a file org-journal emptied (`org_journal_carryover_delete_empty_journal`) is removed from disk even when open in the editor; the buffer still holds the old entry with the items just moved to today, and saving writes it back, so the items exist in both files. Emacs kills the buffer first. Unverified. Fix: refuse to delete an open file, or send an edit emptying it.

Documented but worth a sentence: `format_time` writes `%x`, `%A`, `%p` in the C locale. A journal Emacs kept under `en_US.UTF-8` has `* Friday, 10/09/2026` headings; org-lsp looks for `* Friday, 10/09/26`, finds none, and adds a second date heading for the day. Also, org-lsp neither reads nor writes `org-id-locations-file`; the docs do not say so.

Not implemented and not documented as missing while `parity.org` reads the roam section as complete: org-roam dailies entirely (directory, capture templates, the `org-roam-dailies-*` commands; `plugins.org` only says `file+datetree` is refused), `org-roam-node-formatter`, `org-roam-db-node-include-function`, `org-roam-db-extra-links-elements` and `-exclude-keys` (hard-coded), `org-roam-extract-subtree`, `org-roam-refile`, `org-roam-promote/demote-entire-buffer`, `org-roam-ref-find`, `org-roam-graph`; org-journal `org-journal-carryover-headings-only`, `org-journal-find-file`, `org-journal-follow-mode`, `org-journal-enable-cache`, `org-journal-encrypt-on`.

### 3b. Tags, properties, priorities, sparse trees, motion, link search

All low; nothing corrupts text or loses data.

- **`file:x.org::/regexp/` and `[[/regexp/]]` are refused** (`server/org-core/src/open.rs:661-663`, "Searches by regular expression are not supported"). `org-link-search` runs `org-occur` and shows a sparse tree. Verified. Not in `parity.org`. Fix: route `/…/` searches through the sparse-tree path, or document.
- **Setting `SCHEDULED`/`DEADLINE` as a property is refused** (`properties.rs:842`, "Set SCHEDULED with its own command"). `org-entry-put` calls `org-schedule`/`org-deadline` with the value (empty removes). Verified. Fix: return a planning change and route it, or document.
- **Priority changes realign tags even with `org_auto_align_tags` off** (`edit.rs:220-225` builds `priority::Settings` without the flag; `priority.rs:306` always pads). `org-priority` ends with `(when org-auto-align-tags (org-align-tags))`. Certain from the mechanism; visible only with the option off.
- **`S-up`/`S-down` do not see cookies Emacs reads** (`priority.rs:169-181` accepts one alphanumeric or 10 to 64; `org-priority-regexp` is `[A-Z0-9]+`). Verified: `* [#AB] x` + up gives `* [#B] [#AB] x`, two cookies. Garbage in, but Emacs removes the out-of-range cookie instead.
- **`C-c C-c` on a headline whose LOGBOOK holds a checkbox item** (`checkbox.rs:138-142` skips drawers before the first item; `org-toggle-checkbox` takes the first item after the heading, drawer or not, as the reference box). Unverified; worth one recording from the headline.
- **"Setting file tags is not supported yet"** (`tags.rs:534`): whether Org 9.8's `org-set-tags-command` sets `#+FILETAGS` before the first headline or errors could not be settled from memory; no recorded case has the cursor there. Worth a recording.

Options named in the manual with no setting here (defaults always apply; not in `parity.org`): `org-id-method`, `org-id-prefix`, `org-id-ts-format` (IDs are always UUIDs), `org-property-format`, `org-effort-durations`, `org-fast-tag-selection-include-todo`, `org-tags-match-list-sublevels`, `org-occur-case-fold-search`, `org-sparse-tree-default-date-type`, `org-link-search-must-match-exact-headline` (no "create as a new heading?" offer either), `org-priority-enable-commands`, `org-indent-indentation-per-level`, `org-description-max-indent`, `org-cite-insert-processor`.

### 3c. Docs website

- **Low, verified** — Search result highlighting corrupts its HTML with two terms (`site/assets/app.js:141-148`): terms are applied one after another on the already-marked HTML, so a later term matching inside `<mark>` or an entity breaks the markup (`highlight('the agenda marks', ['agenda','mark'])` gives `<<mark>mark</mark>>agenda</<mark>mark</mark>>`). Fix: one alternation applied once on the plain text, escaping as it is emitted.
- Notes: the README's logo link copies `site/assets/logo.svg` a second time under `site/site/assets/`; the README screenshots have empty `alt`.

---

## 4. Checked and found sound this round

- #227 server fixes confirmed as listed in section 1, each by re-running the round-three trigger: refiled capture clock, `Later` deadlines with restart, clock state restore, the `begun`/`starting` order, dangling links (target created, link kept), `#` rows on every table key with the cursor back in its column, pure-CRLF tables, `PRIORITY` text per 9.8.10, `org-class`, diagnostics cleared, non-ASCII URLs with `a%20b` kept as is, shutdown codes and cleanup with the real binary, code action kinds, `./` and `../` paths, symlinked agenda files, `**` loops, longer test waits.
- #226 wiring: settings only, JSON across, batched, per-form errors isolated, warning once, the terminal agenda leaves forms empty, docs and checks present for all six editors.
- Editor fixes: `buffers.lua` compares normalised names case-insensitively on Windows and macOS only and creates nothing; `images.lua` forces the PNG coder, renames atomically, refreshes mtime on reuse, caches nothing on failure; the three Zed checks re-excluded and nothing else lost; `stuck` generated everywhere; `update-table-fixtures.py` adds five `#`-row cases and runs under `python3 -I`; the Vim test wait is five minutes.
- Website: 20 pages build with no broken links, every page in NAV and the search index, one `<h1>` on the landing page, anchors, images, tables, description lists and source languages all render, the theme switch reads `localStorage` in `try/catch`.
- Tags, properties, priorities, sparse trees, radio targets, motion, link search, virtual indentation, citations and IDs: every behaviour probed matched the recordings and Emacs (2160 tag cases, 433 property cases, 517 motion cases, 88 sparse cases, 333 citation cases plus fuzzing).
- org-journal: daily/weekly/monthly/yearly naming with week starts and year boundaries, directories created for `%Y/%m/%d.org`, date headings with case folding, carryover of nested items, `org_extend_today_until`, search, scheduled entries. org-roam: file detection, duplicate-ID rule, aliases, refs, slugs with accents and CJK, display templates, find and insert, region descriptions, `roam:` links, tag/alias/ref edits, backlinks, cache invalidation by text hash or mtime.
