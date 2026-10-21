# Issues found, 2026-10-21

Each issue: a heading `## <part>-<n>: <one line>`, then

- State: open | fixing (branch) | fixed (#PR) | not a bug (why)
- Where: editor or client, command, file and line when known
- Repro: the exact steps or script (scripts under `.claude/e2e/repro/`)
- Expected (with Emacs's behaviour when it is the reference) and got


## neovim-1: Neovim nightly aborts (grid.c assertion) redrawing the date prompt in tests/nvim/highlight.lua
- State: open
- Where: Neovim nightly (v0.13.0-dev-1824+g27ee55c09d) only; `tests/nvim/highlight.lua`, the date-prompt part (`calendarJournal.future` look, line 7 of the calendar, `vim.cmd.redraw()` at highlight.lua:212). 0.12.0 and 0.12.5 pass (20 checks).
- Repro: `NVIM=/root/nvim-nightly/bin/nvim scripts/test-nvim.sh` (stops there), or `nvim --headless -u NONE -l tests/nvim/highlight.lua`; `.claude/e2e/repro/neovim/highlight-place.lua` prints the place before each redraw.
- Expected: the test passes. Got: `nvim: …/src/nvim/grid.c:586: grid_line_flush: Assertion 'grid_line_clear_to <= grid_line_maxcol' failed.`, exit 134. Likely a Neovim nightly bug, but triggered by what the calendar draws (extmark highlights of the org-date buffer in a 140-column, 50-line headless UI); see neovim-2+ for whether the calendar alone triggers it.

## neovim-2: on Neovim nightly a command on a selection prints "vim.lsp.util.character_offset is deprecated"
- State: open
- Where: `lua/org-lsp/init.lua` `M.run` (the `position()` of a Visual selection) calls `vim.lsp.util.character_offset`, deprecated in 0.13 (`vim.deprecate(..., 'vim.str_utfindex', '0.14')`).
- Repro: Neovim nightly, an Org file, `V` over two headlines, `>>` (or any `region` command). `tests/nvim/e2e.lua` on nightly prints the message (see its output).
- Expected: no message (use `vim.str_utfindex(line, encoding, col, false)` as the rest of the plugin uses `vim.str_byteindex`). Got: `vim.lsp.util.character_offset is deprecated. Run ":checkhealth vim.deprecated" for more information` in the echo area the first time; it goes away in 0.14.

## emacs-1: emacs_options: a `const` whose value is a keyword is read as a keyword argument, so keyword-valued constants never match (org-export-smart-quotes-alist cannot be set)
- State: open
- Where: `server/org-lsp/src/bridge.el` `org-lsp-bridge--type-keywords` / `--type-args`: `(while (keywordp (car rest)) …)` also eats a final keyword that is the const's value. `(const :tag "Unicode" :utf-8)` gives keywords `(:tag "Unicode" :utf-8 nil)` and no args, so the constant is `nil`. (wid-edit's own `widget-convert` stops when no value follows.)
- Repro: `.claude/e2e/repro/emacs/conv.el`: `emacs --batch -Q -l conv.el --eval '(message "%S" (org-lsp-bridge--option-value (quote (const :tag "U" :utf-8)) ":utf-8"))'` prints `":utf-8"`; `(org-lsp-bridge--type-text '(choice (const :tag "U" :utf-8) (const :html)))` is `"false"`. Over LSP (`t_kinds.py`): `emacs_options = { org-export-smart-quotes-alist = { xx = { primary-opening = { ":utf-8" = "«" } } } }` (or `utf-8`) is refused: "`emacs_options` leaves out `org-export-smart-quotes-alist`: {…} is not a table" (it is a table: the message is misleading too).
- Expected: `(("xx" (primary-opening :utf-8 "«")))` set, as Customize would; the fix is to stop at a keyword with nothing after it (`(and (keywordp (car rest)) (cdr rest))`). Got: refused.

## emacs-2: emacs_options: a property list whose :key-type is `(symbol :tag …)` gets plain symbols for keys (org-html-table-default-attributes set to `(border "1")`, silently)
- State: open
- Where: `bridge.el` `org-lsp-bridge--option-value`, the `plist` branch: keys become keywords only when `(eq key-type 'symbol)`; `org-html-table-default-attributes` has `:key-type (symbol :tag "Property")`, so `border` is interned as `border`. The plist widget accepts any symbol, so the value matches and is set.
- Repro: `t_kinds.py` (or conv.el `(conv-try "org-html-table-default-attributes" "ox-html" "{\"border\":\"1\",\"rules\":\"all\"}")`): Emacs ends with `(border "1" rules "all")`, no notice; an HTML export then has none of its table attributes (ox-html reads `:border` …).
- Expected (docs/emacs-options.org "How values become Lisp": "a property list, whose keys are keywords (`scale` is `:scale`)"): `(:border "1" :rules "all")`. Check the head of the key type, not the type itself (`(memq (if (consp key-type) (car key-type) key-type) '(symbol sexp))`).

## emacs-3: emacs_options: `(repeat :inline t …)` inside a list is not handled: org-latex-classes with sectioning cannot be set
- State: open
- Where: `bridge.el` `--option-value`, the `list`/`group` branch maps items one to one onto the type's args; an `:inline t` repeat (org-latex-classes, `(list string string (repeat :inline t (choice (cons string string) …)))`) should take the rest of the items.
- Repro: `emacs_options = { org-latex-classes = [["mini", "\\documentclass{article}", ["\\section{%s}", "\\section*{%s}"]]] }` (`t_kinds.py`): "`emacs_options` leaves out `org-latex-classes`: [[…]] is not a list". conv.el shows the item becomes `("\\section{%s}" "\\section*{%s}")` (a list, not the cons) inside a list of 3. Only `[["mini","\\documentclass{article}"]]` (no sections) is accepted.
- Expected: `(("mini" "\\documentclass{article}" ("\\section{%s}" . "\\section*{%s}")))`: org-latex-classes is one of the most customized LaTeX options, and the page lists it as settable. Also org-html-infojs-options is a `(set :inline t …)`.

## emacs-4: emacs_options: in a `choice`, the first alternative a value fits wins, so some values cannot be said (python `auto`, infojs `path`, `(not "LOGBOOK")`)
- State: open
- Where: `bridge.el` `--option-value`, `choice` branch (tries alternatives in order, takes the first that matches).
- Repro: conv.el / `t_kinds.py`:
  - `org-babel-python-command-session = "auto"` (and `org-babel-python-command`): type `(choice string (const auto))`, so `"auto"` stays the string `"auto"`, which ob-python then runs as a command named `auto`; the default symbol `auto` cannot be given back.
  - `org-html-infojs-options = { path = "x.js" }`: `(choice (symbol …) (string …))` makes it the symbol `x.js` (a publishing property for Org), not the file `"x.js"`.
  - `org-export-with-drawers = ["not", "LOGBOOK"]` fits `(repeat string)` first and becomes `("not" "LOGBOOK")`, which exports only drawers named "not" and "LOGBOOK", the opposite of the default `(not "LOGBOOK")`, which cannot be said at all.
  All three are set without a notice.
- Expected: a string equal to a `const`'s symbol name taken as that constant before a generic `string`/`symbol` alternative (Customize's user picks the alternative; here the only way to say it is the name); a string where a `string` alternative exists stays a string before `symbol`; and a list whose first item names a `const` of a `list` alternative (`not`) goes to that alternative. Or document the limits on the page.

## emacs-5: emacs_options: function-valued options are listed as settable but cannot be set, not even to a function's name
- State: open
- Where: `scripts/org-options.json` marks `emacs` 19 options whose type is just `function` (org-html-format-drawer-function, org-html-format-headline-function, org-latex-format-headline-function, org-ascii-format-drawer-function, org-odt-format-*-function, org-texinfo-format-*-function, org-babel-latex-preamble/begin-env/end-env, org-babel-csharp-generate-*-command…), and `org-columns-summary-types` (`:value-type function`); `bridge.el` keeps a string a string for `function`, which never matches.
- Repro: `emacs_options = { org-html-format-headline-function = "org-html-format-headline-default-function" }`: "`emacs_options` leaves out `org-html-format-headline-function`: "org-html-format-headline-default-function" is not a function, which goes in `emacs_config`". The docs/emacs-options.org intro says "those whose value is Lisp (a function, a hook) go in `emacs_config`… The other options of Org are not here", yet these 19 are on the page (with defaults like a printed closure, see emacs-7).
- Expected: either mark them `lisp` (the server then says so before Emacs starts, and they leave the page), or (better, as Customize takes a symbol for a `function` type) intern a string naming a defined function (`fboundp`) for `function`/`command` types: `"my-drawer"` defined in `emacs_config` would then work, which the message's own advice suggests.

## emacs-6: emacs_options from the editor is merged into the file's key by key, deep: a table value (alist) gets the file's entries too, and Neovim's `[]` drops the file's emacs_options whole
- State: open
- Where: `server/org-lsp/src/config.rs` `load` → `merge` (objects merged recursively at every depth); `Options.emacs_options` reads `[]` as empty (`table_or_empty_list`) after the merge.
- Repro: `.claude/e2e/repro/emacs/t_precedence.py`. Shared file `[emacs_options] org-attach-method = "mv"`, `org-babel-load-languages = { python = true, emacs-lisp = true }`, `org-export-with-toc = false`.
  - Editor `emacs_options = { "org-babel-load-languages": { "shell": true }, "org-export-with-toc": 3 }`: Emacs gets `((python . t) (emacs-lisp . t) (shell . t))`; toc 3 and attach `mv` are right.
  - Editor `emacs_options = []` (Neovim's `vim.empty_dict()`-less `{}`): Emacs gets none of the file's options (attach `ln` from emacs_config, toc `t`), while `{}` keeps them.
- Expected (docs/emacs-options.org: "The editor's `emacs_options` goes over the file's option by option"): the editor's value of an option replaces the file's value of that option whole, so `((shell . t))`; and `[]` is an empty table, merging nothing (normalize `[]` to `{}` before `merge`, as `load` already does for the whole options). The same deep merge applies to any object-valued server option (worth checking for the others, e.g. tables inside `org_*` settings).

## neovim-3: docs say M-RET after a checkbox item makes `[ ]`; it does not, and neither does Emacs
- State: open (docs)
- Where: `scripts/features.mjs:284` (TESTING.org "Lists and checkboxes": "after a checkbox item the new one has `[ ]`: `<Leader><CR>`"), and the `return_uses_meta_return` description in `server/org-lsp/src/settings.rs` ("numbered and with a checkbox as M-RET makes it").
- Repro: `.claude/e2e/repro/neovim/structure.lua` (section lists): `- [ ] a` / `- b`, cursor on line 1, `<Leader><CR>` gives `- ` with no box. Emacs 9.8.10: `(call-interactively #'org-meta-return)` at the end of `- [ ] a` gives `- [ ] a|- |- b` (org-meta-return calls `org-insert-item` with no CHECKBOX; only M-S-RET, `org-insert-todo-heading`, adds one).
- Expected: the docs to say what Org does (the behaviour is Emacs's). Got: the check as written fails.

## neovim-4: errors of commands show as `org-lsp: <Emacs's text>` at WARN level, not Emacs's text alone
- State: open (low; maybe deliberate)
- Where: `lua/org-lsp/init.lua` `M.run` → `handle`: `notify('org-lsp: ' .. tostring(err.message), vim.log.levels.WARN)`.
- Repro: `.claude/e2e/repro/neovim/structure.lua` / `config.lua`: `<Leader>oK` on the first subtree, `g{` on a top headline, C-k on a folded headline with `org_ctrl_k_protect_subtree = true` answered `n`.
- Expected (CLAUDE.md "Say what Emacs says"): `Cannot move past superior level or buffer limit`, `Already at top level of the outline`, `‘C-k’ aborted as it would kill a hidden subtree`, as Emacs's echo area shows them. Got: `org-lsp: Cannot move past superior level or buffer limit` etc., as warnings; the messages of results (`result.message`) have no prefix, so the two kinds read differently. The notification is already titled `Org`.

## neovim-5: a FileType autocmd's 'conceallevel' is overridden to 2 in Org windows
- State: open (low)
- Where: `lua/org-lsp/init.lua` `conceal()`: sets `conceallevel = 2` on attach and on every `BufWinEnter`.
- Repro: `.claude/e2e/repro/neovim/config.lua` (section FileType autocmd): `autocmd FileType org setlocal conceallevel=1 shiftwidth=4 spell textwidth=70`, open an Org file: shiftwidth/spell/textwidth stay, conceallevel is 2.
- Expected: the user's own window option kept (as `set_window_folds` keeps later changes of the user's), or the docs to say `conceal = false` is the way to keep one's own 'conceallevel'. Got: 2 whatever the user set; only `conceal = false` keeps it.

## emacs-7: C-c C-a u (org-attach-url) always fails: "Cannot prompt about "…" interactively in batch mode.  Aborting"; the settings org_resource_download_policy / org_safe_remote_resources do not reach Emacs
- State: open
- Where: `bridge.el` `org-lsp-bridge-attach` "u" → `org-attach-url` → `org--confirm-resource-safe`, which errors when `noninteractive` instead of asking (so the bridge's y-or-n-p advice never sees it). `org-resource-download-policy` and `org-safe-remote-resources` are `setting` in scripts/org-options.json (so `emacs_options` refuses them: "it is the setting `org_resource_download_policy`"), but `Options::emacs_lisp` does not pass them to Emacs.
- Repro: `.claude/e2e/repro/emacs/t_attach_url.py`: `org.attach` `u` `file:///etc/hostname` with no setting, with `org_resource_download_policy = true`, and with `org_safe_remote_resources = ["\\`file:///etc/"]`: the same error each time, nothing attached. The same applies to a remote `#+INCLUDE:`/`#+SETUPFILE:` in an export.
- Expected: Emacs asks "Fetch and attach …?" (Emacs interactively offers `!`/`f`/`d`/`s`/`q`): put the question to the editor (advise `org--confirm-resource-safe`, or bind `noninteractive` nil around it with the y-or-n-p advice), and pass the two settings to Emacs (as `emacs_lisp` does for the tangling ones). Workaround today: `(setq org-resource-download-policy t)` in `emacs_config`.

## emacs-8: a message Emacs gives is dropped when the command also changes the text (first C-c C-a a: no "File "a.txt" is now an attachment")
- State: open
- Where: `server/org-lsp/src/server.rs` `emacs_edited`: `said` (the `message` the bridge recorded) is shown only when `new == old`; when the command edits the document it is never shown.
- Repro: `.claude/e2e/repro/emacs/t_attach.py`: `org.attach` `a` `a.txt` on an entry without an ID: the ID and `:ATTACH:` are added, no message; the next `c`/`l`/`y`/`m` (no text change) each show `File "c.txt" is now an attachment`. Same for any Emacs-run command that edits and messages (e.g. `org-table-recalculate`'s, footnote commands, `org-attach-set-directory`'s questions' follow-ups).
- Expected: Emacs's last message shown (INFO) after the edit is applied, as the echo area would show it.

## emacs-9: the attachment dispatcher's keys are hard-coded: no `q` (Abort), and an unknown key says "No attachment command under x" instead of Emacs's "No such attachment command: x"
- State: open
- Where: `server.rs` `attach` (`COMMANDS`, 14 keys); Org 9.8's `org-attach-commands` also has `q` ("Abort.", messages "Abort"), `b`, `O`, `F` and the `C-` variants of each key.
- Repro: `t_attach.py`: `org.attach` with input `q` → error -32803 "No attachment command under q"; `x` → "No attachment command under x".
- Expected: `q` offered and answering it ends the command with "Abort" (or quietly); for another key Emacs's text `No such attachment command: x` ("Say what Emacs says"). `b`/`O`/`F` are Emacs-buffer/Dired variants and fine to leave out (worth a line in the docs). The `org-attach-commands` option the page lists has no effect on this menu.

## emacs-10: docs/emacs-options.org: docstrings written without org-mode-map ("Uses keymap `org-mode-map', which is not currently defined."), closures with their captured variables as defaults, odd "Since" lines
- State: open
- Where: the generator behind `scripts/emacs-options.json` (`scripts/emacs_options_reference.py`), the page `docs/emacs-options.org`.
- Repro: `.claude/e2e/repro/emacs/dump_options.el` + `cmp_options.py` (all 480 entries against Emacs with Org 9.8.10): docstring text, type and default agree otherwise, but
  - org-confirm-babel-evaluate (page line 333) and org-babel-no-eval-on-ctrl-c-ctrl-c (365) start with "Uses keymap ~org-mode-map~, which is not currently defined." and say ~M-x org-ctrl-c-ctrl-c~, where `C-h v` (Org loaded) says "Remove code block evaluation from the `C-c C-c' key binding." (`\<org-mode-map>` substituted before `org` was loaded: require `org` first);
  - function-valued defaults print as interpreted closures with their environment, e.g. org-html-format-drawer-function's Default `(closure (htmlize-buffer-places org-html-format-table-no-css htmlize-css-name-prefix htmlize-output-type htmlize-output-type htmlize-css-name-prefix t) (_name contents) contents)` (also org-ascii-/org-latex-/org-odt-/org-texinfo-format-drawer-function, org-babel-latex-preamble/begin-env/end-env): `C-h v` in an installed (compiled) Org shows a byte-code object; the source form `(lambda (_name contents) contents)` (`(car (get sym 'standard-value))`) is what helps a reader;
  - "Since :: Org. (9.8)" for the five ob-csharp options (Org's own typo `:package-version '(Org. "9.8")`) and "Since :: Org (9.4)" for org-babel-haskell-compiler (`'(Org "9.4")`, a list): normalize to "Org 9.8"/"Org 9.4".
- Checked fine: 480 level-3 headings = the 480 options, no stray headings or unbalanced blocks (org-element), every CUSTOM_ID is GitHub's anchor (org-babel-C++-compiler → `org-babel-c-compiler-1`), all 394 TOML examples parse and equal `json` (the intro's example aside), every `json` default validates against both schemas.

## neovim-6: Neovim: typing a date at the prompt does not show what it means (no `org.date.preview`)
- State: open
- Where: `lua/org-lsp/init.lua` `M.run` (the `vim.ui.input` of a command's `input`); nothing in `lua/` calls `org.date.preview` (only `editors/vscode/src/extension.ts:439` does). docs/protocol.org ("While the answer is typed, `org.date.preview` also shows it in the minibuffer line") and TESTING.org (Dates and times, `only: ['neovim', 'vscode']` in `scripts/features.mjs:439`) say Neovim does it.
- Repro (real TUI in tmux, `.claude/e2e/repro/neovim/tui.sh`): open `* TODO Task`, `\ois`, `i`, type `fri`: the calendar's last line stays `SCHEDULED Date+time [2026-10-21]:    => <2026-10-21 Wed>` (waited 3 s); Enter then schedules `<2026-10-23 Fri>` correctly.
- Expected (Emacs's `org-read-date-display`): the last line follows the typing, `SCHEDULED Date+time [2026-10-21]: fri    => <2026-10-23 Fri>`. Got: the default's meaning only. (A `CmdlineChanged` autocommand while the input is open could send `org.date.preview`.)

## neovim-7: a count does nothing in the date prompt's calendar (`3l`, `2j` move one day/week)
- State: open (low)
- Where: server `server/org-lsp/src/server.rs` `calendar_command` (~7959: `open.select(at + 1 …)`, `at + 7`, months, years) ignores `context.count`; the Neovim plugin sends it (`M.run(name, { count = vim.v.count })`).
- Repro: tmux TUI (`.claude/e2e/repro/neovim/tui.sh`), `* TODO Task`, `\ois`, `2j` → `=> <2026-10-28 Wed>` (one week), `3l` → one day. Headless: `dates.lua` section calendar keys (`5l`). `tests/e2e/scenarios/calendar.json` has `"count": 2` on `org.calendar.week.later`, and its expected cursor does not show two weeks either.
- Expected: Vim counts work as docs/keys/neovim.org says ("Counts work as in Vim"), and as Emacs's `calendar-forward-day`/`calendar-forward-week` take a prefix argument in the calendar. Got: the count is dropped.

## lsp-1: citation keys are completed (and quick-fixed) twice when the same .bib is named by `#+BIBLIOGRAPHY:` and `org_cite_global_bibliography`
- State: open
- Where: server, `textDocument/completion` after `[cite:@`, `org.cite.insert`'s choices, and the quick fixes of the "Unknown citation key" diagnostic (`server/org-lsp/src/server.rs` near the `Suggestions:` hover, ~15134).
- Repro: `.claude/e2e/repro/lsp/completion.py plain` (sample workspace of `workspace.py`: `gtd.org` has `#+BIBLIOGRAPHY: refs.bib`, the options name the same file absolutely). Completion at `[cite:@` gives `['knuth1984', 'lamport1994', 'knuth1984', 'lamport1994']`; `org.cite.insert` offers four choices; code actions on `@knuht1984` give two identical "Replace with @knuth1984".
- Expected: each key once, as Emacs's completion table (`org-cite-basic--key-completion-table`, a hash keyed by the completion string) has it. Emacs does list the file twice (`org-cite-list-bibliography-files` gives "refs.bib" and the absolute path, `delete-dups` keeps both), and its help echo of an unknown key does say `knuth1984 knuth1984`, so the hover and the diagnostic message matching Emacs is fine; the completion items, the `org.cite.insert` choices and the duplicate quick fixes are not.

## lsp-2: protocol.org's list of requests leaves out several the server answers (references, rename, workspace symbols, document links, inlay hints, formatting)
- State: open
- Where: `docs/protocol.org`, "Capabilities": the server announces `referencesProvider`, `renameProvider` (with prepare), `workspaceSymbolProvider`, `documentLinkProvider` (with resolve), `inlayHintProvider`, `documentFormattingProvider`, and `org.cycle` / `org.date.preview` among `executeCommandProvider`'s commands (315, the manifest has 313).
- Repro: `.claude/e2e/repro/lsp/caps.py` prints the capabilities.
- Expected: each listed with what it does; e.g. that `textDocument/references` gives the links that lead to the *entry* at the cursor (by ID, headline search or CUSTOM_ID: references on a `<<target>>` or a tag give only the enclosing headline), that a rename renames a headline or a tag (and answers prompts), what the inlay hints are. Got: only `rangeFormatting` mentions `textDocument/formatting` in passing; the rest is not described (any-editor.org names some in one line).

## lsp-3: the TODO list and tags views leave out Emacs's "Press ‘N r’ … to search again: (0)[ALL] (1)CANCELLED …" lines, a difference written down nowhere
- State: open
- Where: `org.agenda.todos`, `org.agenda.dispatch` `t` / `m` / `M`; `scripts/agenda_reference.py` `clean()` drops those lines from the recorded cases ("what a server cannot or should not reproduce"), but no doc says so.
- Repro: `.claude/e2e/repro/lsp/emacs_agenda.py <ws> "(org-todo-list)"` vs the server's `org.agenda.todos` in the same workspace (`sweep.py full org.agenda.todos`).
- Expected (CLAUDE.md: "Write down any deliberate difference"): the difference in `docs/parity.org` (agenda section), or the line kept without its Emacs key hint (the keyword list with its numbers is what tells the user which `N r` to type; the server has no `N r` anyway). Got: silently missing.

## lsp-4: org-lint flags `[[roam:Title]]` as "Unknown fuzzy location" though org-roam is on, and for a node that exists until something loads org-roam's index
- State: open
- Where: diagnostics (org-lint `invalid-fuzzy-link`) of a file with `roam:` links, `org_roam_enabled = true`.
- Repro: `.claude/e2e/repro/lsp/roamdiag.py`: open `gtd.org` (has `[[roam:Beta]]`, and `roam/beta.org` is the node "Beta"); the only `publishDiagnostics` (waits 3 s) has `Unknown fuzzy location "roam:Beta"`. After an `org.open` on a roam link (`roamlink.py`) the warning on `roam:Beta` is gone, and `[[roam:Nope]]` (no such node) in `roam/beta.org` still says `Unknown fuzzy location "roam:Nope"`.
- Expected: with org-roam loaded, Emacs registers the `roam` link type, so org-lint (which checks only links of type `fuzzy`) says nothing about `roam:` links, existing or not (following one to a missing node starts a capture, which the server does). Got: the warning, depending on whether the roam index has been loaded yet.

## emacs-11: a synchronous Emacs call freezes the whole server: 40 s with a slow emacs_config, 30 s on a stuck `%(…)`, 120 s before giving up
- State: open
- Where: `server/org-lsp/src/server.rs`: the main loop calls `self.emacs.call(…)` (export menu, capture `%(sexp)`, agenda prefixes, diary sexps, attach-files, timer-default, src-block-names, refile verify, crypt-before-save on willSave…) and waits; `emacs.rs` `Caller::call` does not count Emacs's start (up to `SETUP_TIME`, 120 s) against the timeout. docs/protocol.org "Emacs": "from a thread of its own: an export never holds up the server's other answers" holds for exports only.
- Repro: `.claude/e2e/repro/emacs/t_failures.py slow` and `stuck`: with `emacs_config = "(sleep-for 40)"`, the first `org.export.dispatch` takes 40.8 s and a `textDocument/documentSymbol` sent 0.5 s later answers after 40.3 s; with `(sleep-for 130)` both wait 120 s and the command fails with only "Emacs did not answer in time" (not "Emacs could not load Org with your configuration: …"). A capture template `%(while t)` freezes every request for 30 s ("Emacs did not answer in time"; Emacs is then replaced, fine).
- Expected: other requests (symbols, highlighting, completion, edits) answered while Emacs starts or works: make these commands answer later (as `start` does for exports), or at least cap the wait while Emacs starts and say that the configuration is still loading/too slow.

## lsp-5: column view `e` (org.columns.edit) asks other questions than Emacs: "KEY value [v]: " with no allowed values, no TODO state asked, and " value: " before the view has come
- State: open
- Where: `server/org-lsp/src/server.rs` `columns_question` (~2171) and `columns_command`; `bridge.el` `org-lsp-bridge-columns-command` (fast selection off).
- Repro: `.claude/e2e/repro/lsp/columns.py` (full client, sample workspace; `gtd.org` has `#+PROPERTY: Effort_ALL 0:10 0:30 1:00 2:00` and `#+TODO: TODO NEXT WAIT(w@) | DONE(d!) CANCELLED(c@)`): `org.columns` on "Write the report", then `org.columns.edit` with `column` 2 (Effort), 1 (TODO), 0 (ITEM). Emacs side: `org-columns`, point on the Effort column, `org-columns-edit-value` (see the `emacs.py` one-liner in notes-lsp.md).
- Expected (`org-columns-edit-value`, org-colview.el:647): a property with allowed values is read with `completing-read "Value: "` over them (here 0:10 0:30 1:00 2:00, a match required); one without is `read-string "Edit: "` with the value as initial input; the TODO column runs `org-todo` interactively, which with keys in `#+TODO` asks the state by fast selection. Got: `{"kind":"text","prompt":"EFFORT value [1:00]: "}` with no completions (any text accepted), a free-text property gets "KEY value [v]: " too, and the TODO column cycles to the next state without asking. Also, `org.columns.edit` sent before the view has come back from Emacs (right after `org.columns`, first Emacs start ~1 s) asks `" value: "` (no key) even on the TODO column, and the answer then goes to whatever the column is. (What Emacs then writes, `:EFFORT:` renamed from `:Effort:`, the format stored as `%EFFORT(Effort){:}`, and the tags pulled to column ~56 in column view, Emacs batch does exactly the same.)

## emacs-12: setting a countdown while one runs fails with "Wrong type argument: listp, 2" whatever the answer to "Replace current timer?"
- State: open
- Where: `bridge.el` `org-lsp-bridge-timer-set` binds `read-from-minibuffer` (cl-letf) to return the minutes; `org-timer-set-timer` then asks `y-or-n-p "Replace current timer? "`, whose advice `org-lsp-bridge--ask` reads the editor's answer with `read-from-minibuffer` too, and gets the minutes ("2") instead of `{"answer": …}` (`json-read-from-string "2"` → `(alist-get 'answer 2)`). The real answer line is left on Emacs's input and read later as a request ("Unknown method nil", ignored).
- Repro: `.claude/e2e/repro/emacs/t_countdown2.py`: `org.timer.set` `1`, then `org.timer.set` `2`; the editor gets "Replace current timer?" (Yes/No); either answer → ERROR "Wrong type argument: listp, 2", the first countdown stays.
- Expected (Emacs): Yes → the countdown is replaced (2 minutes); No → "No timer set". Bind the minutes only for the prompt "How much time left?" (or let `--ask` read with the original function, e.g. `(let ((org-lsp-bridge--reading t)) …)` calling the unadvised/un-letf'd reader).

## neovim-8: keys typed right after the calendar's RET go to the calendar (E21), or a server request lands in 'indentexpr' (E565)
- State: open (low)
- Where: `lua/org-lsp/view.lua` (`window/showDocument` handler, line 790 `nvim_win_set_buf`), `lua/org-lsp/indent.lua:40` (`expr()` does `client:request_sync`, which runs other handlers while textlock is on).
- Repro 1 (TUI, `.claude/e2e/repro/neovim/tui.sh`): `x` on line 2, `\oi.`, then `l`, `Enter` and `o` sent in one go (as a macro replays them): `E21: Cannot make changes, 'modifiable' is off` — the `o` ran in the calendar, which closes only once the server answers. With 20 ms between Enter and `o` it works.
- Repro 2 (headless, deterministic): `.claude/e2e/repro/neovim/e565b.lua`: `<Leader>oi.`, `l`, `<CR>`, then `o<Esc>` at once: `LSP[org_lsp]: Error SERVER_REQUEST_HANDLER_ERROR: ".../view.lua:790: E565: Not allowed to change text or change window"` in :messages. The `showDocument` that puts the cursor back in the file arrives while `o` evaluates 'indentexpr', whose `request_sync` waits and dispatches it under textlock.
- Expected: as Emacs's minibuffer, RET finishes the prompt before the next key is read (a macro `\oi.<CR>o` works); a server request arriving during 'indentexpr' is deferred (`vim.schedule`) rather than failing. Got: E21/E565, the cursor not placed.

## emacs-13: commands run through the bridge see `this-command` nil: "‘nil’ failed to load required package "gnuplot"" for C-c " g
- State: open
- Where: `bridge.el` `org-lsp-bridge-run` calls the command with `apply`/`call-interactively` without binding `this-command` (and `last-command`); Org's `org-require-package` formats `this-command` into its message.
- Repro: `.claude/e2e/repro/emacs/t_edit.py`, `org.plot.gnuplot` on a table (no gnuplot.el installed): ERROR `‘nil’ failed to load required package "gnuplot"`.
- Expected (Emacs): `‘org-plot/gnuplot’ failed to load required package "gnuplot"`. Bind `this-command` (and `real-this-command`) to the command around the call; other Org code reads it too (`org-table-sum`'s message is already special-cased).

## lsp-6: C-c C-c (org.checkbox.toggle) on a headline's statistics cookie toggles the boxes below (or fails "No item in subtree") instead of counting the cookie again
- State: open
- Where: `org.checkbox.toggle` (the `C-c C-c` of the manifest) with the cursor on a cookie of a headline (`[33%]`, `[1/3]`).
- Repro: `.claude/e2e/repro/lsp/cookie.py`: `gtd.org` of the sample workspace with `** TODO Groceries [10%]` (wrong count, items `[X] milk`, `[ ] bread`, `[ ] 卵`); `org.checkbox.toggle` with the cursor on `[`, `1`, `%` or `]` of the cookie gives `[0%]` with every box unchecked. `cookie2.py`: on `[0/3]` of `** TODO Project Alpha [0/3]` (counts TODO children, no checkboxes) it fails with `No item in subtree`.
- Expected (docs/protocol.org, `org.checkbox.toggle`: "on a statistics cookie the cookies are counted again"; Emacs `org-ctrl-c-ctrl-c`, context `statistics-cookie` → `org-update-statistics-cookies`): `** TODO Groceries [33%]` with the boxes untouched (Emacs batch, `.claude/e2e/repro/lsp/emacs.py`: `(search-forward "[10%") (org-ctrl-c-ctrl-c)` gives `[33%]`), and `[1/3]` for Project Alpha. Got: boxes toggled, or the error.
- Note: elsewhere on a headline the server runs `org-toggle-checkbox` (documented), where Emacs's `C-c C-c` runs `org-set-tags-command` (org.el:18169); that difference is not marked as one in the docs or parity.org.

## emacs-14: a link abbreviation calling a function fails with "Disabling unsafe link abbrev: %(my-abbrev)…" unless the function is marked safe, which the docs do not say
- State: open (docs)
- Where: docs/protocol.org and docs/configuration.org, `org_link_abbrev_alist`: "~%(my-function)~ calls an Emacs function with what follows the colon (through Emacs)". Org 9.8 refuses a function not marked `org-link-abbrev-safe`; `bridge.el` `org-lsp-bridge-expand-abbrev` turns the warning into the error.
- Repro: `.claude/e2e/repro/emacs/t_lisp.py` with `org_link_abbrev_alist = { ex = "%(my-abbrev)" }` and `(defun my-abbrev …)` in `emacs_config`: `org.open` on `[[ex:foo]]` → ERROR "Disabling unsafe link abbrev: %(my-abbrev)\nYou may mark function safe via (put 'my-abbrev 'org-link-abbrev-safe t)". With the `put` in `emacs_config` it opens `https://example.org/FOO`.
- Expected: the option's description says the function must be marked safe in `emacs_config` (`(put 'my-abbrev 'org-link-abbrev-safe t)`), as Org's own docstring of `org-link-abbrev-alist` does.

## emacs-15: a diary sexp that signals an error is left out of the agenda silently; Emacs says "Bad sexp at line N in FILE: …"
- State: open (minor)
- Where: `bridge.el` `org-lsp-bridge-diary-sexp` (`org-diary-sexp-entry`, `beep`/`sleep-for` stubbed); the message goes to Emacs's stderr; the server shows nothing.
- Repro: `t_lisp.py`: an agenda file with `%%(error "boom")`: the day view has no line for it and no message (the other sexps, `%%(diary-float t 3 3)` and one defined in `emacs_config`, show).
- Expected: the message shown once (as an agenda `%(…)` error is: "the error is said once"), with the file and line, as Emacs's echo area shows it.

## emacs-16: a code block that fails (non-zero exit, stderr) shows nothing: Emacs's *Org-Babel Error Output* is lost
- State: open
- Where: `bridge.el` `org-lsp-bridge-run` returns the text, point and last `message`; `org-babel-eval-error-notify` writes stderr and "[ Babel evaluation exited with code N ]" to the buffer `*Org-Babel Error Output*` and displays it (no `message`), which the bridge neither reports nor clears.
- Repro: `.claude/e2e/repro/emacs/t_babel_err.py` (Babel through `emacs_options` org-babel-load-languages python/shell): `raise ValueError("bad")` in python → only an empty `#+RESULTS:` is inserted, no message; `echo out; echo err >&2; exit 3` in shell → `: out`, nothing about `err` or the exit code. Emacs batch on the same text: the error buffer holds the traceback / `err` and `[ Babel evaluation exited with code 1 ]` (`… code 3 ]`). (Elisp errors, unknown languages and `read-string` are reported: "Wrong type argument: listp, 1", "No org-babel-execute function for nosuchlang!", "Emacs asked a question org-lsp cannot answer: Your name:".)
- Expected: what Emacs shows reaches the editor: the error buffer's text as a file shown with `window/showDocument` (as an export to a buffer is), or at least a WARNING/ERROR message with its last line ("[ Babel evaluation exited with code 1 ]") and the stderr.

## emacs-17: VS Code's `org-lsp.settings` description gives its example with the old option names (agendaFiles, todoKeywords, logDone)
- State: open (minor, VS Code; found checking the emacs_options schema)
- Where: `editors/vscode/package.json` line ~1765, `org-lsp.settings` `markdownDescription` (written by hand).
- Repro: read it: "for example `{ "agendaFiles": ["~/org"], "todoKeywords": [...], "logDone": "time" }`. Without `agendaFiles`…". The server takes these but says "options are named after Org's variables now: …" (docs/configuration.org "Names").
- Expected: `org_agenda_files`, `org_todo_keywords`, `org_log_done` (and maybe an `emacs_options` example).

## lsp-7: TODO list: an entry marked done from the agenda loses its keyword on its line when its parent's cookie is recounted and the parent's tags move
- State: open
- Where: the agenda's in-place line change (`org-agenda-change-all-lines`) in the TODO list (`org.agenda.todos`); the week agenda is right.
- Repro: `.claude/e2e/repro/lsp/done_keyword_lost.py`: `a.org` is `* TODO Project [0/2] :work:` / `** TODO Build it` / `** TODO Ship it`; open the TODO list, `org.todo.next` on the "Ship it" line. Also in the sample workspace (`agenda_flow.py full todo-list-done`, "Build it" / "Ship it" under `** TODO Project Alpha [1/3]`, whose tags are not in their column).
- Expected (Emacs batch, `agenda_flow.py`): `  a:          DONE Ship it      …      :work::`. Got: `  a:          Ship it      …      :work::` (no keyword) while the file has `** DONE Ship it` and the parent `[1/2]` with its tags moved to their column. With the parent's tags already in their column, or without the cookie, the line is right; `org.agenda.redo` shows it right too.

## lsp-8: a habit marked done from the agenda keeps its graph on its line, where Emacs shows it with its scheduled leader and no graph
- State: open (minor: the line until the agenda is built again)
- Where: agenda, `org.todo` on a habit's line (`org-agenda-change-all-lines` for a habit).
- Repro: `.claude/e2e/repro/lsp/agenda_flow.py full habit-done` (sample workspace, "Water the plants", `STYLE: habit`, scheduled yesterday `.+2d`; answer `d`).
- Expected (Emacs batch): `  gtd:        Sched. 1x:  DONE Water the plants` — Emacs formats the changed line again from its `extra` property (the leader) without the habit flag, so the leader appears and the consistency graph goes until the next redo. Got: `  gtd:        DONE Water the plants                   * * *  !` (graph kept, no leader). The file is the same as Emacs's.

## emacs-18: each time Emacs starts again, the emacs_options warnings come again; "Emacs stopped" says nothing of the exit status
- State: open (minor)
- Where: `bridge.el` `--set-option` notices are sent at every start (`emacs.rs` `start` → `setup`); `emacs.rs` `request`'s `stopped` only adds stderr.
- Repro: `.claude/e2e/repro/emacs/t_restart.py`: `emacs_options = { org-export-with-toc = "bad" }` and a block `(kill-emacs 3)` run twice, then another block: three times the same WARNING "`emacs_options` leaves out `org-export-with-toc`: "bad" is not false, true or an integer", and ERROR "Emacs stopped" twice (no "exit status 3", stderr empty because the bridge inhibits messages).
- Expected: a notice already shown in this session not repeated (the server can remember the texts it showed); "Emacs stopped (exit status 3)" when there is no stderr to show.

## emacs-19: emacs_options messages for a compound value name the whole value, not the part that is wrong; a TOML date shows the parser's internals
- State: open (minor)
- Where: `bridge.el` `--set-option` / `--type-text` (one text per type head: "a table", "a list"); the TOML reader's datetime representation reaches Emacs as JSON.
- Repro: `t_names.py`, `t_edge.py`:
  - `org-export-backends = ["ascii", "html", "nosuch"]` → "`emacs_options` leaves out `org-export-backends`: ["ascii","html","nosuch"] is not a list" (it is a list; `nosuch` is not a backend);
  - `org-babel-load-languages = { emacs-lisp = true, nosuchlang = true }` → "… {…} is not a table" (it is; `nosuchlang` is not a language);
  - `org-export-default-language = 2026-10-21` (TOML date) → "… {"$__toml_private_datetime":"2026-10-21"} is not a string".
- Expected: the item that does not fit, and what that item may be ("`nosuch` is not one of ascii, beamer, html, …"); a TOML date/time given as its text (or said to be a date, which no option takes). Also worth a line in docs/emacs-options.org: JSON `null` is `nil` (`"org-export-with-toc": null` sets it to nil, it does not reset it).

## neovim-9: keys typed ahead of a one-key prompt (agenda menu, TODO keys, capture templates) run as normal commands: `nmap <F5> <Leader>oaa` does not work
- State: open
- Where: `lua/org-lsp/init.lua` `M.run`: the command goes to the server first and `M.read_key` (`getcharstr`) runs only when the answer comes back; keys already in the typeahead have been executed by then.
- Repro (tmux TUI, `.claude/e2e/repro/neovim/tui.sh`, server already running): `:nmap <F5> \oaa`, then `<F5>`: the `a` appends in the Org file (`-- INSERT --`) and the agenda menu then waits for a key. The same with `\oa` and `a` sent in one go (a macro, a fast typist); with 30 ms between them it works.
- Expected: as in Emacs, where `C-c a a` in a keyboard macro or bound to a key opens the week agenda, the key after the prefix answers the menu. Got: it is run as a Normal-mode command, and the menu stays open. The same race holds for every `input.kind == 'key'` prompt (fast TODO selection `cit` + key, `<Leader>oc` + template key, the export menu).

## neovim-10: Neovim's hover on a link in an Org file stays Markdown: its Org block is not highlighted, and a user's `FileType markdown` settings apply
- State: open (low)
- Where: `lua/org-lsp/preview.lua` `to_org`/`watch`: only a float whose first line is ```` ```org ```` (the agenda's hover) is made an Org buffer. A link's hover is `` `LINK: *Other` `` + blank + ```` ```org … ``` ```` (`.claude/e2e/repro/neovim/hover.lua` prints it).
- Repro (TUI): `:autocmd FileType markdown setlocal spell number cursorline`, then `K` on `[[*Other][other]]`: the float shows line numbers (`1 LINK: *Other`, `4 * TODO Other`), wraps the planning line at the narrow width, and the target's headline is not in Org's colors (no Org tree-sitter parser). In the agenda the same autocmd is undone (`org false false false` for filetype/spell/number/cursorline): that part works.
- Expected: the link's preview looks as the agenda's does (Org highlighting, Markdown settings undone), or the docs say only agenda hovers are converted. Got: Markdown float with the user's Markdown options.
