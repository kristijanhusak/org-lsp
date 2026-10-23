# Audit findings, round eight

An audit of `main` at b38c1bd (#388), which gained 32 pull requests since
round seven: the round-seven fixes (#357, #358, #360), the end-to-end round
(#366-#369), `emacs_options` (#362-#366), `org-lsp mcp` (#371-#381), `org-lsp
notify` (#377-#388), code-block ranges for editors (#386) and the new
highlights (#370, #387). `scripts/check.sh` passes on it here. Earlier
rounds are in this file's git history.

Seven reviewers, one each for: the MCP server, `notify`, `emacs_options`
and configuration loading, the rest of the server, `org-core` and the
parser, the editor plugins and scripts, and the parity and documentation
check. Every finding below was verified by running unless it says
"by reading" (no Emacs, macOS or Windows is available here); the High and
Medium ones were reproduced a second time by the author of this file.

The maintainer's own `.claude/e2e/issues.md` (95 items from the end-to-end
round) was read; nothing it marks fixed is re-reported unless the fix is
wrong, and nothing it marks "not a bug" is re-reported.

Deliberately left as they are, per the maintainer, and not reported:
diagnostics debounce, workspace file list cache, Sublime paths, coc.nvim on
Windows, the Neovim edit version check, roam finalize after a refiled
capture, the encryption docs, tangle paths, the synchronous Enter/o/Tab
waits, the `zed-extension` hash.

---

## 1. Status of the earlier findings on `main`

| ID | Finding | Status |
| --- | --- | --- |
| P1/P2 | outline and `hides_stars` quadratic | **Fixed** (#358), timed in release: outline of 8 000 lines 9.5 ms, 30 000 lines 38 ms, 120 000 lines 167 ms (linear); virtual indent 0.4 / 1.8 / 5.6 ms; with inline tasks on 8.9 / 47 / 177 ms. `outline.rs:523` takes `min_stars` once; the remaining `startup_words` callers run once per command |
| P3 | remote images on the main loop | Fixed; residue in 2.16 |
| P4 | duration regexes per call | **Fixed**: `patterns()` caches per unit list (thread-local), `BARE` is a `LazyLock`; 3.4 µs per `to_minutes_with` call |
| P5 | `id:` lookup parses every file | Fixed |
| P6 | parser quadratic scans | **Fixed**: `key_from`, `inline_name_len` and the diary `)` search are memoised per scanner; 53 hostile 100 KB paragraphs parse in at most 41 ms and scale linearly to 1.6 MB. The new `token_at_offset` agrees with rowan's at 1.66 M offsets and is 9-90× faster |
| P7 | 557 KB search index on every page | **Fixed**: loaded lazily from `assets/search.js` when the search is first opened |
| P8 | subprocesses without a deadline | Fixed; `process.rs` reads both pipes on threads with `take(size + 1)`, kills the group on Unix, and its tests (`yes & yes`, `trap '' PIPE`, 300 MB to stderr) pass |
| P9 | Neovim preview options dump, foldtext per character | **Fixed**: `preview.lua:245-251` returns early unless a `nofile` buffer with an `org` fence; `folds.lua:214-250` reads the line's extmarks once |
| S1 / 7.2.2 | `man:` words as options or files | **Fixed**: `man_refused` refuses `-` and `/` words and is called before `run` (`server/manual.rs:58`) |
| S2-S5 | Windows quoting, open-source confinement, clock save, socket path | Fixed |
| S6 | `install.sh` silent checksum skip | **Fixed**: a missing `SHA256SUMS` is announced, any other failure exits 3, a missing `sha256sum` is announced |
| C1 | debug overflow panics | **Fixed** for every user-written number tried (dates, repeaters, cookies, ~70 `#+TBLFM` formulas, column widths, durations); two unchecked spots remain, 2.8 and 2.9 |
| C3 | extract path as capture template | Fixed |
| C4, 4a.1-3 | smaller divergences, message texts | Not re-checked this round beyond what the end-to-end round fixed (`shell:` prompt and output limits, `elisp:` prompt, checkbox `C-c C-c`: consistent now, see section 3) |
| C5 | editor and test items | Partly: `view.lua`'s and Vim's path echo now guard buffer and mode; `view.lua:827-831` still retries `show_document` without a `pcall` (Low); the rest not re-checked |
| 4b | Helix and Zed checklists, `docs/vim.org`, `docs/sublime.org` | **All right now**: every "left out" command named in `docs/helix.org` and `docs/zed.org` is in neither a key nor a code action (computed from the manifest, `keymap.json`/`tasks.json` and `code_actions_at`); the Vim version matches `has('patch-9.0.1485')`; the Sublime wrapping note matches `features.mjs` |
| 7.2.1 | curl without `--globoff` | **Fixed**, `remote_images.rs:385`, with `--` before the URL and a size check after the download (`:395`) that covers chunked responses on curl older than 8.4 |
| 7.2.3 | program output read whole | Fixed (`take(size + 1)` on reader threads) |
| 7.2.4 | group kill Unix-only | As before: on Windows only the child is killed (documented in the module header) |
| 7.2.5 | remote image residue | Partly: one download at a time, stale jobs dropped at their turn; new residue in 2.16 |
| 7.2.6 | `cmd /C` through `Command::arg` | **Fixed** with `raw_arg` (by reading; a line with a newline runs only its first line under `cmd`, harmless) |
| 7.2.7 | empty `working_directory` | **Fixed**: an absolute directory is required after `~` expansion; its unit test covers "", "." and a relative name |

---

## 2. New findings, by severity

### 2.1 High — `org-lsp mcp`: a file listed twice is refused as ambiguous, which the default configuration does

`server/org-lsp/src/mcp.rs:540-546` (`editable_files` = agenda files +
capture target files, no deduplication), `:1173-1176` (`refile_editable`
adds every `org_refile_targets` file on top), `:583-590` (a bare name is
looked up among them and refused when two entries carry it, even when both
are the same path).

The views label entries `[tasks.org:12]` and `docs/mcp.org` says that
label is what the edit tools take. `editable_file` first tries the name
relative to the server's working directory; that only matches when `org-lsp
mcp` is started in the agenda directory (the inline tests do, which is why
they pass). Started anywhere else, as Claude Desktop or Claude Code do, the
by-name branch runs and finds the file twice: with the default
`org_refile_targets` (the agenda files) `refile` and `refile_targets` never
work by name, and with a capture template targeting an agenda file (the
usual inbox setup) `set_todo_state`, `schedule`, `deadline`, `set_tags`,
`set_property`, `set_priority`, `set_effort` and `add_note` all refuse too.
`delete_entry` and `archive` use the agenda files alone and are fine.

Reproduced twice (config with `org_agenda_files = [".../org/tasks.org"]`,
`mcp_edits = true` and one `file+headline` template on that file, server
started in another directory):

```
set_todo_state {"file":"tasks.org","line":2,"state":"DONE","dry_run":true}
refile_targets {"file":"tasks.org"}
-> "More than one file is named tasks.org: give its path (/…/org/tasks.org, /…/org/tasks.org)"
```

Deduplicate by `canonical()` in `editable_files` and `refile_editable`, or
before counting `named`.

### 2.2 High — the agenda is quadratic in diary sexps: a line number is recounted from the start of the file per sexp and day

`server/org-core/src/agenda/mod.rs:862` (`Builder::sexps`) and `:1196`
(`Builder::timestamps`, for `<%%(…)>` stamps) build a `diary::Place`
eagerly for every sexp on every day of the view, and `line_number`
(`:2171`) counts newlines from byte 0, so the cost is days × sexps × file
size. Introduced by #366 (`git log -S"fn line_number"`), and the place is
only needed when the sexp falls through to Emacs.

Release timings, a 60-day view, entries with a 300-byte body and one
`%%(diary-date 1 1 2030)` line each:

| Sexps | Size | Time | Same file with `<2030-01-01 Tue>` instead |
| --- | --- | --- | --- |
| 1 000 | 343 KB | 5.3 s | 0.4 ms |
| 4 000 | 1.37 MB | 78.6 s | 0.9 ms |

With `line_number` stubbed to a constant the 1 000-sexp view takes 17 ms.
Compute the place lazily (a closure evaluated only in the `outside`
branch) or index the newlines once per file. This hits the LSP agenda,
`org-lsp agenda`, `org-lsp mcp` and `org-lsp notify` alike.

### 2.3 Medium — `org-lsp agenda` and `org-lsp mcp` lose every setting, or fail whole, on one malformed option in the configuration file

`server/org-lsp/src/cli.rs:150-153` (`agenda_configured`) and
`server/org-lsp/src/mcp.rs:504` (`build_view`) pass `loaded.merged`, the
raw merged JSON before the field-by-field `Options::read` dropped what it
cannot read, as `--options` to `agenda_view`. There (`cli.rs:233-237`) it
is parsed three times: `AgendaOptions` and `InheritanceOptions` with `?`,
so any wrong-typed field is a hard error, and `Options` with `.ok()`, so
any wrong-typed field drops every server setting at once (TODO keywords,
priorities, inline task level). The server itself is lenient per field.

```
org_todo_keywords = ["TODO", "NEXT", "|", "DONE"]
org_agenda_files = ["a.org"]
org_log_done = 5            # wrong type
$ ORG_LSP_CONFIG=config.toml org-lsp agenda --view todo
org-lsp agenda: the option `org_log_done` is malformed and is ignored
Global list of TODO items of type: ALL
  a:          TODO Task one          <- "NEXT Task two" is gone; exit 0
```

With `org_agenda_span = {}` instead: `--options: data did not match any
variant of untagged enum SpanOption`, exit 2; the same for
`org_use_tag_inheritance = 5`. Pass `serde_json::to_value(&loaded.options)`
(it is `Serialize`) or the `keep_readable` result instead of `merged`.

### 2.4 Medium (by reading) — `org_src_lang_modes` and `org_babel_tangle_lang_exts` make every Emacs command fail

`server/org-lsp/src/config.rs:2118-2138` emits `(setq org-src-lang-modes
(cons (cons "ipython" (intern "python")) org-src-lang-modes))` (and the
same shape for `org-babel-tangle-lang-exts`); `server.rs:1680-1690` puts
that Lisp in `config`, which `bridge.el:90-93` evaluates **before**
`(require 'org)` at `:95`, in `emacs -Q --batch`. Neither variable carries
an autoload cookie (checked in `org-src.el:214` and `ob-tangle.el:59` of
Org 9.7, the same in 9.8), so the expected result is `Symbol's value as
variable is void: org-src-lang-modes` for every Emacs command of a user
who sets either option; the only test with `org_src_lang_modes`
(`tests/lsp.rs:747`) does not go through Emacs. Pre-existing, in the path
`emacs_options` now shares. Wrap the forms in `with-eval-after-load`
(`'org-src`, `'ob-tangle`), as `emacs_options` does for its own.

### 2.5 Medium (by reading, macOS) — `notify`: a title starting with `-` is parsed by `osascript` as options

`server/org-lsp/src/notify.rs:758-770` runs `osascript -e "on run argv" -e
… -e "end run" <title> <body>` with no `--`. `osascript` reads its
arguments with getopt, so a title beginning with `-` is an option, and
`-e` adds a script line. The default title (`Appointment in N minutes`) is
safe; with `notification_title_format` starting with an entry placeholder
(`"%h"`, `"%p"`, `"%c"`) a headline `* -l JavaScript` breaks the script
(reminder lost, exit 1 every minute), and `* -e property p : (do shell
script "…")` is evaluated when the script is compiled. A dry run shows the
title that would be passed:

```
$ org-lsp notify --dry-run --now "2026-10-22 10:00" --options '{"notification_title_format":"%h"}'
-e do shell script "touch /tmp/pwned"
```

Add `--` after the last `-e` statement, or pass title and body through the
environment as the Windows path does. `notify-send` already gets `--`;
`terminal-notifier` takes pairs, so it is safe.

### 2.6 Medium — semantic tokens are quadratic in hidden ranges, and #387 tripled the token count

`server/org-lsp/src/highlight.rs:772` (`with_hidden`) and `:736`
(`with_radio_links`) scan every range for every token and then every token
for every range. #387 splits a bracket link into 3-5 tokens, so for a
client without `org-lsp/display` and anything `stand_ins` returns
(`org-pretty-entities`, sub/superscripts, bullets, shrunk table fields) one
`textDocument/semanticTokens/full` on a 10 000-line file with one entity
per line went from 37 ms (130 600 tokens, no hidden ranges) to 2.3 s
(release; CRLF 1.9-2.2 s). Both lists are sorted: merge them in one pass.

### 2.7 Medium — `org-lsp mcp`: `roam_find` and `roam_node` are quadratic in the number of nodes

`server/org-lsp/src/mcp.rs:1880` finds each candidate's node with a
linear scan of `db.nodes`, and `node_reference` (`:1846`) clones the whole
file label list once per node listed. Release: 9 000 nodes 1.8 s / 1.4 s,
30 000 nodes 14 s / 6 s, 90 000 nodes 85 s / 20 s (`roam_find` with a
`query` still walks every candidate). Index the nodes by id and compute
the labels once.

### 2.8 Low — capture template widths: overflow panics and an uncatchable allocation abort

`server/org-core/src/capture.rs:295` accumulates a `%<…>` width unchecked
and `:342` pads with it; `text.rs:54` and `:287` (`format_strings`) do the
same for `%Ns`. `%<%99999999999999999999Y>` panics with an overflow in
debug, `%<%18446744073709551615Y>` with `capacity overflow`, and `* %<%99999999999Y>`
aborts the process with `memory allocation of 99999999995 bytes failed`
in release too (not a panic; the server dies). Sources are the user's own
templates (`org_capture_templates`, org-roam templates,
`org_roam_extract_new_file_path`, `org_property_format`), so this is a
robustness item, not an attack. Cap the width.

### 2.9 Low — a client-supplied prefix count of ±i64::MAX panics (debug)

`server/org-core/src/edit.rs:225` (`count * -1` with `i64::MIN`),
`stamp.rs:530-534` and `date.rs:60,64,127` overflow when a client sends
`count` near the limits to a timestamp shift. Only a hostile LSP client
sends one; release wraps to a nonsense date.

### 2.10 Low — `notify`: `-NNNh` warning of a timestamp overflows

`server/org-lsp/src/notify.rs:457` `hours * 60` is unchecked and runs for
every entry, even with `notification_use_timestamp_warning` off.
`<2026-10-22 Thu 12:00 -999999999999999999h>` makes the debug binary
panic (`attempt to multiply with overflow`, exit 101, every reminder of
that minute lost); release wraps. `checked_mul(60)`.

### 2.11 Low — Neovim: the first language's Vim syntax file is never included

`lua/org-lsp/src.lua:116` decides whether to `syntax include` a language's
file with `pcall(nvim_exec2, 'syntax list @cluster')`. That command only
errors when the buffer has syntax items and the cluster is missing; with
`replace_syntax = true` (the default) the buffer has none, the listing
prints "No Syntax items defined" without an error, the include is
skipped, and the `syntax region … contains=@orgLspSrc_lisp` that follows
creates an empty cluster, so every later check succeeds too
(`cluster=NONE` forever). Only the first by-syntax language of an apply
is hit, typically Emacs Lisp. Reproduced headless on Neovim 0.12.0 (three
applies: `orgLspSrc_lisp cluster=NONE`, code at (3,2) only in
`orgLspSrcCode_lisp`); with the check reading the listing's text, as Vim's
`org_lsp.vim:3093` does, the first apply gives `lispList lispDecl`. The
e2e check at `tests/nvim/e2e.lua:1581` passes vacuously because
`find('lisp')` matches the region's own name `orgLspSrcCode_lisp`; assert a
group of `lisp.vim` as the Python check asserts `pythonStatement`.

### 2.12 Low — Neovim and Vim: three runtime-path scans per code block per notification

`lua/org-lsp/src.lua:28-33` (`parser_for`: `language.add` scans
`parser/NAME.*`, then `inspect` adds again) and `:42` (`syntax_for`:
`nvim_get_runtime_file`) run for every block on every `org-lsp/codeBlocks`,
before the unchanged-text check, with no memo: 200 blocks = 600 lookups,
27 ms per apply of unchanged text with 17 runtime-path entries. Vim has
the same shape (`org_lsp.vim:3085`, `globpath` per block from both
`s:code_blocks` and `s:refresh`). Memoise per language name.

### 2.13 Low — `org-lsp mcp` robustness

- `mcp.rs:60-61`: one invalid-UTF-8 line ends the session (`line?`
  propagates; `printf '\xff\xfe\n{"jsonrpc":"2.0","id":9,"method":"ping"}\n'`
  exits 1 and the ping is never answered). Read bytes, answer -32700.
- `mcp.rs:80`: a JSON-RPC batch (an array) has no `id` and is dropped as a
  notification; the advertised protocol versions allow batches, so such a
  client hangs. Answer or reject with -32600.
- `mcp.rs:599-616`: a `0444` file is replaced through the temp-and-rename
  write (Emacs would refuse a read-only buffer); and nothing checks the
  file is unchanged between the tool's read and the rename, so an editor's
  save in between is silently overwritten (the docs describe only the
  other direction).
- `mcp/entry.rs:575-585` with `mcp_edits` on: `set_property ARCHIVE` to
  any path, then `archive`, writes an entry (with a body the assistant
  chose through `capture`/`add_note`) to any file the user can write;
  reproduced into a scratch `.profile`. This is Org's own semantics and the
  docs' "agenda files and their archive files" is literally true, but it
  makes the write confinement nominal against a prompt-injected assistant;
  consider restricting archive targets to the agenda directories.
- `mcp/prompts.rs:247`: file text goes inside a three-backtick fence; an
  entry containing such a line ends the fence early (reproduced with
  `process_inbox`). Use a longer fence or indentation.
- `mcp.rs:673`: the `headline` guard is a substring test (`"lph"` guards
  `* TODO Alpha`); documented, but it is `delete_entry`'s only safety.
- Costs: `refile` opens every agenda file twice, `process_inbox` about
  three times per file; `plan_my_day` on 90 000 headlines takes 6.8 s
  (three full views). Fine for hundreds of files; a cache keyed on mtime
  would help the prompts most.

### 2.14 Low — `notify` smaller items

- `docs/notifications.org`, "Sending them elsewhere": the `curl … -d "%b"`
  example makes curl upload a file when a body placed first in the
  argument starts with `@` (`notification_body_format = "%h"` and a
  headline `@/home/me/.ssh/id_rsa`). Use `--data-raw` and say that a value
  first in an argument can be read as an option by the program.
- `notification_deadline_summary_time = "29:00"` or `"noon"` is silently
  ignored (`notify.rs:300-301` `.ok()`): no summary, no problem printed.
- `notify-send`'s body is markup on most daemons; `<b>` and bare `&` in a
  headline are stripped or mangled. Cosmetic.
- A relative `XDG_DATA_HOME` is used as is (`./rel/org-lsp/icon.png`); the
  specification says to ignore it.
- Unreadable agenda files print a problem but exit 0, while a failed send
  exits 1.
- `string_to_number` for `APPT_WARNTIME` gives 0 for a 20-digit number and
  30 for `30.5`, where Emacs's `appt-add` raises "Argument WARNTIME must be
  an integer" and aborts `org-agenda-to-appt`. A deliberate difference
  worth a line in the docs.

### 2.15 Low — configuration and Emacs bridge

- `config.rs:3132-3133`, `:3318`: `ORG_LSP_CONFIG` naming a file that does
  not exist is silent for `org-lsp agenda` and `mcp` (`org-lsp config`
  does say "not found"); a typo in the variable is hard to notice.
- `cli.rs:153-156` → `workspace.rs:218-221`: a relative `org_agenda_files`
  entry in the configuration file resolves against the current directory,
  not the file's, and the error says "no org_agenda_files in the
  configuration file" although it is set.
- `emacs.rs:437-445`: the stdout reader uses `lines()`, so one non-UTF-8
  byte printed by the user's Lisp or a package tears down a healthy Emacs
  (the request returns `stopped()` and the process is restarted). Split on
  `\n` and decode lossily.
- `emacs.rs:482-486`: the last 20 stderr lines are kept across jobs, so an
  Emacs that dies silently after old warnings is reported with those.
- The "#376 every option is checked to read the same from TOML" test
  exempts integration and editor-only settings and the 46 options with no
  default, and compares the raw merged JSON, not what `Options::read`
  makes of it. Not a bug; the claim is narrower than it sounds.

### 2.16 Low — remote images: identical queued previews download again

`server/org-lsp/src/server/remote_images.rs:273-300`: previews of the
same document and version queued while a download runs are not coalesced,
and each job calls `download()` for every URL even when the file arrived
meanwhile. In `cache` mode, three `org.link.preview` requests sent during a
1.5 s download produced three sequential downloads of `/slow.png`
(expected one). A client re-requesting previews on scroll while a slow
host answers builds a queue that drains at 30 s per distinct URL; a job is
dropped only when the document's version changed, not when a newer
identical job exists. A running job for a closed document runs to
completion.

### 2.17 Low — the rest

- `server.rs:2037-2066` (by reading): diagnostics are rescheduled 200 ms
  after *every* message, requests included, and deferred while the
  receiver is non-empty; a client that sends anything more often than every
  200 ms (cursor-hold, range token requests while scrolling) starves the
  changed document's diagnostics. Reschedule on `didChange` of that
  document only, or bound the deferral. (The debounce itself is the
  maintainer's deliberate choice; the trigger set is the point here.)
- `server/manual.rs:84` (parity question): `info:/etc/hostname` passes the
  path to `info --file`, which reads any readable file, as Emacs's
  `Info-find-node` also would; the same class `man:` now refuses.
- `server.rs:5988-6020`: `find_id` scans every open document's text per
  lookup, and `export_links` calls it once per distinct `id:` link.
- `agenda/sorting.rs:418` (by reading, changed in #358): `[3/]` now counts
  as a cookie worth 0 (`[50%] [3/]` → 0, lone `[3/]` → 0). Emacs's
  `org-refresh-stats-properties` regexp needs denominator digits, so
  `[3/]` is no cookie (50 and nothing, respectively), and `[3/00]` signals
  `arith-error` rather than 0.
- `planning.rs` (by reading, pre-existing, ~80 % confidence): the old-date
  branch of `org--deadline-or-schedule` keeps only `[.+]+` repeaters, so
  rescheduling `<2026-05-20 Wed -2d>` to 2026-06-01 gives `<2026-06-01 Mon>`
  in Emacs; the port keeps `-2d`. Not covered by `tests/planning/cases.json`.
- `editors/sublime/LSP-org/plugin.py:182-192`: `os.remove` of the old
  unversioned binary is unguarded; on Windows, with that server still
  running (`OrgLspUpdateServerCommand` downloads while it runs), a
  `PermissionError` aborts `install_or_update` after a successful download.
- `editors/vim/autoload/org_lsp/adapter/vim_lsp.vim:93,116` call vim-lsp's
  script-local functions by `<SNR>`; a rename there raises E117 from the
  global `BufReadPost` autocmd with no `try`.
- `lua/org-lsp/init.lua:270-281`: any typeahead (`getchar(1) ~= 0`,
  including fast typing) routes commands through `request_sync` with a
  10 s timeout; by design per #369, but the trigger is wider than mappings
  and macros.
- With `replace_syntax = false` on Neovim 0.12, its own `syntax/org.vim`
  `orgCodeBlock` region spans the block, so the plugin's code region never
  starts (inline blocks work).
- `agenda::day_items` is superlinear in entries on one day: 10 000
  same-day entries 4.6 s, 30 000 46 s (release); spread over a year, 0.17
  s. Unrealistic agendas; noted for completeness.

---

## 3. Parity and documentation

The generators pass in check mode (`generate-docs.mjs`, Sublime, Zed, VS
Code). The settings dump (458 options) matches `docs/configuration.org`,
`config.schema.json`, `config.toml` and `types.lua` except the 18
client-only options, which `docs/protocol.org` documents; all 351
`setting` and 479 `emacs` marks in `scripts/org-options.json` resolve. 32
spot-checked `docs/emacs-options.org` entries have Org 9.8.10's types and
defaults, and "How values become Lisp" matches `bridge.el` point by point.
`docs/mcp.org` matches `tools/list` with `mcp_edits` off and on (13 reading
tools, 13 editing tools, 5 prompts, arguments and requiredness).
`docs/notifications.org`'s placeholders, limits, exit codes, icon paths,
cron/systemd/launchd/schtasks lines and example outputs reproduce. The ten
`experimental.org*` capabilities in `docs/protocol.org` are exactly those
`server.rs:1531-1625` reads. The changed `features.mjs` rows (Helix and
Zed "left out" lists, highlight claims, `shell:`/`elisp:`/`man:` messages,
checkbox `C-c C-c`, org-protocol confinement, Sublime and Zed downloads)
agree with the code.

Three discrepancies:

- `docs/protocol.org:136-138` says `executeCommandProvider` adds two
  commands to the manifest's (`org.cycle`, `org.date.preview`); `commands.rs:1908-1909`
  now adds a third, `org.emacs.answer`, mentioned only under
  `message_requests` at `:802`.
- `docs/mcp.org:66-67`: `text_search` also takes `todo_only` (boolean),
  not documented.
- `scripts/generate-docs.mjs:843-852`: `ACTIONS_IN_AGENDA` includes
  `org.agenda.save`, which `server.rs:4472-4477` never offers to Helix
  (`message_prompts` is true there); nothing generated is wrong today
  because `features.mjs:529-530` special-cases Helix, but the model the
  file says to keep in step with `code_actions_at` is out of step on that
  id.

---

## 4. Checked and found sound

- MCP: `mcp_edits` off by default and honoured by all 13 editing tools
  (absent from `tools/list`); configuration only from `ORG_LSP_CONFIG`,
  XDG and `~/.config`, never the working directory; `../`, absolute
  outside paths, `/etc/passwd`, case variants and unknown names refused;
  writes through a temp file and rename, CRLF and missing final newline
  kept, invalid UTF-8 refused rather than written lossily; JSON-RPC parse
  errors, unknown methods, non-string names, unknown protocol versions and
  non-object messages handled without a panic; line lookups at 0, negative,
  float, past the end, non-headline and CRLF lines refused cleanly; the
  `regex` crate is linear-time; `clock_report`'s span is whitelisted;
  `delete_entry` is `destructiveHint` and needs `headline`;
  `org_files_below` is bounded.
- `notify`: the command is argv, never a shell; placeholders are expanded
  once and not re-expanded; `notify-send` gets `--`; PowerShell reads the
  environment; the logo is written next to its final name and renamed in
  the user's own data directory; `reminds` and `appt_message_warning_time
  = i64::MAX` do not overflow; `time_string`, `is_done`, deduplication,
  sorting, `APPT_WARNTIME` inheritance and the calendar-day choice match
  `org-agenda-to-appt`; `parse_now` rejects bad dates and `24:00`.
- `emacs_options`: no workspace-local configuration file exists; values
  travel as ASCII-escaped JSON to `json-read-from-string`, no `read`,
  `eval`, backquote or `format` with a user format string; `function`
  types only store an `fboundp` symbol; option names are restricted to
  Org's `defcustom`s server-side; `--set-option` runs under
  `condition-case`; the Lisp built from settings escapes `\` and `"` and
  validates symbol names, file modes and floats; VS Code's
  `untrustedWorkspaces: limited` restricts the settings in Restricted Mode.
- Server: curl's `--globoff`, `--proto` lists, `--` and the post-download
  size check; download names ASCII-sanitised with a counter, keyed by exact
  URL; `sh -c` gets one argument and every substituted value is
  `shell_quote`d (POSIX and w32 forms match `shell-quote-argument`);
  `elisp:` asks before evaluating; `source_file` requires an absolute
  directory and checks the prefix component-wise; `encode` is linear and
  right on UTF-16 and CRLF; `org-lsp/codeBlocks` is linear, CRLF-aware,
  sent only to clients announcing it and once empty when the last block
  goes; `$/cancelRequest` and `emacs_wait` are bounded; no lock is held
  across I/O; `symbols.rs` hoists `outline_stars_in` out of its loop.
- Core: `non_ascii.rs` passes (36 s wall on this box); the robustness
  tests pass with their new hostile inputs; `inlinetask.rs`, `refile.rs`
  and `structure.rs` refactors are equivalent to the old code;
  `file_apps.rs` w32 quoting matches Emacs; `planning.rs`'s TIME-branch
  regexp and case folding are exact; `table.rs` `FORMULAS_UPDATED` is per
  line as Emacs messages; degenerate inputs (empty headline, a file of
  `*`, CRLF, 20 000-deep nesting) parse linearly without a panic.
- Editors: language names are validated before reaching tree-sitter or a
  syntax path; byte columns via `str_byteindex` with a fallback; detach
  cleans state; Vim's `:Org` range, completion, `read_text` mapping restore,
  terminal keys and `fillchars` handling verified in Vim 9.1; VS Code
  compiles (`tsc --noEmit`), `.vscodeignore` ships no private files, the
  publisher rename is consistent with `install.sh`; Sublime parses under
  Python 3.8, no `shell=True`; Zed's tasks run fixed command strings; the
  Neovim e2e run passes (413 checks) on 0.12.0.
- `scripts/install.sh` escaping for systemd, cron, launchd and VBS;
  `scripts/version.sh`; the site's lazy search index.
