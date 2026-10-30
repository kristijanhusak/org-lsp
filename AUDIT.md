# Audit findings, round nine

An audit of `main` at 06ee364 (version 0.1.2, #419), which gained 31 pull
requests since round eight: the round-eight fixes (#389), the typed
end-to-end round (#393), the Vim work (#390-#395, #399-#404), the
incremental parser (#397), faster tokens and lint (#396), shift-select
(#417), the `emacs`/`emacs_full` key presets (#418), and the release and
publishing changes (#398, #409, #412, #414, #415, #419). `scripts/check.sh`
passes on it here. Earlier rounds are in this file's git history.

Five reviewers: one re-ran every round-eight reproduction against `main`,
one took the server and parser (with a differential test of the
incremental parse), one the Vim plugin (with Vim 9.1 and the real server),
one the other editors and scripts (with Neovim 0.12.0 headless), one the
parity and documentation check. "Verified" means run here; "by reading"
means it could not be (no Emacs, macOS or Windows). The High findings were
confirmed in the code a second time by the author of this file.

Deliberately left as they are, per the maintainer, and not reported:
diagnostics debounce, workspace file list cache, Sublime paths, coc.nvim on
Windows, the Neovim edit version check, roam finalize after a refiled
capture, the encryption docs, tangle paths, the synchronous Enter/o/Tab
waits, the `zed-extension` hash.

---

## 1. Status of the round-eight findings on `main`

Every item of round eight is fixed on `main` (#389), each reproduction
re-run:

| Item | Status |
| --- | --- |
| 2.1 MCP file named twice | Fixed: canonical dedup (`mcp.rs:688`), bare names resolve from any working directory; two files really sharing a name are still refused and the views then label them by full path; a symlinked agenda file is written through the link |
| 2.2 agenda quadratic in diary sexps | Fixed: newline index per file in a `OnceCell` (`agenda/mod.rs:641`); release 1 000 sexps 23 ms, 4 000 sexps 57 ms (was 5.3 s / 79 s) |
| 2.3 one malformed option drops every setting | Fixed: `keep_readable` (`config.rs:3694`); `org_log_done = 5` and `org_agenda_span = {}` are each reported and the rest kept |
| 2.4 `org_src_lang_modes` before Org loads | Fixed: wrapped in `with-eval-after-load` (`config.rs:2150`) |
| 2.5 `osascript` without `--` | Fixed (`notify.rs:788`, with a unit test); the `notify-send` body is escaped for markup |
| 2.6 quadratic hidden ranges | Fixed: one-pass `Overlaps` sweep (`highlight.rs:852-871`), identical output to the old code over randomized and edge cases; 50 000 tokens with 10 000 hidden and 10 000 radio ranges in 2.4 ms |
| 2.7 MCP roam quadratic | Fixed: `RoamIndex` built once per call (`mcp.rs:1991`); 30 000 nodes in 0.7-1.1 s, all of it reading and parsing |
| 2.8-2.10 capture width, prefix count, `-NNNh` overflow | Fixed: `WIDEST` cap (`text.rs:22`), checked arithmetic (`edit.rs`, `stamp.rs`, `notify.rs:476`); none of the inputs panics |
| 2.11 Neovim syntax include | Fixed: the cluster check reads the listing (`src.lua:155`); headless 0.12.0 shows `lispList lispDecl` on the first apply; `e2e.lua:1597` asserts `lispDecl` |
| 2.12 runtime-path scans | Fixed: memoised per language with a 10 s retry and a reset on `'runtimepath'` change |
| 2.13 MCP robustness | Fixed: invalid UTF-8 answers -32700, batches are answered as arrays, `[]` is -32600, a `0444` file answers "Buffer is read-only", a file changed since the read is refused (`may_write`, `mcp.rs:706`), archive targets confined to resolved paths under the agenda directories (`entry.rs:579-587`, `..` and symlinked directories refused), fences lengthened |
| 2.14 notify items | Fixed: bad `notification_deadline_summary_time` reported, relative `XDG_DATA_HOME` ignored, unreadable files exit 1, `--data-raw` in the docs |
| 2.15 config and bridge | Fixed: a missing `ORG_LSP_CONFIG` is reported, relative agenda files that are not there exit 2 with their path |
| 2.16 duplicate queued downloads | Fixed: a preview asked again identically is skipped (`remote_images.rs:293-315`) |
| 2.17 the rest | Diagnostics put off at most 1 s after the last change (`DIAGNOSTICS_MOST`); `find_id_elsewhere` collects IDs once per file; Helix no longer sees `org.agenda.save`; `vim_lsp` adapter guards with `exists()`; Sublime tolerates the `os.remove` failure (with a test); `view.lua` retry under `pcall` |
| Section 3 docs | Fixed: `org.emacs.answer` listed in `docs/protocol.org:136`, `todo_only` in `docs/mcp.org`, the generator's action model |

---

## 2. New findings, by severity

### 2.1 High — Vim: a reload (`:e!`, `:e`, autoread) loses all semantic highlighting, and it does not come back

`editors/vim/autoload/org_lsp.vim:3161` keeps the last painted tokens in
`s:painted[nr]` so that only changed lines are repainted (#404). A reload
drops every text property in Vim, `BufReadPost` asks the server again
(`:3354`), the server answers the same tokens, `s:changed_lines` (`:3117`)
returns nothing, and `paint_absolute` returns at `:3181` without painting.
Later edits repaint only their lines; the rest stays bare until the faces
or line highlights happen to arrive and call `s:repaint_all` (`:3007`, the
only place `s:painted` is cleared). Nothing on `BufReadPost` or in
`org_lsp#reread` (`:259`) clears it.

Reproduced with Vim 9.1, the real server and keys typed through tmux
(text properties per line of a four-line file):

```
after open:                       4,4,6,1
after typing a =verbatim= line:   4,4,6,1,1     (incremental repaint works)
1.5 s after :e!:                  0,0,0,0
after typing another ~code~ line: 0,0,0,1,1     (only the tail repainted)
after typing *x* into line 1:     5,4,0,1,1     (line 3 stays bare)
```

The same path is taken by the external-change reload prompt and by
`:bunload` followed by reopening (the buffer number is reused,
`s:painted` survives). Fix: remove `s:painted[nr]` on `BufReadPost`, or call
`s:repaint_all` there.

### 2.2 High — Vim: the syntax's emphasis lookahead spans the whole paragraph, and big paragraphs disable the syntax or hang Vim

`editors/vim/syntax/org.vim:32-33`: every emphasis region start (`*`, `/`,
`_`, `+`, `=`, `~`) carries the lookahead
`\%(\S\%(\%([^\n]\|\n<element>\@!\)\{-}\S\)\{-,1}MARKER<post>\)\@=`, which
scans to the end of the paragraph for every marker that does not close
(`/usr/bin`, `*nix`, `_foo`). The file's own header says emphasis over
several lines waits for the server, but the pattern spans any number of
lines within a paragraph (so `*a\nb\nc*` is bold here and not in Org, an
unstated difference). Measured in a tmux-hosted Vim 9.1 with `set ft=org`:

- A 500-line paragraph with two unclosed markers per line (22 KB): `G`
  gives `'redrawtime' exceeded, syntax highlighting disabled`; Vim turns
  the syntax off for the buffer, and the code-block colouring with it.
- 2 000- and 4 000-line paragraphs: every redraw prints `E363: Pattern uses
  more memory than 'maxmempattern'` repeatedly, the screen ends in
  `-- More --` and Vim takes no keys until Ctrl-C.
- Single lines: `_a _a …` of 5 KB takes 0.35 s per redraw (all of it in
  `orgSyntax_Any_underline` per `syntime`); 10 KB and 100 KB of `*` give
  E363 and the syntax disabled; from a script a `:redraw` hung over 120 s.
  `'synmaxcol'` does not bound it because the scan starts before column
  3 000. Realistic prose lines were fine below 20 KB.
- `s:closed_by` (`:106`, `:111`) scans to the next headline per unclosed
  `:NAME:` or `#+begin_*` line: 10 000 unclosed drawer lines in a
  headline-less file give E363 (adversarial).

The 30 000-line mixed file is fine (first draw 0.4 s, scrolling 19 ms
total), so the cost is the lookahead on unclosed markers. Bound it to Org's
own rule (`org-emphasis-regexp-components`: at most one newline) or a
character budget.

### 2.3 Medium — Neovim: after `:e!`, `:set syntax=org` or `:syntax on`, Vim's own `syntax/org.vim` replaces the plugin's for good

`lua/org-lsp/init.lua:1503-1515` reloads the plugin's syntax on a `Syntax`
event when `$VIMRUNTIME` comes first in `'runtimepath'`, guarded by `not
vim.b[buf].org_lsp_syntax`; `editors/vim/syntax/org.vim:359` sets that
flag once and nothing clears it. Every later re-set of the syntax runs
`runtime! syntax/org.vim`, Neovim 0.12's own file wins, the autocmd sees
the stale flag and does nothing; the server's `org-lsp/syntax` notification
on the reopen is also dropped because `view.lua:895` sees equal parameters.
The code-block regions are gone too until the next text change. Reproduced
headless on 0.12.0 with the plugin appended to `'runtimepath'` (the native
`packpath`/`vim.pack` case):

```
after open:             plugin items 3  src regions 1  Vim-own items 0
after :e!:              plugin items 0  src regions 1  Vim-own items 1
after :set syntax=org:  plugin items 0  src regions 0  Vim-own items 1
```

With the plugin prepended the reload happens each time. Fix: reload
whenever `b:org_lsp_syntax_loads` did not increase during the event (or
the plugin's groups have no items), then `src.reapply`.

### 2.4 Medium — Vim: every token refresh on a 30 000-line file blocks Vim for 1.4-6.7 s

`editors/vim/autoload/org_lsp.vim:3099-3110` decodes 397 115 token numbers
into 79 000 tokens in a Vimscript loop; `:3116-3151` (`s:changed_lines`)
copies and maps the old token list; `:3171-3180` calls `s:byte` →
`s:encoding` → `s:call` → `org_lsp#adapter()` 28 240 times per paint for
the hidden ranges. Measured with a 10 ms jitter timer on the real server:
painted 8.3-8.9 s after opening (longest block 0.4 s), then after typing a
line at the end the longest block is 6.2-6.8 s, after 12 characters at
line 15 000 it is 1.4-2.2 s, after `o` 2.0 s. Typing is debounced (200 ms
in insert mode), but any 200 ms pause triggers a full round trip and this
block, and leaving insert mode triggers a second one (two requests per
edit observed). Profile of one edit: `s:paint` 0.99 s, `s:paint_absolute`
0.62 s, `s:changed_lines` 0.49 s, the `s:byte` chain 0.4 s. Cheap wins:
hoist `s:encoding()` out of the hidden loop as the paint loop already
does, skip the `map(copy(old))` when nothing moved, or use
`semanticTokens/range` or delta.

### 2.5 Low — VS Code: a second full tokenization per edit for every visible editor, unconditionally

`editors/vscode/src/extension.ts:887-900`: #407 removed `paintFaces`'s
early return when there are no face decorations, so every
`onDidChangeTextDocument` (debounced 300 ms) and every visible-editor
change sends `textDocument/semanticTokens/full` per visible Org or agenda
editor, on top of VS Code's own request, even for a user with no
`org_todo_keyword_faces` and no nested emphasis. On the 14 800-line file
of #404 that is a whole-file tokenization per keystroke burst. Read the
tokens VS Code already fetched (`vscode.provideDocumentSemanticTokens`, as
`:1238` does for hidden ranges), or share one request.

### 2.6 Low — server: a parse that is not of the document's text is edited incrementally

`server/org-lsp/src/document.rs:104` seeds the incremental path with
`self.parse.clone()`. When an earlier `apply` or `new` hit the parser-panic
fallback (`:157-158`, `org_syntax::parse("", todo)`), `self.parse` is an
empty parse while `self.text` is the real text; `Parse::edit` does not
check that the tree covers the old text, so `parse("").edit("text\n* a\n",
0, 0, "x…")` yields a tree whose text is `"xtext\n"` only, and later edits
inside what the stale tree covers keep succeeding. Only reachable after a
parser panic, but it turns a one-off recovery into a sticky partial parse.
Seed the incremental path only when the tree's length equals the text's,
or make `Parse::edit` fall back when `old` is not the tree's text.

The incremental parser itself is sound: 284 000 targeted edits plus 1.4
million random Org-shaped edits over all 166 `.org` files in the repository
gave zero mismatches against a full parse, in tree and in
`todo_keywords()`; a headline edit on a 1.6 MB file takes 48 µs against
33 ms for a full parse.

### 2.7 Low — the rest

- `server/org-lsp/build.rs:14`: `&commit[..commit.len().min(7)]` slices
  bytes; `ORG_LSP_COMMIT='ééééé' cargo build` fails the build script with
  "byte index 7 is not a char boundary". CI always passes a hash.
- `document.rs` whole-text branch: when `runs.len() > 8` sets `parse = None`
  the loop still clones the whole text once per run (`let before =
  text.clone()`), up to N full copies for a large paste.
- `server.rs` `display_in`: finds an open document's `org.indent.mode`
  override by comparing the whole text of every open document, which is
  also ambiguous for two identical files. A key would do.
- Vim `org_lsp.vim:3365`: `listener_add` survives `:bunload` while
  `b:org_lsp_attached` does not, so each unload and reopen adds a listener
  (both only restart the debounce timer).
- Vim `plugin/org_lsp.vim:16` (by reading): the `Syntax` autocmd captures
  `s:nr` by reference into a `timer_start(0, …)`; two `Syntax` events before
  the timers run (session restore, `:bufdo`) both act on the last buffer.
- Vim `s:with_terminal_keys` (`org_lsp.vim:~1128`) maps `<Esc>x` for `<M-x>`
  in the agenda, so a lone `<Esc>` there waits `'timeoutlen'`.
- Sublime `Org.sublime-syntax:662-673` (unchanged complexity): the emphasis
  start regexes scan to the end of the line for every unclosed marker; a
  24 KB line of ` *a` takes 2.2 s in Python's engine (Sublime's is faster,
  same shape). Normal lines are instant; every context pops at `$`, so
  nothing swallows the file. Mis-nested `*bold /ital* more/` (by reading)
  makes ` more/` bold-italic and the rest of the line bold, where Emacs
  leaves ` more/` plain. Cosmetic.
- VS Code `extension.ts:839`: `emphasisDecorations` are never disposed
  (`faceDecorations` are, `:2493`); leaks only across extension reloads.
- Neovim `src.lua:133-190`: `org-lsp/codeBlocks` arrives after every change
  and re-issues one `syntax region` per block without a tree-sitter parser;
  580 `sh` blocks cost about 10 ms per notification. Acceptable; noted
  because the Vim side of #404 waits for typing to stop and Neovim's does
  not.
- `scripts/test-all.sh` uses `wait -n` and `declare -A` (bash 5.1), so it
  does not run under macOS's bash 3.2. Developer-only.
- `npm audit` in `editors/vscode`: 0 with `--omit=dev`; two high in
  `@azure/msal-node` through `@vscode/vsce`, a publishing tool not shipped.

---

## 3. Parity and documentation

The generators pass in check mode; `scripts/version.sh` prints 0.1.2 and
every place a version lives agrees (both `Cargo.lock`s, `package.json` and
its lock, Zed's `extension.toml` and `Cargo.toml`, Sublime's `plugin.py`);
`org-lsp --version` prints `org-lsp 0.1.2-<commit>-dev` as `build.rs`
formats it. The shift-select, key preset, Vim concealment, Vim syntax,
`org.version`, `org.indent.mode`, coc.nvim, nested emphasis and `:Org`
completion rows of `features.mjs` are backed by code in every editor
marked; `docs/vim.org`'s client order and per-client claims match the
adapters; `docs/development.org` describes scripts and variables that
exist with those arguments; the release workflow does what the docs say
(nightly on each push to `main` publishes to no store, `SHA256SUMS`
written, tag checked against `version.sh`, Marketplace through Entra ID,
Open VSX through `OVSX_PAT`, "preview" gone). The VS Code and Sublime key
presets check out: 691 and 551 bindings, every one guarded by preset and
language, none duplicated with different commands, every `emacs` `Ctrl+C`
chord requiring an empty selection, `emacs_full` adding exactly the keys
`docs/keys/*.org` list.

Three discrepancies:

- `docs/vscode.org:135-141` describes the default `emacs` preset as if it
  were `emacs_full`: "Shift and the arrows" bound in Org files, Shift+arrows
  "change timestamps and select text elsewhere", "Ctrl+C waits for a
  second key instead of copying". In `package.json` every Shift+arrow
  binding for Org files requires `emacs_full` or `vim`, and every `emacs`
  `Ctrl+C` chord carries `!editorHasSelection`, so Ctrl+C copies a
  selection. The `org-lsp.keys` description, `docs/keys/vscode.org:7-13`,
  `docs/sublime.org:90-93` and `features.mjs:52-53` say it right; `:145-147`
  then attributes to `emacs_full` what `:135-141` already gave the default.
- `editors/zed/TESTING.org:423` and `editors/helix/TESTING.org:348` say
  "Show the version of the server is not among its keys or code actions",
  but `server.rs:4811` offers `org.version` as a code action wherever
  `more_actions` is on (Zed sets `command_actions`; Helix shows message
  requests). `ACTIONS_IN_FILES` in `scripts/generate-docs.mjs:855-873`
  has every other command of the server's table but this one. `README.org:71-73`
  states it correctly.
- `scripts/features.mjs:1099` writes "Showing the version of the server
  says `org-lsp version 0.1.0-`…" into every `TESTING.org`; a server built
  from this tree says 0.1.2, so the check cannot pass as written.
  `README.org:71`, `docs/neovim.org:51` and `docs/vim.org:59` show
  `0.1.0-f7e7b68` as examples, stale but illustrative.

Minor: `README.org:67` says the binaries are `.tar.gz` while Windows is a
`.zip` (handled by `install.sh`); `docs/development.org:195` omits the
`highlight` value of `ORG_LSP_VIM_TESTS`; `docs/vim.org:26` says vim-lsp's
inlay hints come again only when the cursor moves, while the plugin
re-requests them after every command result and only the server's
`workspace/inlayHint/refresh` is missed.

---

## 4. Checked and found sound

- Incremental parse (#397): zero mismatches over 1.7 million edits (every
  line start and end × 14 insertions, deletions and joins, plus random
  Org-shaped insertions: stars, `#+TODO:`, blocks, drawers, inline tasks,
  radio targets, footnotes, CRLF, multibyte) on all 166 `.org` files; about
  85 % of edits parsed in place; a file with no headline falls back to a
  full parse at full-parse cost.
- Shift-select (#417): `shift_is_org` orders region, timestamp, `always`,
  headline, property and bullet as `org-support-shift-select` does, and
  adds clock tables and tables in the server; the recorded cases match at
  every position.
- `Overlaps` sweep, `legend.rs` TextMate reorder, `completion.rs` property
  keys (`pcomplete/org-mode/prop`, `_ALL` names, `+` suffix), `cli.rs`
  missing-file messages, `bulk.rs` scatter default, `babel_at`,
  `shrunk_after_change`, `emacs.rs` native-comp retry, `planning.rs`
  repeated-timestamp placement, `tags.rs` `global_tags`.
- Workflows: actions pinned, `TAG` and `ORG_LSP_COMMIT` consistent, cache
  keys, Entra ID login, `PUBLISH` computed from secrets in job `env`;
  `ci.yml` is `workflow_dispatch` only (unchanged policy: the Org 9.8.10
  compile and the editor steps run by hand).
- Vim: no `syn region` without an end, `keepend` on blocks and drawers,
  `highlight default link`; concealing holds all positions and re-adds
  matches by id, respects `'concealcursor'` per mode; key prompts catch
  `Vim:Interrupt`, restore `cmdheight` in `finally`, `<Esc>` and `<C-c>`
  answer null; the coc adapter serialises requests after a change and maps
  refusals back; `s:give_view` restores options in `finally`; `s:got_folds`
  is linear; every `augroup` has `autocmd!`; no document text reaches
  `execute`, `eval` or a shell (`shellescape` on the manifest call,
  `fnameescape` on every `:edit`, languages filtered by `^\w\+$`); the test
  harness fails on no `ok` line, caps each Vim, writes results atomically,
  and the typed tests assert positive conditions.
- Neovim: headless 0.12.0 passes syntax (41 files), highlight (22), e2e
  (422), agenda (224), images (33), protocol and the 16 headless scenarios
  against the built server; `read_key`/`show_choices` restore `cmdheight`
  under `pcall`; `src.lua` detaches cleanly and its memo resets on
  `'runtimepath'` change; no new per-keystroke whole-buffer work.
- VS Code: `tsc --noEmit` clean; `package.json` publisher, repository,
  engines, `untrustedWorkspaces: limited` with restricted configurations;
  `.vscodeignore` whitelist; `dot()` writes SVGs only for `^#?\w+$` colours
  into global storage; quick picks disposed; `restartClient`/`deactivate`
  serialised.
- Sublime: keymap parses, `on_query_context` handles `emacs_full`;
  `plugin.py` parses under Python 3.8, no `shell=True`; semantic emphasis
  tokens removed so the syntax's nested looks are not painted over, scope
  stacks consistent with the VS Code `semanticTokenScopes`.
- Zed: 202 tasks, all fixed command strings with no `$ZED_*` or document
  text; every keymap task exists; versions 0.1.2.
- `install.sh` checksum verification in place; `bash -n` clean for
  `install.sh`, `version.sh`, `test-all.sh`.
