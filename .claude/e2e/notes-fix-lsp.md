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
