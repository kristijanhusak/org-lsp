# Notes of the editors fixer (worktree agent-aad9ba18ce3a696fd)

Target dir on /dev/shm/fix-editors-target (worktree `target` symlinked there).

- helix-6: fixed (server applies the edits a result carries for clients that do nothing with results, `message_prompts`; executeCommand and code-action answers). Test agenda_lsp `a_plain_client_gets_the_edits_of_results_and_filters_after_changes`.
- helix-2: fixed (a filter change releases the held agenda changes, so the filtered render is published; before, the held view kept the old lines). Same test.
- helix-1: fixed by leaving /Save all Org files/ out of the agenda's code actions for clients that do nothing with results (the server cannot save; writing the files itself makes Helix refuse `:w` later: "file modified by an external process"); running it there says "The editor saves the files: the server cannot". Docs: keys/helix.org, features.mjs (Helix: `:write-all`).
- helix-7 / vim-17: fixed. With `message_requests` off, Emacs's yes/no is asked as a command's question: org clients with run_command get `org-lsp/runCommand` `org.emacs.answer` (key input "…? (y or n) "), others the code-action choices. Test lsp.rs `asks_emacs_questions_without_message_requests`.
- vim-18: server side: new integration option `message_notifications` / `experimental.orgMessages`: messages as `org-lsp/message`.
