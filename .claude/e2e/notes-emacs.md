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
- t_noemacs.py (PATH without emacs, and `emacs = "/nonexistent/emacs"`): every Emacs command → "Cannot start Emacs (`emacs`, the `emacs` option): No such file or directory (os error 2)" (as a JSON-RPC error for the synchronous ones, an ERROR showMessage for the async ones); org.todo etc. fine; server-side emacs_options name check still reported.
- t_failures.py: Lisp error in emacs_config → "Emacs could not load Org with your configuration: Wrong type argument: listp, 1", every command; killed Emacs → "Emacs stopped", next command restarts it; stuck call → "Emacs did not answer in time" after 30 s, Emacs replaced. emacs-11 (server frozen meanwhile).
- t_crypt.py (gpg key in a short /tmp GNUPGHOME, org-crypt-key via emacs_options): encrypt/decrypt/encrypt.all/decrypt.all OK; symmetric (no key, no pinentry): GPG's 'GPG error: "Encrypt failed", "Exit"' (environment: no pinentry; with a desktop pinentry it may work); before-save magic: first willSaveWaitUntil 1.06 s (Emacs start; VS Code's budget is 1.5 s, a slower start would save plaintext), then 0.01 s.
- t_columns.py: column view values/edit/next/quit and EFFORT key case identical to Emacs; columnview dblock; agenda columns (summary on the header line, as Emacs's org-agenda-colview-summarize); appt "Added 1 event for today".
- t_timer.py / t_countdown*.py: start/insert/pause/continue/item/stop with org-timer-format from emacs_options; countdown fires "Task: time out"; emacs-12 (replace countdown).
- t_edit.py / t_edit2.py: C-c ' src (indentation as Emacs), formulas editor (save writes TBLFM), C-c ` field, formula debugger (questions with substitution history, No → "Abort"), table export (format choice, "Export done."), import, ascii plot. Plot: emacs-13. C-c ` on a hline opens an editor as Emacs does.
- t_lisp.py: capture %(sexp) incl. error form `%![Error: (wrong-type-argument listp 1)]`, %\1 inside %() same as Emacs (\x01), file+function and function targets, refile verify function, diary sexps, agenda prefix %(…) (grid lines evaluated outside Org: "-"), link abbrev function (emacs-14), bad diary sexp (emacs-15).
- t_tangle.py: Lisp :tangle and noweb call (Emacs fallback): same files, modes (shebang 755) and message as Emacs.
- t_edits.py: footnote r/s/n, C-c - / C-c * / C-c ; , drawer, structure template, copy-down, table create: identical to Emacs batch (with emacs_options footnote/table options).
- t_visible.py: visible-only export follows the editor's folds as Emacs's folds do.
- t_babel_err.py: emacs-16 (error output lost); read-string → "Emacs asked a question org-lsp cannot answer: Your name:".
- t_configfile.py: emacs_options from `config_file` (JSON) and $ORG_LSP_CONFIG (TOML) both apply.
- t_restart.py: emacs-18.
- t_misc.py: a failing :set → WARNING "`emacs_options` could not set `org-attach-method`: set failed for mv", Emacs keeps going; a :set that fails when the defcustom itself initializes → "Emacs could not load Org with your configuration: …" on every command.
- Not through Emacs (server-side, not tested here): ctags, man:, info: links.
- Export dispatcher has no C-a (async) toggle: all exports run in the background anyway (not logged).
