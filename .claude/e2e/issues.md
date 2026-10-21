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
