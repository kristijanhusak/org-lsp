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
- t_names.py: server-side name checks: one WARNING joining all with "; " (setting/lisp/emacs-only/other/absent/missing/typo with suggestion/none/underscores) — texts as config.rs. Emacs-side notices (WARNING "org-lsp: …") arrive when the library loads (mid-command: ox at first export, org-attach at first attach). `org-export-time-stamp-file` (obsolete alias) → "Org has no such option (did you mean `org-export-timestamp-file`?)": good.
- t_oldorg.py: with Emacs's own Org 9.6.15: "it is not an option of Org 9.6.15". Options of libraries never loaded: no message (expected).
- org-html-validation-link = false is refused ("false is not a string", the type is string); `""` works. Customize-faithful; worth a doc line maybe.
- t_schema.py: config.schema.json and package.json agree (description vs markdownDescription); 395 defaults validate; typos refused; no item checks inside alists/sets (`{pyton=true}` passes).
- t_export.py: HTML/LaTeX/Markdown/ASCII files identical to Emacs batch with the same setq (after normalizing ids); buffers (A, H body-only, subtree) shown via showDocument; PDF without latex: Emacs's error text; ODT ok; `#` lists backends; `&` stack; `P x` with no projects: "No projects in org-publish-project-alist"; `h o` opens externally.
- t_babel.py: execute.buffer result identical to Emacs batch (elisp, python output, shell value, python :session twice, :var, noweb, #+call, inline src/call, table). Confirmation asked via showMessageRequest; No → "Evaluation of this emacs-lisp code block (one) is aborted." (Emacs's text); remove_result ok.
- t_attach.py: a(mv)/c/l/y/m make the right kinds of files in org-attach-id-dir; n opens the new file; o/f open externally; d picks; D asks twice (Emacs's "Recursive?"), s/S. Issues emacs-7 (u), emacs-8 (message lost on edit), emacs-9 (keys).
- Docs page: all 480 checked (cmp_options.py), parse_page.el, page_toml.py → emacs-10.
