# Notes of the emacs tester (2026-10-21)

Harness: `.claude/e2e/repro/emacs/lsp.py` (own small LSP client).
Env: Emacs /usr/bin/emacs, Org 9.8.10 at /root/org-mode/lisp; gpg present; no gnuplot, no latex.
Server: target/debug/org-lsp (main at 5f01da5 / bad79cc).

## Code read
- emacs_options: config.rs `Options::emacs_options` (name checks, server side), bridge.el `--set-option` (types, Emacs side), notices -> showMessage WARNING "org-lsp: ...".
- config merge (`config::merge`) is a deep merge: suspect for emacs_options tables (alist values) and Neovim `[]`.
- No workspace/configuration: options only from initializationOptions.

## Done so far
- t_roundtrip.py: all 395 `json` examples of scripts/emacs-options.json set via emacs_options (with Org 9.8.10 on load-path) end equal to the standard value, no notice. 85 options have no example.
- t_kinds.py: bool/int/float(int→float)/string/symbol/const-or-int/`{}`/lists/alist/set/cons/coding-system/repeat of choice: OK. Bugs: emacs-1 (keyword consts), emacs-2 (plist key-type), emacs-3 (:inline repeat), emacs-4 (choice order), emacs-5 (function types).
- org-file-apps: server says "it is the setting `org_file_apps`" (triage setting): fine.
- t_precedence.py: file < editor (emacs-6 deep merge; Neovim [] drops file's). emacs_config_file < emacs_config < emacs_options: OK. `with-eval-after-load` in emacs_config: emacs_options still win (set after).
