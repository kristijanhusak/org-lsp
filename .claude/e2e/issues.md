# Issues found, 2026-10-21

Each issue: a heading `## <part>-<n>: <one line>`, then

- State: open | fixing (branch) | fixed (#PR) | not a bug (why)
- Where: editor or client, command, file and line when known
- Repro: the exact steps or script (scripts under `.claude/e2e/repro/`)
- Expected (with Emacs's behaviour when it is the reference) and got


## neovim-1: Neovim nightly aborts (grid.c assertion) redrawing the date prompt in tests/nvim/highlight.lua
- State: documented (#368): Neovim nightly assertion, docs/development.org
- Where: Neovim nightly (v0.13.0-dev-1824+g27ee55c09d) only; `tests/nvim/highlight.lua`, the date-prompt part (`calendarJournal.future` look, line 7 of the calendar, `vim.cmd.redraw()` at highlight.lua:212). 0.12.0 and 0.12.5 pass (20 checks).
- Repro: `NVIM=/root/nvim-nightly/bin/nvim scripts/test-nvim.sh` (stops there), or `nvim --headless -u NONE -l tests/nvim/highlight.lua`; `.claude/e2e/repro/neovim/highlight-place.lua` prints the place before each redraw.
- Expected: the test passes. Got: `nvim: …/src/nvim/grid.c:586: grid_line_flush: Assertion 'grid_line_clear_to <= grid_line_maxcol' failed.`, exit 134. Likely a Neovim nightly bug, but triggered by what the calendar draws (extmark highlights of the org-date buffer in a 140-column, 50-line headless UI); see neovim-2+ for whether the calendar alone triggers it.

## neovim-2: on Neovim nightly a command on a selection prints "vim.lsp.util.character_offset is deprecated"
- State: fixed (#368)
- Where: `lua/org-lsp/init.lua` `M.run` (the `position()` of a Visual selection) calls `vim.lsp.util.character_offset`, deprecated in 0.13 (`vim.deprecate(..., 'vim.str_utfindex', '0.14')`).
- Repro: Neovim nightly, an Org file, `V` over two headlines, `>>` (or any `region` command). `tests/nvim/e2e.lua` on nightly prints the message (see its output).
- Expected: no message (use `vim.str_utfindex(line, encoding, col, false)` as the rest of the plugin uses `vim.str_byteindex`). Got: `vim.lsp.util.character_offset is deprecated. Run ":checkhealth vim.deprecated" for more information` in the echo area the first time; it goes away in 0.14.

## emacs-1: emacs_options: a `const` whose value is a keyword is read as a keyword argument, so keyword-valued constants never match (org-export-smart-quotes-alist cannot be set)
- State: fixed (#366)
- Where: `server/org-lsp/src/bridge.el` `org-lsp-bridge--type-keywords` / `--type-args`: `(while (keywordp (car rest)) …)` also eats a final keyword that is the const's value. `(const :tag "Unicode" :utf-8)` gives keywords `(:tag "Unicode" :utf-8 nil)` and no args, so the constant is `nil`. (wid-edit's own `widget-convert` stops when no value follows.)
- Repro: `.claude/e2e/repro/emacs/conv.el`: `emacs --batch -Q -l conv.el --eval '(message "%S" (org-lsp-bridge--option-value (quote (const :tag "U" :utf-8)) ":utf-8"))'` prints `":utf-8"`; `(org-lsp-bridge--type-text '(choice (const :tag "U" :utf-8) (const :html)))` is `"false"`. Over LSP (`t_kinds.py`): `emacs_options = { org-export-smart-quotes-alist = { xx = { primary-opening = { ":utf-8" = "«" } } } }` (or `utf-8`) is refused: "`emacs_options` leaves out `org-export-smart-quotes-alist`: {…} is not a table" (it is a table: the message is misleading too).
- Expected: `(("xx" (primary-opening :utf-8 "«")))` set, as Customize would; the fix is to stop at a keyword with nothing after it (`(and (keywordp (car rest)) (cdr rest))`). Got: refused.

## emacs-2: emacs_options: a property list whose :key-type is `(symbol :tag …)` gets plain symbols for keys (org-html-table-default-attributes set to `(border "1")`, silently)
- State: fixed (#366)
- Where: `bridge.el` `org-lsp-bridge--option-value`, the `plist` branch: keys become keywords only when `(eq key-type 'symbol)`; `org-html-table-default-attributes` has `:key-type (symbol :tag "Property")`, so `border` is interned as `border`. The plist widget accepts any symbol, so the value matches and is set.
- Repro: `t_kinds.py` (or conv.el `(conv-try "org-html-table-default-attributes" "ox-html" "{\"border\":\"1\",\"rules\":\"all\"}")`): Emacs ends with `(border "1" rules "all")`, no notice; an HTML export then has none of its table attributes (ox-html reads `:border` …).
- Expected (docs/emacs-options.org "How values become Lisp": "a property list, whose keys are keywords (`scale` is `:scale`)"): `(:border "1" :rules "all")`. Check the head of the key type, not the type itself (`(memq (if (consp key-type) (car key-type) key-type) '(symbol sexp))`).

## emacs-3: emacs_options: `(repeat :inline t …)` inside a list is not handled: org-latex-classes with sectioning cannot be set
- State: fixed (#366)
- Where: `bridge.el` `--option-value`, the `list`/`group` branch maps items one to one onto the type's args; an `:inline t` repeat (org-latex-classes, `(list string string (repeat :inline t (choice (cons string string) …)))`) should take the rest of the items.
- Repro: `emacs_options = { org-latex-classes = [["mini", "\\documentclass{article}", ["\\section{%s}", "\\section*{%s}"]]] }` (`t_kinds.py`): "`emacs_options` leaves out `org-latex-classes`: [[…]] is not a list". conv.el shows the item becomes `("\\section{%s}" "\\section*{%s}")` (a list, not the cons) inside a list of 3. Only `[["mini","\\documentclass{article}"]]` (no sections) is accepted.
- Expected: `(("mini" "\\documentclass{article}" ("\\section{%s}" . "\\section*{%s}")))`: org-latex-classes is one of the most customized LaTeX options, and the page lists it as settable. Also org-html-infojs-options is a `(set :inline t …)`.

## emacs-4: emacs_options: in a `choice`, the first alternative a value fits wins, so some values cannot be said (python `auto`, infojs `path`, `(not "LOGBOOK")`)
- State: fixed (#366)
- Where: `bridge.el` `--option-value`, `choice` branch (tries alternatives in order, takes the first that matches).
- Repro: conv.el / `t_kinds.py`:
  - `org-babel-python-command-session = "auto"` (and `org-babel-python-command`): type `(choice string (const auto))`, so `"auto"` stays the string `"auto"`, which ob-python then runs as a command named `auto`; the default symbol `auto` cannot be given back.
  - `org-html-infojs-options = { path = "x.js" }`: `(choice (symbol …) (string …))` makes it the symbol `x.js` (a publishing property for Org), not the file `"x.js"`.
  - `org-export-with-drawers = ["not", "LOGBOOK"]` fits `(repeat string)` first and becomes `("not" "LOGBOOK")`, which exports only drawers named "not" and "LOGBOOK", the opposite of the default `(not "LOGBOOK")`, which cannot be said at all.
  All three are set without a notice.
- Expected: a string equal to a `const`'s symbol name taken as that constant before a generic `string`/`symbol` alternative (Customize's user picks the alternative; here the only way to say it is the name); a string where a `string` alternative exists stays a string before `symbol`; and a list whose first item names a `const` of a `list` alternative (`not`) goes to that alternative. Or document the limits on the page.

## emacs-5: emacs_options: function-valued options are listed as settable but cannot be set, not even to a function's name
- State: fixed (#366)
- Where: `scripts/org-options.json` marks `emacs` 19 options whose type is just `function` (org-html-format-drawer-function, org-html-format-headline-function, org-latex-format-headline-function, org-ascii-format-drawer-function, org-odt-format-*-function, org-texinfo-format-*-function, org-babel-latex-preamble/begin-env/end-env, org-babel-csharp-generate-*-command…), and `org-columns-summary-types` (`:value-type function`); `bridge.el` keeps a string a string for `function`, which never matches.
- Repro: `emacs_options = { org-html-format-headline-function = "org-html-format-headline-default-function" }`: "`emacs_options` leaves out `org-html-format-headline-function`: "org-html-format-headline-default-function" is not a function, which goes in `emacs_config`". The docs/emacs-options.org intro says "those whose value is Lisp (a function, a hook) go in `emacs_config`… The other options of Org are not here", yet these 19 are on the page (with defaults like a printed closure, see emacs-7).
- Expected: either mark them `lisp` (the server then says so before Emacs starts, and they leave the page), or (better, as Customize takes a symbol for a `function` type) intern a string naming a defined function (`fboundp`) for `function`/`command` types: `"my-drawer"` defined in `emacs_config` would then work, which the message's own advice suggests.

## emacs-6: emacs_options from the editor is merged into the file's key by key, deep: a table value (alist) gets the file's entries too, and Neovim's `[]` drops the file's emacs_options whole
- State: fixed (#366)
- Where: `server/org-lsp/src/config.rs` `load` → `merge` (objects merged recursively at every depth); `Options.emacs_options` reads `[]` as empty (`table_or_empty_list`) after the merge.
- Repro: `.claude/e2e/repro/emacs/t_precedence.py`. Shared file `[emacs_options] org-attach-method = "mv"`, `org-babel-load-languages = { python = true, emacs-lisp = true }`, `org-export-with-toc = false`.
  - Editor `emacs_options = { "org-babel-load-languages": { "shell": true }, "org-export-with-toc": 3 }`: Emacs gets `((python . t) (emacs-lisp . t) (shell . t))`; toc 3 and attach `mv` are right.
  - Editor `emacs_options = []` (Neovim's `vim.empty_dict()`-less `{}`): Emacs gets none of the file's options (attach `ln` from emacs_config, toc `t`), while `{}` keeps them.
- Expected (docs/emacs-options.org: "The editor's `emacs_options` goes over the file's option by option"): the editor's value of an option replaces the file's value of that option whole, so `((shell . t))`; and `[]` is an empty table, merging nothing (normalize `[]` to `{}` before `merge`, as `load` already does for the whole options). The same deep merge applies to any object-valued server option (worth checking for the others, e.g. tables inside `org_*` settings).

## neovim-3: docs say M-RET after a checkbox item makes `[ ]`; it does not, and neither does Emacs
- State: fixed (#368)
- Where: `scripts/features.mjs:284` (TESTING.org "Lists and checkboxes": "after a checkbox item the new one has `[ ]`: `<Leader><CR>`"), and the `return_uses_meta_return` description in `server/org-lsp/src/settings.rs` ("numbered and with a checkbox as M-RET makes it").
- Repro: `.claude/e2e/repro/neovim/structure.lua` (section lists): `- [ ] a` / `- b`, cursor on line 1, `<Leader><CR>` gives `- ` with no box. Emacs 9.8.10: `(call-interactively #'org-meta-return)` at the end of `- [ ] a` gives `- [ ] a|- |- b` (org-meta-return calls `org-insert-item` with no CHECKBOX; only M-S-RET, `org-insert-todo-heading`, adds one).
- Expected: the docs to say what Org does (the behaviour is Emacs's). Got: the check as written fails.

## neovim-4: errors of commands show as `org-lsp: <Emacs's text>` at WARN level, not Emacs's text alone
- State: fixed (#368)
- Where: `lua/org-lsp/init.lua` `M.run` → `handle`: `notify('org-lsp: ' .. tostring(err.message), vim.log.levels.WARN)`.
- Repro: `.claude/e2e/repro/neovim/structure.lua` / `config.lua`: `<Leader>oK` on the first subtree, `g{` on a top headline, C-k on a folded headline with `org_ctrl_k_protect_subtree = true` answered `n`.
- Expected (CLAUDE.md "Say what Emacs says"): `Cannot move past superior level or buffer limit`, `Already at top level of the outline`, `‘C-k’ aborted as it would kill a hidden subtree`, as Emacs's echo area shows them. Got: `org-lsp: Cannot move past superior level or buffer limit` etc., as warnings; the messages of results (`result.message`) have no prefix, so the two kinds read differently. The notification is already titled `Org`.

## neovim-5: a FileType autocmd's 'conceallevel' is overridden to 2 in Org windows
- State: fixed (#368)
- Where: `lua/org-lsp/init.lua` `conceal()`: sets `conceallevel = 2` on attach and on every `BufWinEnter`.
- Repro: `.claude/e2e/repro/neovim/config.lua` (section FileType autocmd): `autocmd FileType org setlocal conceallevel=1 shiftwidth=4 spell textwidth=70`, open an Org file: shiftwidth/spell/textwidth stay, conceallevel is 2.
- Expected: the user's own window option kept (as `set_window_folds` keeps later changes of the user's), or the docs to say `conceal = false` is the way to keep one's own 'conceallevel'. Got: 2 whatever the user set; only `conceal = false` keeps it.

## emacs-7: C-c C-a u (org-attach-url) always fails: "Cannot prompt about "…" interactively in batch mode.  Aborting"; the settings org_resource_download_policy / org_safe_remote_resources do not reach Emacs
- State: fixed (#366)
- Where: `bridge.el` `org-lsp-bridge-attach` "u" → `org-attach-url` → `org--confirm-resource-safe`, which errors when `noninteractive` instead of asking (so the bridge's y-or-n-p advice never sees it). `org-resource-download-policy` and `org-safe-remote-resources` are `setting` in scripts/org-options.json (so `emacs_options` refuses them: "it is the setting `org_resource_download_policy`"), but `Options::emacs_lisp` does not pass them to Emacs.
- Repro: `.claude/e2e/repro/emacs/t_attach_url.py`: `org.attach` `u` `file:///etc/hostname` with no setting, with `org_resource_download_policy = true`, and with `org_safe_remote_resources = ["\\`file:///etc/"]`: the same error each time, nothing attached. The same applies to a remote `#+INCLUDE:`/`#+SETUPFILE:` in an export.
- Expected: Emacs asks "Fetch and attach …?" (Emacs interactively offers `!`/`f`/`d`/`s`/`q`): put the question to the editor (advise `org--confirm-resource-safe`, or bind `noninteractive` nil around it with the y-or-n-p advice), and pass the two settings to Emacs (as `emacs_lisp` does for the tangling ones). Workaround today: `(setq org-resource-download-policy t)` in `emacs_config`.

## emacs-8: a message Emacs gives is dropped when the command also changes the text (first C-c C-a a: no "File "a.txt" is now an attachment")
- State: fixed (#366)
- Where: `server/org-lsp/src/server.rs` `emacs_edited`: `said` (the `message` the bridge recorded) is shown only when `new == old`; when the command edits the document it is never shown.
- Repro: `.claude/e2e/repro/emacs/t_attach.py`: `org.attach` `a` `a.txt` on an entry without an ID: the ID and `:ATTACH:` are added, no message; the next `c`/`l`/`y`/`m` (no text change) each show `File "c.txt" is now an attachment`. Same for any Emacs-run command that edits and messages (e.g. `org-table-recalculate`'s, footnote commands, `org-attach-set-directory`'s questions' follow-ups).
- Expected: Emacs's last message shown (INFO) after the edit is applied, as the echo area would show it.

## emacs-9: the attachment dispatcher's keys are hard-coded: no `q` (Abort), and an unknown key says "No attachment command under x" instead of Emacs's "No such attachment command: x"
- State: fixed (#366)
- Where: `server.rs` `attach` (`COMMANDS`, 14 keys); Org 9.8's `org-attach-commands` also has `q` ("Abort.", messages "Abort"), `b`, `O`, `F` and the `C-` variants of each key.
- Repro: `t_attach.py`: `org.attach` with input `q` → error -32803 "No attachment command under q"; `x` → "No attachment command under x".
- Expected: `q` offered and answering it ends the command with "Abort" (or quietly); for another key Emacs's text `No such attachment command: x` ("Say what Emacs says"). `b`/`O`/`F` are Emacs-buffer/Dired variants and fine to leave out (worth a line in the docs). The `org-attach-commands` option the page lists has no effect on this menu.

## emacs-10: docs/emacs-options.org: docstrings written without org-mode-map ("Uses keymap `org-mode-map', which is not currently defined."), closures with their captured variables as defaults, odd "Since" lines
- State: fixed (#366)
- Where: the generator behind `scripts/emacs-options.json` (`scripts/emacs_options_reference.py`), the page `docs/emacs-options.org`.
- Repro: `.claude/e2e/repro/emacs/dump_options.el` + `cmp_options.py` (all 480 entries against Emacs with Org 9.8.10): docstring text, type and default agree otherwise, but
  - org-confirm-babel-evaluate (page line 333) and org-babel-no-eval-on-ctrl-c-ctrl-c (365) start with "Uses keymap ~org-mode-map~, which is not currently defined." and say ~M-x org-ctrl-c-ctrl-c~, where `C-h v` (Org loaded) says "Remove code block evaluation from the `C-c C-c' key binding." (`\<org-mode-map>` substituted before `org` was loaded: require `org` first);
  - function-valued defaults print as interpreted closures with their environment, e.g. org-html-format-drawer-function's Default `(closure (htmlize-buffer-places org-html-format-table-no-css htmlize-css-name-prefix htmlize-output-type htmlize-output-type htmlize-css-name-prefix t) (_name contents) contents)` (also org-ascii-/org-latex-/org-odt-/org-texinfo-format-drawer-function, org-babel-latex-preamble/begin-env/end-env): `C-h v` in an installed (compiled) Org shows a byte-code object; the source form `(lambda (_name contents) contents)` (`(car (get sym 'standard-value))`) is what helps a reader;
  - "Since :: Org. (9.8)" for the five ob-csharp options (Org's own typo `:package-version '(Org. "9.8")`) and "Since :: Org (9.4)" for org-babel-haskell-compiler (`'(Org "9.4")`, a list): normalize to "Org 9.8"/"Org 9.4".
- Checked fine: 480 level-3 headings = the 480 options, no stray headings or unbalanced blocks (org-element), every CUSTOM_ID is GitHub's anchor (org-babel-C++-compiler → `org-babel-c-compiler-1`), all 394 TOML examples parse and equal `json` (the intro's example aside), every `json` default validates against both schemas.

## neovim-6: Neovim: typing a date at the prompt does not show what it means (no `org.date.preview`)
- State: fixed (#368)
- Where: `lua/org-lsp/init.lua` `M.run` (the `vim.ui.input` of a command's `input`); nothing in `lua/` calls `org.date.preview` (only `editors/vscode/src/extension.ts:439` does). docs/protocol.org ("While the answer is typed, `org.date.preview` also shows it in the minibuffer line") and TESTING.org (Dates and times, `only: ['neovim', 'vscode']` in `scripts/features.mjs:439`) say Neovim does it.
- Repro (real TUI in tmux, `.claude/e2e/repro/neovim/tui.sh`): open `* TODO Task`, `\ois`, `i`, type `fri`: the calendar's last line stays `SCHEDULED Date+time [2026-10-21]:    => <2026-10-21 Wed>` (waited 3 s); Enter then schedules `<2026-10-23 Fri>` correctly.
- Expected (Emacs's `org-read-date-display`): the last line follows the typing, `SCHEDULED Date+time [2026-10-21]: fri    => <2026-10-23 Fri>`. Got: the default's meaning only. (A `CmdlineChanged` autocommand while the input is open could send `org.date.preview`.)

## neovim-7: a count does nothing in the date prompt's calendar (`3l`, `2j` move one day/week)
- State: fixed (#368)
- Where: server `server/org-lsp/src/server.rs` `calendar_command` (~7959: `open.select(at + 1 …)`, `at + 7`, months, years) ignores `context.count`; the Neovim plugin sends it (`M.run(name, { count = vim.v.count })`).
- Repro: tmux TUI (`.claude/e2e/repro/neovim/tui.sh`), `* TODO Task`, `\ois`, `2j` → `=> <2026-10-28 Wed>` (one week), `3l` → one day. Headless: `dates.lua` section calendar keys (`5l`). `tests/e2e/scenarios/calendar.json` has `"count": 2` on `org.calendar.week.later`, and its expected cursor does not show two weeks either.
- Expected: Vim counts work as docs/keys/neovim.org says ("Counts work as in Vim"), and as Emacs's `calendar-forward-day`/`calendar-forward-week` take a prefix argument in the calendar. Got: the count is dropped.

## lsp-1: citation keys are completed (and quick-fixed) twice when the same .bib is named by `#+BIBLIOGRAPHY:` and `org_cite_global_bibliography`
- State: fixed (#367)
- Where: server, `textDocument/completion` after `[cite:@`, `org.cite.insert`'s choices, and the quick fixes of the "Unknown citation key" diagnostic (`server/org-lsp/src/server.rs` near the `Suggestions:` hover, ~15134).
- Repro: `.claude/e2e/repro/lsp/completion.py plain` (sample workspace of `workspace.py`: `gtd.org` has `#+BIBLIOGRAPHY: refs.bib`, the options name the same file absolutely). Completion at `[cite:@` gives `['knuth1984', 'lamport1994', 'knuth1984', 'lamport1994']`; `org.cite.insert` offers four choices; code actions on `@knuht1984` give two identical "Replace with @knuth1984".
- Expected: each key once, as Emacs's completion table (`org-cite-basic--key-completion-table`, a hash keyed by the completion string) has it. Emacs does list the file twice (`org-cite-list-bibliography-files` gives "refs.bib" and the absolute path, `delete-dups` keeps both), and its help echo of an unknown key does say `knuth1984 knuth1984`, so the hover and the diagnostic message matching Emacs is fine; the completion items, the `org.cite.insert` choices and the duplicate quick fixes are not.

## lsp-2: protocol.org's list of requests leaves out several the server answers (references, rename, workspace symbols, document links, inlay hints, formatting)
- State: fixed (#367)
- Where: `docs/protocol.org`, "Capabilities": the server announces `referencesProvider`, `renameProvider` (with prepare), `workspaceSymbolProvider`, `documentLinkProvider` (with resolve), `inlayHintProvider`, `documentFormattingProvider`, and `org.cycle` / `org.date.preview` among `executeCommandProvider`'s commands (315, the manifest has 313).
- Repro: `.claude/e2e/repro/lsp/caps.py` prints the capabilities.
- Expected: each listed with what it does; e.g. that `textDocument/references` gives the links that lead to the *entry* at the cursor (by ID, headline search or CUSTOM_ID: references on a `<<target>>` or a tag give only the enclosing headline), that a rename renames a headline or a tag (and answers prompts), what the inlay hints are. Got: only `rangeFormatting` mentions `textDocument/formatting` in passing; the rest is not described (any-editor.org names some in one line).

## lsp-3: the TODO list and tags views leave out Emacs's "Press ‘N r’ … to search again: (0)[ALL] (1)CANCELLED …" lines, a difference written down nowhere
- State: written down as a difference (#367)
- Where: `org.agenda.todos`, `org.agenda.dispatch` `t` / `m` / `M`; `scripts/agenda_reference.py` `clean()` drops those lines from the recorded cases ("what a server cannot or should not reproduce"), but no doc says so.
- Repro: `.claude/e2e/repro/lsp/emacs_agenda.py <ws> "(org-todo-list)"` vs the server's `org.agenda.todos` in the same workspace (`sweep.py full org.agenda.todos`).
- Expected (CLAUDE.md: "Write down any deliberate difference"): the difference in `docs/parity.org` (agenda section), or the line kept without its Emacs key hint (the keyword list with its numbers is what tells the user which `N r` to type; the server has no `N r` anyway). Got: silently missing.

## lsp-4: org-lint flags `[[roam:Title]]` as "Unknown fuzzy location" though org-roam is on, and for a node that exists until something loads org-roam's index
- State: fixed (#367)
- Where: diagnostics (org-lint `invalid-fuzzy-link`) of a file with `roam:` links, `org_roam_enabled = true`.
- Repro: `.claude/e2e/repro/lsp/roamdiag.py`: open `gtd.org` (has `[[roam:Beta]]`, and `roam/beta.org` is the node "Beta"); the only `publishDiagnostics` (waits 3 s) has `Unknown fuzzy location "roam:Beta"`. After an `org.open` on a roam link (`roamlink.py`) the warning on `roam:Beta` is gone, and `[[roam:Nope]]` (no such node) in `roam/beta.org` still says `Unknown fuzzy location "roam:Nope"`.
- Expected: with org-roam loaded, Emacs registers the `roam` link type, so org-lint (which checks only links of type `fuzzy`) says nothing about `roam:` links, existing or not (following one to a missing node starts a capture, which the server does). Got: the warning, depending on whether the roam index has been loaded yet.

## emacs-11: a synchronous Emacs call freezes the whole server: 40 s with a slow emacs_config, 30 s on a stuck `%(…)`, 120 s before giving up
- State: fixed (#366, #369)
- Where: `server/org-lsp/src/server.rs`: the main loop calls `self.emacs.call(…)` (export menu, capture `%(sexp)`, agenda prefixes, diary sexps, attach-files, timer-default, src-block-names, refile verify, crypt-before-save on willSave…) and waits; `emacs.rs` `Caller::call` does not count Emacs's start (up to `SETUP_TIME`, 120 s) against the timeout. docs/protocol.org "Emacs": "from a thread of its own: an export never holds up the server's other answers" holds for exports only.
- Repro: `.claude/e2e/repro/emacs/t_failures.py slow` and `stuck`: with `emacs_config = "(sleep-for 40)"`, the first `org.export.dispatch` takes 40.8 s and a `textDocument/documentSymbol` sent 0.5 s later answers after 40.3 s; with `(sleep-for 130)` both wait 120 s and the command fails with only "Emacs did not answer in time" (not "Emacs could not load Org with your configuration: …"). A capture template `%(while t)` freezes every request for 30 s ("Emacs did not answer in time"; Emacs is then replaced, fine).
- Expected: other requests (symbols, highlighting, completion, edits) answered while Emacs starts or works: make these commands answer later (as `start` does for exports), or at least cap the wait while Emacs starts and say that the configuration is still loading/too slow.

## lsp-5: column view `e` (org.columns.edit) asks other questions than Emacs: "KEY value [v]: " with no allowed values, no TODO state asked, and " value: " before the view has come
- State: fixed (#367)
- Where: `server/org-lsp/src/server.rs` `columns_question` (~2171) and `columns_command`; `bridge.el` `org-lsp-bridge-columns-command` (fast selection off).
- Repro: `.claude/e2e/repro/lsp/columns.py` (full client, sample workspace; `gtd.org` has `#+PROPERTY: Effort_ALL 0:10 0:30 1:00 2:00` and `#+TODO: TODO NEXT WAIT(w@) | DONE(d!) CANCELLED(c@)`): `org.columns` on "Write the report", then `org.columns.edit` with `column` 2 (Effort), 1 (TODO), 0 (ITEM). Emacs side: `org-columns`, point on the Effort column, `org-columns-edit-value` (see the `emacs.py` one-liner in notes-lsp.md).
- Expected (`org-columns-edit-value`, org-colview.el:647): a property with allowed values is read with `completing-read "Value: "` over them (here 0:10 0:30 1:00 2:00, a match required); one without is `read-string "Edit: "` with the value as initial input; the TODO column runs `org-todo` interactively, which with keys in `#+TODO` asks the state by fast selection. Got: `{"kind":"text","prompt":"EFFORT value [1:00]: "}` with no completions (any text accepted), a free-text property gets "KEY value [v]: " too, and the TODO column cycles to the next state without asking. Also, `org.columns.edit` sent before the view has come back from Emacs (right after `org.columns`, first Emacs start ~1 s) asks `" value: "` (no key) even on the TODO column, and the answer then goes to whatever the column is. (What Emacs then writes, `:EFFORT:` renamed from `:Effort:`, the format stored as `%EFFORT(Effort){:}`, and the tags pulled to column ~56 in column view, Emacs batch does exactly the same.)

## emacs-12: setting a countdown while one runs fails with "Wrong type argument: listp, 2" whatever the answer to "Replace current timer?"
- State: fixed (#366)
- Where: `bridge.el` `org-lsp-bridge-timer-set` binds `read-from-minibuffer` (cl-letf) to return the minutes; `org-timer-set-timer` then asks `y-or-n-p "Replace current timer? "`, whose advice `org-lsp-bridge--ask` reads the editor's answer with `read-from-minibuffer` too, and gets the minutes ("2") instead of `{"answer": …}` (`json-read-from-string "2"` → `(alist-get 'answer 2)`). The real answer line is left on Emacs's input and read later as a request ("Unknown method nil", ignored).
- Repro: `.claude/e2e/repro/emacs/t_countdown2.py`: `org.timer.set` `1`, then `org.timer.set` `2`; the editor gets "Replace current timer?" (Yes/No); either answer → ERROR "Wrong type argument: listp, 2", the first countdown stays.
- Expected (Emacs): Yes → the countdown is replaced (2 minutes); No → "No timer set". Bind the minutes only for the prompt "How much time left?" (or let `--ask` read with the original function, e.g. `(let ((org-lsp-bridge--reading t)) …)` calling the unadvised/un-letf'd reader).

## neovim-8: keys typed right after the calendar's RET go to the calendar (E21), or a server request lands in 'indentexpr' (E565)
- State: fixed (#368)
- Where: `lua/org-lsp/view.lua` (`window/showDocument` handler, line 790 `nvim_win_set_buf`), `lua/org-lsp/indent.lua:40` (`expr()` does `client:request_sync`, which runs other handlers while textlock is on).
- Repro 1 (TUI, `.claude/e2e/repro/neovim/tui.sh`): `x` on line 2, `\oi.`, then `l`, `Enter` and `o` sent in one go (as a macro replays them): `E21: Cannot make changes, 'modifiable' is off` — the `o` ran in the calendar, which closes only once the server answers. With 20 ms between Enter and `o` it works.
- Repro 2 (headless, deterministic): `.claude/e2e/repro/neovim/e565b.lua`: `<Leader>oi.`, `l`, `<CR>`, then `o<Esc>` at once: `LSP[org_lsp]: Error SERVER_REQUEST_HANDLER_ERROR: ".../view.lua:790: E565: Not allowed to change text or change window"` in :messages. The `showDocument` that puts the cursor back in the file arrives while `o` evaluates 'indentexpr', whose `request_sync` waits and dispatches it under textlock.
- Expected: as Emacs's minibuffer, RET finishes the prompt before the next key is read (a macro `\oi.<CR>o` works); a server request arriving during 'indentexpr' is deferred (`vim.schedule`) rather than failing. Got: E21/E565, the cursor not placed.

## emacs-13: commands run through the bridge see `this-command` nil: "‘nil’ failed to load required package "gnuplot"" for C-c " g
- State: fixed (#366)
- Where: `bridge.el` `org-lsp-bridge-run` calls the command with `apply`/`call-interactively` without binding `this-command` (and `last-command`); Org's `org-require-package` formats `this-command` into its message.
- Repro: `.claude/e2e/repro/emacs/t_edit.py`, `org.plot.gnuplot` on a table (no gnuplot.el installed): ERROR `‘nil’ failed to load required package "gnuplot"`.
- Expected (Emacs): `‘org-plot/gnuplot’ failed to load required package "gnuplot"`. Bind `this-command` (and `real-this-command`) to the command around the call; other Org code reads it too (`org-table-sum`'s message is already special-cased).

## lsp-6: C-c C-c (org.checkbox.toggle) on a headline's statistics cookie toggles the boxes below (or fails "No item in subtree") instead of counting the cookie again
- State: fixed (#367)
- Where: `org.checkbox.toggle` (the `C-c C-c` of the manifest) with the cursor on a cookie of a headline (`[33%]`, `[1/3]`).
- Repro: `.claude/e2e/repro/lsp/cookie.py`: `gtd.org` of the sample workspace with `** TODO Groceries [10%]` (wrong count, items `[X] milk`, `[ ] bread`, `[ ] 卵`); `org.checkbox.toggle` with the cursor on `[`, `1`, `%` or `]` of the cookie gives `[0%]` with every box unchecked. `cookie2.py`: on `[0/3]` of `** TODO Project Alpha [0/3]` (counts TODO children, no checkboxes) it fails with `No item in subtree`.
- Expected (docs/protocol.org, `org.checkbox.toggle`: "on a statistics cookie the cookies are counted again"; Emacs `org-ctrl-c-ctrl-c`, context `statistics-cookie` → `org-update-statistics-cookies`): `** TODO Groceries [33%]` with the boxes untouched (Emacs batch, `.claude/e2e/repro/lsp/emacs.py`: `(search-forward "[10%") (org-ctrl-c-ctrl-c)` gives `[33%]`), and `[1/3]` for Project Alpha. Got: boxes toggled, or the error.
- Note: elsewhere on a headline the server runs `org-toggle-checkbox` (documented), where Emacs's `C-c C-c` runs `org-set-tags-command` (org.el:18169); that difference is not marked as one in the docs or parity.org.

## emacs-14: a link abbreviation calling a function fails with "Disabling unsafe link abbrev: %(my-abbrev)…" unless the function is marked safe, which the docs do not say
- State: fixed (#366) (docs)
- Where: docs/protocol.org and docs/configuration.org, `org_link_abbrev_alist`: "~%(my-function)~ calls an Emacs function with what follows the colon (through Emacs)". Org 9.8 refuses a function not marked `org-link-abbrev-safe`; `bridge.el` `org-lsp-bridge-expand-abbrev` turns the warning into the error.
- Repro: `.claude/e2e/repro/emacs/t_lisp.py` with `org_link_abbrev_alist = { ex = "%(my-abbrev)" }` and `(defun my-abbrev …)` in `emacs_config`: `org.open` on `[[ex:foo]]` → ERROR "Disabling unsafe link abbrev: %(my-abbrev)\nYou may mark function safe via (put 'my-abbrev 'org-link-abbrev-safe t)". With the `put` in `emacs_config` it opens `https://example.org/FOO`.
- Expected: the option's description says the function must be marked safe in `emacs_config` (`(put 'my-abbrev 'org-link-abbrev-safe t)`), as Org's own docstring of `org-link-abbrev-alist` does.

## emacs-15: a diary sexp that signals an error is left out of the agenda silently; Emacs says "Bad sexp at line N in FILE: …"
- State: fixed (#366) (minor)
- Where: `bridge.el` `org-lsp-bridge-diary-sexp` (`org-diary-sexp-entry`, `beep`/`sleep-for` stubbed); the message goes to Emacs's stderr; the server shows nothing.
- Repro: `t_lisp.py`: an agenda file with `%%(error "boom")`: the day view has no line for it and no message (the other sexps, `%%(diary-float t 3 3)` and one defined in `emacs_config`, show).
- Expected: the message shown once (as an agenda `%(…)` error is: "the error is said once"), with the file and line, as Emacs's echo area shows it.

## emacs-16: a code block that fails (non-zero exit, stderr) shows nothing: Emacs's *Org-Babel Error Output* is lost
- State: fixed (#366)
- Where: `bridge.el` `org-lsp-bridge-run` returns the text, point and last `message`; `org-babel-eval-error-notify` writes stderr and "[ Babel evaluation exited with code N ]" to the buffer `*Org-Babel Error Output*` and displays it (no `message`), which the bridge neither reports nor clears.
- Repro: `.claude/e2e/repro/emacs/t_babel_err.py` (Babel through `emacs_options` org-babel-load-languages python/shell): `raise ValueError("bad")` in python → only an empty `#+RESULTS:` is inserted, no message; `echo out; echo err >&2; exit 3` in shell → `: out`, nothing about `err` or the exit code. Emacs batch on the same text: the error buffer holds the traceback / `err` and `[ Babel evaluation exited with code 1 ]` (`… code 3 ]`). (Elisp errors, unknown languages and `read-string` are reported: "Wrong type argument: listp, 1", "No org-babel-execute function for nosuchlang!", "Emacs asked a question org-lsp cannot answer: Your name:".)
- Expected: what Emacs shows reaches the editor: the error buffer's text as a file shown with `window/showDocument` (as an export to a buffer is), or at least a WARNING/ERROR message with its last line ("[ Babel evaluation exited with code 1 ]") and the stderr.

## emacs-17: VS Code's `org-lsp.settings` description gives its example with the old option names (agendaFiles, todoKeywords, logDone)
- State: fixed (#366) (minor, VS Code; found checking the emacs_options schema)
- Where: `editors/vscode/package.json` line ~1765, `org-lsp.settings` `markdownDescription` (written by hand).
- Repro: read it: "for example `{ "agendaFiles": ["~/org"], "todoKeywords": [...], "logDone": "time" }`. Without `agendaFiles`…". The server takes these but says "options are named after Org's variables now: …" (docs/configuration.org "Names").
- Expected: `org_agenda_files`, `org_todo_keywords`, `org_log_done` (and maybe an `emacs_options` example).

## lsp-7: TODO list: an entry marked done from the agenda loses its keyword on its line when its parent's cookie is recounted and the parent's tags move
- State: fixed (#367)
- Where: the agenda's in-place line change (`org-agenda-change-all-lines`) in the TODO list (`org.agenda.todos`); the week agenda is right.
- Repro: `.claude/e2e/repro/lsp/done_keyword_lost.py`: `a.org` is `* TODO Project [0/2] :work:` / `** TODO Build it` / `** TODO Ship it`; open the TODO list, `org.todo.next` on the "Ship it" line. Also in the sample workspace (`agenda_flow.py full todo-list-done`, "Build it" / "Ship it" under `** TODO Project Alpha [1/3]`, whose tags are not in their column).
- Expected (Emacs batch, `agenda_flow.py`): `  a:          DONE Ship it      …      :work::`. Got: `  a:          Ship it      …      :work::` (no keyword) while the file has `** DONE Ship it` and the parent `[1/2]` with its tags moved to their column. With the parent's tags already in their column, or without the cookie, the line is right; `org.agenda.redo` shows it right too.

## lsp-8: a habit marked done from the agenda keeps its graph on its line, where Emacs shows it with its scheduled leader and no graph
- State: fixed (#367) (minor: the line until the agenda is built again)
- Where: agenda, `org.todo` on a habit's line (`org-agenda-change-all-lines` for a habit).
- Repro: `.claude/e2e/repro/lsp/agenda_flow.py full habit-done` (sample workspace, "Water the plants", `STYLE: habit`, scheduled yesterday `.+2d`; answer `d`).
- Expected (Emacs batch): `  gtd:        Sched. 1x:  DONE Water the plants` — Emacs formats the changed line again from its `extra` property (the leader) without the habit flag, so the leader appears and the consistency graph goes until the next redo. Got: `  gtd:        DONE Water the plants                   * * *  !` (graph kept, no leader). The file is the same as Emacs's.

## emacs-18: each time Emacs starts again, the emacs_options warnings come again; "Emacs stopped" says nothing of the exit status
- State: fixed (#366) (minor)
- Where: `bridge.el` `--set-option` notices are sent at every start (`emacs.rs` `start` → `setup`); `emacs.rs` `request`'s `stopped` only adds stderr.
- Repro: `.claude/e2e/repro/emacs/t_restart.py`: `emacs_options = { org-export-with-toc = "bad" }` and a block `(kill-emacs 3)` run twice, then another block: three times the same WARNING "`emacs_options` leaves out `org-export-with-toc`: "bad" is not false, true or an integer", and ERROR "Emacs stopped" twice (no "exit status 3", stderr empty because the bridge inhibits messages).
- Expected: a notice already shown in this session not repeated (the server can remember the texts it showed); "Emacs stopped (exit status 3)" when there is no stderr to show.

## emacs-19: emacs_options messages for a compound value name the whole value, not the part that is wrong; a TOML date shows the parser's internals
- State: fixed (#366) (minor)
- Where: `bridge.el` `--set-option` / `--type-text` (one text per type head: "a table", "a list"); the TOML reader's datetime representation reaches Emacs as JSON.
- Repro: `t_names.py`, `t_edge.py`:
  - `org-export-backends = ["ascii", "html", "nosuch"]` → "`emacs_options` leaves out `org-export-backends`: ["ascii","html","nosuch"] is not a list" (it is a list; `nosuch` is not a backend);
  - `org-babel-load-languages = { emacs-lisp = true, nosuchlang = true }` → "… {…} is not a table" (it is; `nosuchlang` is not a language);
  - `org-export-default-language = 2026-10-21` (TOML date) → "… {"$__toml_private_datetime":"2026-10-21"} is not a string".
- Expected: the item that does not fit, and what that item may be ("`nosuch` is not one of ascii, beamer, html, …"); a TOML date/time given as its text (or said to be a date, which no option takes). Also worth a line in docs/emacs-options.org: JSON `null` is `nil` (`"org-export-with-toc": null` sets it to nil, it does not reset it).

## neovim-9: keys typed ahead of a one-key prompt (agenda menu, TODO keys, capture templates) run as normal commands: `nmap <F5> <Leader>oaa` does not work
- State: fixed (#368)
- Where: `lua/org-lsp/init.lua` `M.run`: the command goes to the server first and `M.read_key` (`getcharstr`) runs only when the answer comes back; keys already in the typeahead have been executed by then.
- Repro (tmux TUI, `.claude/e2e/repro/neovim/tui.sh`, server already running): `:nmap <F5> \oaa`, then `<F5>`: the `a` appends in the Org file (`-- INSERT --`) and the agenda menu then waits for a key. The same with `\oa` and `a` sent in one go (a macro, a fast typist); with 30 ms between them it works.
- Expected: as in Emacs, where `C-c a a` in a keyboard macro or bound to a key opens the week agenda, the key after the prefix answers the menu. Got: it is run as a Normal-mode command, and the menu stays open. The same race holds for every `input.kind == 'key'` prompt (fast TODO selection `cit` + key, `<Leader>oc` + template key, the export menu).

## neovim-10: Neovim's hover on a link in an Org file stays Markdown: its Org block is not highlighted, and a user's `FileType markdown` settings apply
- State: fixed (#368)
- Where: `lua/org-lsp/preview.lua` `to_org`/`watch`: only a float whose first line is ```` ```org ```` (the agenda's hover) is made an Org buffer. A link's hover is `` `LINK: *Other` `` + blank + ```` ```org … ``` ```` (`.claude/e2e/repro/neovim/hover.lua` prints it).
- Repro (TUI): `:autocmd FileType markdown setlocal spell number cursorline`, then `K` on `[[*Other][other]]`: the float shows line numbers (`1 LINK: *Other`, `4 * TODO Other`), wraps the planning line at the narrow width, and the target's headline is not in Org's colors (no Org tree-sitter parser). In the agenda the same autocmd is undone (`org false false false` for filetype/spell/number/cursorline): that part works.
- Expected: the link's preview looks as the agenda's does (Org highlighting, Markdown settings undone), or the docs say only agenda hovers are converted. Got: Markdown float with the user's Markdown options.

## vim-1: `<Leader>o,` (org.priority) asks with a line to type and Enter, where Emacs reads one key
- State: fixed (#368)
- Where: Vim, all four clients (Neovim the same: `lua/org-lsp/init.lua` sends any input that is not kind `key` to `vim.ui.input`). The server asks with an input of kind `priority` (`server.rs` `ask_priority`), which `org_lsp#prompt` (`editors/vim/autoload/org_lsp.vim:605`) does not know, so it falls to `inputdialog()`.
- Repro: Vim + yegappan/lsp, `* TODO Write report`, `<Leader>o,` then `A`: nothing happens, the command line shows `Priority A-C, SPC to remove: A` and waits for Enter; `SPC` types a space. (`.claude/e2e/repro/vim-helix/vh.sh`.)
- Expected: as `org-priority` (`read-char-exclusive`): one key sets the priority, SPC removes it, at once. Got: a text prompt that needs Enter (and SPC then Enter to remove). docs/protocol.org names the kind `priority` without saying how a client should read it.

## lsp-9: when the server exits while its Emacs is busy, the Emacs is left running (orphaned, at full CPU, memory growing)
- State: fixed (#366)
- Where: `server/org-lsp/src/emacs.rs` (the bridge process); on `shutdown`+`exit` and when stdin closes (editor crash), the server exits at once without killing Emacs. An idle Emacs goes away (its stdin closes); a busy one keeps working until its job ends, then stays reparented to init.
- Repro: `.claude/e2e/repro/lsp/orphan.py`: `org.table.create` with input `3000x3000` (a long Emacs job), wait 2 s, then shutdown+exit (or close stdin); the bridge Emacs is still there 3 s later, ppid 1. With `999999x999999` it grew past 600 MB in seconds (it had to be killed by hand).
- Expected: the server stops its Emacs (kill the child) when it exits, as Emacs's own `C-g`/quitting would end the work. Got: an orphaned `emacs --batch -Q --eval (progn ;;; bridge.el …)` process.

## lsp-10: a synchronous Emacs call (org.edit.special and its kin) blocks every other answer of the server while Emacs is busy, up to its 60 s timeout
- State: fixed (#366, #369)
- Where: `server/org-lsp/src/server.rs` `edit_special` (and the other `self.emacs.call(…, Duration::from_secs(60))` on the main thread); protocol.org says Emacs runs "from a thread of its own: an export never holds up the server's other answers".
- Repro: `.claude/e2e/repro/lsp/busy2.py`: start a long Emacs job (`org.table.create` `2000x2000`), then send `org.edit.special` on a source block and right after it a `textDocument/hover`: the hover is answered only when the edit-special answers (27 s here; 60 s and "Emacs did not answer in time" in `busy.py`). `org.export.dispatch` sent while Emacs is busy did not answer within 90 s.
- Expected: requests that do not need Emacs (hover, tokens, completion, typing-time commands) answered at once while Emacs works; the Emacs-backed ones answered when Emacs is free, or refused with a message, without holding up the rest. Also no way to stop a runaway Emacs job: `$/cancelRequest` does nothing (see notes).

## vim-2: toggling a checkbox turns the numbered items that follow in the same list into `-` items (Emacs leaves them)
- State: fixed (#368)
- Where: server (`org.checkbox.toggle`), seen in Vim with yegappan/lsp (`:Org checkbox toggle`).
- Repro: a file `* Lists`, `- [ ] one`, `- [ ] two`, `- [X] three`, `1. a`, `3. b`, `2. c` (one list: the items share their indentation). Cursor on `- [ ] one`, `:Org checkbox toggle`.
- Expected (Emacs 9.8.10, `emacs --batch -Q … (org-toggle-checkbox)` on that line): only `- [X] one` changes; `1. a`, `3. b`, `2. c` stay as they are. Got: `- [X] one` and the last three become `- a`, `- b`, `- c`.

## vim-3: `<C-Space>` (org.checkbox.toggle, C-c C-c) does nothing in terminal Vim: the terminal sends NUL (`<C-@>`), which nothing maps
- State: fixed (#368)
- Where: Vim in a terminal (tmux, TERM=tmux-256color; also any terminal without modifyOtherKeys/kitty keys), all clients. `commands.rs:798` gives the `vim` preset `<C-Space>` only; the plugin maps `<C-Space>` (`:verbose nmap <C-Space>` → org_lsp.vim:991), `:nmap <C-@>` says "No mapping found".
- Repro: `vh.sh start …` (Vim in tmux), on `- [ ] one`, press Ctrl-Space (`tmux send-keys C-Space`): nothing changes; `:Org checkbox toggle` works.
- Expected: Ctrl-Space toggles the box (Neovim's TUI reads NUL as `<C-Space>`; Vim does not). Map `<C-@>`/`<Nul>` too in Vim when `<C-Space>` is a key, or say in docs/keys/vim.org that terminal Vim needs it mapped. Every TESTING.org check that uses `<C-Space>` (checkbox, renumber, property drawer C-c C-c, TBLFM, code block) fails as written in terminal Vim.

## vim-2: (correction) not a bug: the checkbox toggle is `org-ctrl-c-ctrl-c`, which turns those items into `-` items too
- State: not a bug (vim-2 above compared with `org-toggle-checkbox`; `org.checkbox.toggle` is C-c C-c, and `(org-ctrl-c-ctrl-c)` on `- [ ] one` in Emacs 9.8.10 gives `- [X] one … - a - b - c`, as org-lsp does: `.claude/e2e/repro/vim-helix/em.sh`)
- Where: —
- Repro: as vim-2.
- Expected: as got.

## neovim-11: prompts that Emacs words (column view `e`) show with a doubled colon: `EFFORT value [1:00]: : `
- State: fixed (#368)
- Where: `lua/org-lsp/init.lua` `M.run` appends `': '` to every `input.prompt` (`vim.ui.input({ prompt = input.prompt .. ': ' …})`, also for `vim.ui.select`); the prompts the server takes from Emacs (column view's `org-columns-edit-value`) already end in `": "`.
- Repro: `.claude/e2e/repro/neovim/columns.lua` (headless, Emacs): `<Leader>oxc` in a file with `#+COLUMNS: %25ITEM %TODO %Effort{:}`, `ll`, `e`: `vim.ui.input` is asked `EFFORT value [1:00]: : `; in the agenda's column view the same, and on the PRI column `Priority A-C, SPC to remove: : ` (TUI). The edit itself works and writes `:EFFORT:   2:00`, exactly as Emacs 9.8.10 does (checked in batch).
- Expected: one colon, as Emacs shows `EFFORT value [1:00]: `. Got: `: : `. (Either the server strips the trailing `": "` of Emacs's prompts, or the plugin adds `': '` only when the prompt does not end with one.)

## vim-4: Vim shows a command's error as `org-lsp: <Emacs's text>` in WarningMsg, not Emacs's text alone (as neovim-4)
- State: fixed (#368)
- Where: `editors/vim/autoload/org_lsp.vim:123` `s:warn` (`echomsg 'org-lsp: ' . a:message`), used by `s:executed` for every refused command; all four clients.
- Repro: agenda (`<Leader>oa a`), cursor on a date line (`Saturday 24 October 2026`), `+`: `org-lsp: No entry on this line`. Same for any refusal (`g{` on a top heading etc.).
- Expected: Emacs's message as it is (CLAUDE.md "Say what Emacs says"); for an agenda command on a line without an entry Emacs's `org-agenda-error` says `Command not allowed in this line` (org-agenda.el:9706), not "No entry on this line" (server.rs:7513 and 11 more places).

## vim-5: a count does nothing in the date prompt's calendar in Vim either (`3l` moves one day), as neovim-7
- State: fixed (#368)
- Where: Vim, yegappan/lsp (the calendar keys are the plugin's `calendar` scope, run without the count).
- Repro: agenda, a line's `C-c C-s`, in the calendar `.` then `3l`: the selection is `=> <2026-10-22 Thu>` (one day on), the cursor on 22.
- Expected: as Emacs's calendar (`calendar-forward-day` takes the prefix argument): three days on, 24.

## vim-6: every typed answer with completions fails in Vim: `input()` is called with a dictionary (Neovim's form), E731
- State: fixed (#368)
- Where: `editors/vim/autoload/org_lsp.vim:626` `org_lsp#prompt`: `input({'prompt': …, 'default': …, 'completion': …, 'cancelreturn': …})`. Vim's `input()` takes `input({prompt} [, {text} [, {completion}]])` only; a Dictionary is E731. All four clients. The Vim tests replace `org_lsp#prompt` (test.vim:36), so they never reach it.
- Repro: Vim 9.1 + any client: `<Leader>oxp` on a headline (org.property.set); or `/` in the agenda (filter); or `<Leader>ot` in a file without `#+TAGS` keys; `<Leader>oli` (link insert), `<Leader>oiD` (drawer), capture `%^{Name|a|b}` / `%^g`, `:Org bibtex create`, `<Leader>oix`. `:echo input({'prompt': 'x'})` alone shows the error.
- Expected: the prompt, completed from the list. Got: `Error detected while processing function …<SNR>26_ask[1]..org_lsp#prompt: line 21: E731: Using a Dictionary as a String`, and nothing asked. Use `input(prompt, default, 'customlist,org_lsp#complete_input')` (and a cancel check, e.g. an `inputsave()`/`<Esc>` mapping or `inputdialog` with the cancel value).

## neovim-12: column view: after `<M-Left>`/`<M-Right>` the picked column stays put instead of following the moved column
- State: fixed (#368)
- Where: `lua/org-lsp/columns.lua` (the picked column of the buffer) / the server's `org-lsp/columns` answer to `org.columns.move.left|right`.
- Repro: `.claude/e2e/repro/neovim/colmove.lua`: `#+COLUMNS: %25ITEM %TODO %Effort %Size`, `<Leader>oxc`, `lll` (Size, column 3), `<M-Left>`: `#+COLUMNS:` becomes `… %SIZE(Size) %EFFORT(Effort)` but the picked column is still 3 (now Effort); a second `<M-Left>`… moves Effort, and `<M-Right>` says `Cannot shift this column further to the right`.
- Expected (Emacs `org-columns-move-left`: `(backward-char 1) (org-columns-move-right) (backward-char 1)`, `org-columns-move-right` ends with `(forward-char 1)`): the cursor stays on the column it moved, so `<M-Left><M-Right>` is a no-op. Got: it stays on the index.

## neovim-13: column view `<M-Up>`/`<M-Down>` fail with Emacs 29's own Org (9.6): `Wrong type argument: commandp, org-columns-move-row-down`
- State: fixed (#368)
- Where: server `server/org-lsp/src/server.rs:12433` runs `org-columns-move-row-up/down`, which Org added after 9.6 (Emacs 29.3 ships 9.6.15; /root/org-mode 9.8.10 has them). docs/configuration.org says Emacs "with Org 9.6 or newer".
- Repro: `.claude/e2e/repro/neovim/colmove.lua` (server's Emacs is the system one, Org 9.6.15; `ORG_LISP_DIR` only reaches the tests, not the server): `<Leader>oxc`, `<M-Down>` → notification `Wrong type argument: commandp, org-columns-move-row-down`, nothing moves.
- Expected: the entry moves (fall back to `org-move-subtree-down` + redo, as `org-columns--move-row` does), or the docs name the Org version column view's row moves need. Got: a Lisp error.

## lsp-11: formatting a CRLF file gives `\r\r\n` line endings: each edit's range takes the `\r` (one character past the line's end) and its text ends with `\r`
- State: fixed (#367)
- Where: `textDocument/formatting` and `textDocument/rangeFormatting` on a file whose lines end with `\r\n` (protocol.org: "A file whose every line ends with \r\n keeps its endings in the edits of formatting"). The table commands' `{ "edits" }` are right (`org.table.next.field` spans `\r\n` whole).
- Repro: `.claude/e2e/repro/lsp/crlf.py`: `* TODO A :x:\r\n|a|bb|\r\n…`; formatting answers e.g. line 1 (6 characters) `range 1:0-1:7, newText "| a | bb |\r"`.
- Expected (LSP: `\r\n` ends a line, so line 1 has 6 characters and a character past the end means the end of the line): `range 1:0-1:6, newText "| a | bb |"`. Got: the extra `\r` in the text and the range end at 7, which a client clamps to 6: the result is `| a | bb |\r\r\n` on every changed line (the harness applies edits as the spec says; VS Code and Neovim clamp the same way).

## lsp-12: opening and editing large files is slow and grows faster than the file (debug build): 6000 lines with planning and links take 5 s to open and ~3 s per keystroke; 60 000 lines answered nothing in 5 minutes
- State: fixed (#367) (measured on `target/debug/org-lsp`; a release build is faster, the growth is the point)
- Where: didOpen / didChange of a large Org file (re-parse, diagnostics, …), then any request.
- Repro: `.claude/e2e/repro/lsp/scale.py full 250 500 1000 2000` (entries of `* TODO Heading N :tN:` / `SCHEDULED:` / a link): open + folding 0.36 s, 0.72 s, 1.81 s, 5.16 s (about n^1.5); one typed character then a hover: 0.23 s, 0.50 s, 1.15 s, 2.81 s. Plain text grows linearly (12 000 lines: 1.2 s per keystroke). `robust.py` "huge file" (20 000 entries, 60 000 lines): tokens, folding, hover, a command all time out at 60 s, then `shutdown` times out and the server keeps a CPU at 98% after its stdin closed (killed by hand after 5 min).
- Expected: per-keystroke work bounded (or at least linear and cheap), the server still answering `shutdown` and exiting when stdin closes. Got: the above.

## lsp-13: one malformed message (bad JSON, or a JSON array) stops the server
- State: fixed (#367)
- Where: the server's message loop (`lsp-server`): `org-lsp: malformed LSP payload Error("key must be a string", …)` on stderr, then exit.
- Repro: `.claude/e2e/repro/lsp/robust.py`, section "malformed JSON": send `Content-Length: 10\r\n\r\n{not json}`.
- Expected (JSON-RPC 2.0): an error response with code -32700 (Parse error), or the message dropped with a log line, and the server going on. Got: the server exits, losing the editor's whole session (the editor has to restart it).

## lsp-14: an edit the client refuses (`workspace/applyEdit` answered `applied: false`) is dropped silently
- State: not a bug (the state asked for a note first, so no edit was sent; a refused edit already says so), notes-fix-lsp.md (minor)
- Where: every command that edits through `workspace/applyEdit` (here `org.todo.next` in a file).
- Repro: `.claude/e2e/repro/lsp/robust.py`, section "edits refused by the client" (the harness answers `{"applied": false, "failureReason": …}`).
- Expected: the user told that the change was not made (a `window/showMessage` with the failure reason), as Emacs would signal an error when a buffer is read-only. Got: the command answers `null`, no message.

## vim-7: with yegappan/lsp, a capture (any edit) into an empty target file is lost: the file is written empty and the capture buffer closed
- State: fixed (#368)
- Where: Vim + yegappan/lsp: its `lsp/textedit.vim` `ApplyTextEdits` leaves an empty buffer `['']` for an insert at 0:0 (the server's edit is right: `{"range": 0:0-0:0, "newText": "* TODO Captured\n"}`, `.claude/e2e/repro/vim-helix/capture_empty.py`). The plugin then saves the (modified, still empty) buffer.
- Repro: `org_capture_templates` target `['file', '~/notes/inbox.org']`, `inbox.org` exists with 0 bytes; `<Leader>oc t`, type `Captured`, `C-c`: no message, inbox.org stays empty on disk and in its buffer; the capture is gone. A missing or non-empty inbox.org works. Client alone: `:so .claude/e2e/repro/vim-helix/te_empty.vim` prints `TE [''] mod=true` (empty file) and `TE2 ['* TODO X', 'x']` (one line).
- Expected: `* TODO Captured` in inbox.org. A third-party bug, but it loses notes silently; the plugin could apply edits to an empty buffer itself (as it does for ALE) or report it upstream. Any other command whose edit goes to an empty file (refile/archive to a new empty file, roam/journal files created empty) is hit the same way.

## lsp-15: export through Emacs aborts on links the server itself resolves: `id:` links into org-roam files and `[[roam:…]]` links
- State: fixed (#367)
- Where: `org.export.dispatch` (Emacs, `bridge.el`) with `org_roam_enabled`; Emacs runs without org-roam and with an empty `org-id-locations`.
- Repro: `.claude/e2e/repro/lsp/emacs_cmds.py` (sample workspace: `gtd.org` links `[[id:0a1b2c3d-…-000000000002][Alpha node]]`, a node in `roam/alpha.org`, and `[[roam:Beta]]`): export `h h` (and `t U`, `h H`, `l l`) says `As HTML file: Org export aborted.  Unable to resolve link: "0a1b2c3d-0000-4000-8000-000000000002"`; with that link removed (`NO_ROAM_ID=1`), `… Unable to resolve link: "roam:Beta"`. The server follows both links (`org.open`, `textDocument/definition` for the id).
- Expected: what Emacs with org-roam does: the id is in `org-id-locations` (org-roam keeps it up to date), so the export links to it; `roam:` is a link type of org-roam, exported as a link, not a fuzzy search that fails. The server could give Emacs the ID locations it knows (and register `roam` as a link type) before exporting. Got: no export at all for any file that links to an org-roam node.

## lsp-16: `elisp:`, `help:`, `eshell:`, `eww:`, `irc:`, `gnus:`, `docview:`, `bbdb:` links are completed after `[[` but refused when followed ("Links of type "elisp" are not supported"); parity.org says links are 22 of 22
- State: fixed (#367)
- Where: `server/org-core/src/open.rs:523`; `textDocument/completion` after `[[` offers those types (`completion.py`).
- Repro: `org.open` on `[[elisp:(+ 1 2)]]` in the sample `gtd.org` (`emacs_cmds.py`): error -32803 `Links of type "elisp" are not supported`.
- Expected (Emacs): `elisp:` asks "Execute (+ 1 2) as Elisp? " (`org-link-elisp-confirm-function`) and shows `=> 3`; `help:` shows the documentation; the server has Emacs for that (as for `shell:` links, which it does run after asking). Or, if left out on purpose, the types not offered in completion and the gap written in parity.org's Links section. Got: offered, then refused, and parity claims full coverage.

## vim-8: with vim-lsp, an Org buffer read again from disk (`:e!`, `:checktime`/'autoread') is not told to the server: the agenda keeps the old text until the next typed change
- State: fixed (#368)
- Where: Vim + vim-lsp (prabirshrestha/vim-lsp sends no `didChange`/`didOpen` when a loaded buffer is reloaded). yegappan/lsp, coc.nvim and ALE do see it. CLAUDE.md's pitfall names this for the agenda only (`s:give_view`); ordinary Org buffers are not covered.
- Repro: `.claude/e2e/repro/vim-helix/clients.sh vim_lsp` (step 6), or by hand: agenda files `notes/`, `other.org` loaded in a window; append `* TODO Fourth` + `SCHEDULED: <today>` to other.org on disk; in its window `:e!` (the buffer now has it); agenda `r`: Fourth is missing. Type anything in other.org, `r`: it shows.
- Expected: the agenda built from the buffer as Vim has it (Emacs builds it from the buffer too, which `revert-buffer`/auto-revert has read again). Got: stale agenda (also stale edits: a later `t` on such a line edits the old text).

## vim-9: a capture into an empty (0-byte) target ends with an extra blank line in Vim (vim-lsp, coc.nvim, ALE)
- State: partly fixed (#368): an open empty buffer with vim-lsp or coc.nvim still gets their own appliers
- Where: Vim; ALE through the plugin's own `s:apply_text_edit` (`editors/vim/autoload/org_lsp.vim:1996`): an empty buffer's single `''` line is kept after the inserted text, so the file is written `* TODO Cap empty\n\n`; vim-lsp and coc.nvim's appliers do the same. (yegappan/lsp loses the text instead: vim-7.)
- Repro: `.claude/e2e/repro/vim-helix/clients.sh ale` step 2 (`od -c notes/inbox.org`).
- Expected: `* TODO Cap empty\n` (Emacs, and the server's edit: insert at 0:0 of an empty document). Got: `* TODO Cap empty\n\n`.

## vim-10: with coc.nvim the capture buffer opens in Normal mode (the other clients start Insert mode at `%?`), so typing right away runs Normal commands
- State: documented (#368), docs/vim.org
- Where: Vim + coc.nvim: coc handles `window/showDocument` itself, so `org_lsp#show_document` (`org_lsp.vim:1838`, `feedkeys(... 'A' : 'i')` for a capture buffer) never runs. yegappan/lsp, vim-lsp, ALE: `-- INSERT --` at `%?`.
- Repro: `<Leader>oc t` (template `* TODO %?`), wait 4 s: status line `CAPTURE-inbox.org 1:7`, Normal mode; typing `Cap empty` gives `* TODOap empty` (`C` changed to the end of the line). `.claude/e2e/repro/vim-helix/clients.sh coc` step 2.
- Expected: as with the other clients (and docs/keys/vim.org: "typing goes on there, in insert mode"), or written down in docs/vim.org's coc.nvim paragraph. Also each edit coc applies to a file not loaded echoes `[coc.nvim] Use ':wa' to save changes or ':CocCommand workspace.inspectEdit' to inspect.` though the plugin saves it.

## lsp-17: pasting, archiving and archiving to the sibling shift an inline task's stars with the subtree; shifted up they become two ordinary headlines (`TODO An inline task` and `END`)
- State: fixed (#367)
- Where: `org.paste.subtree` / `org.yank` (level adjusted), `org.archive.subtree` (the subtree goes to level 1 of the archive file), `org.archive.sibling` (one level down). `org.shiftmeta.left/right` and `org.refile` are right (same as Emacs).
- Repro: `.claude/e2e/repro/lsp/compare.py` scenarios (run as in notes-lsp.md): sample `gtd.org`, `** CANCELLED Old idea` followed by `*************** TODO An inline task` / `*************** END` (15 stars, `org-inlinetask-min-level`). `org.archive.subtree` on it: the archive file gets `************** TODO An inline task` / `************** END` (14 stars). `org.paste.subtree` of `** Old\n*************** TODO Inline\n*************** END\n` before `* Archive` (pasted at level 1): 14 stars. `org.archive.sibling`: 16 stars.
- Expected (Emacs batch, Org 9.8.10, `org-inlinetask` loaded): the inline task keeps its 15 stars in all three (`org-paste-subtree` and the archive commands shift headlines, not inline tasks). Got: shifted; at 14 stars they are no longer an inline task but a level-14 headline and a headline titled "END" (the agenda, the outline and the TODO list then show them as such).

## vim-11: prompts that already end in `? ` or `: ` get another `: ` in Vim (and Neovim): `[jkKtTgGSscCiq]? :`, `(default 0): :`, `Tags :work::`
- State: fixed (#368)
- Where: `editors/vim/autoload/org_lsp.vim` `org_lsp#read_key` (`echo a:prompt . ': '`) and `org_lsp#prompt` (`inputdialog(a:input.prompt . ': ', …)`); `lua/org-lsp/init.lua:554` and its `vim.ui.input` the same. The server gives some prompts whole, as Emacs words them (`Dangling clock started N mins ago [jkKtTgGSscCiq]? `, `Execute echo hi in shell? `, `No match - create this as a new heading? `, `Overwrite existing file …? `, `Create new node "…"? `, `You stopped another clock N mins ago; start this one from then? `, `Keep how many minutes (default 0): `), others bare (`Select a TODO state`).
- Repro: Vim + yegappan/lsp: a clock left running across a restart, `<Leader>oxz`: `Dangling clock started 0 mins ago [jkKtTgGSscCiq]? :`; `K`: `Keep how many minutes (default 0): :`. `<Leader>ot` on `:work:`: `Tags :work::`.
- Expected: Emacs's prompt as it is (`…[jkKtTgGSscCiq]? `). Append `: ` only when the prompt does not already end in `? `/`: ` (or have the server always send it whole).

## lsp-18: org.ctags.create says nothing when ctags fails (no TAGS file made), where Emacs shows ctags's error
- State: fixed (#367) (minor)
- Where: `server/org-lsp/src/server/manual.rs` `ctags_create` (`shell_output`, its output and exit status dropped).
- Repro: `org_modules = ["org-habit", "org-ctags"]`, `/usr/bin/ctags` being Emacs's own ctags (`ctags (GNU Emacs 29.3)`, no `--langdef`, the one installed here); `org.ctags.create` in the sample `gtd.org`: answer `null`, no message, no `TAGS`. `org.ctags.find` then offers to rebuild, and again finds nothing, silently.
- Expected (Emacs batch, `(org-ctags-create-tags)` from the file): `shell-command` shows ctags's output, `ctags: unrecognized option '--langdef=orgmode'`, so the user knows Exuberant/Universal ctags is needed. Got: silence, as if it worked.

## lsp-19: Emacs's messages that name the command say `nil`: "‘nil’ failed to load required package "gnuplot"" (the bridge leaves `this-command` unset)
- State: fixed (#366) (minor)
- Where: `bridge.el`, the call of an Org command for `org.plot.gnuplot` (and any other command run through Emacs that reads `this-command`/`last-command`).
- Repro: `sweep.py full org.plot.gnuplot` with `SWEEP_QUIET=3` (gnuplot.el not installed): message `‘nil’ failed to load required package "gnuplot"`.
- Expected (say what Emacs says; `org-require-package` formats `this-command`): `‘org-plot/gnuplot’ failed to load required package "gnuplot"`. The bridge should bind `this-command` (and `real-this-command`) to the command it calls, as `call-interactively` from a key does; `org-priority` and others also look at `last-command`/`this-command`.

## vim-12: table structure edits under a `#+TBLFM:` line never say Emacs's "The formulas in #+TBLFM have been updated"
- State: fixed (#368)
- Where: server (`org.meta.left/right`, `org.shiftmeta.*`, `org.meta.up/down`, `org.table.insert.row.below` in a table), seen in Vim + yegappan/lsp: `:messages` stays empty. No such text anywhere in `server/`.
- Repro: `| a | bb | c |`, `|-`, `| 1 | 2 | 3 |`, `| 4 | 5 | 6 |`, `#+TBLFM: $3=$1*$2`; on `1`: `<Leader>oTd`, `<Leader>oJ`, `<Leader>oTO`, `<s` (`.claude/e2e/repro/vim-helix/step.sh`). The text changes as Emacs's (checked with `em.sh`: same tables, `$2=$INVALID*$1` after `<s`).
- Expected: Emacs's `org-table-fix-formulas` (org-table.el:2315) messages `The formulas in #+TBLFM have been updated` each time such an edit runs with a `#+TBLFM:` line after the table (even when no reference moved). Got: no message.

## neovim-14: a long link wraps as if its concealed target took room: `** [[file:/long/path::6][Note B]] (Top)` shows as `**` / `Note B (Top)` on two screen lines
- State: mitigated and documented (#368): the roam window does not wrap
- Where: Neovim with 'wrap' (the default) and the plugin's conceal (`conceal()` in `lua/org-lsp/init.lua`); most visible in the org-roam buffer, whose backlink headings are `[[file:<absolute path>::N][Title]]`.
- Repro (tmux TUI, 60 columns): a file with `** [[file:/tmp/claude-0/very/long/path/that/goes/on/and/on/and/on/b.org::6][Note B]] (Top)`; off the cursor's line it shows `**` on one screen line and `Note B (Top)` on the next. `\nl` in a note with backlinks, in the default vertical split of an 120-column terminal: `**` / `Head (Head)`, `**` / `N` / `ote B (Top)`.
- Expected (Emacs, `org-link-descriptive`): `** Note B (Top)` on one line. Got: the line broken where the hidden text would have ended. Possible mitigations: 'nowrap' in the org-roam buffer's window, or links relative to the roam directory there.

## vim-13: after a command moves the cursor (`]l`, `[l`, `}` …), the status line / ruler keeps the old line:col until the next key
- State: fixed (#368)
- Where: Vim (seen with yegappan/lsp): the cursor is moved from the server's reply callback (`cursor()`), with no `redrawstatus`/`redraw`; the terminal cursor moves, the status line does not.
- Repro: `set laststatus=2 statusline=%f\ %l:%c` (or 'ruler'), links.org with a link on line 3, cursor 1:1, `]l`: the cursor shows on line 3, the status line says `1:1` (3 s later still), the next key redraws it (`.claude/e2e/repro/vim-helix/vh.sh`, checked with `tmux display -p '#{cursor_y}'` and `:VHCur`).
- Expected: the status line follows the cursor, as after Vim's own motions. (The clock in the statusline, `org_lsp#clock_status()`, may be stale the same way after clock in/out from a key; it was right in the cases tried.)

## neovim-15: following an item of the org-roam buffer opens the file in the org-roam buffer's own window, which goes
- State: fixed (#368)
- Where: `lua/org-lsp/view.lua` `window/showDocument` (the plain-file branch reuses the current window) when `<Leader>oo` is run in the org-roam buffer.
- Repro (tmux TUI, `.claude/e2e/repro/neovim/tui.sh`): note A with backlinks from B, `\nl`, `<C-w>l`, cursor on `** [[file:…/b.org::6][Note B]] (Top)`, `\oo`: b.org line 6 shows in window 2, where the org-roam buffer was; window 1 still shows A; the org-roam buffer is no longer visible (`\nl` needed again).
- Expected (org-roam: the buffer is a side window, and `org-roam-preview-visit`'s `pop-to-buffer-same-window` cannot use it, so the file shows in the other window): the file in the window of the note, the org-roam buffer still beside it. Got: the org-roam buffer replaced.

## vim-14: a closed fold's line is filled with `-` to the edge: the plugin's `fold: ` never applies, since Vim's default 'fillchars' already has `fold:-`
- State: fixed (#368)
- Where: `editors/vim/autoload/org_lsp.vim:2419` `org_lsp#refold`: `if &l:fillchars !~# 'fold:' && &fillchars !~# 'fold:'` — Vim's default is `vert:|,fold:-,eob:~,lastline:@`, so the condition is false unless the user removed `fold:` (Neovim's default has no `fold:`, so lua/org-lsp/init.lua:1051 works there).
- Repro: any Org file with a closed fold in Vim (vh.sh, `#+STARTUP: content`): `* A...-------------------------------------------…` across the window.
- Expected (comment there: "A closed fold shows its first line and `org-ellipsis`, nothing after"; TESTING.org "A folded headline shows as it does open, followed by ..."): `* A...` and blanks. Compare with Vim's default value (or `&fillchars ==# 'vert:|,fold:-,eob:~,lastline:@'`) rather than the presence of `fold:`.

## neovim-16: TAB on a new empty heading changes its level once; a second TAB folds instead of cycling the level again
- State: fixed (#368)
- Where: `<Tab>` (`cycle`, `lua/org-lsp/folds.lua`) → the server's level cycling (`org-cycle-level`); after the edit the cursor is put at column 0 of the headline.
- Repro: `.claude/e2e/repro/neovim/fold2.lua` (and TUI): `* A` / `** `, cursor at the end of line 2 (Normal mode, on the last space), `<Tab>` → `* ` with the cursor at `{2, 0}`; `<Tab>` again leaves `* ` (the cursor is no longer at the end of an empty headline, so it is plain cycling).
- Expected (TESTING.org "Under `* A`, TAB at the end of a new `** ` makes it `* `, again `** `"; Emacs `org-cycle-level` keeps point at the end, so `* ` → `** ` → `* `…): the cursor stays at the end of the headline (its last column in Normal mode) and TAB keeps cycling. Got: stuck at `* `.

## neovim-17: `:Org` completion offers dotted names and ignores the words typed; an unknown command errors about `….dispatch`
- State: fixed (#368)
- Where: `lua/org-lsp/init.lua` `complete_user_command` (completes only the argument under the cursor against `command:gsub('^org%.', '')`) and `user_command` (falls back to `id .. '.dispatch'`).
- Repro: `.claude/e2e/repro/neovim/cmdcomp.lua`: `getcompletion('Org ag', 'cmdline')` → `agenda.dispatch`, `agenda.list`… (docs and TESTING.org write `:Org agenda list`); `getcompletion('Org agenda l', 'cmdline')` → `levels.odd`, `link.next`, `latex.preview`… (the first word is ignored); `getcompletion('Org schedule ', 'cmdline')` → every command. `:Org nosuch command` → ``Language server `org_lsp` does not support command `org.nosuch.command.dispatch`. This command may require a client extension.``
- Expected: completion word by word as the command is typed (`:Org agenda <Tab>` → `list`, `todos`, `dispatch`…; after `:Org schedule ` → `remove`, `delay`), and an unknown command named as typed. Got: the above. (`:Org agenda.list` does work, since the dots survive the join.)

## neovim-18: `win_split_mode = 'tabnew'` with `win_split_size` makes the command line half the screen ('cmdheight' 11)
- State: fixed (#368)
- Where: `lua/org-lsp/view.lua` `open_window`: the size is set on the window the Ex command opened (`nvim_win_set_height`) even when it is the only window of a new tab, where Neovim gives the lines to the command line.
- Repro: `.claude/e2e/repro/neovim/variants.lua` with `SPLIT=tabnew SIZE=0.5`: `setup({ win_split_mode = 'tabnew', win_split_size = 0.5 })`, `<Leader>oa a` → agenda window height 11, `vim.o.cmdheight` 11 (1 without `win_split_size`). docs/configuration.org gives `'tabnew'` as an example of `win_split_mode`.
- Expected: the size only applies to a split (a window that has a neighbour), 'cmdheight' untouched. Got: 'cmdheight' 11 until the user resets it.

## helix-1: the code action /Save all Org files/ says "Saving all Org buffers... done" in Helix but saves nothing
- State: fixed (#368)
- Where: `server.rs:7195` `AGENDA_SAVE` only returns `{ "save": [uris], "message": … }` for the editor to act on; Helix does nothing with a command's result, so the buffers the agenda's edits changed stay unsaved (a.org, which Helix has open, and b.org, which it opened for the edit). TESTING.org (Agenda) lists it as working: "Rebuilding the agenda, saving the changed files, quitting: … /Save all Org files/".
- Repro: `.claude/e2e/repro/vim-helix/hx_save.py`: agenda, /Change the TODO state/ on two lines (DONE in Helix's buffers), /Save all Org files/: message `Saving all Org buffers... done`; both files on disk still `* TODO …`.
- Expected: the files written (the server has their text, or offer `:write-all` in the docs and leave the action out for clients that ignore results), or at least not Emacs's "done" message when nothing was saved.

## helix-2: after an edit on an agenda line, a filter no longer changes the agenda in Helix (message only)
- State: fixed (#368)
- Where: server with a client without org-lsp's editor code (Helix: `show_document`, `message_requests = false`); the filter commands (`org.agenda.filter.category`, seen also with the other filters in a longer run).
- Repro: `.claude/e2e/repro/vim-helix/hx_filter2.py`: week agenda of a.org (Write report) and b.org (Bee), /Change the TODO state/ on the Bee line (the server edits the agenda's line, Helix sends didChange, version 1), then /Agenda: filter by the line's category/: the server answers `Agenda filters: [+b]` and sends no `workspace/applyEdit` for Agenda.org (Helix log, kept with ORG_LSP_HX_KEEP=1); the `a:` line stays. Without the TODO change first (`hx_filter.py`) the same filter rewrites the agenda and hides `a:`. In Vim (which reads the agenda file again) the filter works after edits.
- Expected: the filtered agenda, as Emacs's `<` shows it.

## helix-3: TESTING.org/keys say switching the keyword set and removing dates are not code actions in Helix, but the agenda offers them
- State: fixed (#368)
- Where: `editors/helix/TESTING.org` (TODO keywords: "Switching to another keyword set is left out: it is not among Helix's code actions"; Dates: "Not in Helix: Removing the scheduled date and the deadline"), generated from `scripts/features.mjs`. On an agenda entry's line Helix's code actions include /Switch to the next TODO keyword set/, /Switch to the previous TODO keyword set/, /Remove the scheduled date of the entry/, /Remove the deadline of the entry/, /Delay the scheduled date until a day/, /Warn of the deadline from a day/, /Change the TODO state as if it were yesterday/ (`hx_agenda.py`, "actions on an entry line"); in an Org file they are not.
- Repro: `.claude/e2e/repro/vim-helix/hx_agenda.py`.
- Expected: the lists say "in an Org file" (or the agenda's are checks of their own). Also docs/keys/helix.org's tables give titles that are not the menu's: `org.todo.next` "Cycle the TODO state forward" (menu: "Cycle TODO state forward (→ WAIT)"), `org.meta.right` "…indent the item; move the table column right" (menu: "…indent the item, the drawer or the block; …"), agenda "/Archive the subtree/" where TESTING.org says "/Archive with the default command/".

## helix-4: Helix's queries color emphasis, links, entities and timestamps inside comments, example blocks and fixed-width lines
- State: fixed (#368)
- Where: `editors/helix/runtime/queries/org/highlights.scm`: the word-by-word patterns for markup (`*bold*`, `[[link]]`, `\alpha`, `<timestamp>`) are not kept out of `(comment)`, an example block's contents or a `: ` fixed-width line, and come after `(comment) @comment` / `@markup.raw.block`, so they win.
- Repro: `.claude/e2e/repro/vim-helix/hx_hl.py FILE` (prints each run of text with its scope) on `# \alpha in a comment and *bold* and [[link]]` → `\alpha<constant.character.escape.entity>`, `*bold*<markup.bold>`, `[[link]]<markup.link.url>`; `#+begin_example` / `*not bold* [[nolink]] <2026-05-18 Mon>` / `#+end_example` → bold, link and timestamp scopes, no `markup.raw.block`; `: fixed *not bold*` → bold. tests/visual.org line 114 too (`*not bold*.` in its example block).
- Expected: as Emacs: a comment line all `org-comment`, an example block and a fixed-width line verbatim (`org-block`/`org-code`), no markup inside.

## helix-5: in Helix, a `#` comment line right after a `#+KEYWORD:` line has no color; `<<two words>>`, `\there4`, `\frac12` are not colored
- State: fixed (#368)
- Where: `editors/helix/runtime/queries/org/highlights.scm` (and the grammar's parse: the comment after a directive is not a `(comment)` node; targets and entities are matched word by word / by a pattern without digits).
- Repro: `hx_hl.py` on `#+AUTHOR: A keyword line.` then `# A comment after a keyword.` → the second line plain (the same comment after a text line or a blank line is `<comment>`); tests/visual.org line 87 (`# A comment line.` after `#+AUTHOR:`) the same. `<<one>>` is `label.target` but `<<two words>>` (tests/visual.org line 94, `<<the target>>`, which TESTING.org says is colored) is plain; `\alpha` is an entity, `\there4` (visual.org line 57) and `\frac12` are plain.
- Expected: all colored, as in the other editors (semantic tokens) and Emacs.

## helix-6: in Helix, /Sort the entries…/, /Insert or update a clock table/ and /Update the dynamic block/ change nothing (sort even says "Sorting entries...done")
- State: fixed (#368)
- Where: server, code actions for a client without org-lsp's editor code. These commands return their edit in the result (`{ "edits": [...], "cursor": … }`, `Server::carried`, server.rs:14581), which only the Neovim/Vim plugins apply; the server applies it itself only with `"apply": true` in the argument, which the code action list (server.rs:~4483) gives META_RETURN/INSERT_*/CUT/COPY/PASTE but not SORT, CLOCK_REPORT, DBLOCK_UPDATE (nor the answer of a question they asked). Helix log: `org.clock.report` → `{"result":{"edits":[{… "#+BEGIN: clocktable :scope subtree :maxlevel 2 …"}],"cursor":…}}`, no applyEdit.
- Repro: `.claude/e2e/repro/vim-helix/hx_dblock.py` (`* Top`, `** b`, `** a`, an empty `#+BEGIN: clocktable` block): update → block unchanged; sort `a` → `Sorting entries...done`, still b, a; insert clock table → nothing. TESTING.org (Helix) lists all three as working.
- Expected: the edits applied, as in Vim (where the same commands work: clock table inserted, `<Leader>o^` sorts).

## helix-7: evaluating a code block in Helix is always aborted: Emacs's yes/no comes as `window/showMessageRequest`, which Helix refuses, though `message_requests = false`
- State: fixed (#368)
- Where: server, the Babel confirmation (`org-confirm-babel-evaluate` through the bridge) when the client has `message_requests` off: it still sends `window/showMessageRequest` ("Evaluate this emacs-lisp code block on your system?", Yes/No) instead of the `PROMPT: pick the answer from the code actions` question; Helix answers `Method not found: window/showMessageRequest` and the server says `Evaluation of this emacs-lisp code block is aborted.`
- Repro: `.claude/e2e/repro/vim-helix/hx_babel.py` (Emacs with ORG_LISP_DIR): a `#+begin_src emacs-lisp` `(+ 1 2)` block, /Evaluate the code block or call at the cursor through Emacs/: no question in the code actions, no `#+RESULTS:`, the abort message. TESTING.org (Helix): "Evaluating an emacs-lisp block asks first (yes or no), then puts #+RESULTS: below it".
- Expected: the yes/no among the code actions (as for the server's own questions), then the results.

## vim-15: following a footnote leaves the cursor just after `[fn:1]`, where following again says "No link found" instead of going back (Vim `<Leader>oo`, Helix `gd`)
- State: not a bug: Emacs 9.8.10 says "No link found" there too; TESTING wording fixed (#368)
- Where: server, `org.open` and `textDocument/definition` at a footnote definition: the place is taken as the definition only on its label, not right after it, which is where the server itself puts the cursor (`[fn:1]|` = Emacs's point after `org-footnote-action`).
- Repro: `* A`, `See [fn:1].`, ``, `[fn:1] The note.`; in Vim `<Leader>oo` on the reference → 4:7; `<Leader>oo` again → `org-lsp: No link found` (from 4:3 it goes back to 2:5). Helix: `gd` twice the same (`hx_flows2.py`, (13, 7) twice).
- Expected (Emacs 9.8.10, `em.sh fn.org 4 7 '(org-footnote-action)'`): back to the reference, 2:5. TESTING.org: "Following [fn:1] goes to its definition, and from the definition back to the reference".

## vim-16: TESTING.org's org-lint check "A heading in a paragraph (`text ** heading`)" cannot pass: neither Emacs nor the server reports that text
- State: fixed (#368)
- Where: `scripts/features.mjs` → every `editors/*/TESTING.org` (Diagnostics): the example `text ** heading`. Emacs's `misplaced-heading` checker (org-lint.el:410) wants a non-blank character right before the stars (`text** heading`); `org-lint` in Emacs 9.8.10 on `text ** heading in a paragraph` reports nothing, and the server neither (`.claude/e2e/repro/vim-helix/diag.py`).
- Repro: diag.py; or open such a file in Vim/Helix: only the fuzzy link, the header argument and the open block are reported.
- Expected: the example `text** heading` (or `Some text*** heading`), which Emacs reports.

## neovim-19: detangling (`<Leader>obd`) says nothing; Emacs says `Detangled 1 code blocks`
- State: fixed (#366, emacs-8)
- Where: server, `org.babel.detangle` through Emacs (`org-babel-detangle`): its `message` is not passed on as the result's `message`.
- Repro: `.claude/e2e/repro/neovim/detangle.lua`: a block with `:tangle out.el :comments link`, `<Leader>obt` (says `Tangled 1 code block from t.org`), change the code in out.el, `<Leader>obd`: the block gets the new code (`  (* 3 3)`, as Emacs writes it) but no notification within 6 s.
- Expected (CLAUDE.md "Say what Emacs says"; Org 9.6/9.8 `ob-tangle.el`: `(message "Detangled %d code blocks" counter)`): `Detangled 1 code blocks`. Got: nothing.

## neovim-20: tests/nvim/e2e.lua times out under load (5 s waits): "the server to attach" (0.12.0), "the lines selected made headings" (stable)
- State: fixed (#368)
- Where: `tests/nvim/e2e.lua` `wait()` (5000 ms) for the first attach and for Emacs-backed steps (`org.toggle.heading` on a selection starts Emacs).
- Repro: the first `ORG_LISP_DIR=/root/org-mode/lisp NVIM=… scripts/test-nvim.sh` of this round, run while other testers built and ran servers (load ~4 on 4 cores): 0.12.0 failed at "timed out waiting for the server to attach", stable at "timed out waiting for the lines selected made headings". Three re-runs each (`.claude/e2e/repro/neovim/suite.sh <v> e2e`) passed (376 checks), as did every other file.
- Expected: the suite passes on a busy machine. Got: an occasional false failure; a longer wait for Emacs's first start (as highlight.lua's 10 s) would avoid it.

## vim-17: Emacs's yes/no questions (Babel's "Evaluate this emacs-lisp code block on your system?", during export too) come as `window/showMessageRequest`, which vim-lsp never answers and ALE ignores: `<Leader>obe` does nothing in Vim with those clients
- State: fixed (#368)
- Where: server: the bridge's confirmation goes out as `window/showMessageRequest` even to org-lsp's own Vim plugin (which asks everything else with `{ "input": … }` and `org_lsp#prompt`). vim-lsp's handler (`autoload/lsp/internal/show_message_request.vim`) shows an inputlist but then sends no response (its log has the request and nothing after; it also checks `l:index <= len(l:index)`, so only choice 1 could ever pass); ALE shows nothing at all. yegappan/lsp (inputlist) and coc.nvim (`[1]Yes, (2)No:`) work; Helix refuses it (helix-7).
- Repro: `.claude/e2e/repro/vim-helix/babel_clients.sh vim_lsp` / `ale` / `coc` (Emacs with ORG_LISP_DIR): `#+begin_src emacs-lisp` `(* 6 7)`, `<Leader>obe`, answer 1: vim-lsp and ALE → no `#+RESULTS:`, no message; coc → `: 42`. vim-lsp log: `g:lsp_log_file`.
- Expected: `#+RESULTS:` `: 42` with every client: ask through the plugin's own prompt (an `input` of kind `key` y/n, as `Execute echo hi in shell?` is asked) for clients with `orgClient`.

## vim-18: the server's `window/showMessage` texts (Emacs's messages) are lost with vim-lsp and ALE, and shown with a client prefix with yegappan/lsp (`Info: Lsp(org-lsp) `) and coc.nvim (`[coc.nvim] `)
- State: fixed (#368)
- Where: Vim; the plugin leaves `window/showMessage` to the client. vim-lsp drops Info messages by default (`g:lsp_show_message_log_level`), ALE does not show them; yegappan/lsp and coc.nvim prefix them. The plugin shows only a command's `result.message` itself (`echomsg`, bare).
- Repro: `.claude/e2e/repro/vim-helix/msg_clients.sh CLIENT`: an entry with `Effort 0:30` and 0:40 clocked, `<Leader>oxi`: lsp → `Info: Lsp(org-lsp) Task ‘Long task’ should be finished by now. (0:30)`; coc → `[coc.nvim] Task ‘…’ …`; vim-lsp and ALE → nothing on screen or in `:messages` (only `Clock starts at …`). Same for every message that comes this way: table sum (`Sum of 2 items: 5 …`), field info, timers (`Timer start time set to …`, `TODO Parent: time out`), `hi` from a shell link, `Exported to …`, countdown done.
- Expected: Emacs's text as it is, with every client (CLAUDE.md "Say what Emacs says"): have the plugin handle `window/showMessage` for Org buffers (as Neovim's plugin can), or announce/route these messages as command results.

## vim-19: `:Org …` takes no range, so TESTING.org's "lines turned into headings: `:Org toggle heading`" on a selection cannot be done in Vim
- State: fixed (#368)
- Where: `editors/vim/plugin/org_lsp.vim:8` `command! -nargs=* … Org call org_lsp#user_command(<f-args>)` (no `-range`); `org_lsp#run` sends a region only for Visual-mode keys.
- Repro: lines `plain line`, `another` under `* C`; `Vj` then `:` gives `:'<,'>Org toggle heading` → `E481: No range allowed`. With `:<C-u>Org toggle heading` the selection is lost: only the cursor's line changes (`** plain line`). (`>>` on a Visual selection works: two headlines demoted.)
- Expected (TESTING.org, Headings: "On a selection of whole lines: … lines turned into headings (needs Emacs): `>>`, `:Org toggle heading`"): both lines turned into headings, as `C-c *` on a region in Emacs. Give `:Org` `-range` and pass the region when a range is given, or a key for org.toggle.heading in Visual mode.

## vim-20: `<S-CR>` (org.return.item) cannot be typed in terminal Vim outside xterm-like terminals (tmux, screen, the Linux console): it arrives as `<CR>`
- State: documented (#368), docs/keys/vim.org
- Where: Vim in a terminal without modifyOtherKeys/kitty keys (Vim turns modifyOtherKeys on only for TERM=xterm*): Shift-Enter sends CR. Same family as vim-3 (`<C-Space>` arrives as NUL). docs/keys/vim.org lists both with no word on terminals.
- Repro: Vim in tmux (TERM=tmux-256color), on `- first item text`, `A` then Shift-Enter: a plain new line (`org.return`), not `- ` (the next item). `:Org return item` works.
- Expected: either a second default key that terminals can send, or docs/keys/vim.org (and docs/vim.org) saying which keys need a terminal that reports modifiers, with the `mappings` to set instead.

## vim-21: with yegappan/lsp and vim-lsp, the clock display (`<Leader>oxd`) and shrunk columns never show: nothing asks for inlay hints again after the command
- State: fixed (#368)
- Where: Vim; the server only sends `workspace/inlayHint/refresh` to clients that announce `refreshSupport` (server.rs:1550, 15495); yegappan/lsp and vim-lsp announce none and re-request hints only after a text change (yegappan: `TextChanged` sets `LspInlayHintsNeedsUpdate`, then `CursorHold`), and any change removes the clock display (as Emacs's overlays go). The plugin does not ask the client to update its hints after these commands.
- Repro: yegappan with `LspOptionsSet({'showInlayHints': v:true})` (or vim-lsp `g:lsp_inlay_hints_enabled = 1`), an entry with a 0:40 CLOCK line, `<Leader>oxd`: `Total file time: 0:40 …`, but no `0:40` after the headline, even after 10 s and cursor moves; `:LspInlayHints disable | LspInlayHints enable` then shows `* Task·····…      0:40` (the server had the hint: `.claude/e2e/repro/vim-helix/inlay.py`). yegappan's debug log: its only `textDocument/inlayHint` was before the command (`[]`).
- Expected (TESTING.org, Setup and Clocking): the times next to the headlines right after the command; the plugin could trigger the client's update (`LspInlayHintsUpdateNow`-like, vim-lsp's refresh) when a command's result or `org-lsp/…` notification says hints changed.

## helix-8: in Helix, /Shrink or expand the table column/ shows nothing until the next edit elsewhere (Helix has no inlay hint refresh)
- State: documented (#368)
- Where: server + Helix 25.07: Helix announces `workspace.inlayHint.refreshSupport: false` and asks for hints only on open, change and scroll; `org.table.toggle.column.width` returns `null` and changes no text, so the `…` hints come only after some unrelated edit. (Same root as vim-21 for Vim's clients.)
- Repro: `.claude/e2e/repro/vim-helix/hx_shrink.py` (`display-inlay-hints = true`): `| <5> |` table, the action → the screen unchanged, no inlayHint request in Helix's log; type `x` on line 1 → `| <5>  … |`, `|  long …text here |`.
- Expected (TESTING.org, Tables): the `…` right after the action. A no-op edit (or a version bump) after the toggle would make Helix ask again; or say so in docs/helix.org.

## vscode-1: clocking in while a clock is left running in the same file: the clock-in edit names a stale version, VS Code refuses it ("The editor did not make the change")
- State: fixed (#369)
- Where: server, `org.clock.in` after resolving a dangling clock (`org-resolve-clocks` answered `K`): it sends two `workspace/applyEdit`s for the same file, both with `textDocument.version` 1 (the resolve edit, then the new `:LOGBOOK:`/`CLOCK:` lines). The second is refused by any client that checks versions; VS Code does (`applied: false`), so the entry is never clocked in and the server shows the warning `The editor did not make the change` (server.rs:3148). TESTING.org: "With a clock left running and none clocked, clocking into another entry asks about it first".
- Repro: VS Code (`.claude/e2e/repro/vscode-zed-sublime/vsc/cases4.cjs`, case `dangling-clock`: `ONLY=dangling node run.mjs cases4.cjs out.json settings4.json` under `xvfb-run -a`); over plain LSP `.claude/e2e/repro/vscode-zed-sublime/lsp/dangling.py` prints both edits with `"version": 1` (and the client's "version mismatch … edit 1, document 2"). File: `* TODO Left running\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 09:00]\n:END:\n* TODO Other\n`, `org.clock.in` on `Other`, answer `K`, then Enter for the minutes.
- Expected (Emacs): the dangling clock closed (`--[… 10:30] =>  1:30`) and `Other` clocked in (`:LOGBOOK:\nCLOCK: [2026-05-18 Mon 10:30]\n:END:` under it), the clock running. Got: the first edit only; `Other` not clocked; warning `The editor did not make the change`.

## fix-1: a server test reads the real ~/.emacs.d/.org-id-locations
- State: fixed (#369)
- Where: `server/org-lsp/tests/agenda_lsp.rs` `completes_id_links_to_entries` (found by the emacs fixer); the test's server reads `org-id-locations-file` from the real home, so a locations file left by any Emacs run (here a tester's, 06:12) changes its completions and fails it.
- Repro: have `/root/.emacs.d/.org-id-locations` list some other file with IDs, run `cargo test -p org-lsp --test agenda_lsp completes_id_links_to_entries`.
- Expected: the tests that start a server give it a HOME (or `org_id_locations_file`) of their own. Got: failure depending on the machine. (The file was moved aside to the scratchpad, `org-id-locations.from-testers`.)

## zed-1: Zed cannot give an empty answer to a prompt, so a prompt's default (Org's "empty for the default") can never be taken
- State: fixed (#369)
- Where: Zed, every prompt answered "with a rename at the cursor" (`message_prompts`): the server's `textDocument/prepareRename` answers `{"range": <empty at the cursor>, "placeholder": ""}`, and Zed sends no `textDocument/rename` for an empty new name (Enter on the empty rename box just closes it), so the server keeps waiting for the answer. Affects `Insert link (default file:…::*Target)` (take the stored link), `Table size Columns x Rows (empty for Org's default)` (TESTING.org (Zed) Tables: "Creating a table asks for its size (~3x2~; empty for Org's default)"), a date prompt's empty answer (today / the default date), `Keep how many minutes (default N)`, capture `%^{Name}` with an empty answer, the link description, …
- Repro: `.claude/e2e/repro/vscode-zed-sublime/zed/explore_zed.py OUT.json empty-answer` (with `ZED`, `ZED_ORG_GRAMMAR`; built on editors/zed/test/run.py): `org.link.store` on `* Target`, then `org.link.insert` with answers `""`, `Desc` → "timed out waiting for the rename to be answered"; the same with answers `*Target`, `Desc2` inserts `[[*Target][Desc2]]`. `org.table.create` with answer `""`: the same time-out. The proxy log shows the prepareRename result with `"placeholder": ""` and no rename after it.
- Expected: some way to take a prompt's default in Zed (e.g. prepareRename's placeholder set to the prompt's default or a marker the server reads as empty, with Zed sending the unchanged name, or a documented answer such as a single space meaning empty), and docs/keys/zed.org saying it. Got: the default cannot be chosen; the user must type it out (and for "empty for Org's default" there is nothing to type).

## sublime-1: after Package Control updates LSP-org, the server downloaded for the old release keeps being used
- State: fixed (#369)
- Where: `editors/sublime/LSP-org/plugin.py`: `downloaded_path` (`Package Storage/LSP-org/org-lsp`) has no release in it, and `needs_update_or_installation` is `server_path(...) is None`, so once any server was downloaded it is never fetched again when `VERSION`/`RELEASE` change. The package's keymap, commands and `semantic_tokens` are generated from the server of its own version (CLAUDE.md, generated files), so an updated package runs against an older server (commands it binds may not exist there). The Zed extension keeps a directory per release (`org-lsp-{version}`) and fetches a new one; the Sublime package does not. The README for Package Control and docs/sublime.org say the server of "the package's own release" is downloaded "once".
- Repro: `.claude/e2e/repro/vscode-zed-sublime/sublime/stale_server.py` (the plugin under test_plugin.py's fake sublime/LSP): download for v0.1.0, then the package becomes 0.2.0 (`VERSION`, `RELEASE`): `needs_update_or_installation()` is False and `server_path` is still the v0.1.0 binary; only v0.1.0's URL was fetched.
- Expected: the server of the package's release, downloaded again when the package's `RELEASE` changes (e.g. a `org-lsp-<release>` directory, or a version file next to the binary checked in `needs_update_or_installation`; for `nightly`, as now, by the command). Got: the old server until the user runs /Org: Download the latest server/ (whose name also says "latest" though it fetches the package's own release).

## sublime-2: TESTING.org (Sublime Text) asks to check the date prompt's calendar marking journal days, but the package shows no date prompt
- State: fixed (#369)
- Where: `scripts/features.mjs:711` (`The date prompt's calendar marks the days with a journal entry`, `not: ['zed']`) → `editors/sublime/TESTING.org:253`. The same section's *Partly* (line 240) says "The calendar and the keys org-journal adds to it are left out: the package shows no date prompt", and Dates (line 146) "Dates are typed, without the calendar". The three calendar checks after it are `only: ['neovim', 'vim', 'vscode']`.
- Repro: read editors/sublime/TESTING.org, A journal (org-journal).
- Expected: the check left out for Sublime Text (`only: ['neovim', 'vim', 'vscode']`, as its neighbours). Got: a check that cannot pass.

## sublime-3: prompts that already end in `: ` get another `:` in Sublime Text's input panel (`Keep how many minutes (default 90): :`)
- State: fixed (#369)
- Where: `editors/sublime/LSP-org/plugin.py` `ask`: `window.show_input_panel(request.get('prompt', '') + ':', …)`. The server gives some prompts whole, as Emacs words them (`Keep how many minutes (default N): `, `EFFORT value [1:00]: ` from column view, …); tags show as `Tags :work::`. Same bug as vim-11 (Vim/Neovim), in the Sublime package.
- Repro: `.claude/e2e/repro/vscode-zed-sublime/sublime/prompt_colon.py` (the plugin under test_plugin.py's fake sublime): captions `'Keep how many minutes (default 90): :'`, `'EFFORT value [1:00]: :'`, `'Tags :work::'`; a bare prompt gets `'Number of clones to produce:'` (right).
- Expected: Emacs's prompt as it is; add `:` only when the prompt does not already end in `: `/`? `.

## sublime-4: the package ignores a prompt's `completions`, so finding a roam node, removing an alias, a BibTeX entry's type … offer nothing to pick, as TESTING.org (Sublime Text) says they do
- State: fixed (#369)
- Where: `editors/sublime/LSP-org/plugin.py` `ask`: only `choices` go to the quick panel; a request with `completions` (Emacs's `completing-read`, what VS Code shows in a QuickPick with what is typed as the first item, `typeOrPick`) falls through to `show_input_panel` with no completion at all. Prompts with `completions` (seen in VS Code, cases3/6.cjs): roam `Node` (find, insert), roam alias `Remove`, `Tags` typed, `Property` and `Globally remove property`, `Insert link` (link types), BibTeX `Type`, `Capture columns …`.
- Repro: read `ask` (plugin.py:988); in VS Code the same commands log `kind: typed` with items (`.claude/e2e/repro/vscode-zed-sublime/vsc/out` of cases3: `{"kind": "typed", "prompt": "Node", "items": ["B note", "A note"]}`). TESTING.org (Sublime Text) has no *Partly* for org-roam or Citations, and its checks say "finding a node offers the titles and aliases of its files", "removing it offers the aliases and takes the one picked out", "A new BibTeX entry asks for its type (completed)"; only Properties says "without completion".
- Expected: the completions offered (a quick panel with the typed text as the first item, as VS Code does, or `show_input_panel` plus a quick panel of the completions), or the checks and the *Partly* notes saying they are not. Got: an empty input panel; the user must know the exact title/alias/type.

## vscode-typed-1: a key typed in a key menu picks the wrong choice (`n` among `[m] Meeting` and `[n] Note` takes the meeting)
- State: fixed (this branch, "VS Code: the scenarios typed as the user types them; a key typed picks its choice")
- Where: `editors/vscode/src/extension.ts` `ask`: key choices went to `showQuickPick`, where Enter takes the first item VS Code's filter keeps: the first whose text has the letter.
- Repro: capture.json typed (`npm test -- scenarios`, step 8 `w` then `n`): "typed "n", Enter would pick "[m] Meeting", not "[n] Note"".
- Expected (Emacs's fast selection): the key's choice. Now `pickKey` makes the choice of the key typed (or its name, `SPC`, `RET`, `TAB`) the one Enter takes; other text narrows the list.

## vscode-typed-2: an input box that starts with text (the time of a timestamp changed) loses it when the user types
- State: fixed (this branch, "VS Code: an input box keeps the text it starts with, typing after it")
- Where: `extension.ts` `ask`: `showInputBox({ value })` without `valueSelection`, so VS Code selects the text and the first key replaces it.
- Repro: `* Meeting <2026-05-18 Mon 10:00>`, Org: Change the timestamp at the cursor, `i`, type ` fri`, Enter: `<2026-05-22 Fri>`, the time gone. Test: "keeps the text a date prompt starts with, typing after it, as Emacs does".
- Expected (Emacs's `read-string` with initial input, point after it): `10:00 fri` → `<2026-05-22 Fri 10:00>`.

## vscode-typed-3: the TODO menu shows no colors
- State: fixed (this branch, "VS Code: the TODO menu shows its colors; typed tags go after a headline's")
- Where: `extension.ts` `choiceIcon`: dots written in the global storage and given as its `vscode-userdata:` URI, which a quick pick does not show; keywords without a face got theme icons, which a quick pick shows in one color.
- Repro: `.claude/e2e/repro/typed-vscode/b2c.cjs`, `icons.cjs` (screenshots). Test: "picks a TODO state by key and asks for its note" (file URIs, `org-todo` / `org-done` colors for light and dark).

## vscode-typed-4: typing tags (C-c C-q, `:` in the agenda) replaces the headline's tags
- State: fixed (this branch, "VS Code: the TODO menu shows its colors; typed tags go after a headline's")
- Where: `extension.ts` `typeOrPick`: `pick.value` given before `show()`, which VS Code selects. Fixed with `valueSelection` at its end (VSCodium 1.135 takes it for a quick pick; setting the value after `show()` instead lost the first keys typed, a race the typed agenda-filters scenario hit).
- Repro: `* B :old:`, C-c C-q, type `new`, Enter: `:new:` (Emacs: `:old:new:`). Test: "keeps the tags a headline has as tags are typed after them, as Emacs does" (typed with xdotool).

## vscode-typed-5: DONE has TODO's color, a done headline any headline's, in VS Code's default themes
- State: fixed ("VS Code: TODO and DONE, done and todo headlines colored apart": scopes the themes color apart come first)
- Where: `todoKeyword` → `keyword.other.todo.org`, `todoKeyword.done` → `keyword.other.done.org`: VS Code uses the first scope of the list a theme rule matches, and every theme's `keyword` rule matches both, so both get the keyword color. The same for `headline.done` (`markup.heading.done.org` matched by `markup.heading`) and the TODO headline look.
- Repro: VSCodium 1.135, default theme (2026 Dark; Dark+ and Dark Modern too), tests/visual.org: TODO, NEXT, DONE and CANCELLED all #FF7B72 (`.claude/e2e/repro/typed-vscode/colors.sh` on the screenshot of b1.cjs `visual`).
- Expected (TESTING.org, visual.org): done keywords in a color of their own. A fix: scopes that only org rules or the generic fallbacks match, e.g. `markup.inserted.done.org` before `markup.inserted`, `markup.deleted.todo.org` before `markup.deleted`.

## vscode-typed-6: S-RET at the start of a table row says Emacs's refusal as an error message
- State: fixed ("What Emacs refuses is said as a warning, as the server's own refusals")
- Where: `org.table.copy.down` through Emacs: `Not in table data field` comes as `window/showMessage` type Error (an error toast in VS Code), where other refusals are error answers (`-32803`) that the extension shows as warnings; off a table the answer is `notApplicable` and the key keeps its meaning (right).
- Repro: `python3 .claude/e2e/repro/typed-vscode/copy_down.py`: at (1, 0) of `| 1 | 2 |\n|   |   |` the result is null and the message `{'type': 1, 'message': 'Not in table data field'}`.

## vscode-typed-7: the agenda's bulk scatter (`B S`) puts its default, 7, in the box as text the answer starts with
- State: fixed ("Agenda: scatter's 7 days are a default, as read-number shows it")
- Where: Emacs asks with `(read-number "Scatter tasks across how many days: " 7)`: the minibuffer is empty, the prompt says `(default 7)`, and RET alone takes 7. The server sends `value: "7"`, which docs/protocol.org defines as the text the prompt starts with (Emacs's initial input).
- Repro: agenda-bulk.json step 27 typed in VS Code (`npm test -- scenarios`, before the adapter selected the box's text first): typing `1` gives `71`. Neovim's `vim.ui.input({ default = "7" })` gives `71` the same way.
- Expected: `value: ""` and the default in the prompt (`Scatter tasks across how many days (default 7)`), the empty answer standing for 7, as `read-number` does.

## Server bugs of the typed round (Neovim, Vim), fixed
- Neovim S1, bulk tag completion without `#+TAGS:`/`org_tag_alist`: fixed ("Agenda: tags complete from #+TAGS and org_tag_alist too").
- Neovim S2, column view PRIORITY read as text: fixed ("Column view: the priority is one key, as org-priority reads it").
- Neovim S3, file-name prompts without file completion: fixed ("File name questions say so, and start at the file's directory", and Vim and Neovim complete them); VS Code, Sublime Text, Zed and Helix ask as text, from the directory.
- Vim 1, `\oi.` twice puts the range inside the timestamp: fixed ("Timestamp again: the range goes after the timestamp").
- Vim 2, `#+STARTUP: shrink` forgotten after a didChange of the same text: fixed ("Shrunk columns stay when the same text is sent again").
- Vim 3, `shell:` output ignoring `message_notifications`: fixed ("Shell links say their output as the server's other messages").
- Vim 4a, property names at `:Co`: fixed ("Property names complete as pcomplete/org-mode/prop offers them").
- Vim 4b, `man:` linted with `ol-man`: fixed ("man: links are links with ol-man on").
- Vim 4c, the field editor keeps a newline from text above its `#` lines: not a bug, `org-table-finish-edit-field` does the same (checked in Emacs: "new line\nmore\nold").
- Vim 4d, the first `\oe` without `[#]`/`[&]`: the server always sends both, and the first menu has them at 80x24 and 120x45 with yegappan/lsp on this build: not reproduced.
