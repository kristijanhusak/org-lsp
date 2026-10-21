# Notes of the lsp fixer (worktree agent-a4fa46448bdc13e40)

- lsp-1: fixed. Completion after `@`, org.cite.insert's choices and the
  quick fixes' `citationKeys` keep each key once; the diagnostic message and
  hover still say `knuth1984 knuth1984` as Emacs does. Test
  `citation_keys_once_when_the_bibliography_is_named_twice` (tests/lsp.rs).
- lsp-11: fixed in `line_edits` (server.rs): a line's `\r` before `\n` is
  left out of both the range and the text when old and new both end with
  it. The test `formats_tables_and_tags` expected the old (wrong) edits;
  changed to the LSP ones.
- lsp-13: fixed. New `server/org-lsp/src/stdio.rs` replaces
  `lsp_server::Connection::stdio()`: same framing, but a body that is not a
  message (bad JSON, a batch array, not UTF-8) is logged (stderr and
  `window/logMessage` type Error) and skipped; broken headers still end the
  connection (no way to find the next message). No -32700 response: its id
  would be null, matching no request. Unit tests in stdio.rs.
- lsp-14: not a bug. In robust.py the line is `** NEXT …` and the next
  state `WAIT(w@)` asks for a note: the server opened `CAPTURE-Note.org`
  (showDocument) and sent no edit, so nothing was refused. A refused edit
  does give `window/showMessage` WARNING "The editor did not make the
  change: <failureReason>" and takes back the clock/edited state
  (server.rs `response`; test in tests/lsp.rs "read-only"). Checked with a
  plain `* TODO A` file, both profiles (scratch refuse.py).
- lsp-12: release build, `scale.py`-style file (entries of headline+tag,
  SCHEDULED, link), PLAIN client. Before: open+folding 6000 lines 0.27 s,
  12k 0.68 s, 24k 1.95 s, 60k 10.1 s (superlinear); keystroke+hover 6000
  0.23-0.28 s, 24k 1.3 s, 60k 5.5 s. Hotspot 1: rowan's `token_at_offset`
  scans every child of a node (the document root has one child per
  headline), called per timestamp by `Outline::collect_stamps` →
  quadratic. New `org_syntax::token_at_offset` (binary search on each
  level, same answers: test `token_at_offset_is_rowans`), used everywhere
  instead. Hotspot 2: `org-lint` over the whole file on every didChange
  (linear but ~1.5 s at 60k lines): diagnostics after a change now wait for
  200 ms without a message from the editor (`DIAGNOSTICS_DELAY`,
  `diagnostics_later`), never while messages wait; didOpen still publishes
  at once. After (scale2.py, ten keystrokes each followed by a hover):
  6000 lines open+folding 0.43 s, keystroke+hover 46 ms; 24k 0.54 s / 152
  ms; 60k 0.57 s (without open lint; ~2 s with it) / ~470 ms (linear: the
  re-parse etc.). Lint itself left linear (each checker linear, measured).
  Remaining per-keystroke work (full re-parse, virtual indent, display) is
  linear, ~8 us/line; not made incremental.
- lsp-2: protocol.org Capabilities: formatting, documentLink(+resolve),
  references, prepareRename/rename, workspace/symbol, inlayHint,
  documentHighlight described; executeCommandProvider's two extra commands.
- lsp-16: `elisp:` links are followed: "Execute FORM as Elisp? " (key y/n,
  step `elisp`), then Emacs's bridge `eval` (existing method, no bridge.el
  change) evaluates `(format "%s" (eval (read FORM)))` (or
  call-interactively for a symbol), message `FORM => VALUE`, Emacs's error
  text on failure; done in Emacs's own buffer, not the file (written in
  parity). help/eshell/eww/w3m/irc/gnus/rmail/mhe/bbdb/docview/shortdoc
  are no longer completed after `[[`, and parity now has a ❌ line for them
  (Links no longer 22 of 22). Test `evaluates_elisp_links`.
- lsp-6: C-c C-c on a headline's cookie runs `org-update-statistics-cookies`
  (port that existed in archive.rs, made pub(crate)); test
  `server/org-core/tests/statistics_cookie.rs` with Emacs's outputs (batch,
  5 cases incl. TODO children, leaf → [0/0]/[100%], COOKIE_DATA todo).
  Headline elsewhere: still org-toggle-checkbox (Emacs: org-set-tags-command)
  — documented as a difference in parity.
- lsp-17: inline tasks keep their stars in `refile::shift_levels` (archive,
  archive sibling, refile, capture) and `structure::paste` (paste/yank), as
  Emacs (checked in batch: paste up/down, no END line, archive, sibling).
  Test `shifting_a_subtree_leaves_its_inline_tasks` (org-core inlinetask).
- lsp-3: documented (parity, Agenda, ❌ line): the "Press ‘N r’ … to search
  again" / "C-u r" / "add/sub word" hint lines and `N r` itself are left
  out; `T` in the dispatcher asks for the keyword.
- lsp-4: fixed. `lint::Context` has `link_types` (types packages add); the
  server passes `["roam"]` when org-roam is on, and `invalid-fuzzy-link`
  leaves links of those types alone. (The warning "going away" in the repro
  was the roam: link being replaced by an id: link on save, not the index.)
  Test `roam_links_are_no_fuzzy_links_with_org_roam` (org-core tests/lint.rs).
- lsp-18: fixed. `org.ctags.create` says what ctags wrote, or "(Shell
  command succeeded with no output)" / "(Shell command failed with code N
  and no output)", as `shell-command` does (Org ignores the exit code).
  Test added to `follows_links_to_ctags_tags`.
- lsp-5: fixed. `columns_question` asks as Org 9.8.10's
  `org-columns-edit-value`: a property with allowed values →
  kind choice "Value: " (or text with completions when `:ETC`), else
  "Edit: " with the value; TODO column → the server's own `org.todo` on the
  headline (fast selection as from its key), column view following the
  change; before the view has come from Emacs → refused "Column view is not
  shown yet". (Agenda column view: same questions; its TODO column still
  goes to Emacs, which cycles.) Unit test `column_view_asks_as_emacs_does`;
  the two Emacs tests now expect "Edit: ". Checked with columns.py.
