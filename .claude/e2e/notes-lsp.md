# Notes of the lsp tester, 2026-10-21

Server: `/home/user/org-lsp/target/debug/org-lsp` (built from main, Oct 20 19:29),
driven over stdio by `.claude/e2e/repro/lsp/client.py` (no editor code).
Emacs: /usr/bin/emacs; Org 9.8.10 from /root/org-mode/lisp. The server does
not read `ORG_LISP_DIR` itself (only its tests do, `server/org-lsp/tests/lsp.rs`):
to make its Emacs use that Org, the harness passes
`emacs_config = (add-to-list 'load-path "/root/org-mode/lisp")`.
Settings reach the server only through `initializationOptions` and the shared
config file (`$ORG_LSP_CONFIG`, `$XDG_CONFIG_HOME/org-lsp/config.toml`); the
harness sets `XDG_CONFIG_HOME` to a scratch dir so no user file leaks in.
No `workspace/configuration` request is made by the server.

Manifest: 313 commands (`org-lsp manifest`).

