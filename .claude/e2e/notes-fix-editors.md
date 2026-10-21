# Notes of the editors fixer (worktree agent-aad9ba18ce3a696fd)

Target dir on /dev/shm/fix-editors-target (worktree `target` symlinked there).

- helix-6: fixed (server applies the edits a result carries for clients that do nothing with results, `message_prompts`; executeCommand and code-action answers). Test agenda_lsp `a_plain_client_gets_the_edits_of_results_and_filters_after_changes`.
- helix-2: fixed (a filter change releases the held agenda changes, so the filtered render is published; before, the held view kept the old lines). Same test.
- helix-1: fixed by leaving /Save all Org files/ out of the agenda's code actions for clients that do nothing with results (the server cannot save; writing the files itself makes Helix refuse `:w` later: "file modified by an external process"); running it there says "The editor saves the files: the server cannot". Docs: keys/helix.org, features.mjs (Helix: `:write-all`).
- helix-7 / vim-17: fixed. With `message_requests` off, Emacs's yes/no is asked as a command's question: org clients with run_command get `org-lsp/runCommand` `org.emacs.answer` (key input "…? (y or n) "), others the code-action choices. Test lsp.rs `asks_emacs_questions_without_message_requests`.
- vim-18: server side: new integration option `message_notifications` / `experimental.orgMessages`: messages as `org-lsp/message`.
- vim-17/vim-18 plugin side: Vim plugin sets `message_requests: false`, `message_notifications: true`; shows `org-lsp/message` itself (s:say, Emacs's text, ErrorMsg/WarningMsg by type); all four adapters route it.
- vim-6: fixed: org_lsp#read_text uses input(prompt, value, 'customlist,…') with a temporary cmap <Esc> for cancel (C-c caught). Test test.vim "reads typed answers…" calls the real function.
- vim-11 / neovim-11: fixed: prompt_text (Neovim M.prompt_text, Vim org_lsp#prompt_text): prompts ending in a blank as they are, ending ':'/'?' get ' ', titles get ': '.
- vim-1 (and Neovim): priority kind read as one key.
- vim-4 / neovim-4: command errors shown without `org-lsp: ` (Emacs's text; WARN level kept).
- vim-14: fixed (Vim's default fold:- replaced, user's own fill char kept).
- vim-3: fixed (<C-Space> also mapped as <C-@> in terminal Vim). vim-20: documented in docs/keys/vim.org (terminal modifiers; mappings example).
- vim-19 + neovim-17: `:Org` takes a range (lines as region) and completes word by word; unknown command named as typed (Neovim).
- vim-13: redrawstatus after cursor moves from callbacks.
- vim-8: vim-lsp adapter: BufReadPost of an attached Org buffer feeds vim-lsp's listener two changes so it sends the whole text (vim-lsp's own version).
- vim-21: after each command, yegappan (LspInlayHintsUpdateNow) / vim-lsp (its inlay request + set) asked to show inlay hints again.
- vim-7 / vim-9: server writes a capture into an existing empty, unopened target file (as for a missing one) instead of an edit. Test agenda_lsp `a_capture_into_an_empty_file_writes_it`. Not covered: an *open* empty buffer with vim-lsp/coc (their appliers).
- neovim-16 (and Vim): TAB on the trailing blank of a new headline sends the end of line, so level cycling goes on.
- neovim-2: vim.str_utfindex. neovim-5: conceallevel only raised from 0. neovim-6: CmdlineChanged → org.date.preview while a date is typed. neovim-7/vim-5: server honours count in calendar commands (scenario calendar.txt updated: its counts now count). neovim-8: commands run synchronously when keys are typed ahead (so RET then o goes to the file); showDocument retried after E565. neovim-9: same sync (keys typed ahead answer the menu). neovim-15: following from the org-roam buffer opens in the other window. neovim-18: no size for a new tab's only window. neovim-20: e2e waits 15 s.
- neovim-1: documented in docs/development.org (Neovim nightly assertion; calendar marks are within lines, not avoidable cheaply).
- Committed (rebased on main 1b72453, #366): 01fcbb2 server, 1ce8bd1 Vim, 69bf673 Neovim, f1470f0 docs.
- neovim-12: fixed (server sends `column` = Emacs's cursor column after a column view command; Neovim, Vim, VS Code pick it).
- neovim-13: server says in words that column row moves need a newer Org than Emacs 29's 9.6.15; documented in configuration.org.
- vim-4 agenda message: "Command not allowed in this line" (org-agenda-error) instead of "No entry on this line".
- vim-15: matches Emacs (C-c C-o right after a definition's label says "No link found" in Emacs 9.8.10 too; checked in batch); TESTING wording fixed.
- vim-16, neovim-3, helix-3: docs (features.mjs, settings.rs, keys/helix.org).
- neovim-19: covered by #366's emacs-8 fix (Emacs's message shown after its edit).
- 1046759 vim-12 fixed (org-core table Done.message, FORMULAS_UPDATED; test org-core tests/table.rs).
- 888de27: helix-4/helix-5 fixed in highlights.scm (checked with hx_hl.py); neovim-10 fixed (link hover → Org float, test agenda.lua 16); neovim-14 mitigated (roam window nowrap) + documented; helix-8 documented (features.mjs); vim-10 documented (docs/vim.org coc paragraph).
- ba9d1cf tests updated (command count, capture into empty file read from disk); ce822e0 Helix exception for agenda-entries step 14 (Save all not offered).
- Final runs (branch rebased on 1b72453): check.sh all passed; Neovim stable and 0.12.0 all files pass (e2e 401, agenda 224, scenarios 16/16, …); Vim all four clients: cases + highlighting ok (256 ok lines), scenarios 16/16 each; Helix run.py 13 ok, 3 skip, then agenda-entries ok after its exception.
- Not fixed: vim-9 for an *open* empty buffer with vim-lsp/coc (their own appliers); neovim-1 (Neovim nightly assertion, documented); vim-2 not a bug.
