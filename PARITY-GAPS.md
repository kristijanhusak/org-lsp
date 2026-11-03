# Parity gaps: what Emacs Org does that org-lsp neither does nor documents

A review of `docs/parity.org` against the code (commit `85952cb`). Each item
below is either a claim in the parity table that is not true, or a feature
of Org 9.8.10 that org-lsp does not have and that nothing in the repository
(`docs/`, `scripts/features.mjs`, `scripts/org-options.json`, the manifest)
says is missing. Items the parity table already marks ❌, or that the
Options section lists as "Missing" or "Not done" (org-feed, org-mobile,
speed keys, org-tempo, the agenda's `i` and `v c`, `C-c =`), are left out.

## How this was checked

- No Emacs or Org sources were in the container, so the Org side is Org
  9.8's commands and keymaps as known, not read from `lisp/`. Check each item
  against the Org 9.8.10 sources (`ORG_LISP_DIR`) before porting it.
- Commands and keys come from `org-lsp manifest` and
  `server/org-lsp/src/commands.rs`; documentation was searched with `grep`.
- Items marked **confirmed** were sent to a debug build of the server over
  stdio (`initialize`, `didOpen`, `workspace/executeCommand` with
  `{ textDocument, position, count?, input? }`), without the Emacs bridge.
  The reply is quoted.

## What to do with each item

For every item, either port it (`.claude/skills/port-from-emacs`, and
`.claude/skills/editor-feature` where an editor needs code), or write it
down as not done: a `false` entry in `scripts/features.mjs` (then
`scripts/regen.sh`), and for an option, the right status in
`scripts/org-options.json`. One pull request per change, as `CLAUDE.md`
says.

## 1. Claims in the parity table that are wrong

1. **Agenda dispatcher `T`.** `docs/parity.org:303` (from
   `scripts/features.mjs:474`) says "`T` in the agenda dispatcher asks for
   the keyword instead". The dispatcher (`fn dispatch`,
   `server/org-lsp/src/server.rs`, `let built_in` near line 8813) offers
   only `a t m M s S #`, and `Query::Todo` (`server/org-lsp/src/views.rs`)
   takes no keyword. **Confirmed:** `org.agenda.dispatch` with input `T`
   gives `No agenda command for this key`. Either add `T`
   (`org-todo-list` with a keyword, `Keyword (or KWD1|KWD2|...)` as the
   sparse tree's `T` already asks) or correct the text.
2. **Going to a date in the agenda is `J`, not `j`.** `docs/parity.org:325`
   and `docs/configuration.org:3928` call it `j`; `commands.rs:493` binds
   `J` (Shift+J in `docs/keys/vscode.org`). In Emacs, `j` is
   `org-agenda-goto-date` and `J` is `org-agenda-clock-goto`, which has no
   key in the agenda here. Decide on the keys (the Emacs preset could have
   `j`, Vim keeps `J`) and make the docs match.

## 2. Not done and not written down

### In-buffer settings

3. **`#+SETUPFILE:` is not read.** TODO keywords, tags, `#+STARTUP`,
   `#+LINK`, `#+PRIORITIES` and the rest in a setup file are ignored
   (`org-set-regexps-and-options` reads them in Emacs). `SETUPFILE` appears
   only in export settings and in org-lint
   (`server/org-core/src/lint.rs`). **Confirmed:** with
   `#+SETUPFILE: ./setup.org` and `setup.org` holding `#+TODO: FOO | BAR`,
   `org.todo.next` on `* item` inserts `TODO `; with `#+TODO: FOO | BAR` in
   the file itself it inserts `FOO `. Probably the largest gap here.

### Agenda

4. Agenda dispatcher entries `/` (`org-occur-in-agenda-files`), `e`
   (`org-store-agenda-views`, the `:exports` of custom commands), `<` and
   `>` (restrict to the buffer or subtree).
5. The restriction lock: `C-c C-x <` and `C-c C-x >`
   (`org-agenda-set-restriction-lock`, `org-agenda-remove-restriction-lock`).
   Its option `org-agenda-restriction-lock-highlight-subtree` is
   `emacs-only` in `scripts/org-options.json:267`, which hides that the
   feature is missing.
6. Agenda keys with no command in the manifest:
   - `J` `org-agenda-clock-goto` (see item 2)
   - `T` `org-agenda-show-tags`
   - `>` `org-agenda-date-prompt`
   - `M-<up>`, `M-<down>` `org-agenda-drag-line-backward/forward`
   - `C-c C-x b` `org-agenda-tree-to-indirect-buffer`
   - `G` `org-agenda-toggle-time-grid`, `!` `org-agenda-toggle-deadlines`,
     `~` `org-agenda-limit-interactively`
   - `A` `org-agenda-append-agenda`
   - `P` `org-agenda-show-priority`
   - `C-c C-a` `org-attach` from the agenda
   - `C-/` `org-agenda-undo` (undoes the change in the file)
   - `C-c C-x C-e` `org-clock-modify-effort-estimate`
   - the calendar keys `c` `org-agenda-goto-calendar`, `C`
     `org-agenda-convert-date`, `M` `org-agenda-phases-of-moon`, `S`
     `org-agenda-sunrise-sunset`, `H` `org-agenda-holidays`
   Smaller ones of the Emacs preset: `l` (log mode, only `v l` here), `g`
   (`org-agenda-redo-all`), `d` / `w` / `y` (only `v d` …), `v SPC`
   (`org-agenda-reset-view`).
7. Changing the list of agenda files: `C-c [` `org-agenda-file-to-front`,
   `C-c ]` `org-remove-file`, `C-,` / `C-'` `org-cycle-agenda-files`.

### Capture

8. Prefix arguments of `org-capture`: `C-u C-c c` (go to the template's
   target, `org-capture-goto-target`), `C-u C-u C-c c`
   (`org-capture-goto-last-stored`), `C-0 C-c c` (insert at the cursor).
   `CaptureContext` (`server/org-lsp/src/server.rs:547`) has no count. The
   parity table says Capture is 100%.

### Clocking and dates

9. `S-M-<up>` / `S-M-<down>` on a `CLOCK:` line shift both timestamps
   (`org-shiftmetaup` → `org-clock-timestamps-up`). **Confirmed:**
   `org.shiftmeta.up` there gives `Not at a table`. `C-S-<up>` /
   `C-S-<down>` on clock lines (`org-shiftcontrolup`) have no command
   either.
10. `C-c C-c` on a `CLOCK:` line (`org-clock-update-time-maybe`) works only
    with the cursor on a timestamp. **Confirmed:** on the word `CLOCK:` it
    gives `Not at an item or heading, and no active region`.
11. `C-c C-y` `org-evaluate-time-range` and `C-c <`
    `org-date-from-calendar`.
12. `C-c C-x C-e` `org-clock-modify-effort-estimate` in Org files.
13. `org-clock-auto-clockout-timer` (clock out after idle time) is
    `emacs-only` in `scripts/org-options.json:456`, though it is a feature
    of the clock.

### Lists and checkboxes

14. `C-u C-c C-c` adds or removes a checkbox; `C-u C-u C-c C-c` sets
    `[-]` (`org-toggle-checkbox` with a prefix). **Confirmed:**
    `org.checkbox.toggle` with `count: 4` on `- item one` changes nothing;
    with `count: 16` on `- [ ] item one` it ticks the box (`X`).
15. Radio checkbox lists: `org-list-checkbox-radio-mode`,
    `org-toggle-radio-button`, `#+ATTR_ORG: :radio t`.
16. `C-c C-*` `org-list-make-subtree`.
17. `C-c #` (`org-update-statistics-cookies`) has no key, so `C-u C-c #`
    (every cookie of the file) cannot be run; only `C-c C-c` on a cookie.

### Editing and structure

18. `M-<up>` / `M-<down>` on a paragraph, block or other element drag it
    (`org-drag-element-backward/forward`). Written down only for inline
    tasks (`server/org-core/src/structure.rs:489`). **Confirmed:**
    `org.meta.up` on a paragraph gives `Not at a headline`.
19. Org's element commands: `M-{` / `M-}` (`org-backward/forward-element`),
    `C-c C-^` / `C-c C-_` (`org-up-element`, `org-down-element`), `C-M-t`
    (`org-transpose-element`), `M-h` (`org-mark-element`), `C-c @`
    (`org-mark-subtree`).
20. Filling as Org fills: `M-q` (`org-fill-paragraph`) and auto-fill
    (`org-auto-fill-function`), which know items, comments, tables and
    headlines.
21. `C-c :` `org-toggle-fixed-width`, `C-c C-x v` `org-copy-visible`, the
    mark ring (`C-c %` `org-mark-ring-push`, `C-c &`
    `org-mark-ring-goto`; `org-mark-ring-length` is `emacs-only`).
22. `C-c C-c` on a footnote reference or definition goes to the other
    (`org-footnote-action` through `org-ctrl-c-ctrl-c`). **Confirmed:** it
    gives `Not at an item or heading, and no active region`. `C-c C-x f`
    works.

### Visibility, tags, entities

23. No command found for `C-c TAB` off a table (`org-fold-show-children`),
    `C-c C-k` (`org-kill-note-or-show-branches`) or `C-c C-TAB`
    (`org-force-cycle-archived`). Check what each editor's own folding
    gives before calling these missing.
24. `C-c C-x q` `org-toggle-tags-groups`.
25. `C-c C-x \` `org-toggle-pretty-entities` as a command (only
    `#+STARTUP: entitiespretty` and the setting exist).

### Tables

26. `C-c }` `org-table-toggle-coordinate-overlays`, `C-#`
    `org-table-rotate-recalc-marks`, `org-table-transpose-table-at-point`,
    `C-c ~` `org-table-create-with-table.el` (and `table.el` tables at
    all), `org-table-follow-field-mode` (only its option
    `org-table-exit-follow-field-mode-when-leaving-table` is listed).

### Code blocks (Babel)

27. None of these exist: `C-c C-v I` `org-babel-view-src-block-info`,
    `v` `org-babel-expand-src-block`, `c` `org-babel-check-src-block`,
    `j` `org-babel-insert-header-arg`, `l` `org-babel-load-in-session`,
    `z` `org-babel-switch-to-session`, `a` `org-babel-sha1-hash`, `d`
    `org-babel-demarcate-block`, `o` `org-babel-open-src-block-result`,
    `h` `org-babel-mark-block`, `x`
    `org-babel-do-key-sequence-in-edit-buffer`, `f`
    `org-babel-tangle-file`, `r` `org-babel-goto-named-result`, `u`
    `org-babel-goto-src-block-head`. Most could go through Emacs, as
    `org.babel.execute` does.

### Column view

28. `s` `org-columns-edit-attributes` and `M-S-<right>` `org-columns-new`.

## 3. Only implied, worth a sentence each

29. Writing the agenda as `.html`, `.htm`, `.ps`, `.pdf` or `.ics` is
    refused in code (`server/org-lsp/src/server.rs:8950`, "Writing the
    agenda as .{extension} is Emacs's …"); the parity table only says
    "`.org` and plain text".
30. `orgtbl-mode`, radio tables and radio lists (`#+ORGTBL: SEND`,
    `org-list-send-list`) appear only as two option names
    (`orgtbl-optimized`, `orgtbl-radio-table-templates`), never as a
    feature.
31. Pasting and dropping images (`org-yank-image-save-method`,
    `org-yank-dnd-method` and their kin, `scripts/org-options.json:884`
    on) are `emacs-only`; an editor could paste an image into a file and a
    link.

## Reproducing the confirmed items

Build the server (`cargo build -p org-lsp`) and talk to
`target/debug/org-lsp` over stdio with `Content-Length` framing:
`initialize` with `{"capabilities": {"workspace": {"workspaceEdit":
{"documentChanges": true}}, "general": {"positionEncodings": ["utf-8"]},
"experimental": {"orgClient": true}}, "initializationOptions":
{"config_file": false}}`, then `initialized`, `textDocument/didOpen`, and
`workspace/executeCommand` with `arguments: [{"textDocument": {"uri": …},
"position": {"line": …, "character": …}}]`, adding `"count"` for a prefix
argument and `"input"` for an answer. Answer `workspace/applyEdit` with
`{"applied": true}`. The tests in `server/org-lsp/tests/agenda_lsp.rs` do
the same through `Client`; a new test there is the better way to pin each
item down.
